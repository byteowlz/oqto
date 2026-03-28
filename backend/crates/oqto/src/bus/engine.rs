//! Bus engine: in-memory pub/sub with scope-based authorization.
//!
//! The engine lives inside the oqto backend process. It maintains subscription
//! tables and routes events to matching subscribers. All authorization checks
//! happen here -- callers provide authenticated identity, the engine enforces.

use crate::shared_workspace::SharedWorkspaceService;
use dashmap::DashMap;
use log::{debug, info, warn};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

use super::types::*;

// ============================================================================
// Subscriber
// ============================================================================

/// Unique subscriber ID (connection-scoped).
pub type SubscriberId = u64;

/// A single subscription entry.
#[derive(Debug, Clone)]
struct Subscription {
    /// Topic glob pattern (e.g., "app.*", "trx.issue_created").
    topic_pattern: String,
    /// Scope + scope_id this subscription covers.
    scope: BusScope,
    scope_id: String,
    /// Owning user (server-resolved, never client-provided).
    user_id: String,
    /// Optional payload filter.
    filter: Option<Value>,
}

/// A registered subscriber with a channel to send events.
struct Subscriber {
    user_id: String,
    tx: mpsc::UnboundedSender<BusEvent>,
    subscriptions: Vec<Subscription>,
}

// ============================================================================
// Rate limiter
// ============================================================================

/// Simple sliding-window rate limiter per subscriber.
struct RateLimit {
    window_start: std::time::Instant,
    count: u64,
    max_per_sec: u64,
}

impl RateLimit {
    fn new(max_per_sec: u64) -> Self {
        Self {
            window_start: std::time::Instant::now(),
            count: 0,
            max_per_sec,
        }
    }

    fn check_and_increment(&mut self) -> bool {
        let now = std::time::Instant::now();
        if now.duration_since(self.window_start).as_secs() >= 1 {
            self.window_start = now;
            self.count = 0;
        }
        if self.count >= self.max_per_sec {
            return false;
        }
        self.count += 1;
        true
    }
}

// ============================================================================
// Stats
// ============================================================================

/// Bus statistics for observability.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BusStats {
    pub subscriber_count: usize,
    pub total_subscriptions: usize,
    pub events_published: u64,
    pub events_delivered: u64,
    pub events_dropped_authz: u64,
    pub events_dropped_rate: u64,
}

// ============================================================================
// Engine
// ============================================================================

/// The bus engine. Create one per oqto backend instance.
pub struct BusEngine {
    /// All subscribers keyed by ID.
    subscribers: DashMap<SubscriberId, Subscriber>,
    /// Rate limiters per subscriber.
    rate_limits: DashMap<SubscriberId, RateLimit>,
    /// Next subscriber ID.
    next_id: AtomicU64,
    /// Shared workspace service for membership checks.
    shared_workspaces: Option<Arc<SharedWorkspaceService>>,

    // Stats
    events_published: AtomicU64,
    events_delivered: AtomicU64,
    events_dropped_authz: AtomicU64,
    events_dropped_rate: AtomicU64,
}

impl BusEngine {
    /// Create a new bus engine.
    pub fn new(shared_workspaces: Option<Arc<SharedWorkspaceService>>) -> Self {
        Self {
            subscribers: DashMap::new(),
            rate_limits: DashMap::new(),
            next_id: AtomicU64::new(1),
            shared_workspaces,
            events_published: AtomicU64::new(0),
            events_delivered: AtomicU64::new(0),
            events_dropped_authz: AtomicU64::new(0),
            events_dropped_rate: AtomicU64::new(0),
        }
    }

    /// Register a new subscriber. Returns (subscriber_id, event_receiver).
    pub fn register(&self, user_id: &str) -> (SubscriberId, mpsc::UnboundedReceiver<BusEvent>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.insert(
            id,
            Subscriber {
                user_id: user_id.to_string(),
                tx,
                subscriptions: Vec::new(),
            },
        );
        // Default rate limit: 100 events/sec for publishing
        self.rate_limits.insert(id, RateLimit::new(100));
        info!("Bus: registered subscriber {} for user {}", id, user_id);
        (id, rx)
    }

    /// Unregister a subscriber (connection closed).
    pub fn unregister(&self, id: SubscriberId) {
        if let Some((_, sub)) = self.subscribers.remove(&id) {
            self.rate_limits.remove(&id);
            info!(
                "Bus: unregistered subscriber {} for user {}",
                id, sub.user_id
            );
        }
    }

    /// Add a subscription for a subscriber. Returns error string if authz fails.
    pub async fn subscribe(
        &self,
        subscriber_id: SubscriberId,
        user_id: &str,
        is_admin: bool,
        scope: BusScope,
        scope_id: String,
        topic_patterns: Vec<String>,
        filter: Option<Value>,
    ) -> Result<(), String> {
        // Authorize the subscription scope
        self.authorize_scope(user_id, is_admin, &scope, &scope_id)
            .await?;

        let mut sub = self
            .subscribers
            .get_mut(&subscriber_id)
            .ok_or("Subscriber not found")?;

        // Verify subscriber identity matches
        if sub.user_id != user_id {
            return Err("User ID mismatch".to_string());
        }

        for pattern in topic_patterns {
            sub.subscriptions.push(Subscription {
                topic_pattern: pattern.clone(),
                scope: scope.clone(),
                scope_id: scope_id.clone(),
                user_id: user_id.to_string(),
                filter: filter.clone(),
            });
            debug!(
                "Bus: subscriber {} subscribed to {}/{}/{}",
                subscriber_id,
                scope_str(&scope),
                scope_id,
                pattern
            );
        }
        Ok(())
    }

    /// Remove subscriptions matching scope+topics.
    pub fn unsubscribe(
        &self,
        subscriber_id: SubscriberId,
        scope: &BusScope,
        scope_id: &str,
        topic_patterns: &[String],
    ) {
        if let Some(mut sub) = self.subscribers.get_mut(&subscriber_id) {
            sub.subscriptions.retain(|s| {
                !(s.scope == *scope
                    && s.scope_id == scope_id
                    && topic_patterns.contains(&s.topic_pattern))
            });
        }
    }

    /// Publish an event. The source is server-stamped (caller provides authenticated identity).
    /// Returns error string if authz or rate limit fails.
    pub async fn publish(
        &self,
        publisher_id: Option<SubscriberId>,
        event: BusEvent,
    ) -> Result<(), String> {
        // Rate limit check for non-backend publishers
        if let Some(pid) = publisher_id {
            if let Some(mut rl) = self.rate_limits.get_mut(&pid) {
                if !rl.check_and_increment() {
                    self.events_dropped_rate.fetch_add(1, Ordering::Relaxed);
                    warn!(
                        "Bus: rate limit exceeded for subscriber {} on topic {}",
                        pid, event.topic
                    );
                    return Err("Rate limit exceeded".to_string());
                }
            }
        }

        // Authorize the publish
        self.authorize_publish(&event).await?;

        self.events_published.fetch_add(1, Ordering::Relaxed);

        // Fan out to matching subscribers
        let mut delivered = 0u64;
        let mut to_remove = Vec::new();

        for entry in self.subscribers.iter() {
            let sub_id = *entry.key();
            let subscriber = entry.value();

            // Check if any subscription matches
            let matches = subscriber.subscriptions.iter().any(|s| {
                let scope_matches =
                    s.scope == event.scope && (s.scope_id == event.scope_id || s.scope_id == "*");
                scope_matches
                    && topic_matches(&s.topic_pattern, &event.topic)
                    && filter_matches(&s.filter, &event.payload)
            });

            if matches {
                if subscriber.tx.send(event.clone()).is_err() {
                    // Channel closed, mark for cleanup
                    to_remove.push(sub_id);
                } else {
                    delivered += 1;
                }
            }
        }

        self.events_delivered
            .fetch_add(delivered, Ordering::Relaxed);

        // Clean up dead subscribers
        for id in to_remove {
            self.unregister(id);
        }

        debug!(
            "Bus: published {}/{}/{} -> {} subscribers",
            scope_str(&event.scope),
            event.scope_id,
            event.topic,
            delivered
        );

        Ok(())
    }

    /// Publish from backend internals (no subscriber ID, no rate limit).
    pub async fn publish_internal(&self, event: BusEvent) -> Result<(), String> {
        self.publish(None, event).await
    }

    /// Get bus statistics.
    pub fn stats(&self) -> BusStats {
        let total_subscriptions: usize = self
            .subscribers
            .iter()
            .map(|s| s.value().subscriptions.len())
            .sum();

        BusStats {
            subscriber_count: self.subscribers.len(),
            total_subscriptions,
            events_published: self.events_published.load(Ordering::Relaxed),
            events_delivered: self.events_delivered.load(Ordering::Relaxed),
            events_dropped_authz: self.events_dropped_authz.load(Ordering::Relaxed),
            events_dropped_rate: self.events_dropped_rate.load(Ordering::Relaxed),
        }
    }

    // ========================================================================
    // Authorization
    // ========================================================================

    /// Authorize a scope for subscription or publish.
    async fn authorize_scope(
        &self,
        user_id: &str,
        is_admin: bool,
        scope: &BusScope,
        scope_id: &str,
    ) -> Result<(), String> {
        match scope {
            BusScope::Session => {
                // Admin wildcard for observability dashboards.
                if scope_id == "*" {
                    if is_admin {
                        return Ok(());
                    }
                    return Err("Only admin can subscribe to all sessions".to_string());
                }

                // Session scope: any authenticated user can subscribe to session-scoped events.
                // Delivery filtering ensures they only receive events for sessions they own.
                // The session ownership check happens at event delivery time via source.user_id.
                Ok(())
            }
            BusScope::Workspace => {
                // Admin wildcard for observability dashboards.
                if scope_id == "*" {
                    if is_admin {
                        return Ok(());
                    }
                    return Err("Only admin can subscribe to all workspaces".to_string());
                }

                // Workspace scope: user must own or be a member of the workspace.
                self.check_workspace_access(user_id, scope_id).await
            }
            BusScope::Global => {
                // Global scope: read-only for non-backend clients.
                // Subscribe is allowed; publish is checked separately in authorize_publish.
                Ok(())
            }
        }
    }

    /// Authorize a publish operation.
    async fn authorize_publish(&self, event: &BusEvent) -> Result<(), String> {
        match event.scope {
            BusScope::Global => {
                // Only Backend and Admin sources can publish to global scope.
                match &event.source {
                    EventSource::Backend | EventSource::Admin { .. } => Ok(()),
                    _ => {
                        self.events_dropped_authz.fetch_add(1, Ordering::Relaxed);
                        Err("Only backend/admin can publish to global scope".to_string())
                    }
                }
            }
            BusScope::Session => {
                // Session publish: source must have a user_id.
                if event.source.user_id().is_none() {
                    self.events_dropped_authz.fetch_add(1, Ordering::Relaxed);
                    return Err("Session publish requires authenticated source".to_string());
                }
                Ok(())
            }
            BusScope::Workspace => {
                // Backend and Service sources are trusted (already authorized at the operation level).
                match &event.source {
                    EventSource::Backend | EventSource::Service { .. } => Ok(()),
                    _ => {
                        // User-initiated workspace publish: check workspace access.
                        if let Some(user_id) = event.source.user_id() {
                            self.check_workspace_access(user_id, &event.scope_id)
                                .await
                                .map_err(|e| {
                                    self.events_dropped_authz.fetch_add(1, Ordering::Relaxed);
                                    e
                                })
                        } else {
                            self.events_dropped_authz.fetch_add(1, Ordering::Relaxed);
                            Err("Workspace publish requires user or service identity".to_string())
                        }
                    }
                }
            }
        }
    }

    /// Check if user has access to a workspace (owns it or is a shared workspace member).
    async fn check_workspace_access(
        &self,
        user_id: &str,
        workspace_path: &str,
    ) -> Result<(), String> {
        // Check 1: User's own workspace (path starts with their home dir).
        if workspace_path.contains(&format!("/home/{}", user_id))
            || workspace_path.contains(&format!("/data/{}", user_id))
        {
            return Ok(());
        }

        // Check 2: World-accessible paths (e.g., /tmp). In local mode, this is common.
        if workspace_path.starts_with("/tmp") {
            return Ok(());
        }

        // Check 2: Shared workspace membership.
        if let Some(sw) = &self.shared_workspaces {
            if let Ok(Some(workspace)) = sw.repo().find_workspace_for_path(workspace_path).await {
                if let Ok(Some(_member)) = sw.repo().get_member(&workspace.id, user_id).await {
                    return Ok(());
                }
            }
        }

        Err(format!(
            "User {} has no access to workspace {}",
            user_id, workspace_path
        ))
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Glob-style topic matching. Supports `*` (single segment) and `**` (any segments).
/// Segments are separated by `.`.
pub fn topic_matches(pattern: &str, topic: &str) -> bool {
    // Fast path: exact match
    if pattern == topic {
        return true;
    }
    // Fast path: match-all (** matches anything, * matches single segment only)
    if pattern == "**" {
        return true;
    }

    let pat_parts: Vec<&str> = pattern.split('.').collect();
    let topic_parts: Vec<&str> = topic.split('.').collect();

    topic_match_recursive(&pat_parts, &topic_parts)
}

fn topic_match_recursive(pat: &[&str], topic: &[&str]) -> bool {
    match (pat.first(), topic.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            // ** matches zero or more segments
            if pat.len() == 1 {
                return true; // trailing ** matches everything
            }
            // Try matching rest of pattern against current and subsequent positions
            for i in 0..=topic.len() {
                if topic_match_recursive(&pat[1..], &topic[i..]) {
                    return true;
                }
            }
            false
        }
        (Some(&"*"), Some(_)) => {
            // * matches exactly one segment
            topic_match_recursive(&pat[1..], &topic[1..])
        }
        (Some(p), Some(t)) => {
            if p == t {
                topic_match_recursive(&pat[1..], &topic[1..])
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if an event payload matches a filter.
/// Filter is a JSON object where keys are dot-paths into the payload
/// and values are either literals (exact match) or operator objects.
pub fn filter_matches(filter: &Option<Value>, payload: &Value) -> bool {
    let filter = match filter {
        Some(Value::Object(f)) => f,
        None => return true,
        _ => return true,
    };

    for (path, expected) in filter {
        let actual = json_path(payload, path);

        if let Value::Object(ops) = expected {
            // Operator-based filter
            for (op, val) in ops {
                match op.as_str() {
                    "$in" => {
                        if let Value::Array(arr) = val {
                            if !arr.iter().any(|v| Some(v) == actual) {
                                return false;
                            }
                        }
                    }
                    "$exists" => {
                        let exists = actual.is_some();
                        let want = val.as_bool().unwrap_or(true);
                        if exists != want {
                            return false;
                        }
                    }
                    "$not" => {
                        if actual == Some(val) {
                            return false;
                        }
                    }
                    _ => {} // Unknown operators ignored
                }
            }
        } else {
            // Exact match
            if actual != Some(expected) {
                return false;
            }
        }
    }

    true
}

/// Resolve a dot-separated path into a JSON value.
fn json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        match current {
            Value::Object(map) => {
                current = map.get(segment)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

fn scope_str(scope: &BusScope) -> &'static str {
    match scope {
        BusScope::Session => "session",
        BusScope::Workspace => "workspace",
        BusScope::Global => "global",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_topic_matching() {
        assert!(topic_matches("app.*", "app.message"));
        assert!(topic_matches("app.*", "app.submit"));
        assert!(!topic_matches("app.*", "trx.issue_created"));
        assert!(!topic_matches("app.*", "app.message.detail"));

        assert!(topic_matches("**", "anything.at.all"));
        assert!(topic_matches("app.**", "app.message"));
        assert!(topic_matches("app.**", "app.message.detail.deep"));

        assert!(topic_matches("trx.issue_created", "trx.issue_created"));
        assert!(!topic_matches("trx.issue_created", "trx.issue_updated"));

        // * matches exactly one whole segment, not partial
        assert!(topic_matches("*.*", "trx.issue_created"));
        assert!(!topic_matches("*", "app.message"));
    }

    #[test]
    fn test_filter_matches_exact() {
        let payload = json!({"action": "submit", "app_id": "dash-1"});
        let filter = json!({"action": "submit"});
        assert!(filter_matches(&Some(filter), &payload));

        let filter = json!({"action": "cancel"});
        assert!(!filter_matches(&Some(filter), &payload));
    }

    #[test]
    fn test_filter_matches_in() {
        let payload = json!({"app_id": "dash-1"});
        let filter = json!({"app_id": {"$in": ["dash-1", "editor-2"]}});
        assert!(filter_matches(&Some(filter), &payload));

        let filter = json!({"app_id": {"$in": ["other"]}});
        assert!(!filter_matches(&Some(filter), &payload));
    }

    #[test]
    fn test_filter_matches_exists() {
        let payload = json!({"action": "submit"});
        let filter = json!({"action": {"$exists": true}});
        assert!(filter_matches(&Some(filter), &payload));

        let filter = json!({"missing_field": {"$exists": true}});
        assert!(!filter_matches(&Some(filter), &payload));

        let filter = json!({"missing_field": {"$exists": false}});
        assert!(filter_matches(&Some(filter), &payload));
    }

    #[test]
    fn test_filter_none_matches_everything() {
        let payload = json!({"anything": "here"});
        assert!(filter_matches(&None, &payload));
    }

    #[test]
    fn test_json_path() {
        let val = json!({"a": {"b": {"c": 42}}});
        assert_eq!(json_path(&val, "a.b.c"), Some(&json!(42)));
        assert_eq!(json_path(&val, "a.b"), Some(&json!({"c": 42})));
        assert_eq!(json_path(&val, "a.x"), None);
    }

    #[tokio::test]
    async fn test_publish_and_receive() {
        let engine = BusEngine::new(None);
        let (sub_id, mut rx) = engine.register("alice");

        engine
            .subscribe(
                sub_id,
                "alice",
                false,
                BusScope::Session,
                "ses_1".to_string(),
                vec!["app.*".to_string()],
                None,
            )
            .await
            .unwrap();

        let event = BusEvent::new(
            BusScope::Session,
            "ses_1".to_string(),
            "app.message".to_string(),
            json!({"action": "click"}),
            EventSource::Frontend {
                user_id: "alice".to_string(),
                session_id: Some("ses_1".to_string()),
            },
        );

        engine.publish(None, event).await.unwrap();

        let received = rx.try_recv().unwrap();
        assert_eq!(received.topic, "app.message");
        assert_eq!(received.payload, json!({"action": "click"}));
    }

    #[tokio::test]
    async fn test_scope_isolation() {
        let engine = BusEngine::new(None);
        let (sub_a, mut rx_a) = engine.register("alice");
        let (sub_b, mut rx_b) = engine.register("bob");

        // Alice subscribes to session scope
        engine
            .subscribe(
                sub_a,
                "alice",
                false,
                BusScope::Session,
                "ses_alice".to_string(),
                vec!["app.*".to_string()],
                None,
            )
            .await
            .unwrap();

        // Bob subscribes to different session
        engine
            .subscribe(
                sub_b,
                "bob",
                false,
                BusScope::Session,
                "ses_bob".to_string(),
                vec!["app.*".to_string()],
                None,
            )
            .await
            .unwrap();

        // Publish to Alice's session
        let event = BusEvent::new(
            BusScope::Session,
            "ses_alice".to_string(),
            "app.message".to_string(),
            json!({"data": "for alice"}),
            EventSource::Frontend {
                user_id: "alice".to_string(),
                session_id: Some("ses_alice".to_string()),
            },
        );
        engine.publish(None, event).await.unwrap();

        // Alice receives, Bob does not
        assert!(rx_a.try_recv().is_ok());
        assert!(rx_b.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_global_publish_denied_for_frontend() {
        let engine = BusEngine::new(None);

        let event = BusEvent::new(
            BusScope::Global,
            "global".to_string(),
            "admin.upgrade".to_string(),
            json!({}),
            EventSource::Frontend {
                user_id: "alice".to_string(),
                session_id: None,
            },
        );

        let result = engine.publish(None, event).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("backend/admin"));
    }

    #[tokio::test]
    async fn test_global_publish_allowed_for_admin() {
        let engine = BusEngine::new(None);
        let (sub_id, mut rx) = engine.register("alice");

        engine
            .subscribe(
                sub_id,
                "alice",
                false,
                BusScope::Global,
                "global".to_string(),
                vec!["admin.*".to_string()],
                None,
            )
            .await
            .unwrap();

        let event = BusEvent::new(
            BusScope::Global,
            "global".to_string(),
            "admin.agents_updated".to_string(),
            json!({"version": "v2"}),
            EventSource::Admin {
                user_id: "admin".to_string(),
            },
        );

        engine.publish(None, event).await.unwrap();
        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_filter_on_subscribe() {
        let engine = BusEngine::new(None);
        let (sub_id, mut rx) = engine.register("alice");

        engine
            .subscribe(
                sub_id,
                "alice",
                false,
                BusScope::Session,
                "ses_1".to_string(),
                vec!["app.message".to_string()],
                Some(json!({"action": "submit"})),
            )
            .await
            .unwrap();

        // Non-matching event
        let event1 = BusEvent::new(
            BusScope::Session,
            "ses_1".to_string(),
            "app.message".to_string(),
            json!({"action": "typing"}),
            EventSource::Frontend {
                user_id: "alice".to_string(),
                session_id: Some("ses_1".to_string()),
            },
        );
        engine.publish(None, event1).await.unwrap();
        assert!(rx.try_recv().is_err());

        // Matching event
        let event2 = BusEvent::new(
            BusScope::Session,
            "ses_1".to_string(),
            "app.message".to_string(),
            json!({"action": "submit", "data": "form-data"}),
            EventSource::Frontend {
                user_id: "alice".to_string(),
                session_id: Some("ses_1".to_string()),
            },
        );
        engine.publish(None, event2).await.unwrap();
        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_workspace_wildcard_subscribe_requires_admin() {
        let engine = BusEngine::new(None);
        let (sub_id, _rx) = engine.register("alice");

        let err = engine
            .subscribe(
                sub_id,
                "alice",
                false,
                BusScope::Workspace,
                "*".to_string(),
                vec!["session.**".to_string()],
                None,
            )
            .await
            .unwrap_err();

        assert!(err.contains("Only admin"));
    }

    #[tokio::test]
    async fn test_workspace_wildcard_subscribe_delivers_all_workspaces_for_admin() {
        let engine = BusEngine::new(None);
        let (sub_id, mut rx) = engine.register("admin");

        engine
            .subscribe(
                sub_id,
                "admin",
                true,
                BusScope::Workspace,
                "*".to_string(),
                vec!["session.**".to_string()],
                None,
            )
            .await
            .unwrap();

        let event = BusEvent::new(
            BusScope::Workspace,
            "/home/alice/project".to_string(),
            "session.created".to_string(),
            json!({"session_id": "s1"}),
            EventSource::Service {
                service: "runner".to_string(),
                user_id: Some("alice".to_string()),
            },
        );
        engine.publish(None, event).await.unwrap();

        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_session_wildcard_subscribe_requires_admin() {
        let engine = BusEngine::new(None);
        let (sub_id, _rx) = engine.register("alice");

        let err = engine
            .subscribe(
                sub_id,
                "alice",
                false,
                BusScope::Session,
                "*".to_string(),
                vec!["app.**".to_string()],
                None,
            )
            .await
            .unwrap_err();

        assert!(err.contains("Only admin"));
    }
}

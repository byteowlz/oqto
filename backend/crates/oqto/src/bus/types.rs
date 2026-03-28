//! Bus event types and wire protocol.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::SystemTime;

// ============================================================================
// Scopes
// ============================================================================

/// Event scope determines isolation boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusScope {
    /// Per-session isolation (default). scope_id = session_id.
    Session,
    /// Per-workspace. scope_id = workspace path or workspace_id.
    Workspace,
    /// Global (read-only for most clients, publish by backend/admin only).
    Global,
}

// ============================================================================
// Source identity (always server-stamped)
// ============================================================================

/// Identifies who published an event. Set by the backend, never by the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventSource {
    App {
        app_id: String,
        user_id: String,
        session_id: String,
    },
    Agent {
        user_id: String,
        session_id: String,
        runner_id: String,
    },
    Runner {
        user_id: String,
        runner_id: String,
    },
    Frontend {
        user_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    Service {
        service: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_id: Option<String>,
    },
    Admin {
        user_id: String,
    },
    Backend,
}

impl EventSource {
    /// Extract user_id if present.
    pub fn user_id(&self) -> Option<&str> {
        match self {
            Self::App { user_id, .. }
            | Self::Agent { user_id, .. }
            | Self::Runner { user_id, .. }
            | Self::Frontend { user_id, .. }
            | Self::Admin { user_id, .. } => Some(user_id),
            Self::Service { user_id, .. } => user_id.as_deref(),
            Self::Backend => None,
        }
    }
}

// ============================================================================
// Event envelope
// ============================================================================

/// Canonical bus event envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusEvent {
    /// Unique event ID (server-generated).
    pub event_id: String,
    /// Scope of this event.
    pub scope: BusScope,
    /// Scope identifier (session_id, workspace path, or "global").
    pub scope_id: String,
    /// Topic (e.g., "app.message", "trx.issue_created").
    pub topic: String,
    /// Event payload (arbitrary JSON).
    pub payload: Value,
    /// Server-stamped source identity.
    pub source: EventSource,
    /// Unix timestamp (milliseconds).
    pub ts: u64,
    /// Payload schema version.
    #[serde(default = "default_version")]
    pub v: u32,

    // Optional fields
    /// Priority hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// Time-to-live in milliseconds. Events older than this are dropped from queues.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
    /// Idempotency key for deduplication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// Correlation ID for linking related events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Ack configuration (if ack is expected).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack: Option<AckRequest>,
}

fn default_version() -> u32 {
    1
}

impl BusEvent {
    /// Create a new event with server-generated ID and timestamp.
    pub fn new(
        scope: BusScope,
        scope_id: String,
        topic: String,
        payload: Value,
        source: EventSource,
    ) -> Self {
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            event_id: format!("evt_{}", &uuid::Uuid::new_v4().to_string()[..12]),
            scope,
            scope_id,
            topic,
            payload,
            source,
            ts,
            v: 1,
            priority: None,
            ttl_ms: None,
            idempotency_key: None,
            correlation_id: None,
            ack: None,
        }
    }
}

// ============================================================================
// Ack
// ============================================================================

/// Ack request metadata attached to events that expect confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckRequest {
    /// Topic to publish ack responses to.
    pub reply_to: String,
    /// Timeout in milliseconds.
    pub timeout_ms: u64,
}

/// Ack response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckPayload {
    pub status: AckStatus,
    pub responder_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AckStatus {
    Ok,
    Error,
    Skipped,
    Queued,
}

// ============================================================================
// Wire protocol (WS commands and events for bus channel)
// ============================================================================

/// Commands sent from clients on the "bus" channel.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BusCommand {
    /// Publish an event.
    Publish {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        scope: BusScope,
        scope_id: String,
        topic: String,
        payload: Value,
        #[serde(default = "default_version")]
        v: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        priority: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ttl_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        idempotency_key: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        correlation_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ack: Option<AckRequest>,
    },
    /// Subscribe to topics.
    Subscribe {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Topic patterns (glob-style: "app.*", "trx.issue_*").
        topics: Vec<String>,
        scope: BusScope,
        scope_id: String,
        /// Optional payload filter.
        #[serde(skip_serializing_if = "Option::is_none")]
        filter: Option<Value>,
    },
    /// Unsubscribe from topics.
    Unsubscribe {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        topics: Vec<String>,
        scope: BusScope,
        scope_id: String,
    },
}

/// Events sent to clients on the "bus" channel.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BusWsEvent {
    /// A bus event delivered to a subscriber.
    Event(BusEvent),
    /// Response to a subscribe/unsubscribe/publish command.
    Response {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

//! WebSocket hub for managing user connections and broadcasting events.

use dashmap::DashMap;
use log::{debug, info, warn};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use super::opencode_adapter::OpenCodeAdapter;
use super::types::{SessionSubscription, WsEvent};

/// Size of the broadcast channel for events.
const EVENT_BUFFER_SIZE: usize = 256;

/// Size of the per-connection send buffer.
const CONNECTION_BUFFER_SIZE: usize = 64;

/// A sender for WebSocket messages to a specific client.
pub type WsSender = mpsc::Sender<WsEvent>;

/// WebSocket hub managing all user connections and session subscriptions.
///
/// The hub is responsible for:
/// - Tracking active WebSocket connections per user
/// - Managing session subscriptions (which sessions a user is watching)
/// - Broadcasting events to subscribed users
/// - Managing OpenCode adapters for SSE connections
pub struct WsHub {
    /// User ID -> list of their WebSocket senders
    connections: DashMap<String, Vec<WsSender>>,

    /// Session ID -> set of subscribed user IDs
    session_subscribers: DashMap<String, HashSet<String>>,

    /// Session ID -> OpenCode adapter
    adapters: DashMap<String, Arc<OpenCodeAdapter>>,

    /// Broadcast channel for hub-wide events
    event_tx: broadcast::Sender<(String, WsEvent)>,
}

impl WsHub {
    /// Create a new WebSocket hub.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_BUFFER_SIZE);
        Self {
            connections: DashMap::new(),
            session_subscribers: DashMap::new(),
            adapters: DashMap::new(),
            event_tx,
        }
    }

    /// Register a new WebSocket connection for a user.
    ///
    /// Returns a receiver for events targeted at this connection and the connection ID.
    pub fn register_connection(&self, user_id: &str) -> (mpsc::Receiver<WsEvent>, usize) {
        let (tx, rx) = mpsc::channel(CONNECTION_BUFFER_SIZE);
        let mut conns = self.connections.entry(user_id.to_string()).or_default();
        let conn_id = conns.len();
        conns.push(tx);
        info!(
            "Registered WebSocket connection {} for user {}",
            conn_id, user_id
        );
        (rx, conn_id)
    }

    /// Unregister a WebSocket connection.
    pub fn unregister_connection(&self, user_id: &str, conn_id: usize) {
        if let Some(mut conns) = self.connections.get_mut(user_id) {
            if conn_id < conns.len() {
                conns.remove(conn_id);
                info!(
                    "Unregistered WebSocket connection {} for user {}",
                    conn_id, user_id
                );
            }
        }

        // Clean up empty entries
        self.connections.retain(|_, v| !v.is_empty());
    }

    /// Subscribe a user to a session's events.
    pub async fn subscribe_session(
        &self,
        user_id: &str,
        subscription: SessionSubscription,
    ) -> anyhow::Result<()> {
        let session_id = subscription.session_id.clone();

        // Add user to session subscribers
        self.session_subscribers
            .entry(session_id.clone())
            .or_default()
            .insert(user_id.to_string());

        // Get or create adapter for this session
        if !self.adapters.contains_key(&session_id) {
            let adapter = OpenCodeAdapter::new(
                session_id.clone(),
                subscription.workspace_path.clone(),
                subscription.opencode_port,
            );

            let adapter = Arc::new(adapter);
            self.adapters.insert(session_id.clone(), adapter.clone());

            // Start the adapter and forward events to hub
            let event_tx = self.event_tx.clone();
            let session_id_clone = session_id.clone();

            tokio::spawn(async move {
                adapter
                    .run(move |event| {
                        let _ = event_tx.send((session_id_clone.clone(), event));
                    })
                    .await;
            });
        }

        info!("User {} subscribed to session {}", user_id, session_id);
        Ok(())
    }

    /// Unsubscribe a user from a session.
    pub fn unsubscribe_session(&self, user_id: &str, session_id: &str) {
        if let Some(mut subscribers) = self.session_subscribers.get_mut(session_id) {
            subscribers.remove(user_id);
            info!("User {} unsubscribed from session {}", user_id, session_id);
        }

        // If no more subscribers, consider cleaning up the adapter
        if let Some(subscribers) = self.session_subscribers.get(session_id) {
            if subscribers.is_empty() {
                // TODO: Implement graceful adapter shutdown after idle timeout
                debug!(
                    "Session {} has no subscribers, adapter will continue running",
                    session_id
                );
            }
        }
    }

    /// Send an event to all connections of a specific user.
    pub async fn send_to_user(&self, user_id: &str, event: WsEvent) {
        if let Some(conns) = self.connections.get(user_id) {
            for (i, tx) in conns.iter().enumerate() {
                if tx.send(event.clone()).await.is_err() {
                    warn!("Failed to send event to user {} connection {}", user_id, i);
                }
            }
        }
    }

    /// Subscribe to the broadcast channel for hub events.
    ///
    /// Returns events as (session_id, event) tuples.
    pub fn subscribe_events(&self) -> broadcast::Receiver<(String, WsEvent)> {
        self.event_tx.subscribe()
    }

    /// Check if a user is subscribed to a session.
    pub fn is_subscribed(&self, user_id: &str, session_id: &str) -> bool {
        self.session_subscribers
            .get(session_id)
            .map(|s| s.contains(user_id))
            .unwrap_or(false)
    }

    /// Get all sessions a user is subscribed to.
    pub fn user_subscriptions(&self, user_id: &str) -> Vec<String> {
        self.session_subscribers
            .iter()
            .filter_map(|entry| {
                if entry.value().contains(user_id) {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all users subscribed to a session.
    pub fn session_subscribers(&self, session_id: &str) -> Vec<String> {
        self.session_subscribers
            .get(session_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Send an event to all users subscribed to a session.
    pub async fn send_to_session(&self, session_id: &str, event: WsEvent) {
        let subscribers = self.session_subscribers(session_id);
        for user_id in subscribers {
            self.send_to_user(&user_id, event.clone()).await;
        }
    }

    /// Send an event to ALL connected users (for testing/broadcast).
    pub async fn broadcast_to_all(&self, event: WsEvent) {
        for entry in self.connections.iter() {
            let user_id = entry.key();
            self.send_to_user(user_id, event.clone()).await;
        }
    }

    /// Get count of connected users (for debugging).
    pub fn connected_user_count(&self) -> usize {
        self.connections.len()
    }
}

impl Default for WsHub {
    fn default() -> Self {
        Self::new()
    }
}

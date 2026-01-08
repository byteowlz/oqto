//! OpenCode SSE adapter with automatic reconnection.
//!
//! This adapter maintains a persistent SSE connection to an OpenCode server
//! and translates events into the unified WsEvent format.

use anyhow::{Context, Result};
use futures::StreamExt;
use log::{debug, error, info, warn};
use reqwest_eventsource::{Event, EventSource};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use super::types::{ConnectionState, WsEvent};

/// Maximum number of reconnection attempts before giving up.
const MAX_RECONNECT_ATTEMPTS: u32 = 50;

/// Base delay for exponential backoff (milliseconds).
const BASE_BACKOFF_MS: u64 = 500;

/// Maximum backoff delay (milliseconds).
const MAX_BACKOFF_MS: u64 = 30_000;

/// Keepalive interval - if no events for this long, reconnect.
const KEEPALIVE_TIMEOUT_SECS: u64 = 60;

/// OpenCode SSE adapter that maintains a connection with auto-reconnect.
pub struct OpenCodeAdapter {
    session_id: String,
    workspace_path: String,
    opencode_port: u16,
    state: Arc<RwLock<ConnectionState>>,
    /// Flag to signal shutdown
    shutdown: Arc<RwLock<bool>>,
}

impl OpenCodeAdapter {
    /// Create a new OpenCode adapter.
    pub fn new(session_id: String, workspace_path: String, opencode_port: u16) -> Self {
        Self {
            session_id,
            workspace_path,
            opencode_port,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            shutdown: Arc::new(RwLock::new(false)),
        }
    }

    /// Get the current connection state.
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Signal the adapter to shut down.
    pub async fn shutdown(&self) {
        *self.shutdown.write().await = true;
    }

    /// Check if shutdown was requested.
    async fn should_shutdown(&self) -> bool {
        *self.shutdown.read().await
    }

    /// Run the adapter, calling the callback for each event.
    ///
    /// This function runs indefinitely, automatically reconnecting on failure.
    pub async fn run<F>(&self, mut on_event: F)
    where
        F: FnMut(WsEvent) + Send + 'static,
    {
        let mut attempt = 0u32;

        loop {
            if self.should_shutdown().await {
                info!(
                    "OpenCode adapter for session {} shutting down",
                    self.session_id
                );
                break;
            }

            // Update state
            if attempt > 0 {
                *self.state.write().await = ConnectionState::Reconnecting;
                let delay = calculate_backoff(attempt);
                on_event(WsEvent::AgentReconnecting {
                    session_id: self.session_id.clone(),
                    attempt,
                    delay_ms: delay,
                });
                tokio::time::sleep(Duration::from_millis(delay)).await;
            } else {
                *self.state.write().await = ConnectionState::Connecting;
            }

            // Attempt connection
            match self.connect_and_stream(&mut on_event).await {
                Ok(()) => {
                    // Clean disconnect, reset attempts
                    attempt = 0;
                    info!(
                        "OpenCode SSE stream for session {} ended cleanly",
                        self.session_id
                    );
                }
                Err(e) => {
                    attempt += 1;
                    warn!(
                        "OpenCode SSE connection failed for session {} (attempt {}): {:?}",
                        self.session_id, attempt, e
                    );

                    on_event(WsEvent::AgentDisconnected {
                        session_id: self.session_id.clone(),
                        reason: e.to_string(),
                    });

                    if attempt >= MAX_RECONNECT_ATTEMPTS {
                        error!(
                            "OpenCode adapter for session {} exceeded max reconnect attempts",
                            self.session_id
                        );
                        *self.state.write().await = ConnectionState::Failed;
                        on_event(WsEvent::Error {
                            message: format!(
                                "Failed to connect after {} attempts",
                                MAX_RECONNECT_ATTEMPTS
                            ),
                            session_id: Some(self.session_id.clone()),
                        });
                        break;
                    }
                }
            }
        }

        *self.state.write().await = ConnectionState::Disconnected;
    }

    /// Connect to OpenCode SSE and stream events.
    async fn connect_and_stream<F>(&self, on_event: &mut F) -> Result<()>
    where
        F: FnMut(WsEvent),
    {
        let url = format!(
            "http://localhost:{}/event?directory={}",
            self.opencode_port,
            urlencoding::encode(&self.workspace_path)
        );

        debug!("Connecting to OpenCode SSE at {}", url);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(KEEPALIVE_TIMEOUT_SECS * 2))
            .build()
            .context("Failed to build HTTP client")?;

        let request_builder = client
            .get(&url)
            .header("Accept", "text/event-stream");

        let mut es = EventSource::new(request_builder)?;

        // Connection established
        *self.state.write().await = ConnectionState::Connected;
        on_event(WsEvent::AgentConnected {
            session_id: self.session_id.clone(),
        });

        info!(
            "Connected to OpenCode SSE for session {}",
            self.session_id
        );

        // Process events
        while let Some(event_result) = es.next().await {
            if self.should_shutdown().await {
                break;
            }

            match event_result {
                Ok(Event::Open) => {
                    debug!("SSE connection opened for session {}", self.session_id);
                }
                Ok(Event::Message(msg)) => {
                    // Parse and translate the event
                    if let Some(ws_event) = self.translate_sse_event(&msg.event, &msg.data) {
                        on_event(ws_event);
                    }
                }
                Err(e) => {
                    // Check if this is a recoverable error
                    if is_recoverable_error(&e) {
                        warn!(
                            "Recoverable SSE error for session {}: {:?}",
                            self.session_id, e
                        );
                        // Will reconnect in the outer loop
                        return Err(anyhow::anyhow!("SSE stream error: {:?}", e));
                    } else {
                        error!(
                            "Fatal SSE error for session {}: {:?}",
                            self.session_id, e
                        );
                        return Err(anyhow::anyhow!("Fatal SSE error: {:?}", e));
                    }
                }
            }
        }

        Ok(())
    }

    /// Translate an OpenCode SSE event into a WsEvent.
    fn translate_sse_event(&self, event_type: &str, data: &str) -> Option<WsEvent> {
        let session_id = self.session_id.clone();

        // Parse the data as JSON
        let parsed: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "Failed to parse SSE data for session {}: {:?}",
                    session_id, e
                );
                // Return raw event for debugging
                return Some(WsEvent::OpencodeEvent {
                    session_id,
                    event_type: event_type.to_string(),
                    data: Value::String(data.to_string()),
                });
            }
        };

        // Translate based on event type
        match event_type {
            "message" | "" => self.translate_message_event(&parsed),
            _ => {
                // Unknown event type, forward as-is
                Some(WsEvent::OpencodeEvent {
                    session_id,
                    event_type: event_type.to_string(),
                    data: parsed,
                })
            }
        }
    }

    /// Translate an OpenCode message event.
    fn translate_message_event(&self, data: &Value) -> Option<WsEvent> {
        let session_id = self.session_id.clone();
        
        // OpenCode events have a "type" field
        let event_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            // Session events
            "session.busy" => Some(WsEvent::SessionBusy { session_id }),
            "session.idle" => Some(WsEvent::SessionIdle { session_id }),
            "session.unavailable" => Some(WsEvent::AgentDisconnected {
                session_id,
                reason: "Session unavailable".to_string(),
            }),

            // Message events
            "message.created" | "message.updated" => {
                let message = data.get("properties").cloned().unwrap_or(data.clone());
                Some(WsEvent::MessageUpdated { session_id, message })
            }

            // Part events (streaming)
            "part.created" | "part.updated" => {
                // Extract message ID and part content
                let message_id = data
                    .get("properties")
                    .and_then(|p| p.get("messageID"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let part = data.get("properties").and_then(|p| p.get("part"));
                
                if let Some(part) = part {
                    let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    
                    match part_type {
                        "text" => {
                            let content = part.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            // For updated parts, send the full content as delta
                            // The frontend should handle deduplication
                            if !content.is_empty() {
                                return Some(WsEvent::TextDelta {
                                    session_id,
                                    message_id,
                                    delta: content.to_string(),
                                });
                            }
                        }
                        "thinking" => {
                            let content = part.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            if !content.is_empty() {
                                return Some(WsEvent::ThinkingDelta {
                                    session_id,
                                    message_id,
                                    delta: content.to_string(),
                                });
                            }
                        }
                        "tool-invocation" => {
                            let tool_call_id = part.get("toolInvocationID")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let tool_name = part.get("toolName")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let state = part.get("state").and_then(|v| v.as_str()).unwrap_or("");
                            let input = part.get("input").cloned();
                            let output = part.get("output").cloned();

                            match state {
                                "pending" | "running" => {
                                    return Some(WsEvent::ToolStart {
                                        session_id,
                                        tool_call_id,
                                        tool_name,
                                        input,
                                    });
                                }
                                "completed" => {
                                    return Some(WsEvent::ToolEnd {
                                        session_id,
                                        tool_call_id,
                                        tool_name,
                                        result: output,
                                        is_error: false,
                                    });
                                }
                                "failed" => {
                                    return Some(WsEvent::ToolEnd {
                                        session_id,
                                        tool_call_id,
                                        tool_name,
                                        result: output,
                                        is_error: true,
                                    });
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                
                // Default: forward as raw event
                Some(WsEvent::OpencodeEvent {
                    session_id,
                    event_type: event_type.to_string(),
                    data: data.clone(),
                })
            }

            // Permission events
            "permission.created" | "permission.updated" => {
                let props = data.get("properties");
                let permission_id = props
                    .and_then(|p| p.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tool_name = props
                    .and_then(|p| p.get("toolName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let description = props
                    .and_then(|p| p.get("input"))
                    .map(|v| format!("{}", v))
                    .unwrap_or_default();
                let input = props.and_then(|p| p.get("input")).cloned();

                Some(WsEvent::PermissionRequest {
                    session_id,
                    permission_id,
                    tool_name,
                    description,
                    input,
                })
            }

            "permission.replied" => {
                let props = data.get("properties");
                let permission_id = props
                    .and_then(|p| p.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let granted = props
                    .and_then(|p| p.get("result"))
                    .and_then(|v| v.as_str())
                    .map(|s| s == "granted")
                    .unwrap_or(false);

                Some(WsEvent::PermissionResolved {
                    session_id,
                    permission_id,
                    granted,
                })
            }

            // Keepalive
            "keepalive" => None, // Don't forward keepalive events

            // Forward unknown events as-is
            _ => Some(WsEvent::OpencodeEvent {
                session_id,
                event_type: event_type.to_string(),
                data: data.clone(),
            }),
        }
    }
}

/// Calculate exponential backoff delay.
fn calculate_backoff(attempt: u32) -> u64 {
    let base = BASE_BACKOFF_MS as f64;
    let exp = 2.0_f64.powi(attempt.min(10) as i32);
    let delay = (base * exp) as u64;
    
    // Add jitter (up to 20%)
    let jitter = (delay as f64 * 0.2 * rand::random::<f64>()) as u64;
    
    (delay + jitter).min(MAX_BACKOFF_MS)
}

/// Check if an SSE error is recoverable.
fn is_recoverable_error(error: &reqwest_eventsource::Error) -> bool {
    // Most errors are recoverable - connection drops, timeouts, etc.
    // Only give up on explicit cancellation or invalid states
    matches!(
        error,
        reqwest_eventsource::Error::StreamEnded
            | reqwest_eventsource::Error::InvalidStatusCode(..)
            | reqwest_eventsource::Error::Transport(_)
    )
}

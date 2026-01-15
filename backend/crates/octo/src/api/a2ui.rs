//! A2UI API handlers
//!
//! Provides HTTP endpoints for agents to send A2UI surfaces to the web frontend
//! and receive user responses.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::{Json, extract::State, http::StatusCode};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;
use tracing::{info, warn};

use super::state::AppState;
use crate::ws::WsEvent;

// ============================================================================
// Types
// ============================================================================

/// Request to display an A2UI surface.
#[derive(Debug, Deserialize)]
pub struct A2uiSurfaceRequest {
    /// Session ID (Octo session, not OpenCode session).
    pub session_id: String,
    /// Unique surface identifier.
    #[serde(default = "generate_surface_id")]
    pub surface_id: String,
    /// A2UI messages defining the surface.
    pub messages: Vec<Value>,
    /// Whether to block until user responds (default: false).
    #[serde(default)]
    pub blocking: bool,
    /// Timeout in seconds for blocking requests (default: 300 = 5 minutes).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn generate_surface_id() -> String {
    format!("surface_{}", uuid::Uuid::new_v4())
}

fn default_timeout() -> u64 {
    300
}

/// Response from A2UI surface request.
#[derive(Debug, Serialize)]
pub struct A2uiSurfaceResponse {
    /// Whether the request was successful.
    pub success: bool,
    /// Surface ID that was created.
    pub surface_id: String,
    /// Request ID (for blocking requests).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// User's action (for blocking requests, after user responds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<A2uiActionResult>,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// User action result returned from blocking request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2uiActionResult {
    /// Name of the action that was triggered.
    pub action_name: String,
    /// Component that triggered the action.
    pub source_component_id: String,
    /// Context data from the action.
    pub context: HashMap<String, Value>,
}

// ============================================================================
// Pending Request Storage
// ============================================================================

/// Storage for pending blocking A2UI requests.
/// Maps request_id -> oneshot sender for the response.
pub type PendingA2uiRequests = Arc<DashMap<String, oneshot::Sender<A2uiActionResult>>>;

/// Create a new pending requests store.
pub fn new_pending_requests() -> PendingA2uiRequests {
    Arc::new(DashMap::new())
}

// ============================================================================
// Handlers
// ============================================================================

/// Send an A2UI surface to the frontend.
///
/// POST /a2ui/surface
///
/// For non-blocking requests, returns immediately after sending to frontend.
/// For blocking requests, waits for user response (up to timeout).
pub async fn send_surface(
    State(state): State<AppState>,
    Json(request): Json<A2uiSurfaceRequest>,
) -> Result<Json<A2uiSurfaceResponse>, (StatusCode, Json<A2uiSurfaceResponse>)> {
    let session_id = request.session_id.clone();
    let surface_id = request.surface_id.clone();
    let blocking = request.blocking;
    let timeout_secs = request.timeout_secs;

    // Generate request ID for blocking requests
    let request_id = if blocking {
        Some(format!("req_{}", uuid::Uuid::new_v4()))
    } else {
        None
    };

    // Create the WebSocket event
    let event = WsEvent::A2uiSurface {
        session_id: session_id.clone(),
        surface_id: surface_id.clone(),
        messages: serde_json::to_value(&request.messages).unwrap_or(Value::Array(vec![])),
        blocking,
        request_id: request_id.clone(),
    };

    // For blocking requests, set up the response channel before sending
    let rx = if blocking {
        let (tx, rx) = oneshot::channel();
        if let Some(ref req_id) = request_id {
            state.pending_a2ui_requests.insert(req_id.clone(), tx);
        }
        Some(rx)
    } else {
        None
    };

    // Broadcast to all connected users (for now)
    // TODO: Send only to users viewing the specific session
    let hub = state.ws_hub.clone();
    let user_count = hub.connected_user_count();
    hub.broadcast_to_all(event).await;

    info!(
        session_id = %session_id,
        surface_id = %surface_id,
        blocking = blocking,
        connected_users = user_count,
        "A2UI surface sent"
    );

    // For non-blocking, return immediately
    if !blocking {
        return Ok(Json(A2uiSurfaceResponse {
            success: true,
            surface_id,
            request_id: None,
            action: None,
            error: None,
        }));
    }

    // For blocking, wait for user response
    let rx = rx.unwrap();
    let req_id = request_id.clone().unwrap();

    match tokio::time::timeout(Duration::from_secs(timeout_secs), rx).await {
        Ok(Ok(action)) => {
            info!(
                request_id = %req_id,
                action_name = %action.action_name,
                "A2UI action received"
            );
            Ok(Json(A2uiSurfaceResponse {
                success: true,
                surface_id,
                request_id,
                action: Some(action),
                error: None,
            }))
        }
        Ok(Err(_)) => {
            // Channel was dropped (shouldn't happen normally)
            warn!(request_id = %req_id, "A2UI request channel dropped");
            state.pending_a2ui_requests.remove(&req_id);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(A2uiSurfaceResponse {
                    success: false,
                    surface_id,
                    request_id,
                    action: None,
                    error: Some("Request was cancelled".to_string()),
                }),
            ))
        }
        Err(_) => {
            // Timeout
            warn!(request_id = %req_id, timeout_secs, "A2UI request timed out");
            state.pending_a2ui_requests.remove(&req_id);
            Err((
                StatusCode::REQUEST_TIMEOUT,
                Json(A2uiSurfaceResponse {
                    success: false,
                    surface_id,
                    request_id,
                    action: None,
                    error: Some(format!("Request timed out after {} seconds", timeout_secs)),
                }),
            ))
        }
    }
}

/// Delete/dismiss an A2UI surface.
///
/// DELETE /a2ui/surface/{surface_id}
pub async fn delete_surface(
    State(state): State<AppState>,
    axum::extract::Path((session_id, surface_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<A2uiSurfaceResponse>, (StatusCode, String)> {
    // Send delete event
    let event = WsEvent::A2uiSurface {
        session_id: session_id.clone(),
        surface_id: surface_id.clone(),
        messages: serde_json::json!([{"deleteSurface": {"surfaceId": surface_id}}]),
        blocking: false,
        request_id: None,
    };

    state.ws_hub.broadcast_to_all(event).await;

    Ok(Json(A2uiSurfaceResponse {
        success: true,
        surface_id,
        request_id: None,
        action: None,
        error: None,
    }))
}

/// Handle incoming A2UI action from frontend (called by WS handler).
pub fn handle_a2ui_action(
    pending_requests: &PendingA2uiRequests,
    request_id: &str,
    action_name: String,
    source_component_id: String,
    context: HashMap<String, Value>,
) -> bool {
    if let Some((_, tx)) = pending_requests.remove(request_id) {
        let result = A2uiActionResult {
            action_name,
            source_component_id,
            context,
        };
        if tx.send(result).is_ok() {
            return true;
        }
    }
    false
}

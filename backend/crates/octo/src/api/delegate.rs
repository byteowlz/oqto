//! Pi delegation API endpoints.
//!
//! These endpoints allow Pi to delegate tasks to other sessions.
//! All endpoints are localhost-only for security.

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing::{info, warn};

use crate::session::SessionStatus;

use super::error::{ApiError, ApiResult};
use super::state::AppState;

// ============================================================================
// Request/Response types
// ============================================================================

/// Request to start a new delegated session.
#[derive(Debug, Deserialize)]
pub struct StartDelegateRequest {
    /// Project directory for the session.
    pub directory: String,
    /// Initial prompt/task to send to the session.
    pub prompt: String,
    /// Optional agent name to use.
    pub agent: Option<String>,
}

/// Request to send a prompt to an existing session.
#[derive(Debug, Deserialize)]
pub struct PromptRequest {
    /// The prompt to send.
    pub prompt: String,
}

/// Query parameters for messages endpoint.
#[derive(Debug, Deserialize)]
pub struct MessagesQuery {
    /// Maximum number of messages to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

/// Response for delegation operations.
#[derive(Debug, Serialize)]
pub struct DelegateResponse {
    pub session_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Session info for delegation.
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub status: String,
    pub workspace_path: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Message from a session.
#[derive(Debug, Serialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

// ============================================================================
// Localhost-only guard
// ============================================================================

fn require_localhost(addr: &SocketAddr) -> ApiResult<()> {
    let ip = addr.ip();
    if ip.is_loopback() {
        Ok(())
    } else {
        warn!("Delegation API request rejected from non-localhost: {}", ip);
        Err(ApiError::forbidden(
            "Delegation API is only accessible from localhost",
        ))
    }
}

fn get_delegate_user_id(_state: &AppState) -> String {
    "default".to_string()
}

// ============================================================================
// Handlers
// ============================================================================

/// Start a new delegated session.
///
/// POST /api/delegate/start
pub async fn start_session(
    State(_state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(_request): Json<StartDelegateRequest>,
) -> ApiResult<(StatusCode, Json<DelegateResponse>)> {
    require_localhost(&addr)?;
    Err(ApiError::internal(
        "Delegation via REST is deprecated. Use the multiplexed WebSocket instead.",
    ))
}

/// Send a prompt to an existing session.
///
/// POST /api/delegate/prompt/{session_id}
pub async fn send_prompt(
    State(_state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(_session_id): Path<String>,
    Json(_request): Json<PromptRequest>,
) -> ApiResult<Json<DelegateResponse>> {
    require_localhost(&addr)?;
    Err(ApiError::internal(
        "Delegation via REST is deprecated. Use the multiplexed WebSocket instead.",
    ))
}

/// Get session status.
///
/// GET /api/delegate/status/{session_id}
pub async fn get_status(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionInfo>> {
    require_localhost(&addr)?;

    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get session: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Session {} not found", session_id)))?;

    Ok(Json(SessionInfo {
        id: session.id,
        status: session.status.to_string(),
        workspace_path: session.workspace_path,
        created_at: session.created_at,
        started_at: session.started_at,
        last_activity_at: session.last_activity_at,
        error_message: session.error_message,
        source: None,
    }))
}

/// Get recent messages from a session.
///
/// GET /api/delegate/messages/{session_id}
pub async fn get_messages(
    State(_state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(_session_id): Path<String>,
    Query(_query): Query<MessagesQuery>,
) -> ApiResult<Json<Vec<SessionMessage>>> {
    require_localhost(&addr)?;
    // Messages are now accessed through hstry
    Ok(Json(vec![]))
}

/// Stop a session.
///
/// POST /api/delegate/stop/{session_id}
pub async fn stop_session(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<DelegateResponse>> {
    require_localhost(&addr)?;

    let user_id = get_delegate_user_id(&state);

    info!(
        user_id = %user_id,
        session_id = %session_id,
        "Stopping delegated session"
    );

    state
        .sessions
        .stop_session(&session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to stop session: {}", e)))?;

    Ok(Json(DelegateResponse {
        session_id,
        status: "stopped".to_string(),
        message: Some("Session stopped".to_string()),
    }))
}

/// List all sessions for the user.
///
/// GET /api/delegate/sessions
pub async fn list_sessions(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> ApiResult<Json<Vec<SessionInfo>>> {
    require_localhost(&addr)?;

    let user_id = get_delegate_user_id(&state);

    let all_sessions = state
        .sessions
        .list_sessions()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list sessions: {}", e)))?;

    let sessions: Vec<SessionInfo> = all_sessions
        .into_iter()
        .filter(|s| s.user_id == user_id)
        .filter(|s| {
            matches!(
                s.status,
                SessionStatus::Running | SessionStatus::Starting | SessionStatus::Pending
            )
        })
        .map(|s| SessionInfo {
            id: s.id,
            status: s.status.to_string(),
            workspace_path: s.workspace_path,
            created_at: s.created_at,
            started_at: s.started_at,
            last_activity_at: s.last_activity_at,
            error_message: s.error_message,
            source: None,
        })
        .collect();

    Ok(Json(sessions))
}

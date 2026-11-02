//! Pi delegation API endpoints.
//!
//! These endpoints allow Pi to delegate tasks to other sessions. They require a
//! normal authenticated principal and resolve sessions through the per-user
//! session service; they are mounted on the authenticated router.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::auth::CurrentUser;
use crate::session::SessionStatus;

use super::error::{ApiError, ApiResult};
use super::state::AppState;

// ============================================================================
// Request/Response types
// ============================================================================

/// Request to start a new delegated session.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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
#[allow(dead_code)]
pub struct PromptRequest {
    /// The prompt to send.
    pub prompt: String,
}

/// Query parameters for messages endpoint.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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
// Session ownership
// ============================================================================

/// Resolve a session for the calling principal.
///
/// Ownership is enforced by the per-user session service (`for_user`), not by
/// the transport peer: behind a loopback reverse proxy every remote client looks
/// like 127.0.0.1, so a peer address is not authorization.
fn session_for_user<'a>(
    state: &'a AppState,
    user: &'a CurrentUser,
) -> crate::session::UserSessionService<'a> {
    state.sessions.for_user(user.id())
}

// ============================================================================
// Handlers
// ============================================================================

/// Start a new delegated session.
///
/// POST /api/delegate/start
pub async fn start_session(
    State(_state): State<AppState>,
    _user: CurrentUser,
    Json(_request): Json<StartDelegateRequest>,
) -> ApiResult<(StatusCode, Json<DelegateResponse>)> {
    Err(ApiError::internal(
        "Delegation via REST is deprecated. Use the multiplexed WebSocket instead.",
    ))
}

/// Send a prompt to an existing session.
///
/// POST /api/delegate/prompt/{session_id}
pub async fn send_prompt(
    State(_state): State<AppState>,
    _user: CurrentUser,
    Path(_session_id): Path<String>,
    Json(_request): Json<PromptRequest>,
) -> ApiResult<Json<DelegateResponse>> {
    Err(ApiError::internal(
        "Delegation via REST is deprecated. Use the multiplexed WebSocket instead.",
    ))
}

/// Get session status.
///
/// GET /api/delegate/status/{session_id}
pub async fn get_status(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionInfo>> {
    let session = session_for_user(&state, &user)
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
    _user: CurrentUser,
    Path(_session_id): Path<String>,
    Query(_query): Query<MessagesQuery>,
) -> ApiResult<Json<Vec<SessionMessage>>> {
    // Messages are now accessed through oqto-log
    Ok(Json(vec![]))
}

/// Stop a session.
///
/// POST /api/delegate/stop/{session_id}
pub async fn stop_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<DelegateResponse>> {
    info!(
        user_id = %user.id(),
        session_id = %session_id,
        "Stopping delegated session"
    );

    session_for_user(&state, &user)
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
    user: CurrentUser,
) -> ApiResult<Json<Vec<SessionInfo>>> {
    let all_sessions = session_for_user(&state, &user)
        .list_sessions()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list sessions: {}", e)))?;

    let sessions: Vec<SessionInfo> = all_sessions
        .into_iter()
        .filter(|s| s.user_id == user.id())
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

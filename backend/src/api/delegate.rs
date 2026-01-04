//! Pi delegation API endpoints.
//!
//! These endpoints allow Pi (running as Main Chat) to delegate tasks to OpenCode sessions.
//! All endpoints are localhost-only for security - they should only be called by Pi running
//! on the same machine.
//!
//! The delegation API provides:
//! - Start a new session with an initial prompt
//! - Send follow-up prompts to existing sessions
//! - Check session status
//! - Get recent messages from a session
//! - Stop a session
//! - List all sessions for the user

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{info, warn};

use crate::agent_rpc::{MessagePart, SendMessagePart, SendMessageRequest, StartSessionOpts};
use crate::session::SessionStatus;

use super::error::{ApiError, ApiResult};
use super::state::AppState;

/// Extract text content from message parts.
fn extract_text_from_parts(parts: &[MessagePart]) -> String {
    parts
        .iter()
        .filter_map(|part| match part {
            MessagePart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

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
    /// Session ID.
    pub session_id: String,
    /// Current status.
    pub status: String,
    /// Optional message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Session info for delegation.
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    /// Session ID.
    pub id: String,
    /// Session status.
    pub status: String,
    /// Workspace path.
    pub workspace_path: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Start timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// Last activity timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Source of the session (e.g., "pi_delegate").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Message from a session.
#[derive(Debug, Serialize)]
pub struct SessionMessage {
    /// Role (user or assistant).
    pub role: String,
    /// Message content.
    pub content: String,
    /// Timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

// ============================================================================
// Localhost-only guard
// ============================================================================

/// Check if the request is from localhost.
/// Returns an error if not from localhost.
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

/// Get the user ID for delegation requests.
/// In single-user mode, returns the default user.
/// In multi-user mode, would need to be passed via header or determined from Pi session.
fn get_delegate_user_id(_state: &AppState) -> String {
    // For now, use the default single-user ID
    // TODO: In multi-user mode, extract from X-Octo-User-Id header or Pi session context
    // The Pi session is user-specific, so we could track which user owns it
    "default".to_string()
}

// ============================================================================
// Handlers
// ============================================================================

/// Start a new delegated session.
///
/// POST /api/delegate/start
pub async fn start_session(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<StartDelegateRequest>,
) -> ApiResult<(StatusCode, Json<DelegateResponse>)> {
    require_localhost(&addr)?;

    let user_id = get_delegate_user_id(&state);
    let workdir = PathBuf::from(&request.directory);

    info!(
        user_id = %user_id,
        directory = %request.directory,
        agent = ?request.agent,
        "Pi delegating new session"
    );

    // Check if we have the AgentRPC backend
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    // Start the session
    let opts = StartSessionOpts {
        model: None,
        agent: request.agent,
        resume_session_id: None,
        project_id: None,
        env: std::collections::HashMap::new(),
    };

    let handle = backend
        .start_session(&user_id, &workdir, opts)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start session: {}", e)))?;

    info!(
        session_id = %handle.session_id,
        "Delegated session started, sending initial prompt"
    );

    // Send the initial prompt
    let message = SendMessageRequest {
        parts: vec![SendMessagePart::Text {
            text: request.prompt.clone(),
        }],
        model: None,
    };

    backend
        .send_message(&user_id, &handle.session_id, message)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to send initial prompt: {}", e)))?;

    // TODO: Mark this session as delegated from Pi (update session source field)

    Ok((
        StatusCode::CREATED,
        Json(DelegateResponse {
            session_id: handle.session_id,
            status: "running".to_string(),
            message: Some("Session started with initial prompt".to_string()),
        }),
    ))
}

/// Send a prompt to an existing session.
///
/// POST /api/delegate/prompt/{session_id}
pub async fn send_prompt(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(session_id): Path<String>,
    Json(request): Json<PromptRequest>,
) -> ApiResult<Json<DelegateResponse>> {
    require_localhost(&addr)?;

    let user_id = get_delegate_user_id(&state);

    info!(
        user_id = %user_id,
        session_id = %session_id,
        "Pi sending follow-up prompt"
    );

    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let message = SendMessageRequest {
        parts: vec![SendMessagePart::Text {
            text: request.prompt,
        }],
        model: None,
    };

    backend
        .send_message(&user_id, &session_id, message)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to send prompt: {}", e)))?;

    Ok(Json(DelegateResponse {
        session_id,
        status: "running".to_string(),
        message: Some("Prompt sent".to_string()),
    }))
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

    // First try to get from session service
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
        source: None, // TODO: Add source field to session model
    }))
}

/// Get recent messages from a session.
///
/// GET /api/delegate/messages/{session_id}
pub async fn get_messages(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(session_id): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> ApiResult<Json<Vec<SessionMessage>>> {
    require_localhost(&addr)?;

    let user_id = get_delegate_user_id(&state);

    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    // Get messages through the agent backend
    // Note: session_id in AgentRPC is actually the conversation_id/opencode session ID
    let messages = backend
        .get_messages(&user_id, &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

    // Convert to our response format and limit
    let messages: Vec<SessionMessage> = messages
        .into_iter()
        .rev() // Most recent first
        .take(query.limit)
        .map(|m| SessionMessage {
            role: m.role,
            content: extract_text_from_parts(&m.parts),
            timestamp: Some(m.created_at.to_string()),
        })
        .collect();

    Ok(Json(messages))
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
        "Pi stopping session"
    );

    // Try AgentRPC first
    if let Some(backend) = state.agent_backend.as_ref() {
        backend
            .stop_session(&user_id, &session_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to stop session: {}", e)))?;
    } else {
        // Fall back to session service
        state
            .sessions
            .stop_session(&session_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to stop session: {}", e)))?;
    }

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

    // Filter to user's sessions (in single-user mode, all sessions belong to the default user)
    let sessions: Vec<_> = all_sessions
        .into_iter()
        .filter(|s| s.user_id == user_id)
        .collect();

    let sessions: Vec<SessionInfo> = sessions
        .into_iter()
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

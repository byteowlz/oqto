//! AgentRPC handlers (unified backend API).

use std::convert::Infallible;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

use crate::agent_rpc::{
    self, Conversation as RpcConversation, HealthStatus as RpcHealthStatus, Message as RpcMessage,
    SendMessagePart, SessionHandle,
};
use crate::auth::CurrentUser;

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

// Re-export ask-related types and handlers from agent_ask module
pub use super::agent_ask::{agents_ask, agents_search_sessions};

/// Request to start a new agent session.
#[derive(Debug, Deserialize)]
pub struct StartAgentSessionRequest {
    /// Working directory for the session
    pub workdir: String,
    /// Model to use (optional)
    pub model: Option<String>,
    /// Agent/mode to use (optional, passed to opencode via --agent flag)
    pub agent: Option<String>,
    /// Session ID to resume (optional)
    pub resume_session_id: Option<String>,
    /// Project ID for shared project sessions (optional)
    pub project_id: Option<String>,
}

/// Request to send a message to an agent session.
#[derive(Debug, Deserialize)]
pub struct SendAgentMessageRequest {
    /// Message text
    pub text: Option<String>,
    /// Structured message parts
    pub parts: Option<Vec<SendMessagePart>>,
    /// Optional file part
    pub file: Option<SendAgentFilePart>,
    /// Optional agent mention part
    pub agent: Option<SendAgentAgentPart>,
    /// Model override (optional)
    pub model: Option<agent_rpc::MessageModel>,
}

/// File part for agent messages.
#[derive(Debug, Deserialize)]
pub struct SendAgentFilePart {
    pub mime: String,
    pub url: String,
    pub filename: Option<String>,
}

/// Agent part for agent messages.
#[derive(Debug, Deserialize)]
pub struct SendAgentAgentPart {
    pub name: String,
    pub id: Option<String>,
}

/// List conversations via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_list_conversations(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<RpcConversation>>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let conversations = backend
        .list_conversations(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list conversations: {}", e)))?;

    info!(user_id = %user.id(), count = conversations.len(), "Listed agent conversations");
    Ok(Json(conversations))
}

/// Get a specific conversation via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_get_conversation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(conversation_id): Path<String>,
) -> ApiResult<Json<RpcConversation>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let conversation = backend
        .get_conversation(user.id(), &conversation_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get conversation: {}", e)))?
        .ok_or_else(|| {
            ApiError::not_found(format!("Conversation {} not found", conversation_id))
        })?;

    Ok(Json(conversation))
}

/// Get messages for a conversation via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_get_messages(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(conversation_id): Path<String>,
) -> ApiResult<Json<Vec<RpcMessage>>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let messages = backend
        .get_messages(user.id(), &conversation_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

    info!(user_id = %user.id(), conversation_id = %conversation_id, count = messages.len(), "Listed agent messages");
    Ok(Json(messages))
}

/// Start a new agent session via AgentBackend.
#[instrument(skip(state, user, request))]
pub async fn agent_start_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<StartAgentSessionRequest>,
) -> ApiResult<(StatusCode, Json<SessionHandle>)> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let opts = agent_rpc::StartSessionOpts {
        model: request.model,
        agent: request.agent,
        resume_session_id: request.resume_session_id,
        project_id: request.project_id,
        env: std::collections::HashMap::new(),
    };

    let workdir = std::path::Path::new(&request.workdir);
    let handle = backend
        .start_session(user.id(), workdir, opts)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start session: {}", e)))?;

    info!(user_id = %user.id(), session_id = %handle.session_id, "Started agent session");
    Ok((StatusCode::CREATED, Json(handle)))
}

/// Send a message to an agent session via AgentBackend.
#[instrument(skip(state, user, request))]
pub async fn agent_send_message(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Json(request): Json<SendAgentMessageRequest>,
) -> ApiResult<StatusCode> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let mut parts = request.parts.unwrap_or_default();
    if let Some(file) = request.file {
        parts.push(SendMessagePart::File {
            mime: file.mime,
            url: file.url,
            filename: file.filename,
        });
    }
    if let Some(agent) = request.agent {
        parts.push(SendMessagePart::Agent {
            name: agent.name,
            id: agent.id,
        });
    }
    if let Some(text) = request.text {
        parts.push(SendMessagePart::Text { text });
    }
    if parts.is_empty() {
        return Err(ApiError::bad_request("message must include text or parts"));
    }

    let send_request = agent_rpc::SendMessageRequest {
        parts,
        model: request.model,
    };

    backend
        .send_message(user.id(), &session_id, send_request)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to send message: {}", e)))?;

    info!(user_id = %user.id(), session_id = %session_id, "Sent message to agent session");
    Ok(StatusCode::ACCEPTED)
}

/// Stop an agent session via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_stop_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    backend
        .stop_session(user.id(), &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to stop session: {}", e)))?;

    info!(user_id = %user.id(), session_id = %session_id, "Stopped agent session");
    Ok(StatusCode::NO_CONTENT)
}

/// Get the session URL for an agent session.
#[instrument(skip(state, user))]
pub async fn agent_get_session_url(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionUrlResponse>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let url = backend
        .get_session_url(user.id(), &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get session URL: {}", e)))?;

    Ok(Json(SessionUrlResponse { session_id, url }))
}

/// Response for session URL query.
#[derive(Debug, Serialize)]
pub struct SessionUrlResponse {
    pub session_id: String,
    pub url: Option<String>,
}

/// Health check for the AgentRPC backend.
#[instrument(skip(state))]
pub async fn agent_health(State(state): State<AppState>) -> ApiResult<Json<RpcHealthStatus>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let health = backend
        .health()
        .await
        .map_err(|e| ApiError::internal(format!("Health check failed: {}", e)))?;

    Ok(Json(health))
}

/// Attach to a session's event stream via AgentBackend.
///
/// Returns an SSE stream of agent events (messages, tool calls, etc.).
#[instrument(skip(state, user))]
pub async fn agent_attach(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let event_stream = backend
        .attach(user.id(), &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to attach to session: {}", e)))?;

    // Convert AgentEvent stream to SSE Event stream
    let sse_stream = tokio_stream::StreamExt::map(event_stream, |result| {
        match result {
            Ok(event) => {
                // Serialize the event to JSON
                match serde_json::to_string(&event) {
                    Ok(json) => Ok(Event::default().data(json)),
                    Err(e) => {
                        warn!("Failed to serialize agent event: {}", e);
                        Ok(Event::default().data(format!(r#"{{"error":"{}"}}"#, e)))
                    }
                }
            }
            Err(e) => {
                warn!("Error in agent event stream: {}", e);
                Ok(Event::default().data(format!(r#"{{"error":"{}"}}"#, e)))
            }
        }
    });

    info!(user_id = %user.id(), session_id = %session_id, "Attached to agent session event stream");
    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

// ============================================================================
// In-Session Search Handler
// ============================================================================

/// Query parameters for in-session search.
#[derive(Debug, Deserialize)]
pub struct InSessionSearchQuery {
    /// Search query
    pub q: String,
    /// Maximum number of results
    #[serde(default = "default_in_session_search_limit")]
    pub limit: usize,
}

fn default_in_session_search_limit() -> usize {
    20
}

/// Search result from session search.
#[derive(Debug, Serialize)]
pub struct InSessionSearchResult {
    /// Line number in the source file
    pub line_number: usize,
    /// Match score
    pub score: f64,
    /// Short snippet around the match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Session title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Match type (exact, fuzzy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_type: Option<String>,
    /// Timestamp when the message was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    /// Message ID for direct navigation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

/// Search within a specific Pi session using hstry.
///
/// GET /api/agents/sessions/{session_id}/search?q=query&limit=20
///
/// Note: Legacy MainChatPiService search removed. Returns empty results.
/// In-session search should go through hstry directly.
#[instrument(skip(_state, _user))]
pub async fn agents_session_search(
    State(_state): State<AppState>,
    _user: CurrentUser,
    Path(_session_id): Path<String>,
    Query(_query): Query<InSessionSearchQuery>,
) -> ApiResult<Json<Vec<InSessionSearchResult>>> {
    // Legacy MainChatPiService session search removed.
    // In-session search now goes through hstry.
    Ok(Json(vec![]))
}

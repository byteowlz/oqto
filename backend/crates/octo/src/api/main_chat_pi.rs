//! Pi agent API handlers for Main Chat.
//!
//! Provides WebSocket streaming and REST endpoints for interacting with
//! the Pi agent runtime for Main Chat.

use axum::{
    Json,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::Response,
};
use futures::{SinkExt, StreamExt};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::auth::CurrentUser;
use crate::main_chat::{
    ChatMessage, CreateChatMessage, MainChatPiService, MainChatService, MessageRole, PiSessionFile,
    PiSessionMessage,
};
use crate::pi::{AgentMessage, AssistantMessageEvent, CompactionResult, PiEvent, PiState};

use super::state::MmryState;

use super::error::{ApiError, ApiResult};
use super::state::AppState;

#[derive(Debug, serde::Serialize)]
struct MmryAgentMemoryCreateRequest {
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    importance: Option<i32>,
}

// ========== Request/Response Types ==========

/// Request to send a prompt to Pi.
#[derive(Debug, Deserialize)]
pub struct PromptRequest {
    /// The message to send.
    pub message: String,
}

/// Request to compact the session.
#[derive(Debug, Deserialize)]
pub struct CompactRequest {
    /// Optional custom instructions for compaction.
    pub custom_instructions: Option<String>,
}

/// Request to set the current model.
#[derive(Debug, Deserialize)]
pub struct SetModelRequest {
    pub provider: String,
    #[serde(rename = "model_id")]
    pub model_id: String,
}

/// Response for Pi state.
#[derive(Debug, Serialize)]
pub struct PiStateResponse {
    pub model: Option<PiModelInfo>,
    pub thinking_level: String,
    pub is_streaming: bool,
    pub is_compacting: bool,
    pub session_id: Option<String>,
    pub message_count: u64,
    pub auto_compaction_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct PiModelInfo {
    pub id: String,
    pub provider: String,
    pub name: String,
    #[serde(rename = "context_window")]
    pub context_window: u64,
    #[serde(rename = "max_tokens")]
    pub max_tokens: u64,
}

/// Response for session stats.
#[derive(Debug, Serialize)]
pub struct PiSessionStatsResponse {
    pub session_id: Option<String>,
    pub user_messages: u64,
    pub assistant_messages: u64,
    pub tool_calls: u64,
    pub total_messages: u64,
    pub tokens: PiSessionTokensResponse,
    pub cost: f64,
}

#[derive(Debug, Serialize)]
pub struct PiSessionTokensResponse {
    pub input: u64,
    pub output: u64,
    #[serde(rename = "cache_read")]
    pub cache_read: u64,
    #[serde(rename = "cache_write")]
    pub cache_write: u64,
    pub total: u64,
}

/// Response for available models.
#[derive(Debug, Serialize)]
pub struct PiModelsResponse {
    pub models: Vec<PiModelInfo>,
}

/// Prompt command info for Pi (slash command templates).
#[derive(Debug, Serialize)]
pub struct PiPromptCommandInfo {
    pub name: String,
    pub description: String,
}

/// Response for prompt commands.
#[derive(Debug, Serialize)]
pub struct PiPromptCommandsResponse {
    pub commands: Vec<PiPromptCommandInfo>,
}

// ========== Handlers ==========

/// Check if Pi session is ready.
///
/// GET /api/main/pi/status
pub async fn get_pi_status(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Value>> {
    let pi_service = get_pi_service(&state)?;
    let main_chat_service = get_main_chat_service(&state)?;

    // Check if Main Chat exists
    if !main_chat_service.main_chat_exists(user.id()) {
        return Ok(Json(serde_json::json!({
            "exists": false,
            "session_active": false
        })));
    }

    let session_active = pi_service.has_session(user.id()).await;

    Ok(Json(serde_json::json!({
        "exists": true,
        "session_active": session_active
    })))
}

/// Start or get the Pi session.
///
/// POST /api/main/pi/session
pub async fn start_pi_session(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;
    let main_chat_service = get_main_chat_service(&state)?;

    // Ensure Main Chat exists
    if !main_chat_service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found(
            "Main Chat not found. Initialize it first.",
        ));
    }

    // Get or create session
    let session = pi_service
        .get_or_create_session(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start Pi session: {}", e)))?;

    // Get current state
    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {}", e)))?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Get Pi session state.
///
/// GET /api/main/pi/state
pub async fn get_pi_state(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .get_session(user.id())
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;

    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {}", e)))?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Send a prompt to Pi.
///
/// POST /api/main/pi/prompt
pub async fn send_prompt(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<PromptRequest>,
) -> ApiResult<StatusCode> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .get_session(user.id())
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;

    session
        .prompt(&req.message)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to send prompt: {}", e)))?;

    Ok(StatusCode::ACCEPTED)
}

/// Abort current Pi operation.
///
/// POST /api/main/pi/abort
pub async fn abort_pi(State(state): State<AppState>, user: CurrentUser) -> ApiResult<StatusCode> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .get_session(user.id())
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;

    session
        .abort()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to abort: {}", e)))?;

    Ok(StatusCode::OK)
}

/// Get messages from Pi session.
///
/// GET /api/main/pi/messages
pub async fn get_messages(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<AgentMessage>>> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .get_session(user.id())
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;

    let messages = session
        .get_messages()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

    Ok(Json(messages))
}

/// Compact Pi session context.
///
/// POST /api/main/pi/compact
pub async fn compact_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<CompactRequest>,
) -> ApiResult<Json<CompactionResult>> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .get_session(user.id())
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;

    let result = session
        .compact(req.custom_instructions.as_deref())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to compact: {}", e)))?;

    Ok(Json(result))
}

/// Set the current model.
///
/// POST /api/main/pi/model
pub async fn set_model(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<SetModelRequest>,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .get_session(user.id())
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;

    session
        .set_model(&req.provider, &req.model_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set model: {}", e)))?;

    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {}", e)))?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Get available models for this session.
///
/// GET /api/main/pi/models
pub async fn get_available_models(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<PiModelsResponse>> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .get_session(user.id())
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;

    let models = session
        .get_available_models()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get models: {}", e)))?;

    let mapped = models
        .into_iter()
        .map(|model| PiModelInfo {
            id: model.id,
            provider: model.provider,
            name: model.name,
            context_window: model.context_window,
            max_tokens: model.max_tokens,
        })
        .collect();

    Ok(Json(PiModelsResponse { models: mapped }))
}

/// Get available prompt commands (slash templates) for Pi.
///
/// GET /api/main/pi/commands
pub async fn get_prompt_commands(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<PiPromptCommandsResponse>> {
    let service = get_main_chat_service(&state)?;

    if !service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found("Main Chat not found"));
    }

    let main_chat_dir = service.get_main_chat_dir(user.id());
    let mut commands = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push_from_dir = |dir: std::path::PathBuf| {
        if !dir.exists() {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !(ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("txt")) {
                    continue;
                }
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if name.is_empty() || seen.contains(&name) {
                    continue;
                }
                let description = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|content| {
                        content
                            .lines()
                            .map(|line| line.trim())
                            .find(|line| !line.is_empty())
                            .map(|line| line.trim_start_matches('#').trim().to_string())
                    })
                    .unwrap_or_else(|| "Custom prompt".to_string());
                seen.insert(name.clone());
                commands.push(PiPromptCommandInfo { name, description });
            }
        }
    };

    let local_pi_dir = main_chat_dir.join(".pi");
    push_from_dir(local_pi_dir.join("prompts"));
    push_from_dir(local_pi_dir.join("commands"));

    if let Some(home) = dirs::home_dir() {
        let global_pi_dir = home.join(".pi").join("agent");
        push_from_dir(global_pi_dir.join("prompts"));
        push_from_dir(global_pi_dir.join("commands"));
    }

    Ok(Json(PiPromptCommandsResponse { commands }))
}

/// Start a new Pi session (clear history).
///
/// POST /api/main/pi/new
pub async fn new_session(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;

    // Reset the Pi process so that context injection runs for the new session.
    // This aligns "new chat" semantics with the session-boundary architecture.
    let session = pi_service
        .reset_session(user.id(), false)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to reset session: {}", e)))?;

    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {}", e)))?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Reset Pi session - closes and recreates the session.
/// This re-reads PERSONALITY.md and USER.md files.
///
/// POST /api/main/pi/reset
pub async fn reset_session(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .reset_session(user.id(), false)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to reset session: {}", e)))?;

    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {}", e)))?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Get session statistics.
///
/// GET /api/main/pi/stats
pub async fn get_session_stats(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<PiSessionStatsResponse>> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .get_session(user.id())
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;

    let stats = session
        .get_session_stats()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get stats: {}", e)))?;

    Ok(Json(PiSessionStatsResponse {
        session_id: stats.session_id,
        user_messages: stats.user_messages,
        assistant_messages: stats.assistant_messages,
        tool_calls: stats.tool_calls,
        total_messages: stats.total_messages,
        tokens: PiSessionTokensResponse {
            input: stats.tokens.input,
            output: stats.tokens.output,
            cache_read: stats.tokens.cache_read,
            cache_write: stats.tokens.cache_write,
            total: stats.tokens.total,
        },
        cost: stats.cost,
    }))
}

/// Close Pi session.
///
/// DELETE /api/main/pi/session
pub async fn close_session(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<StatusCode> {
    let pi_service = get_pi_service(&state)?;

    pi_service
        .close_all_sessions(user.id(), false)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to close session: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Query params for history endpoint.
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Optional session ID to filter messages.
    pub session_id: Option<String>,
}

/// Query params for search endpoint.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Search query string.
    pub q: String,
    /// Maximum number of results.
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    50
}

/// Search result hit.
#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub agent: String,
    pub source_path: String,
    pub session_id: String,
    pub message_id: Option<String>,
    pub line_number: usize,
    pub snippet: Option<String>,
    pub score: f64,
    pub timestamp: Option<i64>,
    pub role: Option<String>,
    pub title: Option<String>,
}

/// Search response.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub total: usize,
}

/// Get chat history from database (persistent display history).
/// If session_id is provided, returns only messages for that session.
///
/// GET /api/main/pi/history
/// GET /api/main/pi/history?session_id=<id>
pub async fn get_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<Vec<ChatMessage>>> {
    let main_chat_service = get_main_chat_service(&state)?;

    let messages = if let Some(session_id) = query.session_id {
        main_chat_service
            .get_messages_by_session(user.id(), &session_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get session history: {}", e)))?
    } else {
        main_chat_service
            .get_all_messages(user.id())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get history: {}", e)))?
    };

    Ok(Json(messages))
}

/// Clear chat history (for fresh start).
///
/// DELETE /api/main/pi/history
pub async fn clear_history(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Value>> {
    let main_chat_service = get_main_chat_service(&state)?;

    let deleted = main_chat_service
        .clear_messages(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to clear history: {}", e)))?;

    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

/// Add a session separator to history (legacy; used by older frontend).
///
/// POST /api/main/pi/history/separator
pub async fn add_separator(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<ChatMessage>> {
    let main_chat_service = get_main_chat_service(&state)?;

    let content = serde_json::json!([{
        "type": "separator",
        "text": "New conversation started"
    }]);

    let message = main_chat_service
        .add_message(
            user.id(),
            CreateChatMessage {
                role: MessageRole::System,
                content,
                pi_session_id: None,
            },
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to add separator: {}", e)))?;

    Ok(Json(message))
}

/// List Pi sessions for Main Chat from disk.
///
/// GET /api/main/pi/sessions
pub async fn list_pi_sessions(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<PiSessionFile>>> {
    let pi_service = get_pi_service(&state)?;

    let sessions = pi_service
        .list_sessions(user.id())
        .map_err(|e| ApiError::internal(format!("Failed to list Pi sessions: {}", e)))?;

    Ok(Json(sessions))
}

/// Search Main Chat Pi sessions for message content.
/// This provides a direct search fallback when cass doesn't index Pi sessions.
///
/// GET /api/main/pi/sessions/search?q=query&limit=50
pub async fn search_pi_sessions(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<SearchResponse>> {
    let pi_service = get_pi_service(&state)?;

    let query_str = query.q.trim();
    if query_str.is_empty() {
        return Ok(Json(SearchResponse {
            hits: vec![],
            total: 0,
        }));
    }

    let sessions = pi_service
        .list_sessions(user.id())
        .map_err(|e| ApiError::internal(format!("Failed to list sessions: {}", e)))?;

    let query_lower = query_str.to_lowercase();
    let mut all_hits = Vec::new();

    for session in sessions {
        // Load session messages
        let messages = match pi_service.get_session_messages(user.id(), &session.id) {
            Ok(msgs) => msgs,
            Err(_) => continue,
        };

        // Search through messages
        for (line_idx, msg) in messages.iter().enumerate() {
            // Extract text content from the message
            let text_content = extract_message_text(&msg.content);

            if text_content.to_lowercase().contains(&query_lower) {
                let snippet = create_snippet(&text_content, &query_lower, 100);

                all_hits.push(SearchHit {
                    agent: "pi_agent".to_string(),
                    source_path: format!("pi:{}:{}", session.id, msg.id),
                    session_id: session.id.clone(),
                    message_id: Some(msg.id.clone()),
                    line_number: line_idx + 1,
                    snippet: Some(snippet),
                    score: 1.0,
                    timestamp: Some(msg.timestamp),
                    role: Some(msg.role.to_string()),
                    title: session.title.clone(),
                });

                // Stop if we have enough hits for this session
                if all_hits.len() >= query.limit {
                    break;
                }
            }
        }

        if all_hits.len() >= query.limit {
            break;
        }
    }

    let total = all_hits.len();
    Ok(Json(SearchResponse {
        hits: all_hits,
        total,
    }))
}

/// Extract text content from a Pi message content field.
fn extract_message_text(content: &serde_json::Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|part| {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    Some(text.to_string())
                } else if let Some(content) = part.get("content").and_then(|c| c.as_str()) {
                    Some(content.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        Value::Object(obj) => {
            if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                text.to_string()
            } else if let Some(content) = obj.get("content").and_then(|c| c.as_str()) {
                content.to_string()
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

/// Create a snippet around the first match of query in text.
fn create_snippet(text: &str, query: &str, context_chars: usize) -> String {
    let text_lower = text.to_lowercase();
    if let Some(pos) = text_lower.find(query) {
        let start = pos.saturating_sub(context_chars);
        let end = (pos + query.len() + context_chars).min(text.len());

        // Find word boundaries
        let snippet_start = text[..start].rfind(' ').map(|p| p + 1).unwrap_or(start);
        let snippet_end = text[end..].find(' ').map(|p| end + p).unwrap_or(end);

        let mut snippet = String::new();
        if snippet_start > 0 {
            snippet.push_str("...");
        }
        snippet.push_str(&text[snippet_start..snippet_end]);
        if snippet_end < text.len() {
            snippet.push_str("...");
        }
        snippet
    } else {
        text.chars().take(200).collect()
    }
}

/// Start a fresh Pi session and return its state.
///
/// POST /api/main/pi/sessions
pub async fn new_pi_session(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .reset_session(user.id(), false)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start new session: {}", e)))?;

    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {}", e)))?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Load a specific Pi session's messages from disk.
///
/// GET /api/main/pi/sessions/{session_id}
pub async fn get_pi_session_messages(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> ApiResult<Json<Vec<PiSessionMessage>>> {
    let pi_service = get_pi_service(&state)?;

    let messages = match pi_service.get_session_messages(user.id(), &session_id) {
        Ok(messages) => messages,
        Err(err) => {
            if pi_service.is_active_session(user.id(), &session_id).await
                && err.to_string().contains("Session not found")
            {
                Vec::new()
            } else {
                return Err(ApiError::internal(format!(
                    "Failed to load Pi session: {}",
                    err
                )));
            }
        }
    };

    Ok(Json(messages))
}

/// Resume a specific Pi session (switch active session).
///
/// POST /api/main/pi/sessions/{session_id}
pub async fn resume_pi_session(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;

    let session = pi_service
        .resume_session(user.id(), &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resume Pi session: {}", e)))?;

    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {}", e)))?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Request body for updating a Pi session.
#[derive(Debug, Deserialize)]
pub struct UpdatePiSessionRequest {
    /// New title for the session
    pub title: Option<String>,
}

/// Update a Pi session's metadata (e.g., title).
///
/// PATCH /api/main/pi/sessions/{session_id}
pub async fn update_pi_session(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(request): Json<UpdatePiSessionRequest>,
) -> ApiResult<Json<PiSessionFile>> {
    let pi_service = get_pi_service(&state)?;

    if let Some(title) = request.title {
        let session = pi_service
            .update_session_title(user.id(), &session_id, &title)
            .map_err(|e| ApiError::internal(format!("Failed to update Pi session: {}", e)))?;

        info!("Updated Pi session title: session_id={}, title={}", session_id, title);
        Ok(Json(session))
    } else {
        // No updates requested, return current session info
        let sessions = pi_service
            .list_sessions(user.id())
            .map_err(|e| ApiError::internal(format!("Failed to list sessions: {}", e)))?;

        let session = sessions
            .into_iter()
            .find(|s| s.id == session_id)
            .ok_or_else(|| ApiError::not_found(format!("Session not found: {}", session_id)))?;

        Ok(Json(session))
    }
}

/// WebSocket endpoint for streaming Pi events.
///
/// GET /api/main/pi/ws
pub async fn ws_handler(
    State(state): State<AppState>,
    user: CurrentUser,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    info!("Pi WebSocket connection request from user {}", user.id());

    let pi_service = get_pi_service(&state)?;
    let main_chat_service = get_main_chat_service(&state)?;

    // Ensure Main Chat exists
    if !main_chat_service.main_chat_exists(user.id()) {
        warn!("Main Chat not found for user {}", user.id());
        return Err(ApiError::not_found("Main Chat not found"));
    }

    // Get or create session
    let session = pi_service
        .get_or_create_session(user.id())
        .await
        .map_err(|e| {
            warn!("Failed to get Pi session for user {}: {}", user.id(), e);
            ApiError::internal(format!("Failed to get Pi session: {}", e))
        })?;

    let user_id = user.id().to_string();
    let main_chat_svc = state.main_chat.clone();
    let mmry_state = state.mmry.clone();
    info!("Upgrading to WebSocket for user {}", user_id);

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, session, user_id, main_chat_svc, mmry_state)))
}

/// Handle WebSocket connection for Pi events.
async fn handle_ws(
    socket: WebSocket,
    session: Arc<crate::main_chat::UserPiSession>,
    user_id: String,
    main_chat_svc: Option<Arc<MainChatService>>,
    mmry_state: super::state::MmryState,
) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to Pi events
    let mut event_rx = session.subscribe().await;

    // Snapshot Pi session id for traceability (best-effort).
    let pi_session_id = session.get_state().await.ok().and_then(|s| s.session_id);

    // Only one WS connection should persist assistant output for a session.
    let persistence_guard = session.claim_persistence_writer();
    let can_persist = persistence_guard.is_some();

    // Send connected message
    let connected_msg = serde_json::json!({"type": "connected"});
    if sender
        .send(Message::Text(connected_msg.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    // Replay any in-progress assistant message to avoid WS gaps
    let snapshot_events = session.stream_snapshot_events().await;
    for event in snapshot_events {
        if sender
            .send(Message::Text(event.to_string().into()))
            .await
            .is_err()
        {
            return;
        }
    }

    // Message accumulator for saving assistant messages
    let message_accumulator = Arc::new(tokio::sync::Mutex::new(MessageAccumulator::new()));
    let accumulator_for_events = Arc::clone(&message_accumulator);
    let main_chat_for_events = main_chat_svc.clone();
    let user_id_for_events = user_id.clone();
    let pi_session_id_for_events = pi_session_id.clone();
    let mmry_state_for_events = mmry_state.clone();

    // Persist Pi auto-compaction summaries to main_chat.db so they can be injected
    // even when the OpenCode-side plugin is not active.
    let history_for_events = main_chat_svc.clone();

    // Spawn task to forward Pi events to WebSocket
    // Transform raw Pi events into simplified format for frontend
    let send_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            // Accumulate message content for saving (only from the primary WS connection).
            if can_persist {
                let mut acc = accumulator_for_events.lock().await;
                acc.process_event(&event);

                // When agent completes, save the assistant message
                if matches!(event, PiEvent::AgentEnd { .. }) {
                    if let Some(svc) = &main_chat_for_events {
                        if let Some(content) = acc.take_message() {
                            let persisted = svc
                                .add_message(
                                    &user_id_for_events,
                                    CreateChatMessage {
                                        role: MessageRole::Assistant,
                                        content: content.clone(),
                                        pi_session_id: pi_session_id_for_events.clone(),
                                    },
                                )
                                .await;

                            match persisted {
                                Ok(saved) => {
                                    // Best-effort: index a compact turn chunk into mmry.
                                    // This uses the existing mmry proxy surface so Octo doesn't need a dedicated client.
                                    if let Err(e) = index_turn_to_mmry(
                                        &mmry_state_for_events,
                                        &user_id_for_events,
                                        &pi_session_id_for_events,
                                        &saved,
                                    )
                                    .await
                                    {
                                        warn!("Failed to index turn into mmry: {}", e);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to save assistant message: {}", e);
                                }
                            }
                        }
                    }
                }

                // Persist auto-compaction output for continuity
                if let PiEvent::AutoCompactionEnd {
                    result: Some(result),
                    aborted: false,
                    ..
                } = &event
                {
                    if let Some(svc) = &history_for_events {
                        if let Err(e) = svc
                            .add_history(
                                &user_id_for_events,
                                crate::main_chat::CreateHistoryEntry {
                                    entry_type: crate::main_chat::HistoryEntryType::Summary,
                                    content: result.summary.clone(),
                                    session_id: None,
                                    meta: Some(serde_json::json!({
                                        "source": "pi_auto_compaction",
                                        "first_kept_entry_id": result.first_kept_entry_id,
                                        "tokens_before": result.tokens_before,
                                        "details": result.details,
                                    })),
                                },
                            )
                            .await
                        {
                            warn!("Failed to persist compaction summary: {}", e);
                        }
                    }
                }
            }

            // Transform Pi events into frontend-friendly format
            let ws_event = transform_pi_event_for_ws(&event);
            if ws_event.is_none() {
                continue; // Skip events we don't need to forward
            }

            let json = match serde_json::to_string(&ws_event) {
                Ok(j) => j,
                Err(e) => {
                    warn!("Failed to serialize Pi event: {}", e);
                    continue;
                }
            };

            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming WebSocket messages (commands from client)
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Parse as JSON command
                match serde_json::from_str::<WsCommand>(&text) {
                    Ok(cmd) => {
                        // Save user message before sending to Pi
                        if let WsCommand::Prompt { ref message }
                        | WsCommand::Steer { ref message }
                        | WsCommand::FollowUp { ref message } = cmd
                        {
                            if let Some(svc) = &main_chat_svc {
                                let content =
                                    serde_json::json!([{"type": "text", "text": message}]);
                                if let Err(e) = svc
                                    .add_message(
                                        &user_id,
                                        CreateChatMessage {
                                            role: MessageRole::User,
                                            content,
                                            pi_session_id: pi_session_id.clone(),
                                        },
                                    )
                                    .await
                                {
                                    warn!("Failed to save user message: {}", e);
                                }
                            }
                        }

                        if let Err(e) = handle_ws_command(&session, cmd).await {
                            warn!("Failed to handle WS command: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse WS command: {}", e);
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                warn!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    send_task.abort();
    info!("WebSocket closed for user {}", user_id);
}

/// Commands that can be sent over WebSocket.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsCommand {
    Prompt { message: String },
    Steer { message: String },
    FollowUp { message: String },
    Abort,
    NewSession,
    Compact { custom_instructions: Option<String> },
}

async fn handle_ws_command(
    session: &crate::main_chat::UserPiSession,
    cmd: WsCommand,
) -> anyhow::Result<()> {
    match cmd {
        WsCommand::Prompt { message } => {
            session.prompt(&message).await?;
        }
        WsCommand::Steer { message } => {
            session.steer(&message).await?;
        }
        WsCommand::FollowUp { message } => {
            session.follow_up(&message).await?;
        }
        WsCommand::Abort => {
            session.abort().await?;
        }
        WsCommand::NewSession => {
            session.new_session().await?;
        }
        WsCommand::Compact {
            custom_instructions,
        } => {
            session.compact(custom_instructions.as_deref()).await?;
        }
    }
    Ok(())
}

// ========== Message Accumulator ==========

/// Accumulates message content during streaming for persistence.
struct MessageAccumulator {
    text: String,
    thinking: String,
    tool_calls: Vec<Value>,
    tool_results: Vec<Value>,
}

impl MessageAccumulator {
    fn new() -> Self {
        Self {
            text: String::new(),
            thinking: String::new(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
        }
    }

    fn process_event(&mut self, event: &PiEvent) {
        match event {
            PiEvent::MessageUpdate {
                assistant_message_event,
                ..
            } => match assistant_message_event {
                AssistantMessageEvent::TextDelta { delta, .. } => {
                    self.text.push_str(delta);
                }
                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                    self.thinking.push_str(delta);
                }
                AssistantMessageEvent::ToolcallEnd { tool_call, .. } => {
                    self.tool_calls.push(serde_json::json!({
                        "type": "tool_use",
                        "id": tool_call.id,
                        "name": tool_call.name,
                        "input": tool_call.arguments
                    }));
                }
                _ => {}
            },
            PiEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                ..
            } => {
                self.tool_results.push(serde_json::json!({
                    "type": "tool_result",
                    "id": tool_call_id,
                    "name": tool_name,
                    "content": result
                }));
            }
            _ => {}
        }
    }

    /// Take the accumulated message content as JSON and reset the accumulator.
    fn take_message(&mut self) -> Option<Value> {
        let mut parts = Vec::new();

        // Add thinking first if present
        if !self.thinking.is_empty() {
            parts.push(serde_json::json!({
                "type": "thinking",
                "text": std::mem::take(&mut self.thinking)
            }));
        }

        // Add text
        if !self.text.is_empty() {
            parts.push(serde_json::json!({
                "type": "text",
                "text": std::mem::take(&mut self.text)
            }));
        }

        // Add tool calls
        for tc in self.tool_calls.drain(..) {
            parts.push(tc);
        }

        // Add tool results
        for tr in self.tool_results.drain(..) {
            parts.push(tr);
        }

        if parts.is_empty() {
            None
        } else {
            Some(Value::Array(parts))
        }
    }
}

// ========== Helper Functions ==========

fn get_pi_service(state: &AppState) -> ApiResult<&MainChatPiService> {
    state
        .main_chat_pi
        .as_ref()
        .map(|arc| arc.as_ref())
        .ok_or_else(|| ApiError::internal("Pi service not configured"))
}

fn get_main_chat_service(state: &AppState) -> ApiResult<&MainChatService> {
    state
        .main_chat
        .as_ref()
        .map(|arc| arc.as_ref())
        .ok_or_else(|| ApiError::internal("Main Chat service not configured"))
}

async fn index_turn_to_mmry(
    mmry: &MmryState,
    user_id: &str,
    pi_session_id: &Option<String>,
    assistant_message: &ChatMessage,
) -> Result<(), String> {
    if !mmry.enabled {
        return Ok(());
    }

    let pi_session_id = match pi_session_id.as_deref() {
        Some(id) if !id.trim().is_empty() => id,
        _ => return Ok(()),
    };

    // Best-effort extraction: store assistant text for now.
    // We can extend this to include the paired user message later.
    let content = format!(
        "source: octo_main_chat\nuser_id: {user_id}\npi_session_id: {pi_session_id}\n\n{body}",
        body = assistant_message.content
    );

    let tags = vec![
        "source:octo".to_string(),
        "domain:main_chat".to_string(),
        "kind:assistant_chunk".to_string(),
        format!("pi_session_id:{pi_session_id}"),
    ];

    let req = MmryAgentMemoryCreateRequest {
        content,
        category: Some("octo_main_chat".to_string()),
        tags: Some(tags),
        importance: Some(4),
    };

    let client = reqwest::Client::new();

    let (base_url, store) = if mmry.single_user {
        // Single-user: write to local mmry service. Keep store default.
        (mmry.local_service_url.as_str(), None)
    } else {
        // Multi-user: write to host mmry service using a per-user store.
        (
            mmry.host_service_url.as_str(),
            Some(format!("octo-user-{user_id}")),
        )
    };

    let url = format!("{}/v1/agents/memories", base_url.trim_end_matches('/'));

    let mut req_builder = client.post(url).json(&req);

    if let Some(store) = store.as_deref() {
        req_builder = req_builder.query(&[("store", store)]);
    }

    if let Some(key) = mmry.host_api_key.as_deref() {
        if !key.trim().is_empty() {
            req_builder = req_builder.bearer_auth(key);
        }
    }

    let resp = req_builder
        .send()
        .await
        .map_err(|e| format!("mmry request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("mmry returned {status}: {body}"));
    }

    Ok(())
}

fn pi_state_to_response(state: PiState) -> PiStateResponse {
    PiStateResponse {
        model: state.model.map(|m| PiModelInfo {
            id: m.id,
            provider: m.provider,
            name: m.name,
            context_window: m.context_window,
            max_tokens: m.max_tokens,
        }),
        thinking_level: state.thinking_level,
        is_streaming: state.is_streaming,
        is_compacting: state.is_compacting,
        session_id: state.session_id,
        message_count: state.message_count,
        auto_compaction_enabled: state.auto_compaction_enabled,
    }
}

/// Transform a raw Pi event into a simplified WebSocket event for the frontend.
/// Returns None for events that don't need to be forwarded.
fn transform_pi_event_for_ws(event: &PiEvent) -> Option<Value> {
    match event {
        PiEvent::AgentStart => Some(serde_json::json!({"type": "agent_start"})),
        PiEvent::AgentEnd { .. } => Some(serde_json::json!({"type": "done"})),
        PiEvent::TurnStart => None,      // Don't forward
        PiEvent::TurnEnd { .. } => None, // Don't forward
        PiEvent::MessageStart { message } => {
            if message.role == "assistant" {
                Some(serde_json::json!({"type": "message_start", "role": "assistant"}))
            } else {
                None
            }
        }
        PiEvent::MessageUpdate {
            assistant_message_event,
            ..
        } => {
            // Transform streaming updates into simpler events
            match assistant_message_event {
                AssistantMessageEvent::TextDelta { delta, .. } => Some(serde_json::json!({
                    "type": "text",
                    "data": delta
                })),
                AssistantMessageEvent::ThinkingDelta { delta, .. } => Some(serde_json::json!({
                    "type": "thinking",
                    "data": delta
                })),
                AssistantMessageEvent::ToolcallEnd { tool_call, .. } => Some(serde_json::json!({
                    "type": "tool_use",
                    "data": {
                        "id": tool_call.id,
                        "name": tool_call.name,
                        "input": tool_call.arguments
                    }
                })),
                _ => None, // Skip other message updates
            }
        }
        PiEvent::MessageEnd { .. } => None, // Will be handled by AgentEnd
        PiEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => Some(serde_json::json!({
            "type": "tool_start",
            "data": {
                "id": tool_call_id,
                "name": tool_name,
                "input": args
            }
        })),
        PiEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
            ..
        } => Some(serde_json::json!({
            "type": "tool_result",
            "data": {
                "id": tool_call_id,
                "name": tool_name,
                "content": result
            }
        })),
        PiEvent::AutoCompactionStart { .. } => {
            Some(serde_json::json!({"type": "compaction_start"}))
        }
        PiEvent::AutoCompactionEnd { .. } => Some(serde_json::json!({"type": "compaction"})),
        _ => None, // Skip other events
    }
}

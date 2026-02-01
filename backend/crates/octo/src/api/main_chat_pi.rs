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
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::future::Future;
use std::sync::Arc;

use chrono::{TimeZone, Utc};

use crate::auth::CurrentUser;
use crate::main_chat::{
    MainChatPiService, MainChatService, PiSessionFile, PiSessionMessage, UserPiSession,
};
use crate::pi::{AgentMessage, AssistantMessageEvent, CompactionResult, PiEvent, PiState};

use super::error::{ApiError, ApiResult};
use super::state::AppState;

fn is_runner_writer_error(err: &str) -> bool {
    err.contains("runner pi writer") || err.contains("response channel closed")
}

async fn with_main_chat_session_retry<T, F, Fut>(
    pi_service: &MainChatPiService,
    user_id: &str,
    session_id: &str,
    op: F,
) -> ApiResult<T>
where
    F: Fn(Arc<UserPiSession>) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let session = pi_service
        .resume_session(user_id, session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resume Pi session: {e}")))?;

    match op(Arc::clone(&session)).await {
        Ok(value) => Ok(value),
        Err(err) if is_runner_writer_error(&err.to_string()) => {
            let _ = pi_service.close_session(user_id, session_id, true).await;
            let session = pi_service
                .resume_session(user_id, session_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to restart Pi session: {e}")))?;
            op(session)
                .await
                .map_err(|e| ApiError::internal(format!("Pi session error after restart: {e}")))
        }
        Err(err) => Err(ApiError::internal(format!("Pi session error: {err}"))),
    }
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
    Query(query): Query<MainChatSessionQuery>,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;

    let pi_state = with_main_chat_session_retry(
        pi_service,
        user.id(),
        &query.session_id,
        |session: Arc<UserPiSession>| async move { session.get_state().await },
    )
    .await?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Send a prompt to Pi.
///
/// POST /api/main/pi/prompt
pub async fn send_prompt(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<MainChatSessionQuery>,
    Json(req): Json<PromptRequest>,
) -> ApiResult<StatusCode> {
    let pi_service = get_pi_service(&state)?;

    let message = req.message.clone();
    with_main_chat_session_retry(
        pi_service,
        user.id(),
        &query.session_id,
        |session: Arc<UserPiSession>| {
            let message = message.clone();
            async move { session.prompt(&message).await }
        },
    )
    .await?;

    Ok(StatusCode::ACCEPTED)
}

/// Abort current Pi operation.
///
/// POST /api/main/pi/abort
pub async fn abort_pi(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<MainChatSessionQuery>,
) -> ApiResult<StatusCode> {
    let pi_service = get_pi_service(&state)?;

    with_main_chat_session_retry(
        pi_service,
        user.id(),
        &query.session_id,
        |session: Arc<UserPiSession>| async move { session.abort().await },
    )
    .await?;

    Ok(StatusCode::OK)
}

/// Get messages from Pi session.
///
/// GET /api/main/pi/messages
pub async fn get_messages(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<MainChatSessionQuery>,
) -> ApiResult<Json<Vec<AgentMessage>>> {
    let pi_service = get_pi_service(&state)?;

    let messages = with_main_chat_session_retry(
        pi_service,
        user.id(),
        &query.session_id,
        |session: Arc<UserPiSession>| async move { session.get_messages().await },
    )
    .await?;

    Ok(Json(messages))
}

/// Compact Pi session context.
///
/// POST /api/main/pi/compact
pub async fn compact_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<MainChatSessionQuery>,
    Json(req): Json<CompactRequest>,
) -> ApiResult<Json<CompactionResult>> {
    let pi_service = get_pi_service(&state)?;

    let instructions = req.custom_instructions.clone();
    let result = with_main_chat_session_retry(
        pi_service,
        user.id(),
        &query.session_id,
        |session: Arc<UserPiSession>| {
            let instructions = instructions.clone();
            async move { session.compact(instructions.as_deref()).await }
        },
    )
    .await?;

    Ok(Json(result))
}

/// Set the current model.
///
/// POST /api/main/pi/model
pub async fn set_model(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<MainChatSessionQuery>,
    Json(req): Json<SetModelRequest>,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;

    let provider = req.provider.clone();
    let model_id = req.model_id.clone();
    let pi_state = with_main_chat_session_retry(
        pi_service,
        user.id(),
        &query.session_id,
        |session: Arc<UserPiSession>| {
            let provider = provider.clone();
            let model_id = model_id.clone();
            async move {
                session.set_model(&provider, &model_id).await?;
                session.get_state().await
            }
        },
    )
    .await?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Get available models for this session.
///
/// GET /api/main/pi/models
pub async fn get_available_models(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<MainChatSessionQuery>,
) -> ApiResult<Json<PiModelsResponse>> {
    let pi_service = get_pi_service(&state)?;

    let models = with_main_chat_session_retry(
        pi_service,
        user.id(),
        &query.session_id,
        |session: Arc<UserPiSession>| async move { session.get_available_models().await },
    )
    .await?;

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
    Query(_query): Query<MainChatSessionQuery>,
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

    let user_home = state
        .linux_users
        .as_ref()
        .and_then(|cfg| cfg.get_home_dir(user.id()).ok().flatten())
        .or_else(dirs::home_dir);
    if let Some(home) = user_home {
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
    Query(query): Query<MainChatSessionQuery>,
) -> ApiResult<Json<PiStateResponse>> {
    let pi_service = get_pi_service(&state)?;

    // Restart just this session's process (keep session history, reload USER.md/PERSONALITY.md).
    let _ = pi_service
        .close_session(user.id(), &query.session_id, false)
        .await;
    let pi_state = with_main_chat_session_retry(
        pi_service,
        user.id(),
        &query.session_id,
        |session: Arc<UserPiSession>| async move { session.get_state().await },
    )
    .await?;

    Ok(Json(pi_state_to_response(pi_state)))
}

/// Get session statistics.
///
/// GET /api/main/pi/stats
pub async fn get_session_stats(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<MainChatSessionQuery>,
) -> ApiResult<Json<PiSessionStatsResponse>> {
    let pi_service = get_pi_service(&state)?;

    let stats = with_main_chat_session_retry(
        pi_service,
        user.id(),
        &query.session_id,
        |session: Arc<UserPiSession>| async move { session.get_session_stats().await },
    )
    .await?;

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

/// Chat message returned by `GET /api/main/pi/history`.
///
/// Matches the historical `main_chat.db` message shape for frontend compatibility.
#[derive(Debug, Serialize)]
pub struct MainChatHistoryMessage {
    pub id: i64,
    pub role: String,
    /// JSON array serialized as string.
    pub content: String,
    pub pi_session_id: Option<String>,
    pub timestamp: i64,
    pub created_at: String,
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
) -> ApiResult<Json<Vec<MainChatHistoryMessage>>> {
    let multi_user = state.linux_users.is_some();

    // In multi-user mode, always use octo-runner to access per-user hstry.db.
    if multi_user {
        let Some(runner) = crate::api::handlers::get_runner_for_user(&state, user.id()) else {
            return Err(ApiError::internal(
                "Chat history service not configured for this user.",
            ));
        };

        let session_id = if let Some(session_id) = query.session_id.as_deref() {
            session_id.to_string()
        } else {
            let sessions = runner
                .list_main_chat_sessions()
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Runner list_main_chat_sessions failed: {e}"))
                })?
                .sessions;
            let Some(latest) = sessions.first() else {
                return Ok(Json(Vec::new()));
            };
            latest.id.clone()
        };

        let resp = runner
            .get_main_chat_messages(&session_id, None)
            .await
            .map_err(|e| {
                ApiError::internal(format!("Runner get_main_chat_messages failed: {e}"))
            })?;

        let mut out = Vec::with_capacity(resp.messages.len());
        for (idx, msg) in resp.messages.into_iter().enumerate() {
            let role = match msg.role.as_str() {
                "user" => "user",
                "assistant" => "assistant",
                "system" => "system",
                "tool" | "toolResult" => "assistant",
                _ => "assistant",
            }
            .to_string();

            let parts = if msg.content.is_array() {
                msg.content
            } else if let Some(s) = msg.content.as_str() {
                serde_json::json!([{ "type": "text", "text": s }])
            } else {
                serde_json::json!([])
            };

            let created_at = Utc
                .timestamp_millis_opt(msg.timestamp)
                .single()
                .unwrap_or_else(Utc::now)
                .to_rfc3339();

            out.push(MainChatHistoryMessage {
                id: (idx as i64) + 1,
                role,
                content: parts.to_string(),
                pi_session_id: Some(session_id.clone()),
                timestamp: msg.timestamp,
                created_at,
            });
        }

        return Ok(Json(out));
    }

    // Single-user: use hstry read service.
    let Some(hstry) = &state.hstry else {
        return Ok(Json(Vec::new()));
    };

    let session_id = if let Some(session_id) = query.session_id.as_deref() {
        session_id.to_string()
    } else {
        let summaries = hstry
            .list_conversations(None, Some(1), None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to list hstry conversations: {e}")))?;
        let Some(summary) = summaries.first() else {
            return Ok(Json(Vec::new()));
        };
        let conv = summary.conversation.as_ref();
        let Some(conv) = conv else {
            return Ok(Json(Vec::new()));
        };
        if !conv.external_id.is_empty() {
            conv.external_id.clone()
        } else {
            return Ok(Json(Vec::new()));
        }
    };

    let messages = hstry
        .get_messages(&session_id, None, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load hstry messages: {e}")))?;

    let now_ms = Utc::now().timestamp_millis();
    let mut out: Vec<MainChatHistoryMessage> = Vec::with_capacity(messages.len());

    for (idx, msg) in messages.into_iter().enumerate() {
        let role = match msg.role.as_str() {
            "user" => "user",
            "assistant" => "assistant",
            "system" => "system",
            "tool" | "toolResult" => "assistant",
            _ => "assistant",
        }
        .to_string();

        let parts_value = if !msg.parts_json.is_empty()
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg.parts_json)
            && v.is_array()
        {
            v
        } else if !msg.content.trim().is_empty() {
            serde_json::json!([{ "type": "text", "text": msg.content }])
        } else {
            serde_json::json!([])
        };

        let timestamp_ms = msg.created_at_ms.unwrap_or(now_ms);
        let created_at_str = Utc
            .timestamp_millis_opt(timestamp_ms)
            .single()
            .unwrap_or_else(Utc::now)
            .to_rfc3339();

        out.push(MainChatHistoryMessage {
            id: (idx as i64) + 1,
            role,
            content: parts_value.to_string(),
            pi_session_id: Some(session_id.clone()),
            timestamp: timestamp_ms,
            created_at: created_at_str,
        });
    }

    Ok(Json(out))
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
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list Pi sessions: {}", e)))?;

    Ok(Json(sessions))
}

/// Search Main Chat Pi sessions for message content.
///
/// GET /api/main/pi/sessions/search?q=query&limit=50
pub async fn search_pi_sessions(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<SearchResponse>> {
    if state.linux_users.is_some() {
        // TODO: Add runner-backed hstry search for multi-user mode.
        // For now, avoid leaking backend user's hstry.db.
        let _ = user;
        return Ok(Json(SearchResponse {
            hits: vec![],
            total: 0,
        }));
    }

    let query_str = query.q.trim();
    if query_str.is_empty() {
        return Ok(Json(SearchResponse {
            hits: vec![],
            total: 0,
        }));
    }

    let mut all_hits = Vec::new();

    let hits = crate::history::search_hstry(query_str, query.limit)
        .await
        .map_err(|e| ApiError::internal(format!("hstry search failed: {e}")))?;

    for hit in hits {
        if hit.source_id != "pi" {
            continue;
        }

        let timestamp = hit
            .created_at
            .or(hit.conv_updated_at)
            .map(|dt| dt.timestamp_millis())
            .or_else(|| Some(hit.conv_created_at.timestamp_millis()));

        let session_id = hit
            .external_id
            .clone()
            .unwrap_or_else(|| hit.conversation_id.clone());

        let source_path = hit
            .source_path
            .clone()
            .unwrap_or_else(|| format!("hstry:pi:{}", hit.conversation_id));

        all_hits.push(SearchHit {
            agent: "pi_agent".to_string(),
            source_path,
            session_id,
            message_id: None,
            line_number: (hit.message_idx.max(0) as usize) + 1,
            snippet: Some(hit.snippet.clone()),
            score: f64::from(hit.score),
            timestamp,
            role: Some(hit.role.clone()),
            title: hit.title.clone(),
        });

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

// Search uses hstry index; no local content helpers needed.

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

    let messages = match pi_service
        .get_session_messages(user.id(), &session_id)
        .await
    {
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

    let pi_state = with_main_chat_session_retry(
        pi_service,
        user.id(),
        &session_id,
        |session: Arc<UserPiSession>| async move { session.get_state().await },
    )
    .await?;

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
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update Pi session: {}", e)))?;

        info!(
            "Updated Pi session title: session_id={}, title={}",
            session_id, title
        );
        Ok(Json(session))
    } else {
        // No updates requested, return current session info
        let sessions = pi_service
            .list_sessions(user.id())
            .await
            .map_err(|e| ApiError::internal(format!("Failed to list sessions: {}", e)))?;

        let session = sessions
            .into_iter()
            .find(|s| s.id == session_id)
            .ok_or_else(|| ApiError::not_found(format!("Session not found: {}", session_id)))?;

        Ok(Json(session))
    }
}

/// Delete a Pi session (soft delete).
///
/// DELETE /api/main/pi/sessions/{session_id}
pub async fn delete_pi_session(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> ApiResult<StatusCode> {
    let pi_service = get_pi_service(&state)?;

    let _ = pi_service.close_session(user.id(), &session_id, true).await;
    match pi_service.delete_session_file(user.id(), &session_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(err) if err.to_string().contains("Session not found") => {
            Err(ApiError::not_found(format!(
                "Session not found: {}",
                session_id
            )))
        }
        Err(err) => Err(ApiError::internal(format!(
            "Failed to delete Pi session: {}",
            err
        ))),
    }
}

/// WebSocket endpoint for streaming Pi events.
///
/// GET /api/main/pi/ws?session_id=...
pub async fn ws_handler(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<MainChatWsQuery>,
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

    let session_id = query
        .session_id
        .ok_or_else(|| ApiError::bad_request("session_id is required"))?;

    // Resume specific session and bind WS to it.
    let session = with_main_chat_session_retry(
        pi_service,
        user.id(),
        &session_id,
        |session: Arc<UserPiSession>| async move {
            session.get_state().await?;
            Ok(session)
        },
    )
    .await
    .map_err(|e| {
        warn!("Failed to resume Pi session for user {}: {}", user.id(), e);
        e
    })?;

    let user_id = user.id().to_string();
    let main_chat_svc = state.main_chat.clone();
    let hstry_client = state.hstry.clone();
    let pi_service_for_ws = state
        .main_chat_pi
        .clone()
        .ok_or_else(|| ApiError::internal("Main Chat Pi service not initialized"))?;
    info!("Upgrading to WebSocket for user {}", user_id);

    Ok(ws.on_upgrade(move |socket| {
        handle_ws(
            socket,
            session,
            user_id,
            main_chat_svc,
            Some(pi_service_for_ws),
            hstry_client,
        )
    }))
}

#[derive(Debug, Deserialize)]
pub struct MainChatWsQuery {
    pub session_id: Option<String>,
}

/// Query params for main chat Pi endpoints that operate on a specific session.
#[derive(Debug, Deserialize)]
pub struct MainChatSessionQuery {
    pub session_id: String,
}

/// Handle WebSocket connection for Pi events.
pub(crate) async fn handle_ws(
    socket: WebSocket,
    session: Arc<crate::main_chat::UserPiSession>,
    user_id: String,
    main_chat_svc: Option<Arc<MainChatService>>,
    pi_service: Option<Arc<MainChatPiService>>,
    hstry_client: Option<crate::hstry::HstryClient>,
) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to Pi events
    let mut event_rx = session.subscribe().await;

    // Only one WS connection should persist assistant output for a session.
    let persistence_guard = session.claim_persistence_writer();
    let can_persist = persistence_guard.is_some();

    // Get current session_id for the connected message
    let initial_session_id = session.get_session_id().await;

    // Send connected message with session_id
    let connected_msg = serde_json::json!({
        "type": "connected",
        "session_id": initial_session_id
    });
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

    let user_id_for_events = user_id.clone();
    let session_for_events = Arc::clone(&session);
    let pi_service_for_events = pi_service.clone();
    let hstry_for_events = hstry_client.clone();

    // Persist Pi auto-compaction summaries to main_chat.db so they can be injected
    // even when the OpenCode-side plugin is not active.
    let history_for_events = main_chat_svc.clone();

    // Spawn task to forward Pi events to WebSocket
    // Transform raw Pi events into simplified format for frontend
    let send_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            // Get current session_id dynamically (not from a stale snapshot)
            // This ensures messages are saved to the correct session even after session switches
            let current_session_id = session_for_events.get_session_id().await;

            // Handle extension UI events that should update session metadata.
            if let crate::pi::PiEvent::ExtensionUiRequest(req) = &event
                && req.method == "setTitle"
                && let (Some(title), Some(session_id), Some(pi_svc)) = (
                    req.title.as_ref(),
                    current_session_id.as_ref(),
                    pi_service_for_events.as_ref(),
                )
                && let Err(e) = pi_svc
                    .update_session_title(&user_id_for_events, session_id, title)
                    .await
            {
                warn!("Failed to update Pi session title: {}", e);
            }

            // Persist events (only from the primary WS connection).
            if can_persist {
                // Persist full conversation to hstry on AgentEnd
                if let PiEvent::AgentEnd { messages } = &event
                    && let Some(session_id) = &current_session_id
                {
                    // Write to hstry if enabled
                    if let Some(hstry) = &hstry_for_events {
                        // Convert Pi AgentMessages to hstry proto Messages
                        let proto_messages: Vec<_> = messages
                            .iter()
                            .enumerate()
                            .map(|(idx, msg)| {
                                crate::hstry::agent_message_to_proto(
                                    msg,
                                    idx as i32,
                                    session_id,
                                )
                            })
                            .collect();

                        // Get timestamp from first/last message or use current time
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        let created_at_ms = messages
                            .first()
                            .and_then(|m| m.timestamp)
                            .map(|t| t as i64)
                            .unwrap_or(now_ms);
                        let updated_at_ms =
                            messages.last().and_then(|m| m.timestamp).map(|t| t as i64);

                        // Extract model from last assistant message
                        let (model, provider) = messages
                            .iter()
                            .rev()
                            .find_map(|m| {
                                if m.role == "assistant" {
                                    Some((m.model.clone(), m.provider.clone()))
                                } else {
                                    None
                                }
                            })
                            .unwrap_or((None, None));

                        let metadata_json = pi_service_for_events.as_ref().map(|svc| {
                            let work_dir = svc.main_chat_dir(&user_id_for_events);
                            let sessions_dir = svc.sessions_dir_for_workdir(&user_id_for_events, &work_dir);
                            serde_json::json!({
                                "canonical_id": session_id,
                                "readable_id": serde_json::Value::Null,
                                "workdir": work_dir.to_string_lossy(),
                                "session_dir": sessions_dir.to_string_lossy(),
                            })
                            .to_string()
                        });

                        if let Err(e) = hstry
                            .write_conversation(
                                session_id,
                                None, // title - could be extracted from session metadata
                                None, // workspace - Main Chat doesn't have a workspace path
                                model,
                                provider,
                                metadata_json,
                                proto_messages,
                                created_at_ms,
                                updated_at_ms,
                            )
                            .await
                        {
                            warn!("Failed to persist conversation to hstry: {}", e);
                        } else {
                            debug!(
                                "Persisted {} messages to hstry for session {}",
                                messages.len(),
                                session_id
                            );
                        }
                    }
                }

                // Persist auto-compaction output for continuity
                if let PiEvent::AutoCompactionEnd {
                    result: Some(result),
                    aborted: false,
                    ..
                } = &event
                    && let Some(svc) = &history_for_events
                    && let Err(e) = svc
                        .add_history(
                            &user_id_for_events,
                            crate::main_chat::CreateHistoryEntry {
                                entry_type: crate::main_chat::HistoryEntryType::Summary,
                                content: result.summary.clone(),
                                session_id: current_session_id.clone(),
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

            // Transform Pi events into frontend-friendly format with session_id for validation
            let ws_event = transform_pi_event_for_ws(&event, current_session_id.as_deref());
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
                        // main_chat.db message history is deprecated; Pi session events are persisted to hstry.
                        let _ = &main_chat_svc;

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

pub(crate) fn pi_state_to_response(state: PiState) -> PiStateResponse {
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
/// Includes session_id in all events so frontend can validate message ownership.
fn transform_pi_event_for_ws(event: &PiEvent, session_id: Option<&str>) -> Option<Value> {
    match event {
        PiEvent::AgentStart => Some(serde_json::json!({
            "type": "agent_start",
            "session_id": session_id
        })),
        PiEvent::AgentEnd { .. } => Some(serde_json::json!({
            "type": "done",
            "session_id": session_id
        })),
        PiEvent::TurnStart => None,      // Don't forward
        PiEvent::TurnEnd { .. } => None, // Don't forward
        PiEvent::MessageStart { message } => {
            if message.role == "assistant" {
                Some(serde_json::json!({
                    "type": "message_start",
                    "role": "assistant",
                    "session_id": session_id
                }))
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
                    "data": delta,
                    "session_id": session_id
                })),
                AssistantMessageEvent::ThinkingDelta { delta, .. } => Some(serde_json::json!({
                    "type": "thinking",
                    "data": delta,
                    "session_id": session_id
                })),
                AssistantMessageEvent::ToolcallEnd { tool_call, .. } => Some(serde_json::json!({
                    "type": "tool_use",
                    "data": {
                        "id": tool_call.id,
                        "name": tool_call.name,
                        "input": tool_call.arguments
                    },
                    "session_id": session_id
                })),
                AssistantMessageEvent::TextEnd { content, .. } => Some(serde_json::json!({
                    "type": "text",
                    "data": content,
                    "session_id": session_id
                })),
                AssistantMessageEvent::ThinkingEnd { content, .. } => Some(serde_json::json!({
                    "type": "thinking",
                    "data": content,
                    "session_id": session_id
                })),
                AssistantMessageEvent::Error { reason, .. } => Some(serde_json::json!({
                    "type": "error",
                    "data": reason,
                    "session_id": session_id
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
            },
            "session_id": session_id
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
            },
            "session_id": session_id
        })),
        PiEvent::AutoCompactionStart { .. } => Some(serde_json::json!({
            "type": "compaction_start",
            "session_id": session_id
        })),
        PiEvent::AutoCompactionEnd { .. } => Some(serde_json::json!({
            "type": "compaction",
            "session_id": session_id
        })),
        _ => None, // Skip other events
    }
}

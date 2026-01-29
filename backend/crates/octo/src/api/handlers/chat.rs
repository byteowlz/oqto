//! Chat history handlers.

use std::path::PathBuf;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

use crate::auth::CurrentUser;
use crate::history::{ChatMessage, ChatSession};
use crate::wordlist;

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

use super::trx::is_main_chat_path;

/// Query parameters for listing chat history.
#[derive(Debug, Deserialize)]
pub struct ChatHistoryQuery {
    /// Filter by workspace path.
    pub workspace: Option<String>,
    /// Include child sessions (default: false).
    #[serde(default)]
    pub include_children: bool,
    /// Maximum number of sessions to return.
    pub limit: Option<usize>,
}

/// Check if multi-user mode is enabled (linux_users configured).
/// In multi-user mode, we must NOT fall back to direct filesystem access
/// as that would read from the backend user's home, not the requesting user's.
fn is_multi_user_mode(state: &AppState) -> bool {
    state.linux_users.is_some()
}

/// Create a runner client for a user based on socket pattern.
/// Returns the runner client if available, None for direct access.
pub(crate) fn get_runner_for_user(
    state: &AppState,
    user_id: &str,
) -> Option<crate::runner::client::RunnerClient> {
    // Need runner socket pattern for multi-user mode
    let pattern = state.runner_socket_pattern.as_ref()?;

    // Replace {user} with the user_id (which is the platform user_id, e.g., "wismut")
    // The socket is named after the platform user, not the linux username
    let socket_path = pattern.replace("{user}", user_id);
    let socket = std::path::Path::new(&socket_path);

    if socket.exists() {
        tracing::debug!(
            user_id = %user_id,
            socket = %socket_path,
            "Using runner for chat history"
        );
        Some(crate::runner::client::RunnerClient::new(socket_path))
    } else {
        tracing::debug!(
            user_id = %user_id,
            socket = %socket_path,
            "Runner socket not found, using direct access"
        );
        None
    }
}

fn list_workspace_pi_sessions(state: &AppState, user_id: &str) -> Vec<ChatSession> {
    let Some(workspace_pi) = state.workspace_pi.as_ref() else {
        return Vec::new();
    };

    let workspace_root = state.sessions.for_user(user_id).workspace_root();
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.clone());

    let sessions = match workspace_pi.list_sessions_for_user(user_id) {
        Ok(sessions) => sessions,
        Err(err) => {
            warn!(user_id = %user_id, error = %err, "Failed to list workspace Pi sessions");
            return Vec::new();
        }
    };

    sessions
        .into_iter()
        .filter_map(|session| {
            let workspace_path = session.workspace_path.clone();
            if workspace_path != "global" && !workspace_path.is_empty() {
                let path = PathBuf::from(&workspace_path);
                let canonical = if path.exists() {
                    path.canonicalize().unwrap_or(path.clone())
                } else {
                    path.clone()
                };

                if !canonical.starts_with(&canonical_root) {
                    return None;
                }

                if is_main_chat_path(state, &canonical) {
                    return None;
                }
            }

            let project_name = crate::history::project_name_from_path(&workspace_path);
            Some(ChatSession {
                id: session.id.clone(),
                readable_id: wordlist::readable_id_from_session_id(&session.id),
                title: session.title,
                parent_id: session.parent_id.clone(),
                workspace_path,
                project_name,
                created_at: session.created_at,
                updated_at: session.updated_at,
                version: session.version,
                is_child: session.parent_id.is_some(),
                source_path: session.source_path,
            })
        })
        .collect()
}

/// List all chat sessions from OpenCode history.
///
/// In multi-user mode, this uses the runner to read from the user's home directory.
/// In single-user mode, uses direct filesystem access.
///
/// SECURITY: In multi-user mode, we MUST use the runner. Falling back to direct
/// filesystem access would read from the backend user's home directory, potentially
/// exposing other users' data.
#[instrument(skip(state))]
pub async fn list_chat_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<ChatHistoryQuery>,
) -> ApiResult<Json<Vec<ChatSession>>> {
    let mut sessions: Vec<ChatSession> = Vec::new();
    let mut source = "direct";
    let multi_user = is_multi_user_mode(&state);

    // In multi-user mode, use runner to access user's home directory
    if let Some(runner) = get_runner_for_user(&state, user.id()) {
        match runner.list_opencode_sessions(None, true, None).await {
            Ok(response) => {
                sessions = response
                    .sessions
                    .into_iter()
                    .map(|s| ChatSession {
                        id: s.id,
                        readable_id: s.readable_id,
                        title: s.title,
                        parent_id: s.parent_id,
                        workspace_path: s.workspace_path,
                        project_name: s.project_name,
                        created_at: s.created_at,
                        updated_at: s.updated_at,
                        version: s.version,
                        is_child: s.is_child,
                        source_path: None,
                    })
                    .collect();
                source = "runner";
            }
            Err(e) => {
                // SECURITY: In multi-user mode, do NOT fall back to direct access
                if multi_user {
                    tracing::error!(
                        user_id = %user.id(),
                        error = %e,
                        "Runner failed in multi-user mode, cannot fall back to direct access"
                    );
                    return Err(ApiError::internal(
                        "Chat history service unavailable. Please try again later."
                    ));
                }
                tracing::warn!(user_id = %user.id(), error = %e, "Runner failed, falling back to direct access");
            }
        }
    } else if multi_user {
        // SECURITY: Multi-user mode requires runner, but none available
        tracing::error!(
            user_id = %user.id(),
            "No runner available in multi-user mode"
        );
        return Err(ApiError::internal(
            "Chat history service not configured for this user."
        ));
    }

    // SECURITY: Only use direct filesystem access in single-user mode
    if !multi_user && sessions.is_empty() {
        if let Some(db_path) = crate::history::hstry_db_path() {
            match crate::history::list_sessions_from_hstry(&db_path).await {
                Ok(found) => {
                    sessions = found;
                    source = "hstry";
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "Failed to list chat history via hstry, falling back to direct access"
                    );
                }
            }
        }
    }

    if !multi_user && sessions.is_empty() {
        sessions = crate::history::list_sessions()
            .map_err(|e| ApiError::internal(format!("Failed to list chat history: {}", e)))?;
    }

    sessions.extend(list_workspace_pi_sessions(&state, user.id()));

    if let Some(ref ws) = query.workspace {
        sessions.retain(|s| s.workspace_path == *ws);
    }

    if !query.include_children {
        sessions.retain(|s| !s.is_child);
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    if let Some(limit) = query.limit {
        sessions.truncate(limit);
    }

    info!(user_id = %user.id(), count = sessions.len(), source = source, "Listed chat history");
    Ok(Json(sessions))
}

/// Get a specific chat session by ID.
///
/// SECURITY: In multi-user mode, we MUST use the runner to ensure user isolation.
#[instrument(skip(state))]
pub async fn get_chat_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<ChatSession>> {
    let multi_user = is_multi_user_mode(&state);

    // In multi-user mode, use runner
    if let Some(runner) = get_runner_for_user(&state, user.id()) {
        match runner.get_opencode_session(&session_id).await {
            Ok(response) => {
                if let Some(s) = response.session {
                    return Ok(Json(ChatSession {
                        id: s.id,
                        readable_id: s.readable_id,
                        title: s.title,
                        parent_id: s.parent_id,
                        workspace_path: s.workspace_path,
                        project_name: s.project_name,
                        created_at: s.created_at,
                        updated_at: s.updated_at,
                        version: s.version,
                        is_child: s.is_child,
                        source_path: None,
                    }));
                }
                // Session not found via runner
                if multi_user {
                    return Err(ApiError::not_found(format!("Chat session {} not found", session_id)));
                }
            }
            Err(e) => {
                // SECURITY: In multi-user mode, do NOT fall back
                if multi_user {
                    tracing::error!(
                        user_id = %user.id(),
                        session_id = %session_id,
                        error = %e,
                        "Runner failed in multi-user mode"
                    );
                    return Err(ApiError::internal("Chat history service unavailable."));
                }
            }
        }
    } else if multi_user {
        // SECURITY: Multi-user mode requires runner
        return Err(ApiError::internal("Chat history service not configured for this user."));
    }

    // SECURITY: Only use direct access in single-user mode
    if let Some(db_path) = crate::history::hstry_db_path() {
        match crate::history::get_session_from_hstry(&session_id, &db_path).await {
            Ok(Some(session)) => return Ok(Json(session)),
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(
                    session_id = %session_id,
                    error = %err,
                    "Failed to load chat session via hstry, falling back to direct access"
                );
            }
        }
    }

    // Single-user mode: direct access
    crate::history::get_session(&session_id)
        .map_err(|e| ApiError::internal(format!("Failed to get chat session: {}", e)))?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("Chat session {} not found", session_id)))
}

/// Request to update a chat session.
#[derive(Debug, Deserialize)]
pub struct UpdateChatSessionRequest {
    /// New title for the session
    pub title: Option<String>,
}

/// Update a chat session (e.g., rename).
///
/// SECURITY: In multi-user mode, we MUST use the runner to ensure user isolation.
#[instrument(skip(state))]
pub async fn update_chat_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateChatSessionRequest>,
) -> ApiResult<Json<ChatSession>> {
    let multi_user = is_multi_user_mode(&state);

    // In multi-user mode, use runner
    if let Some(runner) = get_runner_for_user(&state, user.id()) {
        match runner
            .update_opencode_session(&session_id, request.title.clone())
            .await
        {
            Ok(response) => {
                let session = ChatSession {
                    id: response.session.id,
                    readable_id: response.session.readable_id,
                    title: response.session.title,
                    parent_id: response.session.parent_id,
                    workspace_path: response.session.workspace_path,
                    project_name: response.session.project_name,
                    created_at: response.session.created_at,
                    updated_at: response.session.updated_at,
                    version: response.session.version,
                    is_child: response.session.is_child,
                    source_path: None,
                };

                if let Some(ref title) = request.title {
                    info!(session_id = %session_id, title = %title, "Updated chat session title via runner");
                }
                return Ok(Json(session));
            }
            Err(e) => {
                // SECURITY: In multi-user mode, do NOT fall back
                if multi_user {
                    tracing::error!(
                        user_id = %user.id(),
                        session_id = %session_id,
                        error = %e,
                        "Runner failed in multi-user mode"
                    );
                    return Err(ApiError::internal("Chat history service unavailable."));
                }
            }
        }
    } else if multi_user {
        // SECURITY: Multi-user mode requires runner
        return Err(ApiError::internal("Chat history service not configured for this user."));
    }

    // SECURITY: Only use direct access in single-user mode
    if let Some(title) = request.title {
        let session = crate::history::update_session_title(&session_id, &title).map_err(|e| {
            if e.to_string().contains("not found") {
                ApiError::not_found(format!("Chat session {} not found", session_id))
            } else {
                ApiError::internal(format!("Failed to update chat session: {}", e))
            }
        })?;

        info!(session_id = %session_id, title = %title, "Updated chat session title");
        Ok(Json(session))
    } else {
        // No updates requested - just return the current session
        crate::history::get_session(&session_id)
            .map_err(|e| ApiError::internal(format!("Failed to get chat session: {}", e)))?
            .map(Json)
            .ok_or_else(|| ApiError::not_found(format!("Chat session {} not found", session_id)))
    }
}

/// Response for grouped chat history.
#[derive(Debug, Serialize)]
pub struct GroupedChatHistory {
    pub workspace_path: String,
    pub project_name: String,
    pub sessions: Vec<ChatSession>,
}

/// List chat sessions grouped by workspace/project.
///
/// SECURITY: In multi-user mode, we MUST use the runner to ensure user isolation.
#[instrument(skip(state))]
pub async fn list_chat_history_grouped(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<ChatHistoryQuery>,
) -> ApiResult<Json<Vec<GroupedChatHistory>>> {
    let mut sessions: Vec<ChatSession> = Vec::new();
    let mut source = "direct";
    let multi_user = is_multi_user_mode(&state);

    if let Some(runner) = get_runner_for_user(&state, user.id()) {
        match runner.list_opencode_sessions(None, true, None).await {
            Ok(response) => {
                sessions = response
                    .sessions
                    .into_iter()
                    .map(|s| ChatSession {
                        id: s.id,
                        readable_id: s.readable_id,
                        title: s.title,
                        parent_id: s.parent_id,
                        workspace_path: s.workspace_path,
                        project_name: s.project_name,
                        created_at: s.created_at,
                        updated_at: s.updated_at,
                        version: s.version,
                        is_child: s.is_child,
                        source_path: None,
                    })
                    .collect();
                source = "runner";
            }
            Err(e) => {
                // SECURITY: In multi-user mode, do NOT fall back
                if multi_user {
                    tracing::error!(
                        user_id = %user.id(),
                        error = %e,
                        "Runner failed in multi-user mode"
                    );
                    return Err(ApiError::internal("Chat history service unavailable."));
                }
            }
        }
    } else if multi_user {
        // SECURITY: Multi-user mode requires runner
        return Err(ApiError::internal("Chat history service not configured for this user."));
    }

    // SECURITY: Only use direct access in single-user mode
    if !multi_user && sessions.is_empty() {
        if let Some(db_path) = crate::history::hstry_db_path() {
            match crate::history::list_sessions_from_hstry(&db_path).await {
                Ok(found) => {
                    sessions = found;
                    source = "hstry";
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "Failed to list grouped chat history via hstry, falling back to direct access"
                    );
                }
            }
        }
    }

    if !multi_user && sessions.is_empty() {
        sessions = crate::history::list_sessions()
            .map_err(|e| ApiError::internal(format!("Failed to list chat history: {}", e)))?;
    }

    sessions.extend(list_workspace_pi_sessions(&state, user.id()));

    if let Some(ref ws) = query.workspace {
        sessions.retain(|s| s.workspace_path == *ws);
    }

    if !query.include_children {
        sessions.retain(|s| !s.is_child);
    }

    let mut grouped: std::collections::HashMap<String, Vec<ChatSession>> =
        std::collections::HashMap::new();
    for session in sessions {
        grouped
            .entry(session.workspace_path.clone())
            .or_default()
            .push(session);
    }

    let mut result: Vec<GroupedChatHistory> = grouped
        .into_iter()
        .map(|(workspace_path, mut sessions)| {
            sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            if let Some(limit) = query.limit {
                sessions.truncate(limit);
            }
            let project_name = sessions
                .first()
                .map(|s| s.project_name.clone())
                .unwrap_or_else(|| crate::history::project_name_from_path(&workspace_path));
            GroupedChatHistory {
                workspace_path,
                project_name,
                sessions,
            }
        })
        .filter(|g| !g.sessions.is_empty())
        .collect();

    result.sort_by(|a, b| {
        let a_updated = a.sessions.first().map(|s| s.updated_at).unwrap_or(0);
        let b_updated = b.sessions.first().map(|s| s.updated_at).unwrap_or(0);
        b_updated.cmp(&a_updated)
    });

    info!(user_id = %user.id(), count = result.len(), source = source, "Listed grouped chat history");
    Ok(Json(result))
}

/// Query parameters for chat messages endpoint.
#[derive(Debug, Deserialize)]
pub struct ChatMessagesQuery {
    /// If true, include pre-rendered HTML for text parts (slower but saves client CPU)
    #[serde(default)]
    pub render: bool,
}

/// Get all messages for a chat session.
///
/// This reads messages and their parts directly from OpenCode's storage on disk.
/// Uses async I/O with caching for better performance on large sessions.
///
/// Query params:
/// - `render=true`: Include pre-rendered markdown HTML in `text_html` field
///
/// SECURITY: In multi-user mode, we MUST use the runner to ensure user isolation.
#[instrument(skip(state))]
pub async fn get_chat_messages(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<ChatMessagesQuery>,
) -> ApiResult<Json<Vec<ChatMessage>>> {
    let multi_user = is_multi_user_mode(&state);

    // In multi-user mode, use runner
    if let Some(runner) = get_runner_for_user(&state, user.id()) {
        match runner
            .get_opencode_session_messages(&session_id, query.render)
            .await
        {
            Ok(response) => {
                // Convert protocol types to history types
                let messages: Vec<ChatMessage> = response
                    .messages
                    .into_iter()
                    .map(|m| ChatMessage {
                        id: m.id,
                        session_id: m.session_id,
                        role: m.role,
                        created_at: m.created_at,
                        completed_at: m.completed_at,
                        parent_id: m.parent_id,
                        model_id: m.model_id,
                        provider_id: m.provider_id,
                        agent: m.agent,
                        summary_title: m.summary_title,
                        tokens_input: m.tokens_input,
                        tokens_output: m.tokens_output,
                        tokens_reasoning: m.tokens_reasoning,
                        cost: m.cost,
                        parts: m
                            .parts
                            .into_iter()
                            .map(|p| crate::history::ChatMessagePart {
                                id: p.id,
                                part_type: p.part_type,
                                text: p.text,
                                text_html: p.text_html,
                                tool_name: p.tool_name,
                                tool_input: p.tool_input,
                                tool_output: p.tool_output,
                                tool_status: p.tool_status,
                                tool_title: p.tool_title,
                            })
                            .collect(),
                    })
                    .collect();

                info!(user_id = %user.id(), session_id = %session_id, count = messages.len(), render = query.render, "Listed chat messages via runner");
                return Ok(Json(messages));
            }
            Err(e) => {
                // SECURITY: In multi-user mode, do NOT fall back
                if multi_user {
                    tracing::error!(
                        user_id = %user.id(),
                        session_id = %session_id,
                        error = %e,
                        "Runner failed in multi-user mode"
                    );
                    return Err(ApiError::internal("Chat history service unavailable."));
                }
            }
        }
    } else if multi_user {
        // SECURITY: Multi-user mode requires runner
        return Err(ApiError::internal("Chat history service not configured for this user."));
    }

    // SECURITY: Only use direct access in single-user mode
    let messages = if query.render {
        crate::history::get_session_messages_rendered(&session_id).await
    } else {
        crate::history::get_session_messages_async(&session_id).await
    }
    .map_err(|e| ApiError::internal(format!("Failed to get chat messages: {}", e)))?;

    info!(session_id = %session_id, count = messages.len(), render = query.render, "Listed chat messages");
    Ok(Json(messages))
}

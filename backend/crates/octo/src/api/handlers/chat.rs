//! Chat history handlers.

use std::collections::HashMap;
use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};

use crate::auth::CurrentUser;
use crate::history::{ChatMessage, ChatSession};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

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

    // Use for_user_with_pattern which handles both {user} and {uid} placeholders
    match crate::runner::client::RunnerClient::for_user_with_pattern(user_id, pattern) {
        Ok(client) => {
            let socket_path = client.socket_path();
            if socket_path.exists() {
                tracing::debug!(
                    user_id = %user_id,
                    socket = %socket_path.display(),
                    "Using runner for chat history"
                );
                Some(client)
            } else {
                tracing::debug!(
                    user_id = %user_id,
                    socket = %socket_path.display(),
                    "Runner socket not found, using direct access"
                );
                None
            }
        }
        Err(e) => {
            tracing::warn!(
                user_id = %user_id,
                error = %e,
                "Failed to create runner client"
            );
            None
        }
    }
}

fn merge_duplicate_sessions(mut sessions: Vec<ChatSession>) -> Vec<ChatSession> {
    // Keep newest sessions first so we can prefer the freshest metadata.
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let mut by_id: HashMap<String, ChatSession> = HashMap::new();
    let mut by_source: HashMap<String, String> = HashMap::new();
    let mut by_key: HashMap<(String, String), String> = HashMap::new();
    let mut by_readable: HashMap<String, String> = HashMap::new();

    for session in sessions {
        if let Some(source) = session.source_path.clone() {
            if let Some(existing_id) = by_source.get(&source).cloned() {
                if let Some(existing) = by_id.get_mut(&existing_id) {
                    if session.updated_at > existing.updated_at {
                        *existing = session;
                    } else {
                        if existing.title.is_none() && session.title.is_some() {
                            existing.title = session.title;
                        }
                        if existing.readable_id.is_empty() && !session.readable_id.is_empty() {
                            existing.readable_id = session.readable_id;
                        }
                    }
                }
                continue;
            }
            by_source.insert(source, session.id.clone());
        }

        if by_id.contains_key(&session.id) {
            continue;
        }

        let readable = session.readable_id.trim().to_string();
        let normalized_workspace = if session.workspace_path.trim().is_empty() {
            "global".to_string()
        } else {
            session.workspace_path.clone()
        };

        if !readable.is_empty() {
            let key = (normalized_workspace.clone(), readable.clone());
            if let Some(existing_id) = by_key.get(&key).cloned() {
                if let Some(existing) = by_id.get_mut(&existing_id) {
                    if existing.title.is_none() && session.title.is_some() {
                        existing.title = session.title;
                    }
                    if session.updated_at > existing.updated_at {
                        existing.updated_at = session.updated_at;
                    }
                }
                continue;
            }

            if let Some(existing_id) = by_readable.get(&readable).cloned() {
                if let Some(existing) = by_id.get_mut(&existing_id) {
                    let existing_workspace = if existing.workspace_path.trim().is_empty() {
                        "global".to_string()
                    } else {
                        existing.workspace_path.clone()
                    };
                    let prefer_candidate =
                        existing_workspace == "global" && normalized_workspace != "global";
                    if prefer_candidate {
                        *existing = session;
                    } else {
                        if existing.title.is_none() && session.title.is_some() {
                            existing.title = session.title;
                        }
                        if session.updated_at > existing.updated_at {
                            existing.updated_at = session.updated_at;
                        }
                    }
                }
                continue;
            }
            by_key.insert(key, session.id.clone());
            by_readable.insert(readable, session.id.clone());
        }

        by_id.insert(session.id.clone(), session);
    }

    let mut merged: Vec<ChatSession> = by_id.into_values().collect();
    merged.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    merged
}

/// List all chat sessions from hstry.
///
/// In multi-user mode, this uses the runner to query hstry for the user.
/// In single-user mode, queries hstry gRPC directly.
///
/// SECURITY: In multi-user mode, we MUST use the runner. Falling back to direct
/// access would read from the backend user's data, potentially
/// exposing other users' data.
#[instrument(skip(state))]
pub async fn list_chat_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<ChatHistoryQuery>,
) -> ApiResult<Json<Vec<ChatSession>>> {
    let mut sessions: Vec<ChatSession> = Vec::new();
    let mut source = "hstry";
    let multi_user = is_multi_user_mode(&state);

    if let Some(runner) = get_runner_for_user(&state, user.id()) {
        match runner
            .list_workspace_chat_sessions(
                query.workspace.clone(),
                query.include_children,
                query.limit,
            )
            .await
        {
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
                        stats: None,
                        model: s.model,
                        provider: s.provider,
                    })
                    .collect();
                source = "runner";
            }
            Err(e) => {
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
        return Err(ApiError::internal(
            "Chat history service not configured for this user.",
        ));
    }

    if sessions.is_empty() && !multi_user {
        if let Some(hstry) = state.hstry.as_ref() {
            match crate::history::repository::list_sessions_via_grpc(hstry).await {
                Ok(found) => {
                    sessions = found;
                }
                Err(e) => {
                    tracing::error!("Failed to list sessions via hstry gRPC: {}", e);
                    return Err(ApiError::service_unavailable(format!(
                        "Chat history service (hstry) is not reachable: {}. \
                     Try restarting it with: hstry service start",
                        e
                    )));
                }
            }
        }
    }

    sessions = merge_duplicate_sessions(sessions);

    let mut seen = HashSet::new();
    sessions.retain(|session| seen.insert(session.id.clone()));

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

    debug!(user_id = %user.id(), count = sessions.len(), source = source, "Listed chat history");
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
        match runner.get_workspace_chat_session(&session_id).await {
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
                        stats: None,
                        model: s.model,
                        provider: s.provider,
                    }));
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

        if multi_user {
            return Err(ApiError::not_found(format!(
                "Chat session {} not found",
                session_id
            )));
        }
    } else if multi_user {
        // SECURITY: Multi-user mode requires runner
        return Err(ApiError::internal(
            "Chat history service not configured for this user.",
        ));
    }

    // Single-user mode: use hstry gRPC
    if let Some(hstry) = state.hstry.as_ref() {
        match crate::history::repository::get_session_via_grpc(hstry, &session_id).await {
            Ok(Some(session)) => return Ok(Json(session)),
            Ok(None) => {
                return Err(ApiError::not_found(format!(
                    "Chat session {} not found",
                    session_id
                )));
            }
            Err(err) => {
                return Err(ApiError::internal(format!(
                    "Failed to load chat session: {}",
                    err
                )));
            }
        }
    }

    Err(ApiError::internal("hstry not configured"))
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
            .update_workspace_chat_session(&session_id, request.title.clone())
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
                    stats: None,
                    model: response.session.model,
                    provider: response.session.provider,
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
        return Err(ApiError::internal(
            "Chat history service not configured for this user.",
        ));
    }

    // Single-user mode: update via hstry gRPC (partial update)
    if let Some(hstry) = state.hstry.as_ref() {
        if let Some(ref title) = request.title {
            match hstry
                .update_conversation(
                    &session_id,
                    Some(title.clone()),
                    None, // workspace unchanged
                    None, // model unchanged
                    None, // provider unchanged
                    None, // metadata unchanged
                    None, // readable_id unchanged
                    None, // harness unchanged
                    None, // platform_id unchanged
                )
                .await
            {
                Ok(_) => {
                    match crate::history::repository::get_session_via_grpc(hstry, &session_id).await
                    {
                        Ok(Some(session)) => {
                            info!(session_id = %session_id, title = %title, "Updated chat session title");
                            return Ok(Json(session));
                        }
                        Ok(None) => {
                            return Err(ApiError::not_found(format!(
                                "Chat session {} not found after update",
                                session_id
                            )));
                        }
                        Err(err) => {
                            return Err(ApiError::internal(format!(
                                "Failed to fetch updated session: {}",
                                err
                            )));
                        }
                    }
                }
                Err(err) => {
                    return Err(ApiError::internal(format!(
                        "Failed to update chat session: {}",
                        err
                    )));
                }
            }
        } else {
            // No updates requested - just return the current session
            match crate::history::repository::get_session_via_grpc(hstry, &session_id).await {
                Ok(Some(session)) => return Ok(Json(session)),
                Ok(None) => {
                    return Err(ApiError::not_found(format!(
                        "Chat session {} not found",
                        session_id
                    )));
                }
                Err(err) => {
                    return Err(ApiError::internal(format!(
                        "Failed to get chat session: {}",
                        err
                    )));
                }
            }
        }
    }

    Err(ApiError::internal("hstry not configured"))
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
    let mut source = "hstry";
    let multi_user = is_multi_user_mode(&state);

    if let Some(runner) = get_runner_for_user(&state, user.id()) {
        match runner
            .list_workspace_chat_sessions(
                query.workspace.clone(),
                query.include_children,
                query.limit,
            )
            .await
        {
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
                        stats: None,
                        model: s.model,
                        provider: s.provider,
                    })
                    .collect();
                source = "runner";
            }
            Err(e) => {
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
        return Err(ApiError::internal(
            "Chat history service not configured for this user.",
        ));
    }

    let mut hstry_sessions: Vec<ChatSession> = Vec::new();
    if !multi_user && let Some(hstry) = state.hstry.as_ref() {
        match crate::history::repository::list_sessions_via_grpc(hstry).await {
            Ok(found) => {
                hstry_sessions = found;
            }
            Err(e) => {
                tracing::error!("Failed to list sessions via hstry gRPC (grouped): {}", e);
                return Err(ApiError::service_unavailable(format!(
                    "Chat history service (hstry) is not reachable: {}. \
                     Try restarting it with: hstry service start",
                    e
                )));
            }
        }
    }

    if !hstry_sessions.is_empty() {
        let mut by_id: HashMap<String, ChatSession> =
            sessions.into_iter().map(|s| (s.id.clone(), s)).collect();

        for session in hstry_sessions {
            by_id
                .entry(session.id.clone())
                .and_modify(|existing| {
                    // Prefer JSONL metadata (title, readable_id) over hstry when
                    // hstry has gaps, but take the newer updated_at timestamp.
                    if existing.title.is_none() && session.title.is_some() {
                        existing.title = session.title.clone();
                    }
                    if existing.readable_id.is_empty() && !session.readable_id.is_empty() {
                        existing.readable_id = session.readable_id.clone();
                    }
                    if session.updated_at > existing.updated_at {
                        existing.updated_at = session.updated_at;
                    }
                })
                .or_insert(session);
        }
        source = "mixed";

        sessions = by_id.into_values().collect();
    }

    sessions = merge_duplicate_sessions(sessions);

    let mut seen = HashSet::new();
    sessions.retain(|session| seen.insert(session.id.clone()));

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

    debug!(user_id = %user.id(), count = result.len(), source = source, "Listed grouped chat history");
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
/// Get all messages for a chat session via hstry.
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
    let prefer_hstry = !multi_user && state.hstry.is_some();

    // In multi-user mode, use runner -> hstry
    if !prefer_hstry {
        if let Some(runner) = get_runner_for_user(&state, user.id()) {
            match runner
                .get_workspace_chat_session_messages(&session_id, query.render, None)
                .await
            {
                Ok(response) => {
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
                            client_id: None,
                            parts: m
                                .parts
                                .into_iter()
                                .map(|p| crate::history::ChatMessagePart {
                                    id: p.id,
                                    part_type: p.part_type,
                                    text: p.text,
                                    text_html: p.text_html,
                                    tool_name: p.tool_name,
                                    tool_call_id: None,
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
            return Err(ApiError::internal(
                "Chat history service not configured for this user.",
            ));
        }
    }

    if multi_user {
        return Err(ApiError::not_found(format!(
            "Chat session {} not found",
            session_id
        )));
    }

    // Single-user mode: use hstry gRPC directly
    let messages = if let Some(hstry) = state.hstry.as_ref() {
        if query.render {
            crate::history::get_session_messages_rendered_via_grpc(hstry, &session_id).await
        } else {
            crate::history::get_session_messages_via_grpc_cached(hstry, &session_id).await
        }
    } else {
        Err(anyhow::anyhow!("hstry not configured"))
    }
    .map_err(|e| ApiError::internal(format!("Failed to get chat messages: {}", e)))?;

    info!(session_id = %session_id, count = messages.len(), render = query.render, "Listed chat messages");
    Ok(Json(messages))
}

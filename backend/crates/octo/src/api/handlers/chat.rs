//! Chat history handlers.

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

use crate::auth::CurrentUser;
use crate::history::{ChatMessage, ChatSession};
use crate::pi::AgentMessage;

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

fn list_workspace_pi_sessions(state: &AppState, user_id: &str) -> Vec<ChatSession> {
    let Some(workspace_pi) = state.workspace_pi.as_ref() else {
        return Vec::new();
    };

    let workspace_root = state.sessions.for_user(user_id).workspace_root();
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.clone());
    let main_chat_dir = state
        .main_chat
        .as_ref()
        .map(|main_chat| main_chat.get_main_chat_dir(user_id));

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
            let mut workspace_path = session.workspace_path.clone();
            if workspace_path.is_empty() || workspace_path == "global" {
                if let Some(ref main_chat_dir) = main_chat_dir {
                    workspace_path = main_chat_dir.to_string_lossy().to_string();
                }
            }

            if !workspace_path.is_empty() {
                let path = PathBuf::from(&workspace_path);
                let canonical = if path.exists() {
                    path.canonicalize().unwrap_or(path.clone())
                } else {
                    path.clone()
                };

                let allowed = canonical.starts_with(&canonical_root)
                    || main_chat_dir
                        .as_ref()
                        .map(|dir| canonical.starts_with(dir))
                        .unwrap_or(false);

                if !allowed {
                    return None;
                }
            }

            let project_name = crate::history::project_name_from_path(&workspace_path);
            let readable_id = session.readable_id.clone().unwrap_or_default();
            Some(ChatSession {
                id: session.id.clone(),
                readable_id: if readable_id.is_empty() { String::new() } else { readable_id },
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

async fn update_pi_session_title(
    state: &AppState,
    user_id: &str,
    session_id: &str,
    title: &str,
) -> Option<ChatSession> {
    if let Some(workspace_pi) = state.workspace_pi.as_ref() {
        let sessions = list_workspace_pi_sessions(state, user_id);
        if let Some(session) = sessions.iter().find(|s| s.id == session_id) {
            let work_dir = PathBuf::from(&session.workspace_path);
            if let Ok(updated) = workspace_pi
                .update_session_title(user_id, &work_dir, session_id, title)
                .await
            {
                let project_name = crate::history::project_name_from_path(&updated.workspace_path);
                let readable_id = updated.readable_id.clone().unwrap_or_default();
                return Some(ChatSession {
                    id: updated.id.clone(),
                    readable_id,
                    title: updated.title,
                    parent_id: updated.parent_id.clone(),
                    workspace_path: updated.workspace_path,
                    project_name,
                    created_at: updated.created_at,
                    updated_at: updated.updated_at,
                    version: updated.version,
                    is_child: updated.parent_id.is_some(),
                    source_path: updated.source_path,
                });
            }
        }
    }

    None
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
    let mut sessions = list_workspace_pi_sessions(&state, user.id());
    let source = "pi";

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
                    if let Some(ref title) = request.title {
                        if let Some(pi_session) =
                            update_pi_session_title(&state, user.id(), &session_id, title).await
                        {
                            info!(
                                session_id = %session_id,
                                title = %title,
                                "Updated Pi session title via runner"
                            );
                            return Ok(Json(pi_session));
                        }
                    }
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

    // SECURITY: Only use direct access in single-user mode
    if let Some(title) = request.title {
        match crate::history::update_session_title(&session_id, &title) {
            Ok(session) => {
                info!(session_id = %session_id, title = %title, "Updated chat session title");
                return Ok(Json(session));
            }
            Err(err) if err.to_string().contains("not found") => {
                if let Some(pi_session) =
                    update_pi_session_title(&state, user.id(), &session_id, &title).await
                {
                    info!(
                        session_id = %session_id,
                        title = %title,
                        "Updated Pi session title"
                    );
                    return Ok(Json(pi_session));
                }
                return Err(ApiError::not_found(format!(
                    "Chat session {} not found",
                    session_id
                )));
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
        crate::history::get_session(&session_id)
            .map_err(|e| ApiError::internal(format!("Failed to get chat session: {}", e)))?
            .map(Json)
            .ok_or_else(|| ApiError::not_found(format!("Chat session {} not found", session_id)))
    }
}

async fn read_pi_jsonl_messages_from_path(path: &PathBuf) -> anyhow::Result<Vec<AgentMessage>> {
    let file = std::fs::File::open(path).context("opening pi session jsonl")?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if entry.get("type").and_then(|t| t.as_str()) != Some("message") {
            continue;
        }
        if let Some(msg) = entry.get("message") {
            if let Ok(agent_msg) = serde_json::from_value::<AgentMessage>(msg.clone()) {
                messages.push(agent_msg);
            }
        }
    }

    Ok(messages)
}

async fn read_pi_jsonl_messages(
    state: &AppState,
    user_id: &str,
    session: &ChatSession,
) -> anyhow::Result<Vec<AgentMessage>> {
    if let Some(ref source_path) = session.source_path {
        let path = PathBuf::from(source_path);
        if path.exists() {
            return read_pi_jsonl_messages_from_path(&path).await;
        }
    }

    if let Some(main_chat_pi) = state.main_chat_pi.as_ref() {
        if let Some(path) = main_chat_pi.get_session_file_path(user_id, &session.id).await {
            return read_pi_jsonl_messages_from_path(&path).await;
        }
    }

    anyhow::bail!("Pi session file not found for {}", session.id)
}

async fn backfill_pi_session_to_hstry(
    state: &AppState,
    user_id: &str,
    session: &ChatSession,
) -> anyhow::Result<()> {
    let Some(hstry) = state.hstry.as_ref() else {
        return Ok(());
    };
    if !hstry.is_connected().await {
        let _ = hstry.connect().await;
    }

    let existing = hstry
        .get_conversation(&session.id, Some(session.workspace_path.clone()))
        .await?;
    if existing.is_some() {
        return Ok(());
    }

    let messages = read_pi_jsonl_messages(state, user_id, session).await?;
    if messages.is_empty() {
        return Ok(());
    }

    let proto_messages: Vec<_> = messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| {
            crate::hstry::agent_message_to_proto(msg, idx as i32, &session.id)
        })
        .collect();

    let created_at_ms = session.created_at;
    let updated_at_ms = Some(session.updated_at);

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

    let metadata_json = serde_json::json!({
        "canonical_id": session.id,
        "readable_id": session.readable_id,
        "workdir": session.workspace_path,
    })
    .to_string();

    hstry
        .write_conversation(
            &session.id,
            session.title.clone(),
            Some(session.workspace_path.clone()),
            model,
            provider,
            Some(metadata_json),
            proto_messages,
            created_at_ms,
            updated_at_ms,
        )
        .await?;

    Ok(())
}

fn canon_parts_to_chat_parts(
    message_id: &str,
    parts: &[crate::canon::CanonPart],
) -> Vec<crate::history::ChatMessagePart> {
    parts
        .iter()
        .enumerate()
        .filter_map(|(idx, part)| {
            let id = format!("{message_id}-part-{idx}");
            match part {
                crate::canon::CanonPart::Text { text, .. } => Some(crate::history::ChatMessagePart {
                    id,
                    part_type: "text".to_string(),
                    text: Some(text.clone()),
                    text_html: None,
                    tool_name: None,
                    tool_input: None,
                    tool_output: None,
                    tool_status: None,
                    tool_title: None,
                }),
                crate::canon::CanonPart::Thinking { text, .. } => {
                    Some(crate::history::ChatMessagePart {
                        id,
                        part_type: "thinking".to_string(),
                        text: Some(text.clone()),
                        text_html: None,
                        tool_name: None,
                        tool_input: None,
                        tool_output: None,
                        tool_status: None,
                        tool_title: None,
                    })
                }
                crate::canon::CanonPart::ToolCall { name, input, status, .. } => {
                    Some(crate::history::ChatMessagePart {
                        id,
                        part_type: "tool_call".to_string(),
                        text: None,
                        text_html: None,
                        tool_name: Some(name.clone()),
                        tool_input: input.clone(),
                        tool_output: None,
                        tool_status: Some(match status {
                            crate::canon::ToolStatus::Pending => "pending".to_string(),
                            crate::canon::ToolStatus::Running => "running".to_string(),
                            crate::canon::ToolStatus::Success => "success".to_string(),
                            crate::canon::ToolStatus::Error => "error".to_string(),
                        }),
                        tool_title: None,
                    })
                }
                crate::canon::CanonPart::ToolResult {
                    name,
                    output,
                    is_error,
                    title,
                    ..
                } => Some(crate::history::ChatMessagePart {
                    id,
                    part_type: "tool_result".to_string(),
                    text: None,
                    text_html: None,
                    tool_name: name.clone(),
                    tool_input: None,
                    tool_output: output.as_ref().map(|v| v.to_string()),
                    tool_status: Some(if *is_error { "error" } else { "success" }.to_string()),
                    tool_title: title.clone(),
                }),
                _ => None,
            }
        })
        .collect()
}

fn canon_message_to_chat_message(message: crate::canon::CanonMessage) -> ChatMessage {
    let tokens = message.tokens.as_ref();
    let message_id = message.id.clone();
    ChatMessage {
        id: message_id.clone(),
        session_id: message.session_id,
        role: message.role.to_string(),
        created_at: message.created_at,
        completed_at: message.completed_at,
        parent_id: message.parent_id,
        model_id: message.model.as_ref().map(|m| m.full_id()),
        provider_id: None,
        agent: message.agent,
        summary_title: None,
        tokens_input: tokens.and_then(|t| t.input),
        tokens_output: tokens.and_then(|t| t.output),
        tokens_reasoning: tokens.and_then(|t| t.reasoning),
        cost: message.cost_usd,
        parts: canon_parts_to_chat_parts(&message_id, &message.parts),
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
    let mut sessions = list_workspace_pi_sessions(&state, user.id());
    let mut source = "pi";
    let multi_user = is_multi_user_mode(&state);

    let mut hstry_sessions: Vec<ChatSession> = Vec::new();
    if !multi_user && let Some(db_path) = crate::history::hstry_db_path() {
        if let Ok(found) = crate::history::list_sessions_from_hstry(&db_path).await {
            hstry_sessions = found;
        }
    }

    if !hstry_sessions.is_empty() {
        let mut by_id: HashMap<String, ChatSession> =
            sessions.into_iter().map(|s| (s.id.clone(), s)).collect();

        if !hstry_sessions.is_empty() {
            for session in &hstry_sessions {
                by_id.insert(session.id.clone(), session.clone());
            }
            source = "mixed";
        }

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
        return Err(ApiError::internal(
            "Chat history service not configured for this user.",
        ));
    }

    if !multi_user {
        let pi_sessions = list_workspace_pi_sessions(&state, user.id());
        if let Some(pi_session) = pi_sessions.into_iter().find(|s| s.id == session_id) {
            if let Err(err) = backfill_pi_session_to_hstry(&state, user.id(), &pi_session).await {
                tracing::warn!(
                    session_id = %session_id,
                    error = %err,
                    "Failed to backfill Pi session before fetching messages"
                );
            }

            let messages = if query.render {
                crate::history::get_session_messages_rendered(&session_id).await
            } else {
                crate::history::get_session_messages_async(&session_id).await
            }
            .map_err(|e| ApiError::internal(format!("Failed to get chat messages: {}", e)))?;

            if !messages.is_empty() {
                info!(
                    session_id = %session_id,
                    count = messages.len(),
                    render = query.render,
                    "Listed chat messages from hstry for Pi session"
                );
                return Ok(Json(messages));
            }

            if let Ok(raw_messages) = read_pi_jsonl_messages(&state, user.id(), &pi_session).await
            {
                let canon_messages: Vec<_> = raw_messages
                    .iter()
                    .map(|msg| crate::canon::pi_message_to_canon(msg, &session_id))
                    .collect();
                let messages: Vec<_> = canon_messages
                    .into_iter()
                    .map(canon_message_to_chat_message)
                    .collect();
                info!(
                    session_id = %session_id,
                    count = messages.len(),
                    "Listed chat messages from Pi JSONL fallback"
                );
                return Ok(Json(messages));
            }
        }
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

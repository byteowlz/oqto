//! Chat history handlers.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};

use crate::auth::CurrentUser;
use crate::history::{ChatMessage, ChatSession};
use crate::runner::router::{ExecutionTarget, resolve_runner_for_target};
use crate::session_target::{SessionTargetRecord, SessionTargetScope};

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
    /// If set, list sessions from this shared workspace's runner instead of personal.
    pub shared_workspace_id: Option<String>,
}

/// Check if multi-user mode is enabled (linux_users configured).
/// In multi-user mode, we must NOT fall back to direct filesystem access
/// as that would read from the backend user's home, not the requesting user's.
fn is_multi_user_mode(state: &AppState) -> bool {
    state.linux_users.is_some()
}

async fn resolve_session_target(
    state: &AppState,
    user_id: &str,
    session_id: &str,
    shared_workspace_id: Option<&str>,
    multi_user: bool,
) -> ApiResult<ExecutionTarget> {
    if let Some(workspace_id) = shared_workspace_id {
        return Ok(ExecutionTarget::SharedWorkspace {
            workspace_id: workspace_id.to_string(),
        });
    }

    if let Some(record) = state
        .session_targets
        .get(session_id)
        .await
        .map_err(|e| ApiError::internal(format!("session target lookup failed: {}", e)))?
    {
        let target = match record.scope {
            crate::session_target::SessionTargetScope::Personal => {
                if let Some(owner) = record.owner_user_id.as_deref()
                    && owner != user_id
                {
                    // In single-user mode, tolerate stale owner metadata and
                    // continue with self-heal discovery below. In multi-user
                    // mode this remains a hard authz boundary.
                    if multi_user {
                        return Err(ApiError::forbidden("session does not belong to this user"));
                    }
                }
                ExecutionTarget::Personal
            }
            crate::session_target::SessionTargetScope::SharedWorkspace => {
                let workspace_id = record.workspace_id.ok_or_else(|| {
                    ApiError::internal("shared session target missing workspace_id")
                })?;
                ExecutionTarget::SharedWorkspace { workspace_id }
            }
        };
        return Ok(target);
    }

    if multi_user {
        // Self-heal path: discover shared workspace target if canonical metadata
        // is missing for this session (e.g. legacy rows or transient gaps).
        if let Some(sw) = state.shared_workspaces.as_ref() {
            let workspaces = sw
                .list_for_user(user_id)
                .await
                .map_err(|e| ApiError::internal(format!("list shared workspaces: {}", e)))?;

            for workspace in workspaces {
                let target = ExecutionTarget::SharedWorkspace {
                    workspace_id: workspace.id.clone(),
                };
                let runner_opt = resolve_runner_for_target(state, user_id, &target)
                    .await
                    .map_err(|e| ApiError::internal(format!("runner target resolution: {}", e)))?;
                let Some(runner) = runner_opt else {
                    continue;
                };

                match runner.get_workspace_chat_session(session_id).await {
                    Ok(response) if response.session.is_some() => {
                        let workspace_path = response.session.map(|s| s.workspace_path);
                        let record = SessionTargetRecord {
                            session_id: session_id.to_string(),
                            owner_user_id: None,
                            scope: SessionTargetScope::SharedWorkspace,
                            workspace_id: Some(workspace.id.clone()),
                            workspace_path,
                        };
                        if let Err(err) = state.session_targets.upsert(&record).await {
                            tracing::warn!(
                                session_id = %session_id,
                                workspace_id = %workspace.id,
                                error = %err,
                                "failed to persist discovered shared session target"
                            );
                        }
                        return Ok(ExecutionTarget::SharedWorkspace {
                            workspace_id: workspace.id,
                        });
                    }
                    Ok(_) => {}
                    Err(err) => {
                        tracing::debug!(
                            session_id = %session_id,
                            workspace_id = %workspace.id,
                            error = %err,
                            "shared session probe failed"
                        );
                    }
                }
            }
        }

        return Err(ApiError::bad_request(format!(
            "Session target unresolved for {}: missing canonical metadata",
            session_id
        )));
    }

    Ok(ExecutionTarget::Personal)
}

/// Create a runner client for a user based on configured runner endpoint.
/// Returns the runner client if available, None for direct access.
pub(crate) fn get_runner_for_user(
    state: &AppState,
    user_id: &str,
) -> Option<crate::runner::client::RunnerClient> {
    // Need runner endpoint for multi-user mode
    let _endpoint = state.runner_endpoint.as_ref()?;

    // The socket path uses the linux_username (e.g., oqto_hansgerd-vyon),
    // not the platform user_id (e.g., hansgerd-vYoN).
    let effective_user = if let Some(ref lu) = state.linux_users {
        lu.linux_username(user_id)
    } else {
        user_id.to_string()
    };

    match state.runner_client_for_linux_user(&effective_user) {
        Ok(Some(client)) => Some(client),
        Ok(None) => None,
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

/// Create a runner client for a specific Linux username (e.g. shared workspace user).
fn get_runner_for_linux_user(
    state: &AppState,
    linux_username: &str,
) -> Option<crate::runner::client::RunnerClient> {
    match state.runner_client_for_linux_user(linux_username) {
        Ok(Some(client)) => Some(client),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(
                linux_user = %linux_username,
                error = %e,
                "Failed to create runner client for linux user"
            );
            None
        }
    }
}

fn has_non_empty_title(title: &Option<String>) -> bool {
    title.as_ref().is_some_and(|value| !value.trim().is_empty())
}

fn merge_session_metadata(existing: &mut ChatSession, incoming: &ChatSession) {
    if !has_non_empty_title(&existing.title) && has_non_empty_title(&incoming.title) {
        existing.title = incoming.title.clone();
    }

    if existing.readable_id.trim().is_empty() && !incoming.readable_id.trim().is_empty() {
        existing.readable_id = incoming.readable_id.clone();
    }
}

fn replace_session_preserving_metadata(existing: &mut ChatSession, mut incoming: ChatSession) {
    if has_non_empty_title(&existing.title) && !has_non_empty_title(&incoming.title) {
        incoming.title = existing.title.clone();
    }

    if !existing.readable_id.trim().is_empty() && incoming.readable_id.trim().is_empty() {
        incoming.readable_id = existing.readable_id.clone();
    }

    *existing = incoming;
}

fn is_oqto_session_id(session_id: &str) -> bool {
    session_id.starts_with("oqto-")
}

fn normalize_title_for_dedupe(title: &Option<String>) -> Option<String> {
    let raw = title.as_ref()?.trim();
    if raw.is_empty() {
        return None;
    }

    let mut normalized = raw.to_lowercase();
    if let Some((prefix, rest)) = normalized.split_once(':')
        && !rest.trim().is_empty()
        && prefix.len() <= 12
    {
        normalized = rest.trim().to_string();
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn merge_duplicate_sessions(mut sessions: Vec<ChatSession>) -> Vec<ChatSession> {
    // Keep newest sessions first so we can prefer the freshest metadata.
    sessions.sort_by_key(|s| Reverse(s.updated_at));

    let mut by_id: HashMap<String, ChatSession> = HashMap::new();
    let mut by_source: HashMap<String, String> = HashMap::new();
    let mut by_key: HashMap<(String, String), String> = HashMap::new();
    let mut by_readable: HashMap<String, String> = HashMap::new();
    let mut by_title_key: HashMap<(String, String), String> = HashMap::new();

    for session in sessions {
        if let Some(source) = session.source_path.clone() {
            if let Some(existing_id) = by_source.get(&source).cloned() {
                if let Some(existing) = by_id.get_mut(&existing_id) {
                    if session.updated_at > existing.updated_at {
                        replace_session_preserving_metadata(existing, session);
                    } else {
                        merge_session_metadata(existing, &session);
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

        if let Some(normalized_title) = normalize_title_for_dedupe(&session.title) {
            let title_key = (normalized_workspace.clone(), normalized_title);
            if let Some(existing_id) = by_title_key.get(&title_key).cloned()
                && existing_id != session.id
                && let Some(existing) = by_id.get_mut(&existing_id)
            {
                let existing_is_oqto = is_oqto_session_id(&existing.id);
                let incoming_is_oqto = is_oqto_session_id(&session.id);
                if existing_is_oqto != incoming_is_oqto {
                    if incoming_is_oqto {
                        merge_session_metadata(existing, &session);
                        if session.updated_at > existing.updated_at {
                            existing.updated_at = session.updated_at;
                        }
                    } else {
                        replace_session_preserving_metadata(existing, session);
                    }
                    continue;
                }
            } else {
                by_title_key.insert(title_key, session.id.clone());
            }
        }

        if !readable.is_empty() {
            let key = (normalized_workspace.clone(), readable.clone());
            if let Some(existing_id) = by_key.get(&key).cloned() {
                if let Some(existing) = by_id.get_mut(&existing_id) {
                    merge_session_metadata(existing, &session);
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
                        replace_session_preserving_metadata(existing, session);
                    } else {
                        merge_session_metadata(existing, &session);
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
    merged.sort_by_key(|s| Reverse(s.updated_at));
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
    let target = if let Some(ref sw_id) = query.shared_workspace_id {
        ExecutionTarget::SharedWorkspace {
            workspace_id: sw_id.clone(),
        }
    } else {
        ExecutionTarget::Personal
    };

    let runner = resolve_runner_for_target(&state, user.id(), &target)
        .await
        .map_err(|e| ApiError::internal(format!("runner target resolution: {}", e)))?
        .ok_or_else(|| ApiError::internal("Runner is required but not available for this user."))?;

    let mut response = runner
        .list_workspace_chat_sessions(query.workspace.clone(), query.include_children, query.limit)
        .await
        .map_err(|e| ApiError::internal(format!("runner list sessions failed: {}", e)))?;

    // Auto-repair can be expensive on large workspaces. Only trigger it when
    // the initial list is empty for unfiltered queries.
    if query.workspace.is_none()
        && response.sessions.is_empty()
        && runner
            .repair_workspace_chat_history(Some(10_000), None)
            .await
            .is_ok()
        && let Ok(repaired_response) = runner
            .list_workspace_chat_sessions(
                query.workspace.clone(),
                query.include_children,
                query.limit,
            )
            .await
    {
        response = repaired_response;
    }

    let mut sessions: Vec<ChatSession> = response
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

    sessions = merge_duplicate_sessions(sessions);

    let mut seen = HashSet::new();
    sessions.retain(|session| seen.insert(session.id.clone()));

    if let Some(ref ws) = query.workspace {
        sessions.retain(|s| s.workspace_path == *ws);
    }

    if !query.include_children {
        sessions.retain(|s| !s.is_child);
    }

    sessions.sort_by_key(|s| Reverse(s.updated_at));

    if let Some(limit) = query.limit {
        sessions.truncate(limit);
    }

    debug!(user_id = %user.id(), count = sessions.len(), source = "runner", "Listed chat history");
    Ok(Json(sessions))
}

#[derive(Debug, Deserialize)]
pub struct BackfillChatHistoryQuery {
    /// Optional workspace/workdir path to backfill.
    pub workspace: Option<String>,
    /// If set, run backfill on this shared workspace runner.
    pub shared_workspace_id: Option<String>,
    /// Optional upper bound of scanned session files.
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct BackfillChatHistoryResponse {
    pub scanned_files: usize,
    pub repaired_conversations: usize,
    pub skipped_files: usize,
    pub failed_files: usize,
}

/// Trigger JSONL->hstry backfill for personal or shared-workspace sessions.
#[instrument(skip(state))]
pub async fn backfill_chat_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<BackfillChatHistoryQuery>,
) -> ApiResult<Json<BackfillChatHistoryResponse>> {
    let target = if let Some(ref sw_id) = query.shared_workspace_id {
        ExecutionTarget::SharedWorkspace {
            workspace_id: sw_id.clone(),
        }
    } else {
        ExecutionTarget::Personal
    };

    let runner = resolve_runner_for_target(&state, user.id(), &target)
        .await
        .map_err(|e| ApiError::internal(format!("runner target resolution: {}", e)))?
        .ok_or_else(|| ApiError::internal("Runner is required but not available for this user."))?;

    let limit = query.limit.or(Some(10_000));
    let result = runner
        .repair_workspace_chat_history(limit, query.workspace.clone())
        .await
        .map_err(|e| ApiError::internal(format!("backfill failed: {}", e)))?;

    Ok(Json(BackfillChatHistoryResponse {
        scanned_files: result.scanned_files,
        repaired_conversations: result.repaired_conversations,
        skipped_files: result.skipped_files,
        failed_files: result.failed_files,
    }))
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
    let target = resolve_session_target(
        &state,
        user.id(),
        &session_id,
        None,
        is_multi_user_mode(&state),
    )
    .await?;
    let runner = resolve_runner_for_target(&state, user.id(), &target)
        .await
        .map_err(|e| ApiError::internal(format!("runner target resolution: {}", e)))?
        .ok_or_else(|| ApiError::internal("Runner is required but not available for this user."))?;

    let response = runner
        .get_workspace_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::internal(format!("runner get session failed: {}", e)))?;

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

    Err(ApiError::not_found(format!(
        "Chat session {} not found",
        session_id
    )))
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
    Query(query): Query<ChatHistoryQuery>,
    Json(request): Json<UpdateChatSessionRequest>,
) -> ApiResult<Json<ChatSession>> {
    let target = resolve_session_target(
        &state,
        user.id(),
        &session_id,
        query.shared_workspace_id.as_deref(),
        is_multi_user_mode(&state),
    )
    .await?;

    let runner = resolve_runner_for_target(&state, user.id(), &target)
        .await
        .map_err(|e| ApiError::internal(format!("runner target resolution: {}", e)))?
        .ok_or_else(|| ApiError::internal("Runner is required but not available for this user."))?;

    let response = runner
        .update_workspace_chat_session(&session_id, request.title.clone())
        .await
        .map_err(|e| ApiError::internal(format!("runner update session failed: {}", e)))?;

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
    Ok(Json(session))
}

/// Query parameters for deleting chat sessions.
#[derive(Debug, Deserialize)]
pub struct DeleteChatSessionQuery {
    /// If set, route delete to this shared workspace's runner.
    pub shared_workspace_id: Option<String>,
}

/// Delete a chat session from hstry.
///
/// Removes the conversation and all its messages from the history database.
/// In multi-user mode, routes through the runner for user isolation.
#[instrument(skip(state))]
pub async fn delete_chat_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<DeleteChatSessionQuery>,
) -> ApiResult<StatusCode> {
    let target = resolve_session_target(
        &state,
        user.id(),
        &session_id,
        query.shared_workspace_id.as_deref(),
        is_multi_user_mode(&state),
    )
    .await?;

    let runner = resolve_runner_for_target(&state, user.id(), &target)
        .await
        .map_err(|e| ApiError::internal(format!("runner target resolution: {}", e)))?
        .ok_or_else(|| ApiError::internal("Runner is required but not available for this user."))?;

    runner
        .agent_delete_session(&session_id)
        .await
        .map_err(|e| ApiError::internal(format!("runner delete session failed: {}", e)))?;

    state
        .session_targets
        .delete(&session_id)
        .await
        .map_err(|e| {
            ApiError::internal(format!("failed to delete session target metadata: {}", e))
        })?;

    info!(session_id = %session_id, shared_workspace_id = ?query.shared_workspace_id, "Deleted chat session via runner");
    Ok(StatusCode::NO_CONTENT)
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
    let runner = get_runner_for_user(&state, user.id())
        .ok_or_else(|| ApiError::internal("Runner is required but not available for this user."))?;

    let response = runner
        .list_workspace_chat_sessions(query.workspace.clone(), query.include_children, query.limit)
        .await
        .map_err(|e| ApiError::internal(format!("runner grouped list failed: {}", e)))?;

    let mut sessions: Vec<ChatSession> = response
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
            sessions.sort_by_key(|s| Reverse(s.updated_at));
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

    result.sort_by_key(|g| Reverse(g.sessions.first().map(|s| s.updated_at).unwrap_or(0)));

    debug!(user_id = %user.id(), count = result.len(), source = "runner", "Listed grouped chat history");
    Ok(Json(result))
}

/// Query parameters for chat messages endpoint.
#[derive(Debug, Deserialize)]
pub struct ChatMessagesQuery {
    /// If true, include pre-rendered HTML for text parts (slower but saves client CPU)
    #[serde(default)]
    pub render: bool,
    /// If set, route the request to the shared workspace's runner instead of the personal runner.
    pub shared_workspace_id: Option<String>,
}

/// Convert a runner chat messages response to canonical format.
fn convert_runner_response(
    response: crate::runner::protocol::WorkspaceChatSessionMessagesResponse,
) -> Vec<oqto_protocol::messages::Message> {
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
                    tool_call_id: p.tool_call_id,
                    tool_input: p.tool_input,
                    tool_output: p.tool_output,
                    tool_status: p.tool_status,
                    tool_title: p.tool_title,
                })
                .collect(),
        })
        .collect();
    crate::history::legacy_messages_to_canon(messages)
}

/// Get all messages for a chat session.
///
/// Get all messages for a chat session via hstry.
///
/// Query params:
/// - `render=true`: Include pre-rendered markdown HTML in `text_html` field
/// - `shared_workspace_id`: Route to the shared workspace's runner
///
/// SECURITY: In multi-user mode, we MUST use the runner to ensure user isolation.
/// When shared_workspace_id is set, we verify the user is a member before routing.
#[instrument(skip(state))]
pub async fn get_chat_messages(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<ChatMessagesQuery>,
) -> ApiResult<Json<Vec<oqto_protocol::messages::Message>>> {
    let target = resolve_session_target(
        &state,
        user.id(),
        &session_id,
        query.shared_workspace_id.as_deref(),
        is_multi_user_mode(&state),
    )
    .await?;

    let runner = resolve_runner_for_target(&state, user.id(), &target)
        .await
        .map_err(|e| ApiError::internal(format!("runner target resolution: {}", e)))?
        .ok_or_else(|| ApiError::internal("Runner is required but not available for this user."))?;

    let response = runner
        .get_workspace_chat_session_messages(&session_id, query.render, None)
        .await
        .map_err(|e| ApiError::internal(format!("runner get messages failed: {}", e)))?;

    let mut canonical = convert_runner_response(response);
    if canonical.is_empty() {
        // Keep fallback bounded to avoid long blocking requests on large
        // workspaces when a single session is missing from hstry.
        if let Err(err) = runner.repair_workspace_chat_history(Some(500), None).await {
            tracing::debug!(
                user_id = %user.id(),
                session_id = %session_id,
                error = %err,
                "workspace chat history repair before get_chat_messages failed"
            );
        } else if let Ok(repaired_response) = runner
            .get_workspace_chat_session_messages(&session_id, query.render, None)
            .await
        {
            canonical = convert_runner_response(repaired_response);
        }
    }

    info!(
        user_id = %user.id(),
        session_id = %session_id,
        shared_workspace_id = ?query.shared_workspace_id,
        count = canonical.len(),
        "Listed chat messages via runner"
    );
    Ok(Json(canonical))
}

#[cfg(test)]
mod tests {
    use super::merge_duplicate_sessions;
    use crate::history::ChatSession;

    fn build_session(id: &str, title: Option<&str>, updated_at: i64) -> ChatSession {
        ChatSession {
            id: id.to_string(),
            readable_id: "same-readable".to_string(),
            title: title.map(str::to_string),
            parent_id: None,
            workspace_path: "/tmp/workspace".to_string(),
            project_name: "workspace".to_string(),
            created_at: 1,
            updated_at,
            version: None,
            is_child: false,
            source_path: None,
            stats: None,
            model: None,
            provider: None,
        }
    }

    fn build_session_with_readable(
        id: &str,
        readable_id: &str,
        title: Option<&str>,
        updated_at: i64,
    ) -> ChatSession {
        ChatSession {
            readable_id: readable_id.to_string(),
            ..build_session(id, title, updated_at)
        }
    }

    #[test]
    fn merge_duplicate_sessions_preserves_non_empty_title_when_newer_record_is_blank() {
        let older_with_title = build_session("ses_1", Some("Stable title"), 100);
        let newer_blank_title = build_session("ses_2", Some("   "), 200);

        let merged = merge_duplicate_sessions(vec![older_with_title, newer_blank_title]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].title.as_deref(), Some("Stable title"));
        assert_eq!(merged[0].updated_at, 200);
    }

    #[test]
    fn merge_duplicate_sessions_upgrades_missing_title_from_older_duplicate() {
        let older_with_title = build_session("ses_1", Some("Recovered title"), 100);
        let newer_without_title = build_session("ses_2", None, 200);

        let merged = merge_duplicate_sessions(vec![older_with_title, newer_without_title]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].title.as_deref(), Some("Recovered title"));
    }

    #[test]
    fn merge_duplicate_sessions_prefers_non_oqto_id_for_same_title_workspace() {
        let ghost_oqto = build_session_with_readable(
            "oqto-c8f1b697-b5d4-48e4-b20c-8d407a3d2970",
            "",
            Some("Forschung und Entwicklung grundlegender KI-Methoden"),
            100,
        );
        let real_pi = build_session_with_readable(
            "c2d30f2d-a509-4da4-9f0b-539b4539e7b8",
            "eager-tests-junction",
            Some("BMFTR: Forschung und Entwicklung grundlegender KI-Methoden"),
            200,
        );

        let merged = merge_duplicate_sessions(vec![ghost_oqto, real_pi]);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].id,
            "c2d30f2d-a509-4da4-9f0b-539b4539e7b8".to_string()
        );
        assert_eq!(merged[0].readable_id, "eager-tests-junction".to_string());
    }
}

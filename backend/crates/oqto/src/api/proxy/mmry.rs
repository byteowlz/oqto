//! Workspace memory handlers.
//!
//! Memory is embedded `mmry-core` (ADR-0010): each workspace's
//! `<workspace>/.mmry/mmry.jsonl` is read/written directly. There is no mmry
//! daemon, port, or HTTP proxy.

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{Request, Response, StatusCode},
};
use log::error;
use mmry_core::agent_ctx::AgentCtx;
use mmry_core::memory::MemoryType;
use mmry_core::memory_file::{MemoryEntry, MemoryEvent, MemoryFile};

use crate::auth::CurrentUser;
use crate::runner::router::{ExecutionTarget, resolve_target_for_workspace_path};

use super::super::state::AppState;
use super::handlers::WorkspaceProxyQuery;

/// Authorize workspace access and resolve the owning user id.
///
/// Shared workspace paths resolve to the shared workspace's linux user so that
/// frontend memory views align with in-workspace `agntz memory` usage.
async fn resolve_workspace_owner_user_id(
    state: &AppState,
    user: &CurrentUser,
    workspace_path: &str,
) -> Result<String, StatusCode> {
    match resolve_target_for_workspace_path(state, user.id(), workspace_path)
        .await
        .map_err(|e| {
            error!(
                "Failed to resolve execution target for workspace {} and user {}: {:?}",
                workspace_path,
                user.id(),
                e
            );
            StatusCode::SERVICE_UNAVAILABLE
        })? {
        ExecutionTarget::Personal => Ok(user.id().to_string()),
        ExecutionTarget::SharedWorkspace { workspace_id } => {
            let sw = state.shared_workspaces.as_ref().ok_or_else(|| {
                error!(
                    "Shared workspace service not configured while resolving mmry target for {}",
                    workspace_path
                );
                StatusCode::SERVICE_UNAVAILABLE
            })?;

            sw.linux_user_for_id(&workspace_id)
                .await
                .map_err(|e| {
                    error!(
                        "Failed to resolve linux user for shared workspace {} (path {}): {:?}",
                        workspace_id, workspace_path, e
                    );
                    StatusCode::SERVICE_UNAVAILABLE
                })?
                .ok_or_else(|| {
                    error!(
                        "Missing linux user mapping for shared workspace {} (path {})",
                        workspace_id, workspace_path
                    );
                    StatusCode::SERVICE_UNAVAILABLE
                })
        }
    }
}

// ============================================================================
// Workspace-based memory handlers (mmry-core JSONL)
// ============================================================================

#[derive(Debug, serde::Deserialize)]
pub struct WorkspaceMemoryListQuery {
    workspace_path: String,
    #[allow(dead_code)]
    store: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WorkspaceMemoryCreateRequest {
    content: Option<String>,
    text: Option<String>,
    memory: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    importance: Option<i32>,
    memory_type: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WorkspaceMemoryUpdateRequest {
    content: Option<String>,
    text: Option<String>,
    memory: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    importance: Option<i32>,
    memory_type: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WorkspaceMemorySearchRequest {
    query: String,
    limit: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct WorkspaceMemoryListResponse {
    memories: Vec<WorkspaceMemoryDto>,
    total: i64,
    offset: i64,
    limit: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct WorkspaceMemoryCreateResponse {
    memory: WorkspaceMemoryDto,
}

#[derive(Debug, serde::Serialize)]
pub struct WorkspaceMemoryDeleteResponse {
    deleted: bool,
    id: String,
}

#[derive(Debug, serde::Serialize)]
pub struct WorkspaceMemorySearchResponse {
    memories: Vec<WorkspaceMemoryDto>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct WorkspaceMemoryDto {
    id: String,
    memory_type: String,
    content: String,
    metadata: serde_json::Value,
    importance: i32,
    created_at: String,
    updated_at: String,
    category: String,
    tags: Vec<String>,
}

fn dto_from_entry(entry: MemoryEntry) -> WorkspaceMemoryDto {
    let category = entry
        .metadata
        .get("category")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("general")
        .to_string();
    let importance = entry
        .metadata
        .get("importance")
        .and_then(serde_json::Value::as_i64)
        .map(|v| v as i32)
        .unwrap_or(5);

    WorkspaceMemoryDto {
        id: entry.memory_id,
        memory_type: format!("{:?}", entry.memory_type).to_lowercase(),
        content: entry.content,
        metadata: entry.metadata,
        importance,
        created_at: entry.created_at.to_rfc3339(),
        updated_at: entry.updated_at.to_rfc3339(),
        category,
        tags: entry.tags,
    }
}

fn resolve_content(
    content: Option<String>,
    text: Option<String>,
    memory: Option<String>,
) -> Option<String> {
    content
        .filter(|s| !s.trim().is_empty())
        .or_else(|| text.filter(|s| !s.trim().is_empty()))
        .or_else(|| memory.filter(|s| !s.trim().is_empty()))
}

fn parse_memory_type(memory_type: Option<String>) -> MemoryType {
    match memory_type
        .as_deref()
        .unwrap_or("semantic")
        .to_lowercase()
        .as_str()
    {
        "episodic" => MemoryType::Episodic,
        "procedural" => MemoryType::Procedural,
        _ => MemoryType::Semantic,
    }
}

fn memory_file_for_workspace(workspace_path: &str) -> Result<MemoryFile, StatusCode> {
    if workspace_path.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let memory_file = MemoryFile::open_workspace(workspace_path);
    memory_file.init(false).map_err(|e| {
        error!("Failed to initialize workspace memory file: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(memory_file)
}

/// List memories for a workspace (JSONL memory file).
pub async fn proxy_mmry_list_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceMemoryListQuery>,
) -> Result<Json<WorkspaceMemoryListResponse>, StatusCode> {
    let _ = resolve_workspace_owner_user_id(&state, &user, &query.workspace_path).await?;
    let memory_file = memory_file_for_workspace(&query.workspace_path)?;
    let mut memories = memory_file.active_memories().map_err(|e| {
        error!("Failed to read workspace memories: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let total = memories.len() as i64;
    let offset = query.offset.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(50).clamp(1, 100) as usize;

    memories = memories.into_iter().skip(offset).take(limit).collect();

    Ok(Json(WorkspaceMemoryListResponse {
        memories: memories.into_iter().map(dto_from_entry).collect(),
        total,
        offset: offset as i64,
        limit: limit as i64,
    }))
}

/// Add a memory for a workspace (append-only event).
pub async fn proxy_mmry_add_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    Json(payload): Json<WorkspaceMemoryCreateRequest>,
) -> Result<Json<WorkspaceMemoryCreateResponse>, StatusCode> {
    let _ = resolve_workspace_owner_user_id(&state, &user, &query.workspace_path).await?;
    let memory_file = memory_file_for_workspace(&query.workspace_path)?;

    let content = resolve_content(payload.content, payload.text, payload.memory)
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut event = MemoryEvent::add(
        content,
        parse_memory_type(payload.memory_type),
        payload.tags.unwrap_or_default(),
        &AgentCtx::from_env(),
    );
    if let Some(category) = payload.category {
        event.metadata["category"] = serde_json::Value::String(category);
    }
    if let Some(importance) = payload.importance {
        event.metadata["importance"] = serde_json::Value::Number(importance.into());
    }

    memory_file.append(&event).map_err(|e| {
        error!("Failed to append workspace memory event: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let created = memory_file
        .active_memories()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|m| m.memory_id == event.memory_id)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(WorkspaceMemoryCreateResponse {
        memory: dto_from_entry(created),
    }))
}

/// Search memories in a workspace.
pub async fn proxy_mmry_search_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    Json(payload): Json<WorkspaceMemorySearchRequest>,
) -> Result<Json<WorkspaceMemorySearchResponse>, StatusCode> {
    let _ = resolve_workspace_owner_user_id(&state, &user, &query.workspace_path).await?;
    let memory_file = memory_file_for_workspace(&query.workspace_path)?;

    let hits = memory_file
        .search(&payload.query, payload.limit.unwrap_or(50))
        .map_err(|e| {
            error!("Failed to search workspace memories: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(WorkspaceMemorySearchResponse {
        memories: hits.into_iter().map(|h| dto_from_entry(h.memory)).collect(),
    }))
}

/// Get a single memory by id in workspace.
async fn get_workspace_memory_by_id(
    memory_file: &MemoryFile,
    memory_id: &str,
) -> Result<WorkspaceMemoryDto, StatusCode> {
    let entry = memory_file
        .active_memories()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|m| m.memory_id == memory_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(dto_from_entry(entry))
}

/// Get/update/delete a specific memory for a workspace.
pub async fn proxy_mmry_memory_for_workspace(
    State(state): State<AppState>,
    Path(memory_id): Path<String>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let _ = resolve_workspace_owner_user_id(&state, &user, &query.workspace_path).await?;
    let memory_file = memory_file_for_workspace(&query.workspace_path)?;

    match *req.method() {
        axum::http::Method::GET => {
            let memory = get_workspace_memory_by_id(&memory_file, &memory_id).await?;
            let body =
                serde_json::to_vec(&memory).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
        axum::http::Method::DELETE => {
            let event = MemoryEvent::deprecate(memory_id.clone(), &AgentCtx::from_env());
            memory_file
                .append(&event)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let body = serde_json::to_vec(&WorkspaceMemoryDeleteResponse {
                deleted: true,
                id: memory_id,
            })
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
        axum::http::Method::PUT => {
            let bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let payload: WorkspaceMemoryUpdateRequest =
                serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;

            // Deprecate old + add replacement
            let dep = MemoryEvent::deprecate(memory_id, &AgentCtx::from_env());
            memory_file
                .append(&dep)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let content = resolve_content(payload.content, payload.text, payload.memory)
                .ok_or(StatusCode::BAD_REQUEST)?;
            let mut add = MemoryEvent::add(
                content,
                parse_memory_type(payload.memory_type),
                payload.tags.unwrap_or_default(),
                &AgentCtx::from_env(),
            );
            if let Some(category) = payload.category {
                add.metadata["category"] = serde_json::Value::String(category);
            }
            if let Some(importance) = payload.importance {
                add.metadata["importance"] = serde_json::Value::Number(importance.into());
            }
            memory_file
                .append(&add)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let created = memory_file
                .active_memories()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .into_iter()
                .find(|m| m.memory_id == add.memory_id)
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            let body = serde_json::to_vec(&WorkspaceMemoryCreateResponse {
                memory: dto_from_entry(created),
            })
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
        _ => Err(StatusCode::METHOD_NOT_ALLOWED),
    }
}

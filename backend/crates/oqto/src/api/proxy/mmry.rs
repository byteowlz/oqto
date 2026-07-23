//! Workspace memory handlers.
//!
//! Memory is a per-workspace `.mmry/mmry.jsonl` ledger (ADR-0010) served
//! through the per-user/shared oqto-runner (ADR-0026): the backend resolves the
//! workspace's runner and calls it, never opening the ledger in-process. This
//! keeps the runner isolation seam intact and works in container mode.

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{Request, Response, StatusCode},
};
use log::error;

use crate::auth::CurrentUser;
use crate::runner::router::resolve_runner_for_workspace_path;
use oqto_runner::client::RunnerClient;
use oqto_runner::protocol::MemoryEntry;

use super::super::state::AppState;
use super::handlers::WorkspaceProxyQuery;

/// Resolve the runner that owns `workspace_path` for the requesting user.
///
/// Authorizes workspace access (personal or shared) and returns the runner the
/// memory operation must be executed on.
async fn runner_for_workspace(
    state: &AppState,
    user: &CurrentUser,
    workspace_path: &str,
) -> Result<RunnerClient, StatusCode> {
    resolve_runner_for_workspace_path(state, user.id(), workspace_path)
        .await
        .map_err(|e| {
            error!(
                "Failed to resolve runner for workspace {} (user {}): {:?}",
                workspace_path,
                user.id(),
                e
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?
        .ok_or_else(|| {
            error!(
                "No runner available for workspace {} (user {})",
                workspace_path,
                user.id()
            );
            StatusCode::SERVICE_UNAVAILABLE
        })
}

fn runner_error(context: &str, e: impl std::fmt::Debug) -> StatusCode {
    error!("{context}: {e:?}");
    StatusCode::INTERNAL_SERVER_ERROR
}

// ============================================================================
// Workspace-based memory handlers
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
    WorkspaceMemoryDto {
        id: entry.id,
        memory_type: entry.memory_type,
        content: entry.content,
        importance: entry.importance.map(i32::from).unwrap_or(5),
        category: entry.category.unwrap_or_else(|| "general".to_string()),
        tags: entry.tags,
        metadata: entry.metadata,
        created_at: entry.created_at,
        updated_at: entry.updated_at,
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

fn importance_u8(importance: Option<i32>) -> Option<u8> {
    importance.map(|v| v.clamp(0, 255) as u8)
}

/// List memories for a workspace.
pub async fn proxy_mmry_list_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceMemoryListQuery>,
) -> Result<Json<WorkspaceMemoryListResponse>, StatusCode> {
    let runner = runner_for_workspace(&state, &user, &query.workspace_path).await?;
    let offset = query.offset.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(50).clamp(1, 100);

    let resp = runner
        .list_memories(
            query.workspace_path.clone(),
            limit as usize,
            offset as usize,
        )
        .await
        .map_err(|e| runner_error("runner list_memories", e))?;

    Ok(Json(WorkspaceMemoryListResponse {
        total: resp.total as i64,
        offset: resp.offset as i64,
        limit: resp.limit as i64,
        memories: resp.memories.into_iter().map(dto_from_entry).collect(),
    }))
}

/// Add a memory for a workspace.
pub async fn proxy_mmry_add_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    Json(payload): Json<WorkspaceMemoryCreateRequest>,
) -> Result<Json<WorkspaceMemoryCreateResponse>, StatusCode> {
    let runner = runner_for_workspace(&state, &user, &query.workspace_path).await?;
    let content = resolve_content(payload.content, payload.text, payload.memory)
        .ok_or(StatusCode::BAD_REQUEST)?;

    let resp = runner
        .add_memory(
            query.workspace_path.clone(),
            content,
            payload.category,
            importance_u8(payload.importance),
            payload.memory_type,
            payload.tags.unwrap_or_default(),
        )
        .await
        .map_err(|e| runner_error("runner add_memory", e))?;

    Ok(Json(WorkspaceMemoryCreateResponse {
        memory: dto_from_entry(resp.memory),
    }))
}

/// Search memories in a workspace.
pub async fn proxy_mmry_search_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    Json(payload): Json<WorkspaceMemorySearchRequest>,
) -> Result<Json<WorkspaceMemorySearchResponse>, StatusCode> {
    let runner = runner_for_workspace(&state, &user, &query.workspace_path).await?;

    let resp = runner
        .search_memories(
            query.workspace_path.clone(),
            payload.query,
            payload.limit.unwrap_or(50),
            None,
        )
        .await
        .map_err(|e| runner_error("runner search_memories", e))?;

    Ok(Json(WorkspaceMemorySearchResponse {
        memories: resp.memories.into_iter().map(dto_from_entry).collect(),
    }))
}

/// Get/update/delete a specific memory for a workspace.
pub async fn proxy_mmry_memory_for_workspace(
    State(state): State<AppState>,
    Path(memory_id): Path<String>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let runner = runner_for_workspace(&state, &user, &query.workspace_path).await?;

    match *req.method() {
        axum::http::Method::GET => {
            // Fetch the current entry by scanning the workspace ledger.
            let list = runner
                .list_memories(query.workspace_path.clone(), 100_000, 0)
                .await
                .map_err(|e| runner_error("runner list_memories", e))?;
            let entry = list
                .memories
                .into_iter()
                .find(|m| m.id == memory_id)
                .ok_or(StatusCode::NOT_FOUND)?;
            json_ok(&dto_from_entry(entry))
        }
        axum::http::Method::DELETE => {
            runner
                .delete_memory(query.workspace_path.clone(), memory_id.clone())
                .await
                .map_err(|e| runner_error("runner delete_memory", e))?;
            json_ok(&WorkspaceMemoryDeleteResponse {
                deleted: true,
                id: memory_id,
            })
        }
        axum::http::Method::PUT => {
            let bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let payload: WorkspaceMemoryUpdateRequest =
                serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
            let content = resolve_content(payload.content, payload.text, payload.memory)
                .ok_or(StatusCode::BAD_REQUEST)?;

            let resp = runner
                .update_memory(
                    query.workspace_path.clone(),
                    memory_id,
                    content,
                    payload.category,
                    importance_u8(payload.importance),
                    payload.memory_type,
                    payload.tags.unwrap_or_default(),
                )
                .await
                .map_err(|e| runner_error("runner update_memory", e))?;

            json_ok(&WorkspaceMemoryCreateResponse {
                memory: dto_from_entry(resp.memory),
            })
        }
        _ => Err(StatusCode::METHOD_NOT_ALLOWED),
    }
}

fn json_ok<T: serde::Serialize>(value: &T) -> Result<Response<Body>, StatusCode> {
    let body = serde_json::to_vec(value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

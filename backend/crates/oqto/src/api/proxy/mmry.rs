//! Mmry (Memory Service) proxy handlers.
//!
//! Handles proxying requests to per-session or shared mmry instances.

use std::path::{Path as FsPath, PathBuf};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{Request, Response, StatusCode, Uri},
};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use log::{debug, error, warn};

use crate::auth::CurrentUser;
use crate::runner::router::{ExecutionTarget, resolve_target_for_workspace_path};
use crate::session::Session;

use super::super::state::AppState;
use super::builder::{build_mmry_query, get_session_by_id};
use super::handlers::WorkspaceProxyQuery;

// ============================================================================
// Mmry Target Resolution
// ============================================================================

/// Get the mmry target URL for a session.
///
/// In single-user mode, returns the local service URL.
/// In multi-user mode, returns the session's mmry port URL.
fn get_mmry_target(state: &AppState, session: &Session) -> Result<String, StatusCode> {
    if !state.mmry.enabled {
        warn!("mmry integration is not enabled");
        return Err(StatusCode::NOT_FOUND);
    }

    if state.mmry.single_user {
        // Single-user mode: proxy to local mmry service
        Ok(state.mmry.local_service_url.clone())
    } else {
        // Multi-user mode: proxy to session's mmry port
        let port = session.mmry_port.ok_or_else(|| {
            warn!("Session {} does not have mmry enabled", session.id);
            StatusCode::NOT_FOUND
        })?;
        Ok(format!("http://localhost:{}", port))
    }
}

/// Get the mmry target URL for workspace-based access.
///
/// Uses deterministic workspace routing so shared workspace paths resolve to the
/// shared workspace linux user's mmry instance (not the requesting user's personal
/// mmry instance). This keeps frontend memory views aligned with in-workspace
/// `agntz memory` usage.
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

async fn get_mmry_target_for_workspace(
    state: &AppState,
    user: &CurrentUser,
    workspace_path: &str,
) -> Result<String, StatusCode> {
    if !state.mmry.enabled {
        warn!("mmry integration is not enabled");
        return Err(StatusCode::NOT_FOUND);
    }

    // Authorize workspace access and deterministically resolve owner without
    // forcing an IO session resume/start (which can add latency/fail if ports are busy).
    let owner_user_id = resolve_workspace_owner_user_id(state, user, workspace_path).await?;

    if state.mmry.single_user {
        return Ok(state.mmry.local_service_url.clone());
    }

    let mmry_port = state
        .sessions
        .for_user(&owner_user_id)
        .ensure_user_mmry_pinned()
        .await
        .map_err(|e| {
            error!(
                "Failed to ensure per-user mmry for workspace {} (owner {}): {:?}",
                workspace_path, owner_user_id, e
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    Ok(format!("http://localhost:{}", mmry_port))
}

/// Derive mmry store name from session workspace path.
///
/// We always derive a workspace-scoped store name from the session path,
/// regardless of runtime mode. This keeps memory access consistent between:
/// - frontend session mmry proxy routes, and
/// - in-session `agntz memory` calls (which auto-select repo/workspace stores).
///
/// Example: `/home/user/byteowlz/oqto` -> `oqto`
fn get_mmry_store_name(session: &Session) -> Option<String> {
    get_mmry_store_name_from_path(&session.workspace_path)
}

/// Derive mmry store name directly from a workspace path.
///
/// Resolution order:
/// 1. Git remote repository name (origin URL basename without `.git`), if available.
/// 2. Workspace directory basename as fallback.
fn get_mmry_store_name_from_path(workspace_path: &str) -> Option<String> {
    let trimmed = workspace_path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let workspace = FsPath::new(trimmed);
    infer_git_remote_store_name(workspace).or_else(|| {
        workspace
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
    })
}

fn infer_git_remote_store_name(workspace: &FsPath) -> Option<String> {
    let git_dir = find_git_dir(workspace)?;
    let config_path = git_dir.join("config");
    let config = std::fs::read_to_string(config_path).ok()?;
    parse_origin_remote_repo_name(&config)
}

fn find_git_dir(workspace: &FsPath) -> Option<PathBuf> {
    let mut current = Some(workspace);

    while let Some(path) = current {
        let dot_git = path.join(".git");

        if dot_git.is_dir() {
            return Some(dot_git);
        }

        if dot_git.is_file() {
            return resolve_git_file_indirection(path, &dot_git);
        }

        current = path.parent();
    }

    None
}

fn resolve_git_file_indirection(worktree_path: &FsPath, dot_git_file: &FsPath) -> Option<PathBuf> {
    let content = std::fs::read_to_string(dot_git_file).ok()?;
    let gitdir_value = content
        .lines()
        .find_map(|line| line.strip_prefix("gitdir:"))?
        .trim();

    if gitdir_value.is_empty() {
        return None;
    }

    let gitdir = PathBuf::from(gitdir_value);
    if gitdir.is_absolute() {
        Some(gitdir)
    } else {
        Some(worktree_path.join(gitdir))
    }
}

fn parse_origin_remote_repo_name(config: &str) -> Option<String> {
    let mut in_origin_section = false;

    for line in config.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_origin_section = trimmed == "[remote \"origin\"]";
            continue;
        }

        if !in_origin_section {
            continue;
        }

        if let Some(url) = trimmed.strip_prefix("url") {
            let url = url.trim_start_matches([' ', '=']).trim();
            if let Some(repo) = repo_name_from_remote_url(url) {
                return Some(repo);
            }
        }
    }

    None
}

fn repo_name_from_remote_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let last_segment = trimmed.rsplit(['/', ':']).next()?.trim();
    let repo = last_segment.strip_suffix(".git").unwrap_or(last_segment);

    if repo.is_empty() {
        None
    } else {
        Some(repo.to_string())
    }
}

fn resolve_mmry_store_for_workspace(query: &WorkspaceProxyQuery) -> Option<String> {
    if let Some(store) = query.store.as_ref().and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }) {
        return Some(store);
    }
    get_mmry_store_name_from_path(&query.workspace_path)
}

async fn resolve_mmry_session_target(
    state: &AppState,
    session_id: &str,
) -> Result<(String, Option<String>), StatusCode> {
    let session = get_session_by_id(state, session_id).await?;

    // In single-user mode, allow access even when session is inactive
    // since we're proxying to a shared local mmry service
    if !state.mmry.single_user && !session.is_active() {
        warn!("Attempted to proxy mmry to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = get_mmry_target(state, &session)?;
    let store = get_mmry_store_name(&session);
    Ok((target_url, store))
}

// ============================================================================
// Mmry Request Forwarding
// ============================================================================

/// Proxy request to a URL-based target with optional store parameter.
async fn proxy_request_to_url(
    client: Client<HttpConnector, Body>,
    mut req: Request<Body>,
    target_base_url: &str,
    target_path: &str,
    store: Option<&str>,
) -> Result<Response<Body>, StatusCode> {
    let query = req.uri().query().unwrap_or("");
    let mut target_uri = format!("{}/{}", target_base_url.trim_end_matches('/'), target_path);

    // Build query string with optional store parameter
    let has_query = !query.is_empty();
    let has_store = store.is_some();

    if has_query || has_store {
        target_uri.push('?');
        if has_query {
            target_uri.push_str(query);
        }
        if let Some(store_name) = store {
            if has_query {
                target_uri.push('&');
            }
            target_uri.push_str("store=");
            target_uri.push_str(store_name);
        }
    }

    debug!("Proxying mmry request to {}", target_uri);

    let uri: Uri = target_uri.parse().map_err(|e| {
        error!("Invalid target URI {}: {:?}", target_uri, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Update the request URI
    *req.uri_mut() = uri;

    // Ensure Host header matches the target authority.
    if let Some(authority) = req.uri().authority() {
        let value = axum::http::HeaderValue::from_str(authority.as_str()).map_err(|e| {
            error!("Invalid Host header value {}: {:?}", authority.as_str(), e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        req.headers_mut().insert(axum::http::header::HOST, value);
    }

    // Forward the request
    let response = client.request(req).await.map_err(|e| {
        error!("Mmry proxy request failed: {:?}", e);
        if e.is_connect() {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::BAD_GATEWAY
        }
    })?;

    // Convert hyper response to axum response
    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}

async fn proxy_mmry_request_to_url(
    client: Client<HttpConnector, Body>,
    mut req: Request<Body>,
    target_base_url: &str,
    target_path: &str,
    store: Option<&str>,
) -> Result<Response<Body>, StatusCode> {
    let sanitized_query = build_mmry_query(req.uri().query());
    let mut target_uri = format!("{}/{}", target_base_url.trim_end_matches('/'), target_path);

    let has_query = !sanitized_query.is_empty();
    let has_store = store.is_some();

    if has_query || has_store {
        target_uri.push('?');
        if has_query {
            target_uri.push_str(&sanitized_query);
        }
        if let Some(store_name) = store {
            if has_query {
                target_uri.push('&');
            }
            target_uri.push_str("store=");
            target_uri.push_str(&urlencoding::encode(store_name));
        }
    }

    debug!("Proxying mmry request to {}", target_uri);

    let uri: Uri = target_uri.parse().map_err(|e| {
        error!("Invalid target URI {}: {:?}", target_uri, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    *req.uri_mut() = uri;

    if let Some(authority) = req.uri().authority() {
        let value = axum::http::HeaderValue::from_str(authority.as_str()).map_err(|e| {
            error!("Invalid Host header value {}: {:?}", authority.as_str(), e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        req.headers_mut().insert(axum::http::header::HOST, value);
    }

    let response = client.request(req).await.map_err(|e| {
        error!("Mmry proxy request failed: {:?}", e);
        if e.is_connect() {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::BAD_GATEWAY
        }
    })?;

    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}

// ============================================================================
// Session-based Mmry Handlers
// ============================================================================

/// Proxy HTTP requests to a session's mmry service.
///
/// Routes: /session/{session_id}/memories/{*path}
#[allow(dead_code)]
pub async fn proxy_mmry(
    State(state): State<AppState>,
    Path((session_id, path)): Path<(String, String)>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let (target_url, store) = resolve_mmry_session_target(&state, &session_id).await?;
    proxy_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        &path,
        store.as_deref(),
    )
    .await
}

/// Proxy search requests to a session's mmry service.
///
/// Routes: /session/{session_id}/memories/search
pub async fn proxy_mmry_search(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let (target_url, store) = resolve_mmry_session_target(&state, &session_id).await?;
    proxy_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        "v1/federation/search",
        store.as_deref(),
    )
    .await
}

/// Proxy requests to list memories for a session.
///
/// Routes: GET /session/{session_id}/memories
pub async fn proxy_mmry_list(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let (target_url, store) = resolve_mmry_session_target(&state, &session_id).await?;
    proxy_mmry_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        "v1/memories",
        store.as_deref(),
    )
    .await
}

/// Proxy requests to add a memory for a session.
///
/// Routes: POST /session/{session_id}/memories
pub async fn proxy_mmry_add(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let (target_url, store) = resolve_mmry_session_target(&state, &session_id).await?;
    proxy_mmry_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        "v1/memories",
        store.as_deref(),
    )
    .await
}

/// Proxy requests to get/update/delete a specific memory.
///
/// Routes: GET/PUT/DELETE /session/{session_id}/memories/{memory_id}
pub async fn proxy_mmry_memory(
    State(state): State<AppState>,
    Path((session_id, memory_id)): Path<(String, String)>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let (target_url, store) = resolve_mmry_session_target(&state, &session_id).await?;
    let path = format!("v1/memories/{}", memory_id);
    proxy_mmry_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        &path,
        store.as_deref(),
    )
    .await
}

/// Proxy requests to list mmry stores for a session.
///
/// Routes: GET /session/{session_id}/memories/stores
pub async fn proxy_mmry_stores(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let (target_url, _store) = resolve_mmry_session_target(&state, &session_id).await?;
    // Note: stores endpoint doesn't need a store parameter - it lists all stores
    proxy_mmry_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        "v1/stores",
        None,
    )
    .await
}

// ============================================================================
// Workspace-based memory handlers (mmry-core JSONL)
// ============================================================================

use axum::Json;
use mmry_core::agent_ctx::AgentCtx;
use mmry_core::memory::MemoryType;
use mmry_core::memory_file::MemoryEntry;
use mmry_core::memory_file::MemoryEvent;
use mmry_core::memory_file::MemoryFile;

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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{get_mmry_store_name_from_path, parse_origin_remote_repo_name};

    #[test]
    fn parse_origin_remote_repo_name_supports_https_and_ssh() {
        let https_config = r#"
[core]
    repositoryformatversion = 0
[remote "origin"]
    url = https://github.com/byteowlz/oqto.git
    fetch = +refs/heads/*:refs/remotes/origin/*
"#;
        assert_eq!(
            parse_origin_remote_repo_name(https_config),
            Some("oqto".to_string())
        );

        let ssh_config = r#"
[remote "origin"]
    url = git@github.com:byteowlz/oqto.git
"#;
        assert_eq!(
            parse_origin_remote_repo_name(ssh_config),
            Some("oqto".to_string())
        );
    }

    #[test]
    fn store_name_prefers_git_remote_repo_name_over_directory_name() {
        let dir = tempdir().expect("create tempdir");
        let workspace = dir.path().join("oqto_refactor");
        fs::create_dir_all(workspace.join(".git")).expect("create .git");
        fs::write(
            workspace.join(".git").join("config"),
            "[remote \"origin\"]\n\turl = https://github.com/byteowlz/oqto.git\n",
        )
        .expect("write git config");

        let store = get_mmry_store_name_from_path(&workspace.to_string_lossy());
        assert_eq!(store, Some("oqto".to_string()));
    }

    #[test]
    fn store_name_falls_back_to_directory_basename_when_no_git_remote() {
        let dir = tempdir().expect("create tempdir");
        let workspace = dir.path().join("oqto_refactor");
        fs::create_dir_all(&workspace).expect("create workspace");

        let store = get_mmry_store_name_from_path(&workspace.to_string_lossy());
        assert_eq!(store, Some("oqto_refactor".to_string()));
    }
}

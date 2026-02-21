//! Mmry (Memory Service) proxy handlers.
//!
//! Handles proxying requests to per-session or shared mmry instances.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{Request, Response, StatusCode, Uri},
};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use log::{debug, error, warn};

use crate::auth::CurrentUser;
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
/// - Single-user mode: proxy to the configured local mmry service.
/// - Multi-user mode (local): proxy to the user's pinned mmry instance.
async fn get_mmry_target_for_workspace(
    state: &AppState,
    user_id: &str,
) -> Result<String, StatusCode> {
    if !state.mmry.enabled {
        warn!("mmry integration is not enabled");
        return Err(StatusCode::NOT_FOUND);
    }

    if !state.mmry.single_user {
        let port = state
            .sessions
            .for_user(user_id)
            .ensure_user_mmry_pinned()
            .await
            .map_err(|e| {
                error!(
                    "Failed to ensure per-user mmry for workspace access: {:?}",
                    e
                );
                StatusCode::SERVICE_UNAVAILABLE
            })?;
        return Ok(format!("http://localhost:{}", port));
    }

    Ok(state.mmry.local_service_url.clone())
}

/// Derive mmry store name from session workspace path.
///
/// In single-user mode, each workspace maps to a separate mmry store.
/// The store name is derived from the last component of the workspace path.
/// For example: `/home/user/byteowlz/oqto` -> `oqto`
fn get_mmry_store_name(state: &AppState, session: &Session) -> Option<String> {
    if !state.mmry.single_user {
        // In multi-user mode, each session has its own mmry instance
        return None;
    }

    // Extract the last path component as the store name
    std::path::Path::new(&session.workspace_path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.to_string())
}

/// Derive mmry store name directly from a workspace path.
fn get_mmry_store_name_from_path(_state: &AppState, workspace_path: &str) -> Option<String> {
    let trimmed = workspace_path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    std::path::Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.to_string())
}

fn resolve_mmry_store_for_workspace(
    state: &AppState,
    query: &WorkspaceProxyQuery,
) -> Option<String> {
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
    get_mmry_store_name_from_path(state, &query.workspace_path)
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
    let store = get_mmry_store_name(state, &session);
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
// Workspace-based Mmry Handlers (single-user mode)
// ============================================================================

/// Proxy requests to list memories for a workspace (single-user mode).
///
/// Routes: GET /workspace/memories
pub async fn proxy_mmry_list_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let target_url = get_mmry_target_for_workspace(&state, user.id()).await?;
    let store = resolve_mmry_store_for_workspace(&state, &query);
    proxy_mmry_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        "v1/memories",
        store.as_deref(),
    )
    .await
}

/// Proxy requests to add a memory for a workspace (single-user mode).
///
/// Routes: POST /workspace/memories
pub async fn proxy_mmry_add_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let target_url = get_mmry_target_for_workspace(&state, user.id()).await?;
    let store = resolve_mmry_store_for_workspace(&state, &query);
    proxy_mmry_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        "v1/memories",
        store.as_deref(),
    )
    .await
}

/// Proxy search requests for a workspace (single-user mode).
///
/// Routes: POST /workspace/memories/search
pub async fn proxy_mmry_search_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let target_url = get_mmry_target_for_workspace(&state, user.id()).await?;
    let store = resolve_mmry_store_for_workspace(&state, &query);
    proxy_mmry_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        "v1/federation/search",
        store.as_deref(),
    )
    .await
}

/// Proxy requests to get/update/delete a specific memory for a workspace (single-user mode).
///
/// Routes: GET/PUT/DELETE /workspace/memories/{memory_id}
pub async fn proxy_mmry_memory_for_workspace(
    State(state): State<AppState>,
    Path(memory_id): Path<String>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let target_url = get_mmry_target_for_workspace(&state, user.id()).await?;
    let store = resolve_mmry_store_for_workspace(&state, &query);
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

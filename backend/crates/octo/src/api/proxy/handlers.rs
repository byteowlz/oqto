//! HTTP proxy handlers for session services.
//!
//! Contains the public handler functions for proxying to fileserver and sldr.

use axum::{
    body::Body,
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use log::{error, info};

use crate::auth::CurrentUser;
use crate::session::SessionStatus;

use super::super::state::AppState;
use super::builder::{
    build_fileserver_query, ensure_session_for_io_proxy, get_io_session_for_workspace,
    get_session_for_user, proxy_http_request, proxy_http_request_with_query,
};
use super::terminal::handle_terminal_proxy;
use super::websocket::{handle_browser_stream_proxy, handle_voice_ws_proxy};

// ============================================================================
// Query Types
// ============================================================================

#[derive(serde::Deserialize)]
pub struct WorkspaceProxyQuery {
    pub workspace_path: String,
    pub store: Option<String>,
}

// ============================================================================
// Voice WebSocket Proxies
// ============================================================================

/// Proxy WebSocket requests to the configured STT service.
pub async fn proxy_voice_stt_ws(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    if !state.voice.enabled {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = state.voice.stt_url.clone();

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_voice_ws_proxy(socket, target_url).await {
            error!("Voice STT proxy error: {:?}", e);
        }
    }))
}

/// Proxy WebSocket requests to the configured TTS service.
pub async fn proxy_voice_tts_ws(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    if !state.voice.enabled {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = state.voice.tts_url.clone();

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_voice_ws_proxy(socket, target_url).await {
            error!("Voice TTS proxy error: {:?}", e);
        }
    }))
}

// ============================================================================
// Fileserver Proxy
// ============================================================================

/// Proxy HTTP requests to a session's file server.
///
/// In single-user mode, the fileserver runs independently and remains accessible
/// even when the agent session is inactive.
pub async fn proxy_fileserver(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((session_id, path)): Path<(String, String)>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = get_session_for_user(&state, &user, &session_id).await?;
    let session = ensure_session_for_io_proxy(&state, user.id(), &session_id, session).await?;

    let starting = matches!(session.status, SessionStatus::Starting);
    proxy_http_request(
        state.http_client.clone(),
        req,
        session.fileserver_port as u16,
        &path,
        starting,
        state.max_proxy_body_bytes,
    )
    .await
}

/// Proxy HTTP requests to a workspace file server by workspace path.
pub async fn proxy_fileserver_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(path): Path<String>,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = get_io_session_for_workspace(&state, &user, &query.workspace_path).await?;
    let directory_query = build_fileserver_query(&query.workspace_path, req.uri().query());

    let starting = matches!(session.status, SessionStatus::Starting);
    proxy_http_request_with_query(
        state.http_client.clone(),
        req,
        session.fileserver_port as u16,
        &path,
        starting,
        Some(&directory_query),
        state.max_proxy_body_bytes,
    )
    .await
}

// ============================================================================
// SLDR Proxy
// ============================================================================

/// Proxy HTTP requests to per-user sldr-server (root path).
pub async fn proxy_sldr_root(
    State(state): State<AppState>,
    user: CurrentUser,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    proxy_sldr_internal(state, user, String::new(), req).await
}

/// Proxy HTTP requests to per-user sldr-server.
pub async fn proxy_sldr(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(path): Path<String>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    proxy_sldr_internal(state, user, path, req).await
}

async fn proxy_sldr_internal(
    state: AppState,
    user: CurrentUser,
    path: String,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let manager = state.sldr_users.as_ref().ok_or_else(|| {
        error!("sldr manager not configured");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let port = manager.ensure_user_sldr(user.id()).await.map_err(|e| {
        error!("Failed to start sldr for user {}: {:?}", user.id(), e);
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    proxy_http_request(
        state.http_client.clone(),
        req,
        port,
        &path,
        true,
        state.max_proxy_body_bytes,
    )
    .await
}

// ============================================================================
// Terminal WebSocket Proxy
// ============================================================================

/// WebSocket upgrade handler for terminal proxy.
pub async fn proxy_terminal_ws(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    let session = get_session_for_user(&state, &user, &session_id).await?;
    let session = ensure_session_for_io_proxy(&state, user.id(), &session_id, session).await?;

    let session_id_clone = session.id.clone();
    let ttyd_port = session.ttyd_port;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) =
            handle_terminal_proxy(socket, &session_id_clone, ttyd_port as u16, None).await
        {
            error!("Terminal proxy error: {:?}", e);
        }
    }))
}

/// WebSocket upgrade handler for terminal proxy by workspace path.
pub async fn proxy_terminal_ws_for_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceProxyQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    let session = get_io_session_for_workspace(&state, &user, &query.workspace_path).await?;

    let session_id = session.id.clone();
    let ttyd_port = session.ttyd_port;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_terminal_proxy(socket, &session_id, ttyd_port as u16, None).await {
            error!("Terminal proxy error: {:?}", e);
        }
    }))
}

// ============================================================================
// Browser Stream WebSocket Proxy
// ============================================================================

/// WebSocket upgrade handler for browser stream proxy.
///
/// The session_id here is the agent-browser session name (typically the Pi/chat
/// session ID), NOT an octo IO session. We just need it to find the stream port
/// file at `/tmp/agent-browser-{session_id}.stream`.
pub async fn proxy_browser_stream_ws(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    info!("Browser stream WS request for session {}", session_id);

    if !state.sessions.agent_browser_enabled() {
        info!("Browser stream: agent_browser not enabled");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let stream_port = state
        .sessions
        .agent_browser_stream_port(&session_id)
        .map_err(|e| {
            error!(
                "Failed to determine agent-browser stream port for session {}: {:?}",
                session_id, e
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?
        .ok_or_else(|| {
            info!(
                "Browser stream: no stream port found for session {}",
                session_id
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    info!(
        "Browser stream: resolved port {} for session {}",
        stream_port, session_id
    );

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_browser_stream_proxy(socket, stream_port).await {
            error!("Browser stream proxy error: {:?}", e);
        }
    }))
}



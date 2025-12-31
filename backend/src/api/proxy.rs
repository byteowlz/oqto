//! HTTP and WebSocket proxy for container services.

use axum::{
    body::Body,
    extract::{Path, State, WebSocketUpgrade},
    http::{Request, StatusCode, Uri},
    response::{IntoResponse, Response, Sse},
};
use futures::{SinkExt, StreamExt};
use hyper_util::client::legacy::Client;
use log::{debug, error, warn};
use tokio_tungstenite::connect_async;

use crate::session::SessionStatus;

use super::state::AppState;

/// Proxy HTTP requests to a session's opencode server.
pub async fn proxy_opencode(
    State(state): State<AppState>,
    Path((session_id, path)): Path<(String, String)>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let starting = matches!(session.status, SessionStatus::Starting);
    proxy_request(
        state.http_client.clone(),
        req,
        session.opencode_port as u16,
        &path,
        starting,
    )
    .await
}

/// Proxy HTTP requests to a session's file server.
pub async fn proxy_fileserver(
    State(state): State<AppState>,
    Path((session_id, path)): Path<(String, String)>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let starting = matches!(session.status, SessionStatus::Starting);
    proxy_request(
        state.http_client.clone(),
        req,
        session.fileserver_port as u16,
        &path,
        starting,
    )
    .await
}

/// Generic HTTP proxy function.
async fn proxy_request(
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
    mut req: Request<Body>,
    target_port: u16,
    target_path: &str,
    connect_errors_as_unavailable: bool,
) -> Result<Response, StatusCode> {
    let query = req.uri().query().unwrap_or("");
    let mut target_uri = format!("http://localhost:{}/{}", target_port, target_path);
    if !query.is_empty() {
        target_uri.push('?');
        target_uri.push_str(query);
    }

    debug!("Proxying request to {}", target_uri);

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
        error!("Proxy request failed: {:?}", e);
        if connect_errors_as_unavailable && e.is_connect() {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::BAD_GATEWAY
        }
    })?;

    // Convert hyper response to axum response
    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}

/// WebSocket upgrade handler for terminal proxy.
pub async fn proxy_terminal_ws(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Allow terminal connections during session startup. The WS proxy will wait
    // briefly for ttyd to become available instead of immediately failing.
    if matches!(
        session.status,
        SessionStatus::Stopping | SessionStatus::Stopped | SessionStatus::Failed
    ) {
        warn!(
            "Attempted to proxy terminal to inactive session {}",
            session_id
        );
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let ttyd_port = session.ttyd_port;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_terminal_proxy(socket, ttyd_port as u16).await {
            error!("Terminal proxy error: {:?}", e);
        }
    }))
}

/// Handle WebSocket proxy between client and ttyd.
///
/// ttyd uses a binary protocol with command prefixes:
/// Client -> Server:
///   Initial: JSON with {"AuthToken": "", "columns": N, "rows": N}
///   '0' + data  = Input (keystrokes)
///   '1' + JSON  = Resize terminal {"columns": N, "rows": N}
///   '2'         = Pause
///   '3'         = Resume
///
/// Server -> Client:
///   '0' + data  = Output (terminal data)
///   '1' + title = Set window title
///   '2' + JSON  = Set preferences
///
/// This proxy:
/// 1. Connects to ttyd with the 'tty' subprotocol
/// 2. Sends the initial auth/resize message
/// 3. Translates between raw terminal data (from ghostty-web) and ttyd protocol
async fn handle_terminal_proxy(
    client_socket: axum::extract::ws::WebSocket,
    ttyd_port: u16,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message as AxumMessage;
    use tokio::time::{Duration, Instant};
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let ttyd_url = format!("ws://localhost:{}/ws", ttyd_port);
    debug!("Connecting to ttyd at {}", ttyd_url);

    // Connect to ttyd with subprotocol, retrying during startup.
    // This avoids a race where the client opens the WS before ttyd is listening.
    let start = Instant::now();
    let timeout = Duration::from_secs(60);
    let mut attempts: u32 = 0;

    let (ttyd_socket, _) = loop {
        attempts += 1;
        let mut request = ttyd_url.clone().into_client_request()?;
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "tty".parse().unwrap());

        match connect_async(request).await {
            Ok(result) => break result,
            Err(err) => {
                if start.elapsed() >= timeout {
                    return Err(anyhow::anyhow!(
                        "ttyd not available after {} attempts over {:?}: {}",
                        attempts,
                        timeout,
                        err
                    ));
                }

                let backoff_ms = (attempts.min(20) as u64) * 100;
                let backoff = Duration::from_millis(backoff_ms);
                debug!(
                    "ttyd not ready yet (attempt {}): {}; retrying in {:?}",
                    attempts, err, backoff
                );
                tokio::time::sleep(backoff).await;
            }
        }
    };
    let (mut ttyd_write, mut ttyd_read) = ttyd_socket.split();

    // Send initial auth/resize message that ttyd requires
    // This JSON message must be sent immediately after connection
    let init_msg = r#"{"AuthToken":"","columns":120,"rows":40}"#;
    debug!("Sending ttyd init message: {}", init_msg);
    ttyd_write
        .send(TungsteniteMessage::Binary(
            init_msg.as_bytes().to_vec().into(),
        ))
        .await?;

    // Split client socket
    let (mut client_write, mut client_read) = client_socket.split();

    // Forward client -> ttyd (add ttyd protocol prefix)
    let client_to_ttyd = async {
        while let Some(msg) = client_read.next().await {
            match msg {
                Ok(AxumMessage::Text(text)) => {
                    let text_str = text.to_string();
                    // Check if this is a resize command (JSON with columns/rows)
                    if text_str.starts_with('{') && text_str.contains("columns") {
                        // Send as resize command with '1' prefix
                        let mut prefixed = vec![b'1'];
                        prefixed.extend_from_slice(text_str.as_bytes());
                        debug!(
                            "Sending resize to ttyd: {:?}",
                            String::from_utf8_lossy(&prefixed)
                        );
                        if ttyd_write
                            .send(TungsteniteMessage::Binary(prefixed.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    } else {
                        // Regular input - prefix with '0' for ttyd INPUT command
                        let mut prefixed = vec![b'0'];
                        prefixed.extend_from_slice(text_str.as_bytes());
                        if ttyd_write
                            .send(TungsteniteMessage::Binary(prefixed.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                Ok(AxumMessage::Binary(data)) => {
                    // Binary data - check if already prefixed or needs prefix
                    let to_send = if !data.is_empty()
                        && (data[0] == b'0'
                            || data[0] == b'1'
                            || data[0] == b'2'
                            || data[0] == b'3')
                    {
                        // Already has ttyd prefix, pass through
                        data.to_vec()
                    } else {
                        // Add INPUT prefix
                        let mut prefixed = vec![b'0'];
                        prefixed.extend_from_slice(&data);
                        prefixed
                    };
                    if ttyd_write
                        .send(TungsteniteMessage::Binary(to_send.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(AxumMessage::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    };

    // Forward ttyd -> client (strip ttyd protocol prefix for output)
    let ttyd_to_client = async {
        while let Some(msg) = ttyd_read.next().await {
            match msg {
                Ok(TungsteniteMessage::Text(text)) => {
                    let text_str = text.to_string();
                    // ttyd sends text messages - check for command prefix
                    if !text_str.is_empty() {
                        let first_char = text_str.chars().next().unwrap();
                        match first_char {
                            '0' => {
                                // Output data - strip the '0' prefix and send to client
                                let output = &text_str[1..];
                                if client_write
                                    .send(AxumMessage::Text(output.to_string().into()))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            '1' => {
                                // Window title - ignore for now
                                debug!("Received window title from ttyd: {}", &text_str[1..]);
                            }
                            '2' => {
                                // Preferences - ignore for now
                                debug!("Received preferences from ttyd: {}", &text_str[1..]);
                            }
                            _ => {
                                // Unknown, pass through as-is
                                debug!("Received unknown message from ttyd: {}", text_str);
                                if client_write
                                    .send(AxumMessage::Text(text_str.into()))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                }
                Ok(TungsteniteMessage::Binary(data)) => {
                    // Binary message from ttyd
                    if !data.is_empty() {
                        match data[0] {
                            b'0' => {
                                // Output data - strip the '0' prefix and send to client
                                let output = &data[1..];
                                if client_write
                                    .send(AxumMessage::Binary(output.to_vec().into()))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            b'1' | b'2' => {
                                // Window title or preferences - ignore
                                debug!("Received ttyd control message type: {}", data[0] as char);
                            }
                            _ => {
                                // Unknown, pass through as-is
                                if client_write
                                    .send(AxumMessage::Binary(data.to_vec().into()))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                }
                Ok(TungsteniteMessage::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    };

    // Run both directions concurrently
    tokio::select! {
        _ = client_to_ttyd => {}
        _ = ttyd_to_client => {}
    }

    Ok(())
}

/// SSE events proxy for a specific session's opencode server.
pub async fn proxy_opencode_events(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy SSE to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = format!("http://localhost:{}/event", session.opencode_port);
    debug!("Proxying SSE events from {}", target_url);

    // Create HTTP client for SSE
    let client = reqwest::Client::new();

    // During startup, opencode may not be ready yet. Retry the initial connect for a short period.
    let start = tokio::time::Instant::now();
    let timeout = tokio::time::Duration::from_secs(20);
    let mut attempts: u32 = 0;

    let response = loop {
        attempts += 1;
        match client
            .get(&target_url)
            .header("Accept", "text/event-stream")
            .send()
            .await
        {
            Ok(res) => break res,
            Err(err) => {
                // Only retry connection-level failures.
                if !err.is_connect() || start.elapsed() >= timeout {
                    error!(
                        "Failed to connect to opencode SSE after {} attempts over {:?}: {:?}",
                        attempts, timeout, err
                    );
                    return Err(StatusCode::BAD_GATEWAY);
                }

                let backoff_ms = (attempts.min(20) as u64) * 100;
                let backoff = tokio::time::Duration::from_millis(backoff_ms);
                debug!(
                    "opencode SSE not ready yet (attempt {}): {}; retrying in {:?}",
                    attempts, err, backoff
                );
                tokio::time::sleep(backoff).await;
            }
        }
    };

    if !response.status().is_success() {
        error!("Opencode SSE returned status: {}", response.status());
        return Err(StatusCode::BAD_GATEWAY);
    }

    // Convert reqwest byte stream to axum body stream
    let stream = response.bytes_stream();
    let body = Body::from_stream(stream);

    // Build SSE response with proper headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("X-Accel-Buffering", "no") // Disable nginx buffering if present
        .body(body)
        .map_err(|e| {
            error!("Failed to build SSE response: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(response)
}

/// SSE events stream for opencode (legacy global endpoint - deprecated).
pub async fn opencode_events(
    State(_state): State<AppState>,
) -> Result<
    Sse<
        impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    StatusCode,
> {
    use axum::response::sse::Event;
    use std::time::Duration;
    use tokio::time;
    use tokio_stream::{StreamExt, wrappers::IntervalStream};

    // For now, send a keep-alive every 30 seconds
    // This endpoint is deprecated - use /session/:id/code/event instead
    let interval = time::interval(Duration::from_secs(30));
    let stream = StreamExt::map(IntervalStream::new(interval), |_| {
        Ok(Event::default().data("{\"type\":\"keepalive\"}"))
    });

    Ok(Sse::new(stream))
}

// ============================================================================
// Mmry (Memory Service) Proxy Handlers
// ============================================================================

/// Get the mmry target URL for a session.
///
/// In single-user mode, returns the local service URL.
/// In multi-user mode, returns the session's mmry port URL.
fn get_mmry_target(
    state: &AppState,
    session: &crate::session::Session,
) -> Result<String, StatusCode> {
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

/// Proxy request to a URL-based target.
async fn proxy_request_to_url(
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
    mut req: Request<Body>,
    target_base_url: &str,
    target_path: &str,
) -> Result<Response, StatusCode> {
    let query = req.uri().query().unwrap_or("");
    let mut target_uri = format!("{}/{}", target_base_url.trim_end_matches('/'), target_path);
    if !query.is_empty() {
        target_uri.push('?');
        target_uri.push_str(query);
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

/// Proxy HTTP requests to a session's mmry service.
///
/// Routes: /session/{session_id}/memories/{*path}
#[allow(dead_code)]
pub async fn proxy_mmry(
    State(state): State<AppState>,
    Path((session_id, path)): Path<(String, String)>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy mmry to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = get_mmry_target(&state, &session)?;
    proxy_request_to_url(state.http_client.clone(), req, &target_url, &path).await
}

/// Proxy search requests to a session's mmry service.
///
/// This is a separate handler because the search endpoint uses a different path pattern.
/// Routes: /session/{session_id}/memories/search
pub async fn proxy_mmry_search(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy mmry to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = get_mmry_target(&state, &session)?;
    proxy_request_to_url(state.http_client.clone(), req, &target_url, "v1/search").await
}

/// Proxy requests to list memories for a session.
///
/// Routes: GET /session/{session_id}/memories
pub async fn proxy_mmry_list(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy mmry to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = get_mmry_target(&state, &session)?;
    proxy_request_to_url(state.http_client.clone(), req, &target_url, "v1/memories").await
}

/// Proxy requests to add a memory for a session.
///
/// Routes: POST /session/{session_id}/memories
pub async fn proxy_mmry_add(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy mmry to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = get_mmry_target(&state, &session)?;
    proxy_request_to_url(state.http_client.clone(), req, &target_url, "v1/memories").await
}

/// Proxy requests to get/update/delete a specific memory.
///
/// Routes: GET/PUT/DELETE /session/{session_id}/memories/{memory_id}
pub async fn proxy_mmry_memory(
    State(state): State<AppState>,
    Path((session_id, memory_id)): Path<(String, String)>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy mmry to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = get_mmry_target(&state, &session)?;
    let path = format!("v1/memories/{}", memory_id);
    proxy_request_to_url(state.http_client.clone(), req, &target_url, &path).await
}

/// Proxy requests to list mmry stores for a session.
///
/// Routes: GET /session/{session_id}/memories/stores
pub async fn proxy_mmry_stores(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy mmry to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let target_url = get_mmry_target(&state, &session)?;
    proxy_request_to_url(state.http_client.clone(), req, &target_url, "v1/stores").await
}

// ============================================================================
// Sub-Agent Proxy Handlers
// ============================================================================

/// Proxy HTTP requests to a specific agent's opencode server.
///
/// Routes: /session/{session_id}/agent/{agent_id}/code/{*path}
pub async fn proxy_opencode_agent(
    State(state): State<AppState>,
    Path((session_id, agent_id, path)): Path<(String, String, String)>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Resolve the agent's port
    let port = state
        .agents
        .get_agent_port(&session_id, &agent_id)
        .await
        .map_err(|e| {
            error!(
                "Failed to get agent port for {}/{}: {:?}",
                session_id, agent_id, e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Agent {} not found or not running in session {}",
                agent_id, session_id
            );
            StatusCode::NOT_FOUND
        })?;

    let starting = matches!(session.status, SessionStatus::Starting);
    proxy_request(state.http_client.clone(), req, port, &path, starting).await
}

/// SSE events proxy for a specific agent's opencode server.
///
/// Routes: /session/{session_id}/agent/{agent_id}/code/event
pub async fn proxy_opencode_agent_events(
    State(state): State<AppState>,
    Path((session_id, agent_id)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !session.is_active() {
        warn!("Attempted to proxy SSE to inactive session {}", session_id);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Resolve the agent's port
    let port = state
        .agents
        .get_agent_port(&session_id, &agent_id)
        .await
        .map_err(|e| {
            error!(
                "Failed to get agent port for {}/{}: {:?}",
                session_id, agent_id, e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Agent {} not found or not running in session {}",
                agent_id, session_id
            );
            StatusCode::NOT_FOUND
        })?;

    let target_url = format!("http://localhost:{}/event", port);
    debug!(
        "Proxying agent SSE events from {} (agent: {})",
        target_url, agent_id
    );

    // Create HTTP client for SSE
    let client = reqwest::Client::new();

    // During startup, opencode may not be ready yet. Retry the initial connect for a short period.
    let start = tokio::time::Instant::now();
    let timeout = tokio::time::Duration::from_secs(20);
    let mut attempts: u32 = 0;

    let response = loop {
        attempts += 1;
        match client
            .get(&target_url)
            .header("Accept", "text/event-stream")
            .send()
            .await
        {
            Ok(res) => break res,
            Err(err) => {
                // Only retry connection-level failures.
                if !err.is_connect() || start.elapsed() >= timeout {
                    error!(
                        "Failed to connect to agent {} SSE after {} attempts over {:?}: {:?}",
                        agent_id, attempts, timeout, err
                    );
                    return Err(StatusCode::BAD_GATEWAY);
                }

                let backoff_ms = (attempts.min(20) as u64) * 100;
                let backoff = tokio::time::Duration::from_millis(backoff_ms);
                debug!(
                    "Agent {} SSE not ready yet (attempt {}): {}; retrying in {:?}",
                    agent_id, attempts, err, backoff
                );
                tokio::time::sleep(backoff).await;
            }
        }
    };

    if !response.status().is_success() {
        error!(
            "Agent {} SSE returned status: {}",
            agent_id,
            response.status()
        );
        return Err(StatusCode::BAD_GATEWAY);
    }

    // Convert reqwest byte stream to axum body stream
    let stream = response.bytes_stream();
    let body = Body::from_stream(stream);

    // Build SSE response with proper headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("X-Accel-Buffering", "no") // Disable nginx buffering if present
        .body(body)
        .map_err(|e| {
            error!("Failed to build SSE response: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(response)
}

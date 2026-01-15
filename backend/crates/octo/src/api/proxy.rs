//! HTTP and WebSocket proxy for container services.

use axum::{
    body::Body,
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, Request, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use futures::{SinkExt, StreamExt};
use hyper_util::client::legacy::Client;
use log::{debug, error, warn};
use tokio_tungstenite::connect_async;

use crate::session::SessionStatus;

use super::state::AppState;

async fn ensure_session_active_for_proxy(
    state: &AppState,
    session_id: &str,
    session: crate::session::Session,
) -> Result<crate::session::Session, StatusCode> {
    match session.status {
        SessionStatus::Running | SessionStatus::Starting | SessionStatus::Pending => Ok(session),
        SessionStatus::Stopped => {
            warn!(
                "Session {} is stopped; attempting to resume for proxy request",
                session_id
            );
            match state.sessions.resume_session(session_id).await {
                Ok(resumed) => Ok(resumed),
                Err(err) => {
                    error!("Failed to resume session {}: {:?}", session_id, err);
                    Err(StatusCode::SERVICE_UNAVAILABLE)
                }
            }
        }
        SessionStatus::Stopping | SessionStatus::Failed => {
            warn!("Attempted to proxy to inactive session {}", session_id);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

async fn ensure_session_for_io_proxy(
    state: &AppState,
    session_id: &str,
    session: crate::session::Session,
) -> Result<crate::session::Session, StatusCode> {
    if session.status != SessionStatus::Stopped {
        return Ok(session);
    }

    warn!(
        "Session {} is stopped; attempting to resume for IO proxy request",
        session_id
    );
    match state.sessions.resume_session_for_io(session_id).await {
        Ok(resumed) => Ok(resumed),
        Err(err) => {
            error!("Failed to resume session {}: {:?}", session_id, err);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

#[derive(serde::Deserialize)]
pub(crate) struct WorkspaceProxyQuery {
    workspace_path: String,
    store: Option<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct OpencodeEventQuery {
    directory: Option<String>,
}

fn build_fileserver_query(workspace_path: &str, query: Option<&str>) -> String {
    let mut pairs: Vec<String> = Vec::new();
    if let Some(query) = query {
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            if pair.starts_with("workspace_path=") || pair.starts_with("directory=") {
                continue;
            }
            pairs.push(pair.to_string());
        }
    }
    pairs.push(format!("directory={}", urlencoding::encode(workspace_path)));
    pairs.join("&")
}

fn enforce_proxy_body_limit(headers: &HeaderMap, max_body_bytes: usize) -> Result<(), StatusCode> {
    if let Some(value) = headers.get(axum::http::header::CONTENT_LENGTH) {
        let length = value
            .to_str()
            .ok()
            .and_then(|raw| raw.parse::<usize>().ok())
            .ok_or(StatusCode::BAD_REQUEST)?;
        if length > max_body_bytes {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
    }
    Ok(())
}

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

async fn handle_voice_ws_proxy(
    client_socket: axum::extract::ws::WebSocket,
    target_url: String,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message as AxumMessage;
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    debug!("Proxying voice WebSocket to {}", target_url);

    let (server_socket, _) = connect_async(target_url).await?;

    let (mut client_tx, mut client_rx) = client_socket.split();
    let (mut server_tx, mut server_rx) = server_socket.split();

    let client_to_server = async {
        while let Some(msg) = client_rx.next().await {
            let msg = msg?;
            let forward = match msg {
                AxumMessage::Text(text) => TungsteniteMessage::Text(text.to_string().into()),
                AxumMessage::Binary(data) => TungsteniteMessage::Binary(data),
                AxumMessage::Ping(data) => TungsteniteMessage::Ping(data),
                AxumMessage::Pong(data) => TungsteniteMessage::Pong(data),
                AxumMessage::Close(_) => TungsteniteMessage::Close(None),
            };
            server_tx.send(forward).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    let server_to_client = async {
        while let Some(msg) = server_rx.next().await {
            let msg = msg?;
            let forward = match msg {
                TungsteniteMessage::Text(text) => AxumMessage::Text(text.to_string().into()),
                TungsteniteMessage::Binary(data) => AxumMessage::Binary(data),
                TungsteniteMessage::Ping(data) => AxumMessage::Ping(data),
                TungsteniteMessage::Pong(data) => AxumMessage::Pong(data),
                TungsteniteMessage::Close(_) => AxumMessage::Close(None),
                TungsteniteMessage::Frame(_) => continue,
            };
            client_tx.send(forward).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        result = client_to_server => result?,
        result = server_to_client => result?,
    }

    Ok(())
}

/// Proxy HTTP requests to a session's opencode server.
pub async fn proxy_opencode(
    State(state): State<AppState>,
    Path((session_id, path)): Path<(String, String)>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let _requested = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let opencode_session = state
        .sessions
        .get_or_create_opencode_session()
        .await
        .map_err(|e| {
            error!("Failed to get primary opencode session: {:?}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let opencode_session_id = opencode_session.id.clone();
    let opencode_session =
        ensure_session_active_for_proxy(&state, &opencode_session_id, opencode_session).await?;

    let starting = matches!(opencode_session.status, SessionStatus::Starting);
    proxy_request(
        state.http_client.clone(),
        req,
        opencode_session.opencode_port as u16,
        &path,
        starting,
        state.max_proxy_body_bytes,
    )
    .await
}

/// Proxy HTTP requests to a session's file server.
///
/// In single-user mode, the fileserver runs independently and remains accessible
/// even when the opencode session is inactive.
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

    let session = ensure_session_for_io_proxy(&state, &session_id, session).await?;

    let starting = matches!(session.status, SessionStatus::Starting);
    proxy_request(
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
    Path(path): Path<String>,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let session = state
        .sessions
        .get_or_create_io_session_for_workspace(&query.workspace_path)
        .await
        .map_err(|e| {
            error!(
                "Failed to get IO session for workspace {}: {:?}",
                query.workspace_path, e
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let directory_query = build_fileserver_query(&query.workspace_path, req.uri().query());

    let starting = matches!(session.status, SessionStatus::Starting);
    proxy_request_with_query(
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

/// Generic HTTP proxy function.
async fn proxy_request(
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
    req: Request<Body>,
    target_port: u16,
    target_path: &str,
    connect_errors_as_unavailable: bool,
    max_body_bytes: usize,
) -> Result<Response, StatusCode> {
    proxy_request_with_query(
        client,
        req,
        target_port,
        target_path,
        connect_errors_as_unavailable,
        None,
        max_body_bytes,
    )
    .await
}

async fn proxy_request_with_query(
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
    req: Request<Body>,
    target_port: u16,
    target_path: &str,
    connect_errors_as_unavailable: bool,
    query_override: Option<&str>,
    max_body_bytes: usize,
) -> Result<Response, StatusCode> {
    let query = req.uri().query().unwrap_or("");
    let query = query_override.unwrap_or(query);
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

    let (parts, body) = req.into_parts();

    // Log content-type for debugging multipart issues
    if let Some(ct) = parts.headers.get(axum::http::header::CONTENT_TYPE) {
        warn!("Proxy request Content-Type: {:?}", ct);
    }
    if let Some(cl) = parts.headers.get(axum::http::header::CONTENT_LENGTH) {
        warn!("Proxy request Content-Length header: {:?}", cl);
    }

    enforce_proxy_body_limit(&parts.headers, max_body_bytes)?;
    let body_bytes = axum::body::to_bytes(body, max_body_bytes)
        .await
        .map_err(|e| {
            if e.to_string().contains("length limit") {
                warn!(
                    "Proxy request body exceeded limit of {} bytes",
                    max_body_bytes
                );
                return StatusCode::PAYLOAD_TOO_LARGE;
            }
            error!("Failed to buffer proxy request body: {:?}", e);
            StatusCode::BAD_GATEWAY
        })?;

    warn!("Proxy request body size: {} bytes", body_bytes.len());

    let start = tokio::time::Instant::now();
    let timeout = tokio::time::Duration::from_secs(15);
    let mut attempts: u32 = 0;

    let response = loop {
        attempts += 1;
        let mut forwarded = Request::builder()
            .method(parts.method.clone())
            .uri(uri.clone())
            .version(parts.version)
            .body(Body::from(body_bytes.clone()))
            .map_err(|e| {
                error!("Failed to build proxy request: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        *forwarded.headers_mut() = parts.headers.clone();

        // Update Content-Length to match actual body size (important for multipart uploads
        // where the original Content-Length may not match after buffering).
        // Also remove Transfer-Encoding since we're sending a fixed-length body.
        forwarded
            .headers_mut()
            .remove(axum::http::header::TRANSFER_ENCODING);
        let new_content_length = body_bytes.len().to_string();
        warn!(
            "Setting Content-Length to {} (was: {:?})",
            new_content_length,
            parts.headers.get(axum::http::header::CONTENT_LENGTH)
        );
        forwarded.headers_mut().insert(
            axum::http::header::CONTENT_LENGTH,
            axum::http::HeaderValue::from_str(&new_content_length)
                .expect("content-length is valid"),
        );

        // Ensure Host header matches the target authority.
        if let Some(authority) = forwarded.uri().authority() {
            let value = axum::http::HeaderValue::from_str(authority.as_str()).map_err(|e| {
                error!("Invalid Host header value {}: {:?}", authority.as_str(), e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            forwarded
                .headers_mut()
                .insert(axum::http::header::HOST, value);
        }

        match client.request(forwarded).await {
            Ok(res) => break res,
            Err(err) => {
                if connect_errors_as_unavailable && err.is_connect() && start.elapsed() < timeout {
                    let backoff_ms = (attempts.min(20) as u64) * 100;
                    let backoff = tokio::time::Duration::from_millis(backoff_ms);
                    debug!(
                        "Proxy target not ready yet (attempt {}): {}; retrying in {:?}",
                        attempts, err, backoff
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                error!("Proxy request failed: {:?}", err);
                return Err(if connect_errors_as_unavailable && err.is_connect() {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::BAD_GATEWAY
                });
            }
        }
    };

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

    let session = ensure_session_for_io_proxy(&state, &session_id, session).await?;

    let ttyd_port = session.ttyd_port;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_terminal_proxy(socket, ttyd_port as u16, None).await {
            error!("Terminal proxy error: {:?}", e);
        }
    }))
}

/// WebSocket upgrade handler for terminal proxy by workspace path.
pub async fn proxy_terminal_ws_for_workspace(
    State(state): State<AppState>,
    Query(query): Query<WorkspaceProxyQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    let session = state
        .sessions
        .get_or_create_io_session_for_workspace(&query.workspace_path)
        .await
        .map_err(|e| {
            error!(
                "Failed to get IO session for workspace {}: {:?}",
                query.workspace_path, e
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let ttyd_port = session.ttyd_port;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_terminal_proxy(socket, ttyd_port as u16, None).await {
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
    initial_command: Option<String>,
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

    if let Some(command) = initial_command {
        let mut prefixed = vec![b'0'];
        prefixed.extend_from_slice(command.as_bytes());
        ttyd_write
            .send(TungsteniteMessage::Binary(prefixed.into()))
            .await?;
    }

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
    Query(query): Query<OpencodeEventQuery>,
) -> Result<Response, StatusCode> {
    let _requested = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let opencode_session = state
        .sessions
        .get_or_create_opencode_session()
        .await
        .map_err(|e| {
            error!("Failed to get primary opencode session: {:?}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let opencode_session_id = opencode_session.id.clone();
    let opencode_session =
        ensure_session_active_for_proxy(&state, &opencode_session_id, opencode_session).await?;

    let target_url = if let Some(directory) = query.directory.as_deref() {
        format!(
            "http://localhost:{}/event?directory={}",
            opencode_session.opencode_port,
            urlencoding::encode(directory)
        )
    } else {
        format!("http://localhost:{}/event", opencode_session.opencode_port)
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforce_proxy_body_limit_allows_small() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            axum::http::HeaderValue::from_static("10"),
        );
        assert!(enforce_proxy_body_limit(&headers, 20).is_ok());
    }

    #[test]
    fn enforce_proxy_body_limit_rejects_large() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            axum::http::HeaderValue::from_static("128"),
        );
        assert_eq!(
            enforce_proxy_body_limit(&headers, 64).unwrap_err(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[test]
    fn enforce_proxy_body_limit_rejects_invalid_length() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            axum::http::HeaderValue::from_static("nope"),
        );
        assert_eq!(
            enforce_proxy_body_limit(&headers, 64).unwrap_err(),
            StatusCode::BAD_REQUEST
        );
    }
}

/// SSE events stream for opencode global events.
///
/// Proxies to opencode's /global/event endpoint which provides events for all
/// directories/sessions. The SDK expects this endpoint to receive real-time
/// updates about sessions, messages, permissions, etc.
pub async fn opencode_events(State(state): State<AppState>) -> Result<Response, StatusCode> {
    let opencode_session = state
        .sessions
        .get_or_create_opencode_session()
        .await
        .map_err(|e| {
            error!("Failed to get primary opencode session: {:?}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let opencode_session_id = opencode_session.id.clone();
    let opencode_session =
        ensure_session_active_for_proxy(&state, &opencode_session_id, opencode_session).await?;

    let target_url = format!(
        "http://localhost:{}/global/event",
        opencode_session.opencode_port
    );
    debug!("Proxying global SSE events from {}", target_url);

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
                        "Failed to connect to opencode global SSE after {} attempts over {:?}: {:?}",
                        attempts, timeout, err
                    );
                    return Err(StatusCode::BAD_GATEWAY);
                }

                let backoff_ms = (attempts.min(20) as u64) * 100;
                let backoff = tokio::time::Duration::from_millis(backoff_ms);
                debug!(
                    "opencode global SSE not ready yet (attempt {}): {}; retrying in {:?}",
                    attempts, err, backoff
                );
                tokio::time::sleep(backoff).await;
            }
        }
    };

    if !response.status().is_success() {
        error!("Opencode global SSE returned status: {}", response.status());
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
            error!("Failed to build global SSE response: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(response)
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

async fn resolve_mmry_session_target(
    state: &AppState,
    session_id: &str,
) -> Result<(String, Option<String>), StatusCode> {
    let session = state
        .sessions
        .get_session(session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

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

/// Derive mmry store name from session workspace path.
///
/// In single-user mode, each workspace maps to a separate mmry store.
/// The store name is derived from the last component of the workspace path.
/// For example: `/home/user/byteowlz/octo` -> `octo`
fn get_mmry_store_name(state: &AppState, session: &crate::session::Session) -> Option<String> {
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
///
/// In single-user mode, each workspace maps to a separate mmry store.
/// The store name is derived from the last component of the workspace path.
/// For example: `/home/user/byteowlz/octo` -> `octo`
fn get_mmry_store_name_from_path(state: &AppState, workspace_path: &str) -> Option<String> {
    if !state.mmry.single_user {
        return None;
    }

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

/// Get the mmry target URL for workspace-based access (single-user mode only).
fn get_mmry_target_for_workspace(state: &AppState) -> Result<String, StatusCode> {
    if !state.mmry.enabled {
        warn!("mmry integration is not enabled");
        return Err(StatusCode::NOT_FOUND);
    }

    if !state.mmry.single_user {
        // Workspace-based mmry access only works in single-user mode
        warn!("Workspace-based mmry access requires single-user mode");
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(state.mmry.local_service_url.clone())
}

/// Proxy request to a URL-based target with optional store parameter.
async fn proxy_request_to_url(
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
    mut req: Request<Body>,
    target_base_url: &str,
    target_path: &str,
    store: Option<&str>,
) -> Result<Response, StatusCode> {
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

fn build_mmry_query(query: Option<&str>) -> String {
    let mut pairs: Vec<String> = Vec::new();
    if let Some(query) = query {
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            if pair.starts_with("workspace_path=") || pair.starts_with("store=") {
                continue;
            }
            pairs.push(pair.to_string());
        }
    }
    pairs.join("&")
}

async fn proxy_mmry_request_to_url(
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
    mut req: Request<Body>,
    target_base_url: &str,
    target_path: &str,
    store: Option<&str>,
) -> Result<Response, StatusCode> {
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

/// Proxy HTTP requests to a session's mmry service.
///
/// Routes: /session/{session_id}/memories/{*path}
#[allow(dead_code)]
pub async fn proxy_mmry(
    State(state): State<AppState>,
    Path((session_id, path)): Path<(String, String)>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
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
/// This is a separate handler because the search endpoint uses a different path pattern.
/// Routes: /session/{session_id}/memories/search
pub async fn proxy_mmry_search(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
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
) -> Result<Response, StatusCode> {
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
) -> Result<Response, StatusCode> {
    let (target_url, store) = resolve_mmry_session_target(&state, &session_id).await?;
    proxy_mmry_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        "v1/agents/memories",
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
) -> Result<Response, StatusCode> {
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
) -> Result<Response, StatusCode> {
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
// Workspace-based Mmry Proxy Handlers (single-user mode)
// ============================================================================

/// Proxy requests to list memories for a workspace (single-user mode).
///
/// Routes: GET /workspace/memories
pub async fn proxy_mmry_list_for_workspace(
    State(state): State<AppState>,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let target_url = get_mmry_target_for_workspace(&state)?;
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
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let target_url = get_mmry_target_for_workspace(&state)?;
    let store = resolve_mmry_store_for_workspace(&state, &query);
    proxy_mmry_request_to_url(
        state.http_client.clone(),
        req,
        &target_url,
        "v1/agents/memories",
        store.as_deref(),
    )
    .await
}

/// Proxy search requests for a workspace (single-user mode).
///
/// Routes: POST /workspace/memories/search
pub async fn proxy_mmry_search_for_workspace(
    State(state): State<AppState>,
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let target_url = get_mmry_target_for_workspace(&state)?;
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
    Query(query): Query<WorkspaceProxyQuery>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let target_url = get_mmry_target_for_workspace(&state)?;
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
    let _requested = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let opencode_session = state
        .sessions
        .get_or_create_opencode_session()
        .await
        .map_err(|e| {
            error!("Failed to get primary opencode session: {:?}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let opencode_session_id = opencode_session.id.clone();
    let opencode_session =
        ensure_session_active_for_proxy(&state, &opencode_session_id, opencode_session).await?;

    // Resolve the agent's port
    let port = state
        .agents
        .get_agent_port(&opencode_session_id, &agent_id)
        .await
        .map_err(|e| {
            error!(
                "Failed to get agent port for {}/{}: {:?}",
                opencode_session_id, agent_id, e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Agent {} not found or not running in session {}",
                agent_id, opencode_session_id
            );
            StatusCode::NOT_FOUND
        })?;

    let starting = matches!(opencode_session.status, SessionStatus::Starting);
    proxy_request(
        state.http_client.clone(),
        req,
        port,
        &path,
        starting,
        state.max_proxy_body_bytes,
    )
    .await
}

/// SSE events proxy for a specific agent's opencode server.
///
/// Routes: /session/{session_id}/agent/{agent_id}/code/event
pub async fn proxy_opencode_agent_events(
    State(state): State<AppState>,
    Path((session_id, agent_id)): Path<(String, String)>,
    Query(query): Query<OpencodeEventQuery>,
) -> Result<Response, StatusCode> {
    let _requested = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let opencode_session = state
        .sessions
        .get_or_create_opencode_session()
        .await
        .map_err(|e| {
            error!("Failed to get primary opencode session: {:?}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let opencode_session_id = opencode_session.id.clone();
    let _opencode_session =
        ensure_session_active_for_proxy(&state, &opencode_session_id, opencode_session).await?;

    // Resolve the agent's port
    let port = state
        .agents
        .get_agent_port(&opencode_session_id, &agent_id)
        .await
        .map_err(|e| {
            error!(
                "Failed to get agent port for {}/{}: {:?}",
                opencode_session_id, agent_id, e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!(
                "Agent {} not found or not running in session {}",
                agent_id, opencode_session_id
            );
            StatusCode::NOT_FOUND
        })?;

    let target_url = if let Some(directory) = query.directory.as_deref() {
        format!(
            "http://localhost:{}/event?directory={}",
            port,
            urlencoding::encode(directory)
        )
    } else {
        format!("http://localhost:{}/event", port)
    };
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

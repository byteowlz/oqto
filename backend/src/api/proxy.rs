//! HTTP and WebSocket proxy for container services.

use axum::{
    body::Body,
    extract::{Path, State, WebSocketUpgrade},
    http::{Request, StatusCode, Uri},
    response::{IntoResponse, Response, Sse},
};
use futures::{SinkExt, StreamExt};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use log::{debug, error, warn};
use tokio_tungstenite::connect_async;

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

    proxy_request(req, session.opencode_port as u16, &path).await
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

    proxy_request(req, session.fileserver_port as u16, &path).await
}

/// Generic HTTP proxy function.
async fn proxy_request(
    mut req: Request<Body>,
    target_port: u16,
    target_path: &str,
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

    // Create HTTP client
    let client: Client<_, Body> = Client::builder(TokioExecutor::new()).build_http();

    // Forward the request
    let response = client.request(req).await.map_err(|e| {
        error!("Proxy request failed: {:?}", e);
        StatusCode::BAD_GATEWAY
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

    if !session.is_active() {
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
async fn handle_terminal_proxy(
    client_socket: axum::extract::ws::WebSocket,
    ttyd_port: u16,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message as AxumMessage;
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    let ttyd_url = format!("ws://localhost:{}/ws", ttyd_port);
    debug!("Connecting to ttyd at {}", ttyd_url);

    // Connect to ttyd
    let (ttyd_socket, _) = connect_async(&ttyd_url).await?;
    let (mut ttyd_write, mut ttyd_read) = ttyd_socket.split();

    // Split client socket
    let (mut client_write, mut client_read) = client_socket.split();

    // Forward client -> ttyd
    let client_to_ttyd = async {
        while let Some(msg) = client_read.next().await {
            match msg {
                Ok(AxumMessage::Text(text)) => {
                    if ttyd_write
                        .send(TungsteniteMessage::Text(text.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(AxumMessage::Binary(data)) => {
                    if ttyd_write
                        .send(TungsteniteMessage::Binary(data))
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

    // Forward ttyd -> client
    let ttyd_to_client = async {
        while let Some(msg) = ttyd_read.next().await {
            match msg {
                Ok(TungsteniteMessage::Text(text)) => {
                    if client_write
                        .send(AxumMessage::Text(text.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(TungsteniteMessage::Binary(data)) => {
                    if client_write
                        .send(AxumMessage::Binary(data.to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
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

/// SSE events stream for opencode.
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
    // TODO: Aggregate events from all active opencode sessions
    let interval = time::interval(Duration::from_secs(30));
    let stream = StreamExt::map(IntervalStream::new(interval), |_| {
        Ok(Event::default().data("{\"type\":\"keepalive\"}"))
    });

    Ok(Sse::new(stream))
}

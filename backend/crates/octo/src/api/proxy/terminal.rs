//! Terminal (ttyd) WebSocket proxy.
//!
//! Handles the ttyd binary protocol and supports both Unix socket and TCP connections.

use axum::extract::ws::WebSocket;
use futures::{SinkExt, StreamExt};
use log::debug;
use std::path::Path as StdPath;
use std::time::Duration;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;

use super::builder::DEFAULT_WS_TIMEOUT;

// ============================================================================
// Ttyd Connection Types
// ============================================================================

/// Enum to handle both Unix socket and TCP WebSocket connections to ttyd.
enum TtydConnection {
    Unix(WebSocketStream<tokio::net::UnixStream>),
    Tcp(WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>),
}

impl TtydConnection {
    /// Split the connection into write and read halves.
    fn split(self) -> (TtydConnectionWrite, TtydConnectionRead) {
        match self {
            TtydConnection::Unix(ws) => {
                let (write, read) = ws.split();
                (
                    TtydConnectionWrite::Unix(write),
                    TtydConnectionRead::Unix(read),
                )
            }
            TtydConnection::Tcp(ws) => {
                let (write, read) = ws.split();
                (
                    TtydConnectionWrite::Tcp(write),
                    TtydConnectionRead::Tcp(read),
                )
            }
        }
    }
}

enum TtydConnectionWrite {
    Unix(
        futures::stream::SplitSink<
            WebSocketStream<tokio::net::UnixStream>,
            tokio_tungstenite::tungstenite::Message,
        >,
    ),
    Tcp(
        futures::stream::SplitSink<
            WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
            tokio_tungstenite::tungstenite::Message,
        >,
    ),
}

enum TtydConnectionRead {
    Unix(futures::stream::SplitStream<WebSocketStream<tokio::net::UnixStream>>),
    Tcp(
        futures::stream::SplitStream<
            WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        >,
    ),
}

impl TtydConnectionWrite {
    async fn send(
        &mut self,
        msg: tokio_tungstenite::tungstenite::Message,
    ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        match self {
            TtydConnectionWrite::Unix(w) => w.send(msg).await,
            TtydConnectionWrite::Tcp(w) => w.send(msg).await,
        }
    }
}

impl TtydConnectionRead {
    async fn next(
        &mut self,
    ) -> Option<
        Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>,
    > {
        match self {
            TtydConnectionRead::Unix(r) => r.next().await,
            TtydConnectionRead::Tcp(r) => r.next().await,
        }
    }
}

// ============================================================================
// Connection Functions
// ============================================================================

/// Connect to ttyd via Unix socket.
async fn connect_ttyd_unix(socket_path: &StdPath) -> anyhow::Result<TtydConnection> {
    use tokio::net::UnixStream;
    use tokio_tungstenite::client_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let stream = UnixStream::connect(socket_path).await?;

    // For Unix sockets, we still need a valid HTTP request
    // ttyd expects a WebSocket upgrade at /ws
    let mut request = "ws://localhost/ws".into_client_request()?;
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "tty".parse().unwrap());

    let (socket, _response) = client_async(request, stream).await?;
    Ok(TtydConnection::Unix(socket))
}

/// Connect to ttyd via TCP (for container mode or fallback).
async fn connect_ttyd_tcp(port: u16) -> anyhow::Result<TtydConnection> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let url = format!("ws://localhost:{}/ws", port);
    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "tty".parse().unwrap());

    let (socket, _response) = connect_async(request).await?;
    Ok(TtydConnection::Tcp(socket))
}

// ============================================================================
// Terminal Proxy Handler
// ============================================================================

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
/// 1. Connects to ttyd via Unix socket with the 'tty' subprotocol
/// 2. Sends the initial auth/resize message
/// 3. Translates between raw terminal data (from ghostty-web) and ttyd protocol
///
/// The Unix socket approach ensures sandboxed agents cannot connect to ttyd directly,
/// as the socket is in XDG_RUNTIME_DIR which is not mounted into the sandbox.
pub async fn handle_terminal_proxy(
    client_socket: WebSocket,
    session_id: &str,
    ttyd_port: u16, // Kept for fallback/logging
    initial_command: Option<String>,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message as AxumMessage;
    use tokio::time::Instant;
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    // Try Unix socket first (local mode), fall back to TCP (container mode)
    let socket_path = crate::local::ProcessManager::ttyd_socket_path(session_id);

    let start = Instant::now();
    let timeout = DEFAULT_WS_TIMEOUT;
    let mut attempts: u32 = 0;

    // Try to connect via Unix socket or TCP
    let ttyd_socket = loop {
        attempts += 1;

        // First try Unix socket (for local mode)
        if socket_path.exists() {
            match connect_ttyd_unix(&socket_path).await {
                Ok(socket) => {
                    debug!("Connected to ttyd via Unix socket: {:?}", socket_path);
                    break socket;
                }
                Err(err) => {
                    if start.elapsed() >= timeout {
                        return Err(anyhow::anyhow!(
                            "ttyd Unix socket not available after {} attempts over {:?}: {}",
                            attempts,
                            timeout,
                            err
                        ));
                    }
                    debug!(
                        "ttyd Unix socket not ready yet (attempt {}): {}",
                        attempts, err
                    );
                }
            }
        } else {
            // Fall back to TCP (for container mode or if socket doesn't exist yet)
            match connect_ttyd_tcp(ttyd_port).await {
                Ok(socket) => {
                    debug!("Connected to ttyd via TCP port {}", ttyd_port);
                    break socket;
                }
                Err(err) => {
                    if start.elapsed() >= timeout {
                        return Err(anyhow::anyhow!(
                            "ttyd not available after {} attempts over {:?}: {}",
                            attempts,
                            timeout,
                            err
                        ));
                    }
                    debug!("ttyd not ready yet (attempt {}): {}", attempts, err);
                }
            }
        }

        let backoff_ms = (attempts.min(20) as u64) * 100;
        let backoff = Duration::from_millis(backoff_ms);
        tokio::time::sleep(backoff).await;
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

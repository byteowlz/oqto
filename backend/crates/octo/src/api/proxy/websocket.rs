//! WebSocket proxy utilities for voice and browser streaming.
//!
//! Provides bidirectional WebSocket relay functionality.

use axum::extract::ws::WebSocket;
use futures::{SinkExt, StreamExt};
use log::debug;
use tokio_tungstenite::connect_async;

/// Handle bidirectional WebSocket proxy to a voice service (STT/TTS).
pub async fn handle_voice_ws_proxy(
    client_socket: WebSocket,
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

/// Handle WebSocket proxy for agent-browser streaming.
pub async fn handle_browser_stream_proxy(
    client_socket: WebSocket,
    stream_port: u16,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message as AxumMessage;
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    let target_url = format!("ws://127.0.0.1:{}", stream_port);
    debug!("Proxying browser stream WebSocket to {}", target_url);

    let start = tokio::time::Instant::now();
    let timeout = tokio::time::Duration::from_secs(10);
    let mut attempts: u32 = 0;

    let (server_socket, _) = loop {
        attempts += 1;
        match connect_async(&target_url).await {
            Ok(result) => break result,
            Err(err) => {
                if start.elapsed() >= timeout {
                    return Err(anyhow::anyhow!(
                        "agent-browser stream not available after {} attempts over {:?}: {}",
                        attempts,
                        timeout,
                        err
                    ));
                }
                let backoff_ms = (attempts.min(20) as u64) * 100;
                let backoff = tokio::time::Duration::from_millis(backoff_ms);
                debug!(
                    "agent-browser stream not ready yet (attempt {}): {}; retrying in {:?}",
                    attempts, err, backoff
                );
                tokio::time::sleep(backoff).await;
            }
        }
    };

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

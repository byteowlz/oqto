//! WebSocket proxy utilities for voice and browser streaming.
//!
//! Provides bidirectional WebSocket relay functionality.

use axum::extract::ws::WebSocket;
use futures::{SinkExt, StreamExt};
use log::{debug, info};
use serde_json::Value;
use tokio_tungstenite::connect_async;

use crate::agent_browser::BrowserEngine;

const UPSTREAM_DATA_URL_PREFIXES: [&str; 2] = ["data:image/jpeg;base64,", "data:image/png;base64,"];

/// Translate one upstream agent-browser stream message into the message shape
/// the Octo frontend expects (oqto-browserd stream protocol).
///
/// Upstream: {type:"status"|"tabs"|..., data?: "data:image/...;base64,..."}
/// Frontend: {type:"status", connected, screencasting, viewportWidth?,
///           viewportHeight?} | {type:"frame", data: <bare base64>, metadata}
///
/// Returns None for messages the frontend should not receive (tabs,
/// unparseable payloads).
pub fn translate_upstream_stream_message(raw: &str, viewport: &mut (u32, u32)) -> Option<String> {
    let mut msg: Value = serde_json::from_str(raw).ok()?;
    let obj = msg.as_object_mut()?;

    match obj.get("type").and_then(Value::as_str) {
        Some("tabs") | Some("error") => None,
        Some("status") => {
            // Upstream status shape already matches the frontend's
            // StreamMessage["status"]; cache the viewport for frame metadata.
            if let Some(w) = obj.get("viewportWidth").and_then(Value::as_u64) {
                viewport.0 = w as u32;
            }
            if let Some(h) = obj.get("viewportHeight").and_then(Value::as_u64) {
                viewport.1 = h as u32;
            }
            obj.entry("connected").or_insert(Value::Bool(true));
            Some(msg.to_string())
        }
        _ if obj
            .get("data")
            .and_then(Value::as_str)
            .is_some_and(|d| !d.is_empty()) =>
        {
            // Frame: upstream embeds the image as a data URL; the frontend
            // wants the bare base64 payload plus a full metadata object.
            let data = obj.get("data").and_then(Value::as_str)?;
            let b64 = UPSTREAM_DATA_URL_PREFIXES
                .iter()
                .find_map(|p| data.strip_prefix(p))
                .unwrap_or(data);
            let meta = obj.get("metadata").cloned().unwrap_or(Value::Null);
            let get_num = |key: &str, fallback: u32| -> u32 {
                meta.get(key)
                    .and_then(Value::as_f64)
                    .map(|v| v as u32)
                    .unwrap_or(fallback)
            };
            let metadata = serde_json::json!({
                "offsetTop": meta.get("offsetTop").cloned().unwrap_or(Value::from(0)),
                "pageScaleFactor": meta
                    .get("pageScaleFactor")
                    .cloned()
                    .unwrap_or(Value::from(1)),
                "deviceWidth": get_num("deviceWidth", viewport.0),
                "deviceHeight": get_num("deviceHeight", viewport.1),
                "scrollOffsetX": meta
                    .get("scrollOffsetX")
                    .cloned()
                    .unwrap_or(Value::from(0)),
                "scrollOffsetY": meta
                    .get("scrollOffsetY")
                    .cloned()
                    .unwrap_or(Value::from(0)),
            });
            Some(
                serde_json::json!({"type": "frame", "data": b64, "metadata": metadata}).to_string(),
            )
        }
        _ => None,
    }
}

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
    engine: BrowserEngine,
) -> anyhow::Result<()> {
    match engine {
        BrowserEngine::Legacy => handle_browser_stream_proxy_raw(client_socket, stream_port).await,
        BrowserEngine::Upstream => {
            handle_browser_stream_proxy_upstream(client_socket, stream_port).await
        }
    }
}

/// Proxy the upstream agent-browser stream WS, translating its protocol into
/// the oqto-browserd stream protocol the frontend speaks.
async fn handle_browser_stream_proxy_upstream(
    client_socket: WebSocket,
    stream_port: u16,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message as AxumMessage;
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    let target_url = format!("ws://127.0.0.1:{}", stream_port);
    info!(
        "Proxying upstream browser stream WebSocket to {}",
        target_url
    );

    let start = tokio::time::Instant::now();
    let wait = tokio::time::Duration::from_secs(10);
    let mut attempts: u32 = 0;
    let (server_socket, _) = loop {
        attempts += 1;
        match connect_async(&target_url).await {
            Ok(result) => break result,
            Err(err) => {
                if start.elapsed() >= wait {
                    return Err(anyhow::anyhow!(
                        "upstream browser stream not available after {} attempts over {:?}: {}",
                        attempts,
                        wait,
                        err
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_millis(
                    (attempts.min(20) as u64) * 100,
                ))
                .await;
            }
        }
    };

    let (mut client_tx, mut client_rx) = client_socket.split();
    let (mut server_tx, mut server_rx) = server_socket.split();

    // Enable screencasting: upstream waits for this control message before
    // emitting frames.
    server_tx
        .send(TungsteniteMessage::Text(
            serde_json::json!({"type": "screencast", "enabled": true})
                .to_string()
                .into(),
        ))
        .await?;

    // Cache the viewport from upstream status messages for frame metadata.
    let viewport = std::sync::Arc::new(tokio::sync::Mutex::new((0u32, 0u32)));
    let viewport_shared = std::sync::Arc::clone(&viewport);

    let server_to_client = async {
        while let Some(msg) = server_rx.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    debug!("upstream browser stream ended: {}", e);
                    break;
                }
            };
            let forward = match msg {
                TungsteniteMessage::Text(ref text) => {
                    let mut vp = viewport_shared.lock().await;
                    match translate_upstream_stream_message(text, &mut vp) {
                        Some(translated) => AxumMessage::Text(translated.into()),
                        None => continue,
                    }
                }
                TungsteniteMessage::Binary(data) => AxumMessage::Binary(data),
                TungsteniteMessage::Ping(d) => AxumMessage::Ping(d),
                TungsteniteMessage::Pong(d) => AxumMessage::Pong(d),
                TungsteniteMessage::Close(_) => AxumMessage::Close(None),
                TungsteniteMessage::Frame(_) => continue,
            };
            client_tx.send(forward).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    // The frontend already speaks the upstream input vocabulary
    // (input_mouse/input_keyboard/input_touch/status) -- pass through.
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

    tokio::select! {
        r = server_to_client => r,
        r = client_to_server => r,
    }?;
    Ok(())
}

/// Legacy raw proxy (oqto-browserd speaks the frontend protocol natively).
async fn handle_browser_stream_proxy_raw(
    client_socket: WebSocket,
    stream_port: u16,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message as AxumMessage;
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    let target_url = format!("ws://127.0.0.1:{}", stream_port);
    info!("Proxying browser stream WebSocket to {}", target_url);

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

    info!(
        "Browser stream proxy connected to upstream at port {}",
        stream_port
    );

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
                AxumMessage::Close(_) => {
                    info!("Browser stream: client sent close");
                    TungsteniteMessage::Close(None)
                }
            };
            server_tx.send(forward).await?;
        }
        info!("Browser stream: client_to_server loop ended (client disconnected)");
        Ok::<(), anyhow::Error>(())
    };

    let server_to_client = async {
        let mut msg_count: u64 = 0;
        while let Some(msg) = server_rx.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    info!(
                        "Browser stream: upstream recv error after {} msgs: {}",
                        msg_count, e
                    );
                    break;
                }
            };
            let forward = match msg {
                TungsteniteMessage::Text(ref text) => {
                    if msg_count < 3 {
                        debug!(
                            "Browser stream: msg #{} from upstream ({} bytes)",
                            msg_count,
                            text.len()
                        );
                    }
                    msg_count += 1;
                    AxumMessage::Text(text.to_string().into())
                }
                TungsteniteMessage::Binary(data) => {
                    if msg_count < 3 {
                        debug!(
                            "Browser stream: binary msg #{} ({} bytes)",
                            msg_count,
                            data.len()
                        );
                    }
                    msg_count += 1;
                    AxumMessage::Binary(data)
                }
                TungsteniteMessage::Ping(data) => AxumMessage::Ping(data),
                TungsteniteMessage::Pong(data) => AxumMessage::Pong(data),
                TungsteniteMessage::Close(_) => {
                    info!(
                        "Browser stream: upstream sent close after {} msgs",
                        msg_count
                    );
                    AxumMessage::Close(None)
                }
                TungsteniteMessage::Frame(_) => continue,
            };
            if let Err(e) = client_tx.send(forward).await {
                debug!(
                    "Browser stream: client send error after {} msgs: {}",
                    msg_count, e
                );
                break;
            }
        }
        info!(
            "Browser stream: server_to_client loop ended after {} msgs",
            msg_count
        );
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        result = client_to_server => {
            info!("Browser stream: client_to_server completed first: {:?}", result.as_ref().err());
            result?;
        },
        result = server_to_client => {
            info!("Browser stream: server_to_client completed first: {:?}", result.as_ref().err());
            result?;
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::translate_upstream_stream_message;

    #[test]
    fn upstream_status_passes_through_and_caches_viewport() {
        let mut vp = (0u32, 0u32);
        let raw = r#"{"type":"status","connected":true,"screencasting":true,"viewportHeight":720,"viewportWidth":1280}"#;
        let out = translate_upstream_stream_message(raw, &mut vp).expect("status forwarded");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["type"], "status");
        assert_eq!(v["connected"], true);
        assert_eq!(v["screencasting"], true);
        assert_eq!(v["viewportWidth"], 1280);
        assert_eq!(vp, (1280, 720));
    }

    #[test]
    fn upstream_frame_data_url_is_stripped_and_metadata_synthesized() {
        let mut vp = (1280u32, 720u32);
        let raw = r#"{"data":"data:image/jpeg;base64,QWJjZA==","timestamp":123.5}"#;
        let out = translate_upstream_stream_message(raw, &mut vp).expect("frame forwarded");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["type"], "frame");
        assert_eq!(v["data"], "QWJjZA==");
        let meta = &v["metadata"];
        assert_eq!(meta["deviceWidth"], 1280);
        assert_eq!(meta["deviceHeight"], 720);
        assert_eq!(meta["pageScaleFactor"], 1);
        assert_eq!(meta["offsetTop"], 0);
    }

    #[test]
    fn upstream_frame_metadata_is_preferred_over_viewport_fallback() {
        let mut vp = (1280u32, 720u32);
        let raw = r#"{"data":"data:image/jpeg;base64,QWJjZA==","metadata":{"deviceWidth":640,"deviceHeight":360,"offsetTop":12,"pageScaleFactor":0.5,"scrollOffsetX":3,"scrollOffsetY":4}}"#;
        let out = translate_upstream_stream_message(raw, &mut vp).expect("frame forwarded");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["metadata"]["deviceWidth"], 640);
        assert_eq!(v["metadata"]["offsetTop"], 12);
        assert_eq!(v["metadata"]["scrollOffsetY"], 4);
    }

    #[test]
    fn tabs_and_errors_are_dropped() {
        let mut vp = (0u32, 0u32);
        assert!(
            translate_upstream_stream_message(r#"{"type":"tabs","tabs":[]}"#, &mut vp).is_none()
        );
        assert!(
            translate_upstream_stream_message(r#"{"type":"error","message":"x"}"#, &mut vp)
                .is_none()
        );
    }
}

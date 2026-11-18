//! The byoagnt WebSocket binding (byoagnt ADR-0001): one JSON frame per text message, the
//! `byoagnt.v0` subprotocol, and the owner secret as a bearer credential in the upgrade
//! request. Non-browser clients use the `Authorization` header (the ADR's non-browser path);
//! the secret never appears in the URL, in argv or in logs.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// The subprotocol the binding requires.
pub const SUBPROTOCOL: &str = "byoagnt.v0";
/// Largest frame accepted from the host (the contract's maximum `max_frame_bytes`).
pub const MAX_FRAME_BYTES: usize = 16_777_216;

/// A bearer secret. Its `Debug` output never shows the value.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a secret; byoagnt requires 32 to 256 base64url characters.
    pub fn new(value: String) -> Result<Self> {
        let ok = (32..=256).contains(&value.len())
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            bail!("the host secret must be 32 to 256 base64url characters");
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// An open, authenticated connection.
pub struct Connection {
    stream: Stream,
}

/// Open a connection to `url` (`ws://` loopback or behind a trusted TLS terminator).
pub async fn connect(url: &str, secret: &Secret, timeout: Duration) -> Result<Connection> {
    let mut request = url.into_client_request().context("host URL")?;
    if request.uri().query().is_some() {
        bail!("the host URL must not carry a query; credentials never go in URLs");
    }
    let headers = request.headers_mut();
    headers.insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_static(SUBPROTOCOL),
    );
    let mut bearer =
        HeaderValue::from_str(&format!("Bearer {}", secret.expose())).context("secret header")?;
    bearer.set_sensitive(true);
    headers.insert("Authorization", bearer);
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_FRAME_BYTES))
        .max_frame_size(Some(MAX_FRAME_BYTES));
    let attempt = tokio_tungstenite::connect_async_with_config(request, Some(config), false);
    let (stream, response) = tokio::time::timeout(timeout, attempt)
        .await
        .map_err(|_| anyhow!("connecting timed out"))?
        .map_err(|e| anyhow!("connect: {}", describe(&e)))?;
    let chosen = response
        .headers()
        .get("Sec-WebSocket-Protocol")
        .and_then(|v| v.to_str().ok());
    if chosen != Some(SUBPROTOCOL) {
        bail!("the host did not accept {SUBPROTOCOL}");
    }
    Ok(Connection { stream })
}

/// Only fixed strings: an HTTP refusal says which status, never echoes headers.
fn describe(error: &tokio_tungstenite::tungstenite::Error) -> String {
    use tokio_tungstenite::tungstenite::Error;
    match error {
        Error::Http(response) => match response.status().as_u16() {
            401 => "the host refused the secret (401)".to_owned(),
            403 => "the host refused this origin (403)".to_owned(),
            503 => "the host is at capacity (503)".to_owned(),
            other => format!("HTTP {other}"),
        },
        Error::Io(io) => format!("{}", io.kind()),
        other => format!("{other}"),
    }
}

/// What came in.
#[derive(Debug)]
pub enum Incoming {
    /// One frame.
    Frame(Value),
    /// The host closed or the connection broke.
    Closed(String),
}

impl Connection {
    /// Send one frame as one text message.
    pub async fn send(&mut self, frame: &Value) -> Result<()> {
        let text = serde_json::to_string(frame).context("encode frame")?;
        self.stream
            .send(Message::text(text))
            .await
            .context("send frame")
    }

    /// The next frame, skipping pings.
    pub async fn next(&mut self) -> Incoming {
        loop {
            match self.stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    return match serde_json::from_str(text.as_str()) {
                        Ok(value) => Incoming::Frame(value),
                        Err(_) => {
                            Incoming::Closed("the host sent a frame that is not JSON".to_owned())
                        }
                    };
                }
                Some(Ok(Message::Close(frame))) => {
                    let reason = frame
                        .map(|f| format!("{} {}", u16::from(f.code), f.reason))
                        .unwrap_or_default();
                    return Incoming::Closed(format!("closed by host {reason}").trim().to_owned());
                }
                Some(Ok(Message::Binary(_))) => {
                    return Incoming::Closed("binary message from host".to_owned());
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => return Incoming::Closed(describe(&error)),
                None => return Incoming::Closed("connection ended".to_owned()),
            }
        }
    }

    /// Close politely.
    pub async fn close(mut self) {
        let _ = self.stream.close(None).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_debug_is_redacted() -> Result<()> {
        let secret = Secret::new("a".repeat(40))?;
        assert_eq!(format!("{secret:?}"), "Secret(..)");
        assert!(Secret::new("short".into()).is_err());
        assert!(Secret::new(format!("{}/", "a".repeat(40))).is_err());
        Ok(())
    }
}

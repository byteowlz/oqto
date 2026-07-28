//! Live check that a credentialed ttyd actually yields a shell.
//!
//! Three separate failures shipped because each layer was verified alone: the
//! readiness probe rejected the 401, `--check-origin` rejected an upgrade with
//! no Origin, and an empty AuthToken left the socket open with no child
//! process. Only an end-to-end connect catches that class.

use std::process::{Child, Command, Stdio};

use base64::Engine as _;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

fn ttyd_available() -> bool {
    Command::new("ttyd")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .unwrap()
        .port()
}

struct Ttyd(Child);

impl Drop for Ttyd {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn ttyd exactly as the runner does.
fn spawn_ttyd(port: u16, password: &str) -> Ttyd {
    let args = oqto_runner::daemon::server::build_ttyd_args(port, "/tmp", password);
    let child = Command::new("ttyd")
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ttyd");
    Ttyd(child)
}

async fn wait_listening(port: u16) {
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("ttyd never listened on {port}");
}

/// Connect the way the backend proxy does and return bytes produced by the shell.
async fn shell_bytes(port: u16, password: &str, auth_token: &str, origin: Option<&str>) -> usize {
    let credential = base64::engine::general_purpose::STANDARD.encode(format!("oqto:{}", password));

    let mut request = format!("ws://localhost:{port}/ws")
        .into_client_request()
        .expect("request");
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "tty".parse().unwrap());
    request.headers_mut().insert(
        "Authorization",
        format!("Basic {credential}").parse().unwrap(),
    );
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("Origin", origin.parse().unwrap());
    }

    let Ok((mut ws, _)) = tokio_tungstenite::connect_async(request).await else {
        return 0;
    };

    let init = serde_json::json!({
        "AuthToken": auth_token,
        "columns": 80,
        "rows": 24,
    })
    .to_string();
    if ws
        .send(tokio_tungstenite::tungstenite::Message::Binary(
            init.into_bytes().into(),
        ))
        .await
        .is_err()
    {
        return 0;
    }

    let mut total = 0usize;
    for _ in 0..30 {
        match tokio::time::timeout(std::time::Duration::from_millis(300), ws.next()).await {
            Ok(Some(Ok(msg))) => {
                total += msg.len();
                if total > 40 {
                    break;
                }
            }
            Ok(None) | Err(_) => {}
            Ok(Some(Err(_))) => break,
        }
    }
    total
}

#[tokio::test]
async fn a_credentialed_ttyd_yields_a_shell() {
    if !ttyd_available() {
        eprintln!("skipping: ttyd not installed");
        return;
    }
    let port = free_port();
    let password = "0123456789abcdef0123456789abcdef";
    let _ttyd = spawn_ttyd(port, password);
    wait_listening(port).await;

    let credential = base64::engine::general_purpose::STANDARD.encode(format!("oqto:{password}"));
    let origin = format!("http://localhost:{port}");

    let bytes = shell_bytes(port, password, &credential, Some(&origin)).await;
    assert!(
        bytes > 0,
        "the proxy's handshake and AuthToken must produce a shell"
    );
}

#[tokio::test]
async fn an_empty_auth_token_produces_no_shell() {
    if !ttyd_available() {
        eprintln!("skipping: ttyd not installed");
        return;
    }
    let port = free_port();
    let password = "0123456789abcdef0123456789abcdef";
    let _ttyd = spawn_ttyd(port, password);
    wait_listening(port).await;

    let origin = format!("http://localhost:{port}");
    let bytes = shell_bytes(port, password, "", Some(&origin)).await;
    assert_eq!(
        bytes, 0,
        "guards the diagnosis: an empty AuthToken is why a terminal connected \
         and never printed a prompt"
    );
}

#[tokio::test]
async fn a_missing_origin_is_rejected() {
    if !ttyd_available() {
        eprintln!("skipping: ttyd not installed");
        return;
    }
    let port = free_port();
    let password = "0123456789abcdef0123456789abcdef";
    let _ttyd = spawn_ttyd(port, password);
    wait_listening(port).await;

    let credential = base64::engine::general_purpose::STANDARD.encode(format!("oqto:{password}"));
    let bytes = shell_bytes(port, password, &credential, None).await;
    assert_eq!(
        bytes, 0,
        "guards the diagnosis: --check-origin rejects an upgrade with no Origin"
    );
}

#![cfg(target_os = "linux")]

//! ExposePort verb: placement-neutral port exposure. With an expose dir the
//! runner bridges a Unix socket to the loopback port; without one it returns
//! the loopback address itself.

use anyhow::{Context, Result};
use oqto_runner::protocol::ExposedEndpoint;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct RunnerProcess {
    child: std::process::Child,
}

impl Drop for RunnerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn spawn_runner(
    temp: &std::path::Path,
    socket: &std::path::Path,
    expose_dir: Option<&std::path::Path>,
) -> Result<RunnerProcess> {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_oqto-runner"));
    command
        .arg("--socket")
        .arg(socket)
        .env("HOME", temp)
        .env("XDG_STATE_HOME", temp.join("state"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(dir) = expose_dir {
        command.arg("--expose-dir").arg(dir);
    }
    let child = command.spawn().context("spawning oqto-runner")?;
    let process = RunnerProcess { child };

    let client = oqto_runner::client::RunnerClient::new(socket.to_path_buf());
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if client.ensure_ready_with_recovery().await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .context("runner did not become ready")?;
    Ok(process)
}

async fn spawn_echo_server() -> Result<u16> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 64];
                while let Ok(n) = stream.read(&mut buf).await {
                    if n == 0 || stream.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    Ok(port)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expose_port_bridges_unix_socket_to_loopback() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let socket = temp.path().join("runner.sock");
    let expose_dir = temp.path().join("expose");
    let _runner = spawn_runner(temp.path(), &socket, Some(&expose_dir)).await?;
    let client = oqto_runner::client::RunnerClient::new(socket.clone());

    let echo_port = spawn_echo_server().await?;
    let endpoint = client.expose_port(echo_port).await?;
    let ExposedEndpoint::Unix { path } = &endpoint else {
        anyhow::bail!("expected unix endpoint, got {endpoint:?}");
    };
    assert_eq!(
        path,
        &expose_dir.join(format!("port-{echo_port}.sock")),
        "socket path must be deterministic"
    );
    assert!(path.exists());

    // Bytes flow through the bridge to the loopback service.
    let mut stream = tokio::net::UnixStream::connect(path).await?;
    stream.write_all(b"ping").await?;
    let mut reply = [0u8; 4];
    stream.read_exact(&mut reply).await?;
    assert_eq!(&reply, b"ping");

    // Idempotent: a second expose returns the same endpoint.
    assert_eq!(client.expose_port(echo_port).await?, endpoint);

    // Teardown removes the socket.
    client.unexpose_port(echo_port).await?;
    assert!(!path.exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expose_port_without_expose_dir_returns_loopback() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let socket = temp.path().join("runner.sock");
    let _runner = spawn_runner(temp.path(), &socket, None).await?;
    let client = oqto_runner::client::RunnerClient::new(socket);

    let endpoint = client.expose_port(4101).await?;
    assert_eq!(
        endpoint,
        ExposedEndpoint::Tcp {
            host: "127.0.0.1".to_string(),
            port: 4101
        }
    );
    Ok(())
}

//! Unix-socket-to-loopback reverse bridge (host -> container direction).
//!
//! Mirror of `endpoint_bridge`: inside an isolated workspace container the
//! runner listens on a Unix socket in the directory shared with the host
//! (`/run/oqto`) and forwards bytes to a loopback port where a workspace
//! service (fileserver, ttyd, previews) listens. The host connects to the
//! bind-mounted socket; the socket is the only reachability granted.

use anyhow::{Context, Result};
use log::{info, warn};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use tokio::net::{TcpStream, UnixListener};

/// Socket filename for an exposed port.
pub fn socket_name(port: u16) -> String {
    format!("port-{port}.sock")
}

/// A running reverse bridge; dropping the handle tears it down and removes
/// the socket file.
pub struct ReverseBridge {
    pub port: u16,
    pub socket: PathBuf,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for ReverseBridge {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Spawn a reverse bridge: Unix listener at `dir/port-<n>.sock` forwarding to
/// `127.0.0.1:<port>`. The socket is world-connectable when
/// `OQTO_SOCKET_MODE=world` (auto-userns placements), group-connectable
/// otherwise — same policy as the runner socket.
pub fn spawn(dir: &Path, port: u16) -> Result<ReverseBridge> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating expose directory {}", dir.display()))?;
    let socket = dir.join(socket_name(port));
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket)
        .with_context(|| format!("binding expose socket {}", socket.display()))?;
    use std::os::unix::fs::PermissionsExt;
    let mode = if std::env::var("OQTO_SOCKET_MODE").as_deref() == Ok("world") {
        0o666
    } else {
        0o770
    };
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting permissions on {}", socket.display()))?;

    let target = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    info!("reverse bridge: {} -> {target}", socket.display());
    let task = tokio::spawn(async move {
        loop {
            let (mut inbound, _) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    warn!("reverse bridge port {port} accept failed: {error}");
                    continue;
                }
            };
            tokio::spawn(async move {
                match TcpStream::connect(target).await {
                    Ok(mut outbound) => {
                        if let Err(error) =
                            tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await
                        {
                            warn!("reverse bridge port {port} stream ended with error: {error}");
                        }
                    }
                    Err(error) => {
                        warn!("reverse bridge target {target} unavailable: {error}");
                    }
                }
            });
        }
    });

    Ok(ReverseBridge { port, socket, task })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn bridge_forwards_bytes_to_loopback_port() -> Result<()> {
        let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = tcp_listener.local_addr()?.port();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = tcp_listener.accept().await else {
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

        let temp = tempfile::tempdir()?;
        let bridge = spawn(temp.path(), port)?;
        assert!(bridge.socket.exists());

        let mut client = tokio::net::UnixStream::connect(&bridge.socket).await?;
        client.write_all(b"ping").await?;
        let mut reply = [0u8; 4];
        client.read_exact(&mut reply).await?;
        assert_eq!(&reply, b"ping");

        let socket = bridge.socket.clone();
        drop(bridge);
        assert!(!socket.exists());
        Ok(())
    }
}

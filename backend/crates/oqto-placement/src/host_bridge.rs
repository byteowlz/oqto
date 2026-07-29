//! Host side of a granted workspace endpoint: a Unix socket (bind-mounted
//! into the isolated container) forwarded to one configured TCP target.
//! The socket is the capability; the workspace cannot pick targets.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::net::{TcpStream, UnixListener};
use tokio::task::JoinHandle;
use tracing::warn;

pub struct HostEndpointBridge {
    handle: JoinHandle<()>,
    socket_path: PathBuf,
}

impl HostEndpointBridge {
    pub async fn spawn(socket_path: PathBuf, target: String) -> Result<Self> {
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("creating endpoint directory {}", parent.display()))?;
        }
        match tokio::fs::remove_file(&socket_path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("removing stale endpoint socket {}", socket_path.display())
                });
            }
        }
        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("binding endpoint socket {}", socket_path.display()))?;
        // World-connectable: auto-userns containers cannot satisfy uid/group
        // checks. The backend-private parent directory is the access control.
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o666))
                .with_context(|| {
                    format!("setting endpoint socket mode {}", socket_path.display())
                })?;
        }
        let handle = tokio::spawn({
            let target = target.clone();
            async move {
                loop {
                    let (mut inbound, _) = match listener.accept().await {
                        Ok(accepted) => accepted,
                        Err(error) => {
                            warn!(%error, "endpoint bridge accept failed");
                            continue;
                        }
                    };
                    let target = target.clone();
                    tokio::spawn(async move {
                        match TcpStream::connect(&target).await {
                            Ok(mut outbound) => {
                                if let Err(error) =
                                    tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await
                                {
                                    warn!(%error, target, "endpoint stream ended with error");
                                }
                            }
                            Err(error) => {
                                warn!(%error, target, "endpoint target unavailable");
                            }
                        }
                    });
                }
            }
        });
        Ok(Self {
            handle,
            socket_path,
        })
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }
}

impl Drop for HostEndpointBridge {
    fn drop(&mut self) {
        self.handle.abort();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn bridge_forwards_unix_to_tcp() -> Result<()> {
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let target = tcp.local_addr()?.to_string();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = tcp.accept().await else {
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
        let socket = temp.path().join("endpoints").join("svc.sock");
        let bridge = HostEndpointBridge::spawn(socket.clone(), target).await?;

        let mut client = tokio::net::UnixStream::connect(&socket).await?;
        client.write_all(b"ping").await?;
        let mut reply = [0u8; 4];
        client.read_exact(&mut reply).await?;
        assert_eq!(&reply, b"ping");

        drop(bridge);
        assert!(!socket.exists());
        Ok(())
    }
}

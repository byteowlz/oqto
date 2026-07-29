//! Loopback-to-Unix-socket endpoint bridge.
//!
//! Inside an isolated (network=none) workspace container, each granted
//! service endpoint is a bind-mounted Unix socket. Tools expect TCP, so the
//! runner listens on a loopback port and forwards bytes to the socket. The
//! socket is the capability: nothing is reachable that was not mounted.

use anyhow::{Context, Result};
use log::{info, warn};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use tokio::net::{TcpListener, UnixStream};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointBridgeSpec {
    pub name: String,
    pub port: u16,
    pub socket: PathBuf,
}

impl EndpointBridgeSpec {
    /// Parse `name=port`, resolving the socket under `socket_dir`.
    pub fn parse(argument: &str, socket_dir: &std::path::Path) -> Result<Self> {
        let (name, port) = argument
            .split_once('=')
            .context("endpoint must be NAME=PORT")?;
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            anyhow::bail!("endpoint name must be alphanumeric/dash/underscore: {name}");
        }
        let port: u16 = port
            .parse()
            .with_context(|| format!("invalid endpoint port in {argument}"))?;
        if port == 0 {
            anyhow::bail!("endpoint port must be non-zero");
        }
        Ok(Self {
            name: name.to_string(),
            port,
            socket: socket_dir.join(format!("{name}.sock")),
        })
    }
}

/// Serve one endpoint bridge until the process exits.
pub async fn serve(spec: EndpointBridgeSpec) -> Result<()> {
    let listen = SocketAddr::from((Ipv4Addr::LOCALHOST, spec.port));
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("binding endpoint {} on {listen}", spec.name))?;
    info!(
        "endpoint bridge {}: 127.0.0.1:{} -> {}",
        spec.name,
        spec.port,
        spec.socket.display()
    );
    loop {
        let (mut inbound, _) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                warn!("endpoint {} accept failed: {error}", spec.name);
                continue;
            }
        };
        let socket = spec.socket.clone();
        let name = spec.name.clone();
        tokio::spawn(async move {
            match UnixStream::connect(&socket).await {
                Ok(mut outbound) => {
                    if let Err(error) =
                        tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await
                    {
                        warn!("endpoint {name} stream ended with error: {error}");
                    }
                }
                Err(error) => {
                    warn!(
                        "endpoint {name} target {} unavailable: {error}",
                        socket.display()
                    );
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn parse_rejects_bad_specs() {
        let dir = std::path::Path::new("/run/oqto/endpoints");
        assert!(EndpointBridgeSpec::parse("eavs=3033", dir).is_ok());
        assert!(EndpointBridgeSpec::parse("eavs", dir).is_err());
        assert!(EndpointBridgeSpec::parse("bad name=80", dir).is_err());
        assert!(EndpointBridgeSpec::parse("x=0", dir).is_err());
        assert!(EndpointBridgeSpec::parse("x=notaport", dir).is_err());
    }

    #[tokio::test]
    async fn bridge_forwards_bytes_to_unix_socket() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let socket = temp.path().join("echo.sock");
        let unix_listener = tokio::net::UnixListener::bind(&socket)?;
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = unix_listener.accept().await else {
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

        // Pick a free port by binding then dropping.
        let probe = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = probe.local_addr()?.port();
        drop(probe);

        let spec = EndpointBridgeSpec {
            name: "echo".to_string(),
            port,
            socket,
        };
        tokio::spawn(serve(spec));

        let mut client = loop {
            match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                Ok(stream) => break stream,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            }
        };
        client.write_all(b"ping").await?;
        let mut reply = [0u8; 4];
        client.read_exact(&mut reply).await?;
        assert_eq!(&reply, b"ping");
        Ok(())
    }
}

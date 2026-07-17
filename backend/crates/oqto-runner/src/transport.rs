//! Transport seam for runner protocol connections.
//!
//! Protocol code consumes [`RunnerIo`] streams. Concrete connectors only own
//! endpoint discovery and stream establishment; they do not implement runner
//! requests, responses, or event semantics.

use anyhow::{Context, Result};
use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;

/// Bidirectional byte stream accepted by the runner wire protocol.
pub trait RunnerIo: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> RunnerIo for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

/// Type-erased runner byte stream returned by transport connectors.
pub type BoxedRunnerIo = Box<dyn RunnerIo>;

/// Future returned while establishing a runner connection.
pub type ConnectFuture<'a> = Pin<Box<dyn Future<Output = Result<BoxedRunnerIo>> + Send + 'a>>;

/// Future returned while accepting an inbound runner connection.
pub type AcceptFuture<'a> = Pin<Box<dyn Future<Output = Result<BoxedRunnerIo>> + Send + 'a>>;

/// Serializable endpoint selected by placement routing.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum RunnerEndpointConfig {
    Unix {
        path: PathBuf,
    },
    TcpTls {
        address: SocketAddr,
        server_name: String,
        ca: PathBuf,
        certificate: PathBuf,
        key: PathBuf,
    },
}

impl RunnerEndpointConfig {
    pub fn connector(&self) -> Result<Arc<dyn RunnerConnector>> {
        match self {
            Self::Unix { path } => Ok(Arc::new(UnixRunnerConnector::new(path))),
            Self::TcpTls {
                address,
                server_name,
                ca,
                certificate,
                key,
            } => {
                let config = crate::tls::client_config(ca, certificate, key)?;
                Ok(Arc::new(crate::tls::TcpTlsRunnerConnector::new(
                    *address,
                    server_name,
                    config,
                )))
            }
        }
    }
}

/// Establishes authenticated byte streams to one runner endpoint.
///
/// Authentication is transport-specific. The Unix implementation relies on
/// the existing runtime-directory ownership and socket permissions. Network
/// implementations must provide verified peer identity before returning.
pub trait RunnerConnector: fmt::Debug + Send + Sync {
    /// Connect to the configured runner endpoint.
    fn connect(&self) -> ConnectFuture<'_>;

    /// Human-readable endpoint description for diagnostics. Must not contain
    /// credentials.
    fn endpoint_description(&self) -> String;

    /// Return the Unix path when this is a Unix connector.
    fn unix_socket_path(&self) -> Option<&Path> {
        None
    }
}

/// Accepts authenticated byte streams for the runner daemon.
pub trait RunnerListener: fmt::Debug + Send + Sync {
    /// Accept the next authenticated connection.
    fn accept(&self) -> AcceptFuture<'_>;

    /// Human-readable listener description for diagnostics.
    fn endpoint_description(&self) -> String;
}

/// Unix-domain-socket runner connector used by local placements.
#[derive(Clone, Debug)]
pub struct UnixRunnerConnector {
    socket_path: PathBuf,
}

impl UnixRunnerConnector {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }
}

impl RunnerConnector for UnixRunnerConnector {
    fn connect(&self) -> ConnectFuture<'_> {
        Box::pin(async move {
            let stream = UnixStream::connect(&self.socket_path)
                .await
                .with_context(|| {
                    format!("connecting to runner at {}", self.socket_path.display())
                })?;
            Ok(Box::new(stream) as BoxedRunnerIo)
        })
    }

    fn endpoint_description(&self) -> String {
        format!("unix:{}", self.socket_path.display())
    }

    fn unix_socket_path(&self) -> Option<&Path> {
        Some(&self.socket_path)
    }
}

/// Unix-domain-socket listener adapter used by the runner daemon.
#[derive(Debug)]
pub struct UnixRunnerListener {
    listener: tokio::net::UnixListener,
    socket_path: PathBuf,
}

impl UnixRunnerListener {
    pub fn new(listener: tokio::net::UnixListener, socket_path: impl Into<PathBuf>) -> Self {
        Self {
            listener,
            socket_path: socket_path.into(),
        }
    }
}

impl RunnerListener for UnixRunnerListener {
    fn accept(&self) -> AcceptFuture<'_> {
        Box::pin(async move {
            let (stream, _) = self.listener.accept().await.with_context(|| {
                format!(
                    "accepting runner connection on {}",
                    self.socket_path.display()
                )
            })?;
            Ok(Box::new(stream) as BoxedRunnerIo)
        })
    }

    fn endpoint_description(&self) -> String {
        format!("unix:{}", self.socket_path.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn endpoint_config_round_trips_without_transport_secrets() -> Result<()> {
        let endpoint = RunnerEndpointConfig::TcpTls {
            address: "127.0.0.1:7443".parse()?,
            server_name: "runner.internal".to_string(),
            ca: PathBuf::from("/run/oqto/ca.pem"),
            certificate: PathBuf::from("/run/oqto/client.pem"),
            key: PathBuf::from("/run/oqto/client-key.pem"),
        };
        let encoded = serde_json::to_string(&endpoint)?;
        assert!(encoded.contains("tcp_tls"));
        assert!(!encoded.contains("PRIVATE KEY"));
        let _: RunnerEndpointConfig = serde_json::from_str(&encoded)?;
        Ok(())
    }

    #[tokio::test]
    async fn boxed_runner_io_supports_in_process_duplex_streams() -> Result<()> {
        let (client, mut server) = tokio::io::duplex(64);
        let mut stream: BoxedRunnerIo = Box::new(client);

        let server_task = tokio::spawn(async move {
            let mut request = [0_u8; 4];
            server.read_exact(&mut request).await?;
            server.write_all(&request).await?;
            Result::<()>::Ok(())
        });

        stream.write_all(b"ping").await?;
        let mut response = [0_u8; 4];
        stream.read_exact(&mut response).await?;
        assert_eq!(&response, b"ping");
        server_task.await.context("joining duplex server")??;
        Ok(())
    }
}

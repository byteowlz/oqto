//! Mutually authenticated TCP/TLS runner transport.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use tokio_rustls::rustls::server::WebPkiClientVerifier;
use tokio_rustls::rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::transport::{
    AcceptFuture, BoxedRunnerIo, ConnectFuture, RunnerConnector, RunnerListener,
};

fn load_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let file =
        File::open(path).with_context(|| format!("opening certificate file {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let certificates = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("parsing certificate file {}", path.display()))?;
    if certificates.is_empty() {
        anyhow::bail!(
            "certificate file {} contains no certificates",
            path.display()
        );
    }
    Ok(certificates)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)
            .with_context(|| format!("reading key metadata {}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            anyhow::bail!(
                "runner TLS key {} must not be accessible by group or other users (mode {:o})",
                path.display(),
                mode & 0o777
            );
        }
    }
    let file = File::open(path).with_context(|| format!("opening key file {}", path.display()))?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)
        .with_context(|| format!("parsing key file {}", path.display()))?
        .ok_or_else(|| anyhow::anyhow!("key file {} contains no private key", path.display()))
}

fn load_roots(path: &Path) -> Result<RootCertStore> {
    let mut roots = RootCertStore::empty();
    let certificates = load_certificates(path)?;
    let (added, ignored) = roots.add_parsable_certificates(certificates);
    if added == 0 {
        anyhow::bail!(
            "CA file {} contains no usable trust anchors",
            path.display()
        );
    }
    if ignored > 0 {
        tracing::warn!(path = %path.display(), ignored, "ignored malformed CA certificates");
    }
    Ok(roots)
}

/// Build a client configuration that authenticates both server and client.
pub fn client_config(
    ca_path: &Path,
    certificate_path: &Path,
    key_path: &Path,
) -> Result<Arc<ClientConfig>> {
    let roots = load_roots(ca_path)?;
    let certificates = load_certificates(certificate_path)?;
    let key = load_private_key(key_path)?;
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(certificates, key)
        .context("building runner mTLS client configuration")?;
    Ok(Arc::new(config))
}

/// Build a server configuration that requires a trusted client certificate.
pub fn server_config(
    client_ca_path: &Path,
    certificate_path: &Path,
    key_path: &Path,
) -> Result<Arc<ServerConfig>> {
    let client_roots = load_roots(client_ca_path)?;
    let verifier = WebPkiClientVerifier::builder(Arc::new(client_roots))
        .build()
        .context("building runner client-certificate verifier")?;
    let certificates = load_certificates(certificate_path)?;
    let key = load_private_key(key_path)?;
    let config = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certificates, key)
        .context("building runner mTLS server configuration")?;
    Ok(Arc::new(config))
}

/// TCP/mTLS connector for a statically reachable runner.
#[derive(Clone)]
pub struct TcpTlsRunnerConnector {
    address: SocketAddr,
    server_name: String,
    connector: TlsConnector,
}

impl std::fmt::Debug for TcpTlsRunnerConnector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TcpTlsRunnerConnector")
            .field("address", &self.address)
            .field("server_name", &self.server_name)
            .finish_non_exhaustive()
    }
}

impl TcpTlsRunnerConnector {
    pub fn new(
        address: SocketAddr,
        server_name: impl Into<String>,
        config: Arc<ClientConfig>,
    ) -> Self {
        Self {
            address,
            server_name: server_name.into(),
            connector: TlsConnector::from(config),
        }
    }
}

impl RunnerConnector for TcpTlsRunnerConnector {
    fn connect(&self) -> ConnectFuture<'_> {
        Box::pin(async move {
            let tcp = TcpStream::connect(self.address)
                .await
                .with_context(|| format!("connecting to runner at {}", self.address))?;
            tcp.set_nodelay(true).context("enabling TCP_NODELAY")?;
            let server_name = ServerName::try_from(self.server_name.clone())
                .context("invalid runner TLS server name")?;
            let stream = self
                .connector
                .connect(server_name, tcp)
                .await
                .with_context(|| format!("authenticating runner at {}", self.address))?;
            Ok(Box::new(stream) as BoxedRunnerIo)
        })
    }

    fn endpoint_description(&self) -> String {
        format!("tcp+mtls://{} ({})", self.address, self.server_name)
    }
}

/// TCP listener that completes mutual TLS before yielding a runner stream.
pub struct TcpTlsRunnerListener {
    listener: TcpListener,
    acceptor: TlsAcceptor,
    address: SocketAddr,
}

impl std::fmt::Debug for TcpTlsRunnerListener {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TcpTlsRunnerListener")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl TcpTlsRunnerListener {
    pub async fn bind(address: SocketAddr, config: Arc<ServerConfig>) -> Result<Self> {
        let listener = TcpListener::bind(address)
            .await
            .with_context(|| format!("binding runner mTLS listener at {address}"))?;
        let address = listener
            .local_addr()
            .context("reading runner listener address")?;
        Ok(Self {
            listener,
            acceptor: TlsAcceptor::from(config),
            address,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
}

impl RunnerListener for TcpTlsRunnerListener {
    fn accept(&self) -> AcceptFuture<'_> {
        Box::pin(async move {
            let (tcp, peer) =
                self.listener.accept().await.with_context(|| {
                    format!("accepting runner TCP connection on {}", self.address)
                })?;
            tcp.set_nodelay(true).context("enabling TCP_NODELAY")?;
            let stream = self
                .acceptor
                .accept(tcp)
                .await
                .with_context(|| format!("authenticating runner TLS peer {peer}"))?;
            Ok(Box::new(stream) as BoxedRunnerIo)
        })
    }

    fn endpoint_description(&self) -> String {
        format!("tcp+mtls://{}", self.address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{read_json_frame, write_json_frame};
    use rcgen::generate_simple_self_signed;
    use std::io::Write;
    use tempfile::TempDir;
    use tokio::io::BufReader;

    struct IdentityFiles {
        _temp: TempDir,
        cert: std::path::PathBuf,
        key: std::path::PathBuf,
    }

    fn identity_files() -> Result<IdentityFiles> {
        let generated = generate_simple_self_signed(vec!["localhost".to_string()])?;
        let temp = tempfile::tempdir()?;
        let cert = temp.path().join("identity.pem");
        let key = temp.path().join("identity-key.pem");
        File::create(&cert)?.write_all(generated.cert.pem().as_bytes())?;
        File::create(&key)?.write_all(generated.key_pair.serialize_pem().as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(IdentityFiles {
            _temp: temp,
            cert,
            key,
        })
    }

    #[tokio::test]
    async fn mutual_tls_transport_carries_runner_frames() -> Result<()> {
        let identity = identity_files()?;
        let server_config = server_config(&identity.cert, &identity.cert, &identity.key)?;
        let client_config = client_config(&identity.cert, &identity.cert, &identity.key)?;
        let listener = TcpTlsRunnerListener::bind("127.0.0.1:0".parse()?, server_config).await?;
        let connector =
            TcpTlsRunnerConnector::new(listener.local_addr(), "localhost", client_config);

        let server_task = tokio::spawn(async move {
            let stream = listener.accept().await?;
            let mut stream = BufReader::new(stream);
            let request: String = read_json_frame(&mut stream)
                .await?
                .ok_or_else(|| anyhow::anyhow!("client closed before request"))?;
            write_json_frame(stream.get_mut(), &format!("ack:{request}")).await
        });

        let mut stream = connector.connect().await?;
        write_json_frame(&mut stream, &"ping").await?;
        let mut stream = BufReader::new(stream);
        let response: Option<String> = read_json_frame(&mut stream).await?;
        assert_eq!(response.as_deref(), Some("ack:ping"));
        server_task.await.context("joining mTLS server")??;
        Ok(())
    }

    #[tokio::test]
    async fn client_without_trusted_identity_fails_closed() -> Result<()> {
        let server_identity = identity_files()?;
        let untrusted_identity = identity_files()?;
        let server_config = server_config(
            &server_identity.cert,
            &server_identity.cert,
            &server_identity.key,
        )?;
        let untrusted_client = client_config(
            &server_identity.cert,
            &untrusted_identity.cert,
            &untrusted_identity.key,
        )?;
        let listener = TcpTlsRunnerListener::bind("127.0.0.1:0".parse()?, server_config).await?;
        let connector =
            TcpTlsRunnerConnector::new(listener.local_addr(), "localhost", untrusted_client);

        let server_task = tokio::spawn(async move { listener.accept().await });
        let client_result = connector.connect().await;
        let server_result = server_task.await.context("joining rejecting mTLS server")?;
        assert!(client_result.is_err() || server_result.is_err());
        Ok(())
    }
}

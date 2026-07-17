//! Optional Iroh/QUIC runner transport for NATed and changing networks.

use anyhow::{Context, Result};
use iroh::endpoint::{RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, EndpointId};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::transport::{
    AcceptFuture, BoxedRunnerIo, ConnectFuture, RunnerConnector, RunnerListener,
};

pub const OQTO_RUNNER_ALPN: &[u8] = b"oqto/runner/1";

fn bridge_iroh_stream(mut send: SendStream, mut recv: RecvStream) -> BoxedRunnerIo {
    let (client, bridge) = tokio::io::duplex(64 * 1024);
    let (mut bridge_read, mut bridge_write) = tokio::io::split(bridge);

    tokio::spawn(async move {
        let mut buffer = vec![0_u8; 16 * 1024];
        loop {
            match bridge_read.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if send.write_all(&buffer[..count]).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = send.finish();
    });

    tokio::spawn(async move {
        let mut buffer = vec![0_u8; 16 * 1024];
        while let Ok(Some(count)) = recv.read(&mut buffer).await {
            if bridge_write.write_all(&buffer[..count]).await.is_err() {
                break;
            }
        }
        let _ = bridge_write.shutdown().await;
    });

    Box::new(client)
}

#[derive(Clone)]
pub struct IrohRunnerConnector {
    endpoint: Endpoint,
    remote: EndpointAddr,
}

impl std::fmt::Debug for IrohRunnerConnector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IrohRunnerConnector")
            .field("local", &self.endpoint.id())
            .field("remote", &self.remote.id)
            .finish_non_exhaustive()
    }
}

impl IrohRunnerConnector {
    pub fn new(endpoint: Endpoint, remote: EndpointAddr) -> Self {
        Self { endpoint, remote }
    }
}

impl RunnerConnector for IrohRunnerConnector {
    fn connect(&self) -> ConnectFuture<'_> {
        Box::pin(async move {
            let connection = self
                .endpoint
                .connect(self.remote.clone(), OQTO_RUNNER_ALPN)
                .await
                .with_context(|| format!("connecting to Iroh runner {}", self.remote.id))?;
            let (send, recv) = connection
                .open_bi()
                .await
                .context("opening Iroh runner stream")?;
            Ok(bridge_iroh_stream(send, recv))
        })
    }

    fn endpoint_description(&self) -> String {
        format!("iroh:{}", self.remote.id)
    }
}

/// Iroh listener with an explicit endpoint-ID allowlist.
pub struct IrohRunnerListener {
    endpoint: Endpoint,
    allowed_peers: Arc<HashSet<EndpointId>>,
    streams: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<BoxedRunnerIo>>>,
}

impl std::fmt::Debug for IrohRunnerListener {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IrohRunnerListener")
            .field("endpoint", &self.endpoint.id())
            .field("allowed_peer_count", &self.allowed_peers.len())
            .finish_non_exhaustive()
    }
}

impl IrohRunnerListener {
    pub fn new(endpoint: Endpoint, allowed_peers: HashSet<EndpointId>) -> Result<Self> {
        if allowed_peers.is_empty() {
            anyhow::bail!("Iroh runner listener requires at least one authorized endpoint ID");
        }
        let allowed_peers = Arc::new(allowed_peers);
        let (stream_tx, stream_rx) = tokio::sync::mpsc::channel(32);
        let accept_endpoint = endpoint.clone();
        let accept_peers = allowed_peers.clone();
        tokio::spawn(async move {
            while let Some(incoming) = accept_endpoint.accept().await {
                let connection = match incoming.await {
                    Ok(connection) => connection,
                    Err(error) => {
                        tracing::warn!(%error, "failed accepting Iroh runner connection");
                        continue;
                    }
                };
                let remote = connection.remote_id();
                if !accept_peers.contains(&remote) {
                    connection.close(1_u8.into(), b"unauthorized runner endpoint");
                    tracing::warn!(%remote, "rejected unauthorized Iroh runner endpoint");
                    continue;
                }
                let tx = stream_tx.clone();
                tokio::spawn(async move {
                    loop {
                        match connection.accept_bi().await {
                            Ok((send, recv)) => {
                                if tx.send(bridge_iroh_stream(send, recv)).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                tracing::debug!(%remote, %error, "Iroh runner connection closed");
                                break;
                            }
                        }
                    }
                });
            }
        });
        Ok(Self {
            endpoint,
            allowed_peers,
            streams: Arc::new(tokio::sync::Mutex::new(stream_rx)),
        })
    }
}

impl RunnerListener for IrohRunnerListener {
    fn accept(&self) -> AcceptFuture<'_> {
        Box::pin(async move {
            self.streams
                .lock()
                .await
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("Iroh runner endpoint closed"))
        })
    }

    fn endpoint_description(&self) -> String {
        format!("iroh:{}", self.endpoint.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{read_json_frame, write_json_frame};
    use iroh::endpoint::presets::N0DisableRelay;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn authorized_endpoint_connects_without_relay() -> Result<()> {
        let server = Endpoint::builder(N0DisableRelay)
            .alpns(vec![OQTO_RUNNER_ALPN.to_vec()])
            .bind()
            .await?;
        let client = Endpoint::builder(N0DisableRelay).bind().await?;
        let listener = IrohRunnerListener::new(server.clone(), HashSet::from([client.id()]))?;
        let connector = IrohRunnerConnector::new(client.clone(), server.addr());

        let server_task = tokio::spawn(async move {
            for _ in 0..2 {
                let stream = listener.accept().await?;
                let mut stream = BufReader::new(stream);
                let request: String = read_json_frame(&mut stream)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("client closed before request"))?;
                write_json_frame(stream.get_mut(), &format!("ack:{request}")).await?;
            }
            Ok::<_, anyhow::Error>(())
        });

        for _ in 0..2 {
            let mut stream = connector.connect().await?;
            write_json_frame(&mut stream, &"ping").await?;
            let mut stream = BufReader::new(stream);
            let response: Option<String> = read_json_frame(&mut stream).await?;
            assert_eq!(response.as_deref(), Some("ack:ping"));
        }
        server_task.await.context("joining Iroh server")??;
        client.close().await;
        server.close().await;
        Ok(())
    }
}

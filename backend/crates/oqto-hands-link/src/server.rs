//! Pinned-TLS listener for the hands host: the existing personal enrollment
//! route plus the two `oqto.hands.v1` WebSocket paths, and the local owner
//! socket. Foreground only; the caller owns the listener and shutdown.
use crate::{
    config::HostConfig,
    host::HandsHost,
    wire::{CONTROL_PATH, DATA_PATH, MAX_CONTROL_MESSAGE, MAX_DATA_MESSAGE},
};
use anyhow::{Context, Result, bail, ensure};
use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    response::Response,
    routing::get,
};
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use oqto_personal::owner::{OwnerCommand, Supported};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, UnixListener, UnixStream},
    sync::Semaphore,
    task::JoinSet,
};
use tokio_rustls::{TlsAcceptor, rustls::ServerConfig};
use tower::ServiceExt;

const OWNER_LIMIT: u64 = 64 * 1024;

pub fn router(host: Arc<HandsHost>) -> Router {
    Router::new()
        .route(CONTROL_PATH, get(control))
        .route(DATA_PATH, get(data))
        .with_state(Arc::clone(&host))
        .merge(oqto_personal::http::enrollment_router(Arc::clone(
            &host.authority,
        )))
}

async fn control(State(host): State<Arc<HandsHost>>, upgrade: WebSocketUpgrade) -> Response {
    upgrade
        .max_message_size(MAX_CONTROL_MESSAGE)
        .max_frame_size(MAX_CONTROL_MESSAGE)
        .on_upgrade(move |socket| host.serve_control(socket))
}

async fn data(State(host): State<Arc<HandsHost>>, upgrade: WebSocketUpgrade) -> Response {
    upgrade
        .max_message_size(MAX_DATA_MESSAGE)
        .max_frame_size(MAX_DATA_MESSAGE)
        .on_upgrade(move |socket| host.serve_data(socket))
}

/// TLS 1.3 only, no early data or tickets, HTTP/1.1 with upgrades.
pub async fn serve_tls(
    listener: TcpListener,
    config: Arc<ServerConfig>,
    router: Router,
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    let mut config = (*config).clone();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config.max_early_data_size = 0;
    config.send_tls13_tickets = 0;
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let slots = Arc::new(Semaphore::new(64));
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else { continue; };
                let _ = stream.set_nodelay(true);
                let acceptor = acceptor.clone();
                let router = router.clone();
                connections.spawn(async move {
                    let _slot = slot;
                    let Ok(Ok(tls)) = tokio::time::timeout(Duration::from_secs(5), acceptor.accept(stream)).await else { return; };
                    let service = hyper::service::service_fn(move |request| router.clone().oneshot(request));
                    let mut builder = http1::Builder::new();
                    // keep_alive(false) would replace the upgrade Connection header.
                    builder.timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(5));
                    let _ = builder.serve_connection(TokioIo::new(tls), service).with_upgrades().await;
                });
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(())
}

/// Local-owner administration; never mounted on the network listener.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostOwnerCommand {
    /// Issue a one-use pairing QR with no Sessions, Files or terminal grants.
    Pair,
    Devices,
    /// Standing hands delegation of one binding to one paired device.
    Delegate {
        device_id: String,
        binding_id: String,
    },
    Undelegate {
        device_id: String,
        binding_id: String,
    },
    /// Personal revocation plus stop and confirmation of its generations.
    Revoke {
        device_id: String,
    },
    Status,
    Recent {
        limit: u32,
    },
    /// After inspecting an unconfirmed stop, explicitly unfreeze a binding.
    ClearAttention {
        binding_id: String,
    },
}

pub fn owner_socket(config: &HostConfig) -> PathBuf {
    config.state_dir.join("hands-owner.sock")
}

pub fn bind_owner(path: &Path) -> Result<UnixListener> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.file_type().is_socket()
                    && meta.uid() == rustix::process::geteuid().as_raw()
                    && meta.mode() & 0o077 == 0,
                "unrecognized owner socket; preserving it"
            );
            std::fs::remove_file(path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub async fn serve_owner(
    listener: UnixListener,
    host: Arc<HandsHost>,
    origin: String,
    leaf: Vec<u8>,
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    let mut tasks = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            accepted = listener.accept() => {
                let (socket, _) = accepted?;
                if socket.peer_cred()?.uid() != rustix::process::geteuid().as_raw() { continue; }
                if tasks.len() >= 8 { continue; }
                let host = Arc::clone(&host);
                let origin = origin.clone();
                let leaf = leaf.clone();
                tasks.spawn(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(60), owner_connection(socket, host, origin, leaf)).await;
                });
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}

async fn owner_connection(
    socket: UnixStream,
    host: Arc<HandsHost>,
    origin: String,
    leaf: Vec<u8>,
) -> Result<()> {
    let mut reader = BufReader::new(socket);
    let mut line = Vec::new();
    (&mut reader)
        .take(OWNER_LIMIT + 1)
        .read_until(b'\n', &mut line)
        .await?;
    ensure!(
        line.len() as u64 <= OWNER_LIMIT && line.last() == Some(&b'\n'),
        "invalid owner request"
    );
    let command: HostOwnerCommand = serde_json::from_slice(&line)?;
    let reply = match owner_command(&host, command, &origin, &leaf).await {
        Ok(data) => serde_json::json!({"ok": true, "data": data}),
        Err(error) => serde_json::json!({"ok": false, "error": format!("{error:#}")}),
    };
    let mut bytes = serde_json::to_vec(&reply)?;
    bytes.push(b'\n');
    reader.get_mut().write_all(&bytes).await?;
    Ok(())
}

async fn owner_command(
    host: &Arc<HandsHost>,
    command: HostOwnerCommand,
    origin: &str,
    leaf: &[u8],
) -> Result<serde_json::Value> {
    let supported = Supported {
        files: false,
        terminal: false,
        sessions: false,
        control: false,
    };
    match command {
        HostOwnerCommand::Pair => {
            host.authority
                .owner_command(
                    OwnerCommand::Pair {
                        sessions: oqto_personal::SessionAccess::None,
                        files: vec![],
                        full_user_terminal: false,
                    },
                    origin,
                    leaf,
                    supported,
                )
                .await
        }
        HostOwnerCommand::Devices => {
            host.authority
                .owner_command(
                    OwnerCommand::Devices { after: None },
                    origin,
                    leaf,
                    supported,
                )
                .await
        }
        HostOwnerCommand::Delegate {
            device_id,
            binding_id,
        } => {
            ensure!(host.bindings.contains_key(&binding_id), "unknown binding");
            let devices = host
                .authority
                .owner_command(
                    OwnerCommand::Devices { after: None },
                    origin,
                    leaf,
                    supported,
                )
                .await?;
            let known = devices["devices"].as_array().is_some_and(|list| {
                list.iter()
                    .any(|d| d["device_id"] == device_id.as_str() && d["revoked"] == false)
            });
            ensure!(known, "unknown or revoked device");
            Ok(
                serde_json::json!({"delegated": host.journal.delegate(&device_id, &binding_id).await?}),
            )
        }
        HostOwnerCommand::Undelegate {
            device_id,
            binding_id,
        } => host.undelegate(&device_id, &binding_id).await,
        HostOwnerCommand::Revoke { device_id } => host.revoke_device(&device_id).await,
        HostOwnerCommand::Status => host.status().await,
        HostOwnerCommand::Recent { limit } => Ok(serde_json::to_value(
            host.journal.recent(limit.min(64)).await?,
        )?),
        HostOwnerCommand::ClearAttention { binding_id } => {
            Ok(serde_json::json!({"cleared": host.journal.clear_attention(&binding_id).await?}))
        }
    }
}

/// Owner CLI client: never opens the stores itself.
pub async fn request_owner(path: &Path, command: &HostOwnerCommand) -> Result<serde_json::Value> {
    let meta = std::fs::symlink_metadata(path).context("hands host owner socket")?;
    ensure!(
        meta.file_type().is_socket()
            && meta.uid() == rustix::process::geteuid().as_raw()
            && meta.mode() & 0o077 == 0,
        "untrusted owner socket"
    );
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut socket = UnixStream::connect(path).await?;
        let mut bytes = serde_json::to_vec(command)?;
        bytes.push(b'\n');
        socket.write_all(&bytes).await?;
        let mut reader = BufReader::new(socket);
        let mut line = Vec::new();
        (&mut reader)
            .take(OWNER_LIMIT * 4)
            .read_until(b'\n', &mut line)
            .await?;
        let value: serde_json::Value = serde_json::from_slice(&line)?;
        if value["ok"] != true {
            bail!(
                "owner command refused: {}",
                value["error"].as_str().unwrap_or("unknown")
            );
        }
        Ok(value["data"].clone())
    })
    .await
    .context("owner command timed out; inspect before retrying")?
}

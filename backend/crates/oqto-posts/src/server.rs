//! One private listener per configured identity. No ambient environment auth.
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Semaphore, watch};
use tokio::task::{JoinHandle, JoinSet};

use crate::store::{Store, cursor, sequence};
use crate::wire::{self, Filter, Operation, Request, Stamp, WireError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Agent,
    Owner,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub name: String,
    pub role: Role,
    pub stamp: Stamp,
}
impl Registration {
    pub fn validate(&self) -> Result<()> {
        wire::id(&self.name)?;
        ensure!(
            !self.name.contains([':', '.']),
            "socket name uses letters, digits, underscore and hyphen only"
        );
        self.stamp.validate()
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub max_connections: u32,
    pub request_timeout_ms: u32,
    pub write_timeout_ms: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_connections: 64,
            request_timeout_ms: 2000,
            write_timeout_ms: 2000,
        }
    }
}
impl Limits {
    pub fn validate(self) -> Result<()> {
        ensure!(
            (1..=4096).contains(&self.max_connections),
            "max_connections outside 1..4096"
        );
        ensure!(
            (1..=60000).contains(&self.request_timeout_ms)
                && (1..=60000).contains(&self.write_timeout_ms),
            "timeout outside 1..60000ms"
        );
        Ok(())
    }
}

pub struct Server {
    stop: watch::Sender<bool>,
    tasks: Vec<JoinHandle<Result<()>>>,
    sockets: Vec<SocketFile>,
}
impl Server {
    pub async fn bind(
        store: Store,
        directory: &Path,
        registrations: &[Registration],
        limits: Limits,
    ) -> Result<Self> {
        limits.validate()?;
        ensure!(
            !registrations.is_empty(),
            "no posting identities registered"
        );
        private_directory(directory)?;
        let (stop, _) = watch::channel(false);
        let permits = Arc::new(Semaphore::new(limits.max_connections as usize));
        let mut listeners = Vec::new();
        let mut sockets = Vec::new();
        for registration in registrations {
            registration.validate()?;
            let path = directory.join(format!("{}.sock", registration.name));
            // Never remove a socket we did not create (including stale ones).
            let listener = UnixListener::bind(&path)
                .context("binding identity socket (path may already exist)")?;
            let meta = std::fs::symlink_metadata(&path)?;
            sockets.push(SocketFile {
                path: path.clone(),
                inode: meta.ino(),
            });
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            listeners.push((listener, registration.clone()));
        }
        let tasks = listeners
            .into_iter()
            .map(|(listener, registration)| {
                let store = store.clone();
                let permits = permits.clone();
                let shutdown = stop.subscribe();
                tokio::spawn(listen(
                    listener,
                    registration,
                    store,
                    permits,
                    limits,
                    shutdown,
                ))
            })
            .collect();
        Ok(Self {
            stop,
            tasks,
            sockets,
        })
    }
    pub async fn shutdown(mut self) -> Result<()> {
        self.stop.send_replace(true);
        for task in self.tasks.drain(..) {
            task.await.context("joining post listener")??;
        }
        self.sockets.clear();
        Ok(())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

struct SocketFile {
    path: PathBuf,
    inode: u64,
}
impl Drop for SocketFile {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| m.ino() == self.inode) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub fn private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).context("creating post socket directory"),
    }
    let meta = std::fs::symlink_metadata(path).context("checking post socket directory")?;
    ensure!(
        meta.is_dir()
            && meta.uid() == rustix::process::geteuid().as_raw()
            && meta.mode() & 0o077 == 0,
        "socket directory must be owned by Runner uid, mode 0700, not a symlink"
    );
    Ok(())
}

pub fn check_peer(uid: u32, runner_uid: u32) -> Result<()> {
    ensure!(uid == runner_uid, "peer uid does not match Runner");
    Ok(())
}

async fn listen(
    listener: UnixListener,
    registration: Registration,
    store: Store,
    permits: Arc<Semaphore>,
    limits: Limits,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut clients = JoinSet::new();
    loop {
        tokio::select! {
            _=shutdown.changed()=>break,
            Some(_)=clients.join_next(), if !clients.is_empty()=>{},
            accepted=listener.accept()=>{
                let (mut stream,_)=accepted.context("accepting post connection")?;
                let uid=stream.peer_cred().context("checking post peer credentials")?.uid();
                if check_peer(uid,rustix::process::geteuid().as_raw()).is_err() {
                    let _=send(&mut stream,&wire::failure(None,&WireError::new("unauthorized","peer uid mismatch")),limits).await;
                    continue;
                }
                match permits.clone().try_acquire_owned() {
                    Ok(permit)=>{
                        let (store,registration,shutdown)=(store.clone(),registration.clone(),shutdown.clone());
                        clients.spawn(async move {let _permit=permit; let _=connection(stream,registration,store,limits,shutdown).await;});
                    },
                    Err(_)=>{let _=send(&mut stream,&wire::failure(None,&WireError::new("unavailable","connection limit reached")),limits).await;}
                }
            }
        }
    }
    while clients.join_next().await.is_some() {}
    Ok(())
}

/// Bounded read: oversize lines are rejected without buffering the remainder.
async fn frame(reader: &mut BufReader<UnixStream>) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    let n = reader
        .take(wire::MAX_FRAME as u64 + 1)
        .read_until(b'\n', &mut bytes)
        .await?;
    if n == 0 {
        return Ok(None);
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        return Ok(Some(bytes));
    }
    if bytes.len() > wire::MAX_FRAME {
        return Err(WireError::new("too_large", "frame exceeds 65536 bytes").into());
    }
    Err(WireError::new("invalid_request", "unterminated frame").into())
}
async fn send(stream: &mut UnixStream, value: &Value, limits: Limits) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= wire::MAX_FRAME, "response too large");
    bytes.push(b'\n');
    tokio::time::timeout(
        Duration::from_millis(limits.write_timeout_ms.into()),
        stream.write_all(&bytes),
    )
    .await
    .context("post write timeout")??;
    Ok(())
}

fn parse(bytes: &[u8]) -> Result<Request> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| WireError::new("invalid_request", "invalid JSON"))?;
    if value.get("v") != Some(&json!(wire::VERSION)) {
        return Err(WireError {
            code: "unsupported_version".into(),
            message: "protocol version unsupported".into(),
            supported_versions: Some(vec![1]),
        }
        .into());
    }
    let request: Request = serde_json::from_value(value)
        .map_err(|_| WireError::new("invalid_request", "unknown operation or malformed request"))?;
    wire::id(&request.request_id)?;
    match &request.operation {
        Operation::Post { post } => post.validate()?,
        Operation::Reply { reply } => reply.validate()?,
        Operation::Watch { filter, since } => {
            filter.validate()?;
            if let Some(since) = since {
                sequence(since)?;
            }
        }
    }
    Ok(request)
}
fn wire_error(error: &anyhow::Error) -> WireError {
    error
        .downcast_ref::<WireError>()
        .cloned()
        .unwrap_or_else(|| {
            WireError::new(
                "invalid_request",
                "request failed validation or could not be stored",
            )
        })
}

async fn connection(
    stream: UnixStream,
    registration: Registration,
    store: Store,
    limits: Limits,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let read = tokio::select! {
        _=shutdown.changed()=>return Ok(()),
        read=tokio::time::timeout(Duration::from_millis(limits.request_timeout_ms.into()),frame(&mut reader))=>read,
    };
    let bytes = match read {
        Ok(Ok(Some(bytes))) => bytes,
        Ok(Ok(None)) => return Ok(()),
        Ok(Err(error)) => {
            send(
                reader.get_mut(),
                &wire::failure(None, &wire_error(&error)),
                limits,
            )
            .await?;
            return Ok(());
        }
        Err(_) => {
            send(
                reader.get_mut(),
                &wire::failure(None, &WireError::new("invalid_request", "request timeout")),
                limits,
            )
            .await?;
            return Ok(());
        }
    };
    let request = match parse(&bytes) {
        Ok(request) => request,
        Err(error) => {
            let req = serde_json::from_slice::<Value>(&bytes).ok();
            let id = req.as_ref().and_then(|v| v["request_id"].as_str());
            send(
                reader.get_mut(),
                &wire::failure(id, &wire_error(&error)),
                limits,
            )
            .await?;
            return Ok(());
        }
    };
    let now = chrono::Utc::now().timestamp();
    let result = match request.operation {
        Operation::Post { post } if registration.role == Role::Agent => {
            store
                .post(
                    &registration.name,
                    &registration.stamp,
                    &request.request_id,
                    &post,
                    now,
                )
                .await
        }
        Operation::Reply { reply } if registration.role == Role::Owner => {
            store
                .reply(&registration.name, &request.request_id, &reply, now)
                .await
        }
        Operation::Watch { filter, since } if registration.role == Role::Owner => {
            send(
                reader.get_mut(),
                &wire::success(&request.request_id, json!({})),
                limits,
            )
            .await?;
            return watch_posts(reader.get_mut(), &store, filter, since, limits, shutdown).await;
        }
        _ => Err(WireError::new(
            "unauthorized",
            "operation not allowed on this identity socket",
        )
        .into()),
    };
    let response = match result {
        Ok(result) => wire::success(&request.request_id, result),
        Err(error) => {
            // Only typed admission refusals certify no effect. A database or
            // commit failure can have an uncertain outcome: close instead of
            // inventing a rejection that could authorize a blind retry.
            let refusal = error
                .downcast_ref::<WireError>()
                .context("operation outcome uncertain; connection closed")?;
            wire::failure(Some(&request.request_id), refusal)
        }
    };
    send(reader.get_mut(), &response, limits).await
}

async fn watch_posts(
    stream: &mut UnixStream,
    store: &Store,
    filter: Filter,
    since: Option<String>,
    limits: Limits,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut after = since.as_deref().map(sequence).transpose()?.unwrap_or(0);
    loop {
        if *shutdown.borrow() {
            send(
                stream,
                &json!({"v":1,"event":"end","reason":"shutdown"}),
                limits,
            )
            .await?;
            return Ok(());
        }
        let page = store
            .page(after, &filter, chrono::Utc::now().timestamp())
            .await?;
        if let Some(floor) = page.gap {
            send(
                stream,
                &json!({"v":1,"event":"gap","after":cursor(after),"retained_after":cursor(floor)}),
                limits,
            )
            .await?;
        }
        for (seq, post) in page.posts {
            send(
                stream,
                &json!({"v":1,"event":"post","cursor":cursor(seq),"post":post}),
                limits,
            )
            .await?;
        }
        let progressed = page.through > after;
        after = page.through;
        if progressed {
            continue;
        }
        let mut extra = [0_u8; 1];
        tokio::select! {
            _=shutdown.changed()=>{},
            // EOF releases the connection permit even on an otherwise quiet
            // stream. A second request is forbidden on this connection too.
            _=stream.read(&mut extra)=>return Ok(()),
            _=tokio::time::sleep(Duration::from_millis(50))=>{},
        }
    }
}

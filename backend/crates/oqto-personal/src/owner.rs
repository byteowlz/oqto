//! Local-owner administration. Never mount these commands on the HTTPS router.
use crate::*;
use serde::Deserialize;
use std::{
    future::Future,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    task::JoinSet,
};

const LIMIT: u64 = 64 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedRoot {
    pub path: PathBuf,
    pub writable: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnerCommand {
    Pair {
        sessions: SessionAccess,
        files: Vec<SelectedRoot>,
        full_user_terminal: bool,
    },
    Devices {
        after: Option<String>,
    },
    Revoke {
        device_id: String,
    },
}
#[derive(Clone, Copy)]
pub struct Supported {
    pub files: bool,
    pub terminal: bool,
    pub sessions: bool,
    pub control: bool,
}

impl PairingAuthority {
    pub fn owner_socket(&self) -> PathBuf {
        self.root.join("owner.sock")
    }
    pub fn bind_owner_socket(&self) -> Result<UnixListener> {
        let path = self.owner_socket();
        match std::fs::symlink_metadata(&path) {
            Ok(meta) => {
                ensure!(
                    meta.file_type().is_socket()
                        && meta.uid() == rustix::process::geteuid().as_raw()
                        && meta.mode() & 0o077 == 0,
                    "unrecognized owner socket; preserving it"
                );
                // The authority singleton lock is held; another legitimate
                // authority cannot be serving this socket. Never unlink unknown files.
                std::fs::remove_file(&path)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(listener)
    }
    pub async fn owner_command(
        &self,
        command: OwnerCommand,
        origin: &str,
        leaf: &[u8],
        supported: Supported,
    ) -> Result<serde_json::Value> {
        match command {
            OwnerCommand::Pair {
                sessions,
                files,
                full_user_terminal,
            } => {
                ensure!(
                    files.len() <= 32 && (files.is_empty() || supported.files),
                    "Files unavailable"
                );
                ensure!(
                    !full_user_terminal || supported.terminal,
                    "terminal unavailable"
                );
                ensure!(
                    sessions == SessionAccess::None || supported.sessions,
                    "session observation unavailable"
                );
                ensure!(
                    sessions != SessionAccess::Control || supported.control,
                    "session control unavailable"
                );
                let files = files
                    .into_iter()
                    .map(|root| FileGrant::select(&root.path, root.writable))
                    .collect::<Result<Vec<_>>>()?;
                let grants = Grants {
                    sessions,
                    files,
                    full_user_terminal,
                };
                Ok(serde_json::to_value(
                    self.issue_pairing_qr(&grants, origin, leaf).await?,
                )?)
            }
            OwnerCommand::Devices { after } => {
                let rows = sqlx::query("SELECT id,name,created_at,revoked_at,grants_json FROM personal_devices WHERE id>coalesce(?, '') ORDER BY id LIMIT 64").bind(after).fetch_all(&self.pool).await?;
                let mut devices = Vec::new();
                for row in rows {
                    let grants: Grants =
                        serde_json::from_str(&row.try_get::<String, _>("grants_json")?)?;
                    devices.push(serde_json::json!({"device_id":row.try_get::<String,_>("id")?,"name":row.try_get::<String,_>("name")?,"created_at":row.try_get::<i64,_>("created_at")?,"revoked":row.try_get::<Option<i64>,_>("revoked_at")?.is_some(),"sessions":grants.sessions,"file_roots":grants.files.len(),"full_user_terminal":grants.full_user_terminal}));
                }
                let next = if devices.len() == 64 {
                    devices
                        .last()
                        .and_then(|row| row["device_id"].as_str())
                        .map(str::to_owned)
                } else {
                    None
                };
                Ok(
                    serde_json::json!({"host_id":self.host_id(),"devices":devices,"next_after":next}),
                )
            }
            OwnerCommand::Revoke { device_id } => {
                Ok(serde_json::json!({"revoked":self.revoke(&device_id).await?}))
            }
        }
    }
}

pub async fn serve_owner(
    listener: UnixListener,
    authority: Arc<PairingAuthority>,
    origin: String,
    leaf: Vec<u8>,
    supported: Supported,
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    let slots = Arc::new(tokio::sync::Semaphore::new(8));
    let mut tasks = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            accepted = listener.accept() => {
                let (socket,_) = accepted?;
                if socket.peer_cred()?.uid()!=rustix::process::geteuid().as_raw() { continue; }
                let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else { continue; };
                let authority = Arc::clone(&authority); let origin = origin.clone(); let leaf = leaf.clone();
                tasks.spawn(async move {
                    let _slot = slot;
                    let _ = tokio::time::timeout(Duration::from_secs(10),async {
                        let mut reader = BufReader::new(socket); let mut line=Vec::new();
                        use tokio::io::AsyncReadExt;
                        (&mut reader).take(LIMIT+1).read_until(b'\n', &mut line).await?;
                        ensure!(line.len() as u64 <= LIMIT && line.last()==Some(&b'\n'),"invalid owner request");
                        let command = serde_json::from_slice(&line)?;
                        let reply = match authority.owner_command(command,&origin,&leaf,supported).await {
                            Ok(value) => serde_json::json!({"ok":true,"data":value}),
                            Err(_) => serde_json::json!({"ok":false,"error":"owner_command_refused"}),
                        };
                        let mut bytes=serde_json::to_vec(&reply)?; bytes.push(b'\n');
                        reader.get_mut().write_all(&bytes).await?;
                        Ok::<(),anyhow::Error>(())
                    }).await;
                });
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}

/// Client never opens the authority database or becomes another grant issuer.
pub async fn request_owner(state: &Path, command: &OwnerCommand) -> Result<serde_json::Value> {
    use tokio::io::AsyncReadExt;
    let meta = std::fs::symlink_metadata(state)?;
    ensure!(
        meta.is_dir()
            && !meta.file_type().is_symlink()
            && meta.uid() == rustix::process::geteuid().as_raw()
            && meta.mode() & 0o077 == 0,
        "untrusted personal state directory"
    );
    let path = state.join("owner.sock");
    let meta = std::fs::symlink_metadata(&path)?;
    ensure!(
        meta.file_type().is_socket()
            && meta.uid() == rustix::process::geteuid().as_raw()
            && meta.mode() & 0o077 == 0,
        "untrusted owner socket"
    );
    tokio::time::timeout(Duration::from_secs(12), async {
        let mut socket = UnixStream::connect(&path).await?;
        ensure!(
            socket.peer_cred()?.uid() == rustix::process::geteuid().as_raw(),
            "owner peer identity denied"
        );
        let mut bytes = serde_json::to_vec(command)?;
        ensure!((bytes.len() as u64) < LIMIT, "owner command too large");
        bytes.push(b'\n');
        socket.write_all(&bytes).await?;
        let mut reader = BufReader::new(socket);
        let mut line = Vec::new();
        (&mut reader)
            .take(LIMIT + 1)
            .read_until(b'\n', &mut line)
            .await?;
        ensure!(line.len() as u64 <= LIMIT, "owner response too large");
        let value: serde_json::Value = serde_json::from_slice(&line)?;
        ensure!(value["ok"] == true, "personal owner command refused");
        Ok(value["data"].clone())
    })
    .await
    .context("owner command timed out; do not automatically retry an uncertain effect")?
}

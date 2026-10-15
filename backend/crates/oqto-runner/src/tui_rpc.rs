//! Read-only attachment to an externally owned Pi TUI (protocol v1).
//!
//! Protocol/discovery reference: oqto-desktop b788b2fd, live.rs/transport.rs,
//! ADR-0005. Unlike Desktop discovery, source access requires an explicit host
//! authority binding first. This is an adapter, NOT network admission or a grant
//! issuer. Raw events are private translator input, never canonical replay.
//! No lease, prompt, session switch, bulk-history request, spawn or kill exists.
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

const FRAME_LIMIT: u64 = 1024 * 1024;
const MAX_ENTRIES: usize = 8192;
const MAX_CANDIDATES: usize = 64;
const MAX_PENDING: usize = 16;
const RPC_DEADLINE: Duration = Duration::from_millis(500);
const DISCOVERY_DEADLINE: Duration = Duration::from_secs(5);

/// Supplied by trusted host authority after resolving the durable public/native
/// binding and designated roots. Deliberately not deserializable from a request.
#[derive(Clone)]
pub struct ObservationBinding {
    pub account_id: String,
    pub public_session_id: String,
    pub external_session_id: String,
    pub session_file: PathBuf,
    pub session_root: PathBuf,
    pub work_directory: PathBuf,
    pub workspace_root: PathBuf,
    pub socket_root: PathBuf,
    /// Opaque host grant generation, rechecked by the authority on each read.
    pub grant_generation: String,
}

/// The embedding runner admission layer, not the adapter or its caller, owns
/// permission. Implementations must reject stale bindings/generations, and must
/// not infer observe authority from inventory, history access or input leases.
pub trait ObservationAuthority: Send + Sync {
    fn resolve(&self, account: &str, public_session: &str) -> Result<ObservationBinding>;
    fn revalidate(&self, binding: &ObservationBinding) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedState {
    pub streaming: bool,
    pub compacting: bool,
}

/// Native input for the runner's translator; contains private session data.
/// A socket timestamp is NOT a canonical event ID or application replay cursor.
pub struct ObservedEvent {
    pub name: String,
    pub data: Value,
}
impl std::fmt::Debug for ObservedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ObservedEvent([private native input])")
    }
}

#[derive(Deserialize)]
struct NativeState {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "sessionFile")]
    session_file: PathBuf,
    cwd: PathBuf,
    #[serde(default, rename = "isStreaming")]
    streaming: bool,
    #[serde(default, rename = "isCompacting")]
    compacting: bool,
}

/// Dropping this handle only closes OUR socket. The terminal owns Pi's lifecycle.
/// Any read error poisons/closes the attachment: reconnect requires a fresh host
/// binding and canonical snapshot; there is no transparent gap-hiding reconnect.
pub struct TuiObserver {
    authority: Arc<dyn ObservationAuthority>,
    binding: ObservationBinding,
    reader: Option<BufReader<UnixStream>>,
    pending: VecDeque<ObservedEvent>,
    state: ObservedState,
}
impl TuiObserver {
    pub fn public_session_id(&self) -> &str {
        &self.binding.public_session_id
    }
    /// Admission-time snapshot, not a continuously updated state projection.
    pub fn initial_state(&self) -> ObservedState {
        self.state
    }
    /// The runner did not launch this process: containment is ALWAYS unverified.
    pub fn containment_verified(&self) -> bool {
        false
    }

    pub async fn attach(
        authority: Arc<dyn ObservationAuthority>,
        account: &str,
        public_session: &str,
    ) -> Result<Self> {
        // This precedes every filesystem read, directory scan and connection.
        let binding = authority.resolve(account, public_session)?;
        ensure!(
            binding.account_id == account && binding.public_session_id == public_session,
            "Observation identity denied"
        );
        validate_ids(&binding)?;
        authority.revalidate(&binding)?;
        tokio::time::timeout(DISCOVERY_DEADLINE, Self::discover(authority, binding))
            .await
            .context("TUI discovery deadline exceeded")?
    }

    async fn discover(
        authority: Arc<dyn ObservationAuthority>,
        binding: ObservationBinding,
    ) -> Result<Self> {
        validate_source(&binding).await?;
        let uid = rustix::process::geteuid().as_raw();
        let mut entries = tokio::fs::read_dir(&binding.socket_root)
            .await
            .context("TUI socket root unavailable")?;
        let mut candidates = Vec::new();
        let mut count = 0;
        while let Some(entry) = entries.next_entry().await.context("TUI discovery failed")? {
            count += 1;
            ensure!(count <= MAX_ENTRIES, "TUI discovery entry bound exceeded");
            let Some((pid, path)) =
                socket_candidate(&entry.file_name().to_string_lossy(), &entry.path(), uid)
            else {
                continue;
            };
            candidates.push((pid, path));
            ensure!(
                candidates.len() <= MAX_CANDIDATES,
                "TUI discovery candidate bound exceeded"
            );
        }
        let mut found = None;
        for (pid, path) in candidates {
            authority.revalidate(&binding)?;
            let candidate = tokio::time::timeout(
                RPC_DEADLINE,
                Self::probe(authority.clone(), binding.clone(), pid, &path),
            )
            .await;
            if let Ok(Ok(observer)) = candidate {
                ensure!(found.is_none(), "Ambiguous live TUI identity");
                found = Some(observer);
            }
        }
        authority.revalidate(&binding)?;
        found.context("No matching live TUI attachment")
    }

    async fn probe(
        authority: Arc<dyn ObservationAuthority>,
        binding: ObservationBinding,
        pid: u32,
        path: &Path,
    ) -> Result<Self> {
        let uid = rustix::process::geteuid().as_raw();
        let pid_value = rustix::process::Pid::from_raw(pid as i32).context("Invalid TUI PID")?;
        rustix::process::test_kill_process(pid_value).context("Stale TUI process")?;
        let before = tokio::fs::symlink_metadata(path)
            .await
            .context("TUI socket unavailable")?;
        ensure!(
            before.file_type().is_socket() && before.uid() == uid && before.mode() & 0o022 == 0,
            "Unsafe TUI socket"
        );
        ensure!(
            tokio::fs::canonicalize(path).await? == path,
            "TUI socket path changed"
        );
        let stream = UnixStream::connect(path)
            .await
            .context("TUI socket unavailable")?;
        let peer = stream.peer_cred().context("TUI peer unavailable")?;
        validate_peer(uid, peer.uid(), peer.pid(), pid)?;
        let after = tokio::fs::symlink_metadata(path)
            .await
            .context("TUI socket disappeared")?;
        ensure!(
            (before.dev(), before.ino()) == (after.dev(), after.ino()),
            "TUI socket generation changed"
        );
        let mut reader = BufReader::new(stream);
        let hello = read_frame(&mut reader).await?;
        ensure!(
            hello["type"] == "hello"
                && hello["v"].as_u64() == Some(1)
                && hello["pid"].as_u64() == Some(u64::from(pid)),
            "Unsupported TUI hello"
        );
        ensure!(
            hello["commands"]
                .as_array()
                .is_some_and(|commands| commands.iter().any(|c| c == "get_state")),
            "TUI state observation unsupported"
        );
        let id = uuid::Uuid::new_v4().to_string();
        let request = serde_json::to_vec(&json!({"id":id,"type":"get_state"}))?;
        reader.get_mut().write_all(&request).await?;
        reader.get_mut().write_all(b"\n").await?;
        let mut pending = VecDeque::new();
        let state = loop {
            let frame = read_frame(&mut reader).await?;
            match frame["type"].as_str() {
                Some("lease") => {} // A lease notification is never authority.
                Some("event") => {
                    ensure!(
                        pending.len() < MAX_PENDING,
                        "TUI event gap; canonical resnapshot required"
                    );
                    pending.push_back(event(frame)?);
                }
                Some("response") => {
                    ensure!(
                        frame["id"] == id
                            && frame["command"] == "get_state"
                            && frame["success"] == true,
                        "TUI state response rejected"
                    );
                    let state: NativeState = serde_json::from_value(frame["data"].clone())
                        .map_err(|_| anyhow::anyhow!("Invalid TUI state"))?;
                    ensure!(
                        state.session_id == binding.external_session_id
                            && state.session_file == binding.session_file
                            && state.cwd == binding.work_directory,
                        "TUI Session binding mismatch"
                    );
                    break ObservedState {
                        streaming: state.streaming,
                        compacting: state.compacting,
                    };
                }
                _ => bail!("Unexpected TUI frame"),
            }
        };
        authority.revalidate(&binding)?;
        Ok(Self {
            authority,
            binding,
            reader: Some(reader),
            pending,
            state,
        })
    }

    /// No replay promise: EOF/timeout/overflow/session switch requires the host
    /// to rebuild from oqto-log, never text-based deduplication or another Pi.
    pub async fn next_event(&mut self, wait: Duration) -> Result<ObservedEvent> {
        // Own the socket inside this future. Cancellation drops it instead of
        // leaving a half-consumed JSON frame available to a later subscriber.
        let mut reader = self.reader.take().context("TUI attachment disconnected")?;
        let result = self
            .next_checked(&mut reader, wait.min(Duration::from_secs(30)))
            .await;
        if result.is_ok() {
            self.reader = Some(reader);
        } else {
            self.pending.clear();
        }
        result
    }
    async fn next_checked(
        &mut self,
        reader: &mut BufReader<UnixStream>,
        wait: Duration,
    ) -> Result<ObservedEvent> {
        self.authority.revalidate(&self.binding)?;
        if let Some(value) = self.pending.pop_front() {
            return Ok(value);
        }
        let authority = self.authority.clone();
        let binding = self.binding.clone();
        let operation = async {
            loop {
                let frame = read_frame(reader).await?;
                match frame["type"].as_str() {
                    Some("lease") => {}
                    Some("event") => return event(frame),
                    _ => bail!("Unexpected TUI observation frame"),
                }
            }
        };
        tokio::pin!(operation);
        let mut revocation = tokio::time::interval(Duration::from_millis(50));
        let deadline = tokio::time::sleep(wait);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                result = &mut operation => { authority.revalidate(&binding)?; return result; },
                _ = revocation.tick() => authority.revalidate(&binding)?,
                _ = &mut deadline => bail!("TUI observation deadline exceeded; canonical resnapshot required"),
            }
        }
    }
}

fn validate_ids(binding: &ObservationBinding) -> Result<()> {
    for id in [
        &binding.account_id,
        &binding.public_session_id,
        &binding.external_session_id,
        &binding.grant_generation,
    ] {
        ensure!(
            !id.trim().is_empty()
                && id.len() <= 256
                && !id.starts_with("pending-")
                && !id.starts_with("tmp:"),
            "Invalid observation identity"
        );
    }
    ensure!(
        binding.public_session_id != binding.external_session_id,
        "Distinct public/native identities required"
    );
    Ok(())
}

async fn validate_source(binding: &ObservationBinding) -> Result<()> {
    for path in [
        &binding.session_root,
        &binding.workspace_root,
        &binding.socket_root,
        &binding.session_file,
        &binding.work_directory,
    ] {
        ensure!(
            path.is_absolute() && tokio::fs::canonicalize(path).await? == *path,
            "Canonical observation paths required"
        );
    }
    ensure!(
        binding.session_file.starts_with(&binding.session_root)
            && binding.work_directory.starts_with(&binding.workspace_root),
        "Observation outside designated roots"
    );
    let file = tokio::fs::File::open(&binding.session_file)
        .await
        .context("TUI source unavailable")?;
    let metadata = file.metadata().await?;
    ensure!(
        metadata.is_file() && metadata.uid() == rustix::process::geteuid().as_raw(),
        "Unsafe TUI source"
    );
    let header = read_frame(&mut BufReader::new(file)).await?;
    ensure!(
        header["type"] == "session"
            && header["id"] == binding.external_session_id
            && header["cwd"].as_str().map(Path::new) == Some(binding.work_directory.as_path()),
        "Pi source identity mismatch"
    );
    Ok(())
}

fn validate_peer(uid: u32, peer_uid: u32, peer_pid: Option<i32>, expected_pid: u32) -> Result<()> {
    ensure!(
        peer_uid == uid && peer_pid.is_none_or(|pid| pid > 0 && pid as u32 == expected_pid),
        "TUI peer identity denied"
    );
    Ok(())
}

fn socket_candidate(name: &str, path: &Path, uid: u32) -> Option<(u32, PathBuf)> {
    let rest = name.strip_prefix("pi-tui-rpc-")?;
    let (pid, path) = if let Some(pid) = rest.strip_suffix(".sock") {
        (pid.parse::<u32>().ok()?, path.to_path_buf())
    } else {
        let (owner, pid) = rest.split_once('-')?;
        if owner.parse::<u32>().ok()? != uid {
            return None;
        }
        (pid.parse::<u32>().ok()?, path.join("control.sock"))
    };
    (pid > 0 && pid <= i32::MAX as u32).then_some((pid, path))
}

async fn read_frame<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Value> {
    let mut line = Vec::new();
    reader
        .take(FRAME_LIMIT + 1)
        .read_until(b'\n', &mut line)
        .await
        .context("TUI frame unavailable")?;
    ensure!(
        line.len() <= FRAME_LIMIT as usize && line.last() == Some(&b'\n'),
        "Invalid/oversized TUI frame"
    );
    serde_json::from_slice(&line).map_err(|_| anyhow::anyhow!("Invalid TUI JSON frame"))
}

fn event(frame: Value) -> Result<ObservedEvent> {
    let name = frame["event"].as_str().context("Invalid TUI event")?;
    ensure!(
        !matches!(name, "session_start" | "session_shutdown"),
        "TUI Session changed; re-admission required"
    );
    Ok(ObservedEvent {
        name: name.to_owned(),
        data: frame["data"].clone(),
    })
}

#[cfg(test)]
mod tests;

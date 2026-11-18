//! Instant, hint-only Session announces from the Pi extension (oqto-0dfq).
//!
//! The extension connects to a private Unix socket in the source's socket root
//! and writes one versioned line ["I am on this native file"]. The announce is
//! never authority: this module admits exactly the one announced file through
//! the same header, ownership, symlink, Workspace-root and pin checks the
//! discovery sweep uses, then invalidates the catalog so the next listing is
//! current. The five-minute sweep stays the fallback for anything not announced.
use anyhow::{Context, Result, ensure};
use log::{debug, info, warn};
use oqto_protocol::session_announce::{
    AnnounceOutcome, MAX_SESSION_ANNOUNCE_BYTES, SessionAnnounce, SessionAnnounceAck,
    decode_session_announce, encode_session_announce_ack,
};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// Socket file name inside the source's socket root.
pub const SOCKET_NAME: &str = "pi-session-announce.sock";
/// Where the Runner publishes the socket path for extensions whose default
/// socket directory differs from the configured root.
pub const POINTER_NAME: &str = "oqto-session-announce.path";
const ACCEPT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);
/// A fresh session file may still be flushed: re-read the header a few times.
const HEADER_ATTEMPTS: usize = 6;
const HEADER_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

/// A running announce listener. Dropping it removes the socket file.
pub struct AnnounceListener {
    socket: PathBuf,
    pub(crate) pointer: Option<PathBuf>,
    task: tokio::task::JoinHandle<()>,
}
impl AnnounceListener {
    pub fn socket(&self) -> &Path {
        &self.socket
    }
}
impl Drop for AnnounceListener {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.socket);
        if let Some(pointer) = &self.pointer {
            let _ = std::fs::remove_file(pointer);
        }
    }
}

struct AnnounceContext {
    source: Arc<crate::personal_source::PersonalSource>,
    home: PathBuf,
    user: String,
}

/// The directory a Pi extension uses when it has no explicit socket path:
/// `XDG_RUNTIME_DIR` on Linux when private, otherwise the platform temp dir.
pub fn default_socket_directory() -> PathBuf {
    let uid = rustix::process::geteuid().as_raw();
    if cfg!(target_os = "linux")
        && let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        && let Some(runtime) = runtime.to_str().map(PathBuf::from)
        && runtime.is_absolute()
        && let Ok(meta) = std::fs::metadata(&runtime)
        && meta.is_dir()
        && meta.uid() == uid
        && meta.permissions().mode() & 0o077 == 0
    {
        return runtime;
    }
    std::env::temp_dir()
}

fn ensure_socket_root(path: &Path) -> Result<()> {
    let meta =
        std::fs::metadata(path).with_context(|| format!("socket root {}", path.display()))?;
    ensure!(
        meta.is_dir() && meta.uid() == rustix::process::geteuid().as_raw(),
        "announce socket root is not an owned directory"
    );
    Ok(())
}

/// Bind the announce socket for one personal source. Never blocks the caller.
pub fn spawn(
    source: Arc<crate::personal_source::PersonalSource>,
    listener_home: PathBuf,
    user: String,
) -> Result<AnnounceListener> {
    let root = source.socket_root().to_owned();
    ensure_socket_root(&root)?;
    let socket = root.join(SOCKET_NAME);
    // A stale socket from a previous Runner is owned by us and safe to replace;
    // anything else in that path is refused rather than unlinked.
    match std::fs::symlink_metadata(&socket) {
        Ok(meta) if meta.file_type().is_socket() => {
            std::fs::remove_file(&socket)
                .with_context(|| format!("replacing stale announce socket {}", socket.display()))?;
        }
        Ok(_) => anyhow::bail!("announce socket path is occupied by a non-socket file"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspecting announce socket"),
    }
    let listener = UnixListener::bind(&socket)
        .with_context(|| format!("binding announce socket {}", socket.display()))?;
    std::fs::set_permissions(&socket, PermissionsExt::from_mode(0o600))
        .with_context(|| format!("restricting announce socket {}", socket.display()))?;

    // Extensions that start outside Oqto (herdr, a bare `pi`) only know their own
    // default directory. Publish the path there when the root differs.
    let mut pointer = None;
    if let Ok(canonical_root) = root.canonicalize() {
        let default = default_socket_directory()
            .canonicalize()
            .unwrap_or_else(|_| default_socket_directory());
        if default != canonical_root
            && std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(&default)?)
                == rustix::process::geteuid().as_raw()
        {
            let file = default.join(POINTER_NAME);
            if std::fs::write(&file, format!("{}\n", socket.display())).is_ok() {
                let _ = std::fs::set_permissions(&file, PermissionsExt::from_mode(0o600));
                pointer = Some(file);
            }
        }
    }

    info!("session announce listening on {}", socket.display());
    let context = Arc::new(AnnounceContext {
        source,
        home: listener_home,
        user,
    });
    let task = tokio::spawn(accept_loop(listener, context));
    Ok(AnnounceListener {
        socket,
        pointer,
        task,
    })
}

async fn accept_loop(listener: UnixListener, context: Arc<AnnounceContext>) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                warn!("session announce accept failed: {error}");
                continue;
            }
        };
        let context = Arc::clone(&context);
        tokio::spawn(async move {
            if let Err(error) = serve(stream, &context).await {
                debug!("session announce ended: {error:#}");
            }
        });
    }
}

/// Only a header that simply is not there yet deserves another look.
fn header_may_still_be_written(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}");
    text.contains("Incomplete native header")
        || text.contains("Oversized native header")
        || text.contains("No such file or directory")
        || text.contains("os error 2")
}

/// The socket is never authentication: the peer UID must be our own.
fn peer_allowed(ours: u32, peer: u32) -> bool {
    ours == peer
}

async fn serve(mut stream: UnixStream, context: &AnnounceContext) -> Result<()> {
    // The socket directory is private and the socket mode is 0600, but the peer
    // UID is still checked: a socket is not authentication.
    let peer = stream.peer_cred().context("announce peer unavailable")?;
    if !peer_allowed(rustix::process::geteuid().as_raw(), peer.uid()) {
        warn!("session announce from foreign uid {} dropped", peer.uid());
        let ack = SessionAnnounceAck::new(AnnounceOutcome::Denied, "peer uid denied");
        let _ = stream
            .write_all(encode_session_announce_ack(&ack).as_bytes())
            .await;
        return Ok(());
    }
    let raw = tokio::time::timeout(ACCEPT_DEADLINE, read_line(&mut stream))
        .await
        .map_err(|_| anyhow::anyhow!("announce read deadline exceeded"))??;
    let announce = match decode_session_announce(&raw) {
        Ok(announce) => announce,
        Err(error) => {
            warn!("session announce invalid ({}): dropped", error.code());
            let ack = SessionAnnounceAck::new(AnnounceOutcome::Invalid, error.code());
            let _ = stream
                .write_all(encode_session_announce_ack(&ack).as_bytes())
                .await;
            return Ok(());
        }
    };
    let ack = match admit(context, &announce).await {
        Ok(outcome) => {
            debug!(
                "session announce {:?} {} ({} -> {:?})",
                announce.reason, announce.session_id, announce.session_file, outcome
            );
            SessionAnnounceAck::new(outcome, "")
        }
        Err(error) => {
            warn!(
                "session announce refused for {}: {error:#}",
                announce.session_file
            );
            SessionAnnounceAck::new(AnnounceOutcome::Denied, format!("{error:#}"))
        }
    };
    stream
        .write_all(encode_session_announce_ack(&ack).as_bytes())
        .await
        .context("writing announce ack")?;
    let _ = stream.shutdown().await;
    Ok(())
}

async fn read_line(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&buffer[..read]);
        ensure!(
            raw.len() <= MAX_SESSION_ANNOUNCE_BYTES,
            "announce exceeds {} bytes",
            MAX_SESSION_ANNOUNCE_BYTES
        );
        if raw.contains(&b'\n') {
            break;
        }
    }
    Ok(raw)
}

/// Admit exactly the announced file through the existing validation and writer.
async fn admit(context: &AnnounceContext, announce: &SessionAnnounce) -> Result<AnnounceOutcome> {
    let session_file = PathBuf::from(&announce.session_file);
    let cwd = PathBuf::from(&announce.cwd);
    // Hint-only containment before any filesystem work.
    ensure!(
        session_file.starts_with(context.source.session_root()),
        "announce session file outside the native root"
    );
    if let Some(socket) = &announce.live_socket {
        ensure!(
            Path::new(socket).starts_with(context.source.socket_root()),
            "announce live socket outside the socket root"
        );
    }
    // Header, ownership, symlink, root and pin checks (blocking by design).
    // Pi may announce before it has flushed the header line, so an incomplete
    // header is re-read briefly instead of losing the announce to the sweep.
    let mut header = None;
    let mut last = None;
    for attempt in 0..HEADER_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(HEADER_RETRY_DELAY).await;
        }
        let source = Arc::clone(&context.source);
        let path = session_file.clone();
        match tokio::task::spawn_blocking(move || source.header(&path)).await {
            Ok(Ok(found)) => {
                header = Some(found);
                break;
            }
            Ok(Err(error)) => {
                if !header_may_still_be_written(&error) {
                    return Err(error);
                }
                last = Some(error);
            }
            Err(error) => return Err(error).context("announce header reader failed"),
        }
    }
    let header = match (header, last) {
        (Some(header), _) => header,
        (None, Some(error)) => return Err(error),
        (None, None) => anyhow::bail!("announce header unavailable"),
    };
    ensure!(
        header.external_id == announce.session_id,
        "announce identity does not match the native header"
    );
    ensure!(
        header.workspace == cwd,
        "announce Workspace does not match the native header"
    );
    let workspace = header
        .workspace
        .to_str()
        .context("announce Workspace is not valid UTF-8")?
        .to_owned();
    let changed = oqto_history::oqto_log::ops::native_source::discover(
        &context.home,
        &context.user,
        &workspace,
        &announce.session_id,
        &header.identity,
    )
    .await?;
    // The listing is served from cache; drop it so the next read is current.
    crate::history_read::invalidate_catalog(&context.home).await;
    Ok(if changed {
        AnnounceOutcome::Listed
    } else {
        AnnounceOutcome::Duplicate
    })
}

#[cfg(test)]
mod tests;

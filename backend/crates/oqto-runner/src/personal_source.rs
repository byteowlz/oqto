//! Explicit owner-selected native source. This is not Account history authority
//! or a client path. Canonical history stays in the Runner's original HOME.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

pub mod binding;
mod inventory;
mod pin_repair;
pub(crate) use inventory::HeaderScan;
#[cfg(test)]
pub(crate) mod test_support;

const MAX_HEADERS: usize = 32768;
const MAX_DIRECTORIES: usize = 4096;
const MAX_HEADER_BYTES: usize = 16384;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct DirectoryIdentity {
    path: PathBuf,
    dev: u64,
    ino: u64,
}
impl DirectoryIdentity {
    fn read(path: PathBuf) -> Result<Self> {
        ensure!(
            path.is_absolute() && path.canonicalize()? == path,
            "Canonical native directory required"
        );
        let meta = std::fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_dir() && meta.uid() == rustix::process::geteuid().as_raw(),
            "Native directory owner denied"
        );
        Ok(Self {
            path,
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            Self::read(self.path.clone())? == *self,
            "Native directory replaced"
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PersonalSource {
    pub(crate) home: PathBuf,
    session: DirectoryIdentity,
    socket: DirectoryIdentity,
    workspaces: Vec<DirectoryIdentity>,
    workspace_paths: Vec<PathBuf>,
    inventory: std::sync::Arc<std::sync::Mutex<inventory::HeaderCache>>,
    #[cfg(test)]
    header_reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    #[cfg(test)]
    header_pause: std::sync::Arc<std::sync::Mutex<Option<HeaderPause>>>,
    #[cfg(test)]
    pub(crate) header_latency_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
    #[cfg(test)]
    pub(crate) scan_budget_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
}
#[cfg(test)]
#[derive(Debug)]
struct HeaderPause {
    entered: tokio::sync::oneshot::Sender<()>,
    resume: std::sync::mpsc::Receiver<()>,
}
/// Version 1 pin: st_dev is a mount-instance ID on macOS, so these stop matching
/// after a remount (reboot). Still accepted while the device number is unchanged;
/// `repair_pins` upgrades them with explicit owner approval.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SourceIdentity {
    root: DirectoryIdentity,
    file: PathBuf,
    dev: u64,
    ino: u64,
    native_id: String,
    workspace: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct PinnedDirectory {
    path: PathBuf,
    filesystem: String,
    ino: u64,
}
/// Version 2 pin: the persistent volume identity survives remounts.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SourcePin {
    pin: u8,
    root: PinnedDirectory,
    file: PathBuf,
    filesystem: String,
    ino: u64,
    native_id: String,
    workspace: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeHeader {
    pub external_id: String,
    pub workspace: PathBuf,
    pub path: PathBuf,
    /// The pin written by discovery (version 2).
    pub identity: String,
    /// The same source in the version 1 encoding, for pins written before version 2.
    pub legacy_identity: String,
    pub dev: u64,
    pub ino: u64,
}
impl NativeHeader {
    /// Pins that prove this exact source today.
    pub(crate) fn accepted_pins(&self) -> [&str; 2] {
        [&self.identity, &self.legacy_identity]
    }
}
impl PersonalSource {
    pub fn new(
        home: PathBuf,
        session_root: PathBuf,
        socket_root: PathBuf,
        roots: Vec<PathBuf>,
    ) -> Result<Self> {
        ensure!(
            home.is_absolute() && home.canonicalize()? == home,
            "Canonical history HOME required"
        );
        crate::runtime_guard::ensure_history_state_owner(&home)?;
        ensure!(
            !roots.is_empty() && roots.len() <= 32,
            "Explicit native Workspace roots required"
        );
        Ok(Self {
            home,
            session: DirectoryIdentity::read(session_root)?,
            socket: DirectoryIdentity::read(socket_root)?,
            workspace_paths: roots.clone(),
            inventory: Default::default(),
            #[cfg(test)]
            header_reads: Default::default(),
            #[cfg(test)]
            header_pause: Default::default(),
            #[cfg(test)]
            header_latency_ms: Default::default(),
            #[cfg(test)]
            scan_budget_ms: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(5000)),
            workspaces: roots
                .into_iter()
                .map(DirectoryIdentity::read)
                .collect::<Result<_>>()?,
        })
    }
    pub(crate) fn session_root(&self) -> &Path {
        &self.session.path
    }
    pub(crate) fn socket_root(&self) -> &Path {
        &self.socket.path
    }
    pub(crate) fn workspace_roots(&self) -> &[PathBuf] {
        &self.workspace_paths
    }
    #[cfg(test)]
    pub(crate) fn pause_next_header_for_test(
        &self,
    ) -> Result<(
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    )> {
        let (entered, paused) = tokio::sync::oneshot::channel();
        let (resume, resumed) = std::sync::mpsc::channel();
        *self
            .header_pause
            .lock()
            .map_err(|_| anyhow::anyhow!("Header pause poisoned"))? = Some(HeaderPause {
            entered,
            resume: resumed,
        });
        Ok((paused, resume))
    }
    pub(crate) fn validate(&self) -> Result<()> {
        self.session.validate()?;
        self.socket.validate()?;
        for root in &self.workspaces {
            root.validate()?;
        }
        Ok(())
    }
    /// Bounded first-line-only read: deliberately no buffered read-ahead into
    /// bodies, even for headers whose cwd is outside the selected Workspaces.
    pub(crate) fn header(&self, path: &Path) -> Result<NativeHeader> {
        self.read_header(path, true)
    }
    fn read_header(&self, path: &Path, check_unique: bool) -> Result<NativeHeader> {
        #[cfg(test)]
        {
            self.header_reads
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let pause = self
                .header_pause
                .lock()
                .map_err(|_| anyhow::anyhow!("Header pause poisoned"))?
                .take();
            if let Some(pause) = pause {
                let _ = pause.entered.send(());
                pause
                    .resume
                    .recv_timeout(std::time::Duration::from_secs(15))
                    .context("Header read test pause ended")?;
            }
            let delay = self
                .header_latency_ms
                .load(std::sync::atomic::Ordering::SeqCst);
            if delay != 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
        }
        self.validate()?;
        ensure!(
            path.starts_with(self.session_root()) && path.canonicalize()? == path,
            "Native source outside root"
        );
        let parent = path.parent().context("Native source parent missing")?;
        ensure!(
            parent.parent() == Some(self.session_root()),
            "Unexpected native source depth"
        );
        let parent_identity = DirectoryIdentity::read(parent.to_owned())?;
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)?;
        let before = file.metadata()?;
        ensure!(
            before.is_file() && before.uid() == rustix::process::geteuid().as_raw(),
            "Native file owner denied"
        );
        let mut line = Vec::new();
        for _ in 0..MAX_HEADER_BYTES {
            let mut byte = [0];
            ensure!(file.read(&mut byte)? == 1, "Incomplete native header");
            line.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        ensure!(line.last() == Some(&b'\n'), "Oversized native header");
        #[derive(Deserialize)]
        struct Header {
            #[serde(rename = "type")]
            kind: String,
            id: String,
            cwd: PathBuf,
        }
        let header: Header = serde_json::from_slice(&line)?;
        ensure!(
            header.kind == "session"
                && !header.id.is_empty()
                && header.id.len() <= 256
                && header
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "Invalid native header identity"
        );
        ensure!(
            canonical_or_vanished(&header.cwd)?,
            "Canonical native cwd required"
        );
        ensure!(
            self.workspaces
                .iter()
                .any(|root| eligible_workspace(&header.cwd, &root.path)),
            "Native Workspace denied"
        );
        ensure!(
            parent.file_name()
                == Some(std::ffi::OsStr::new(
                    &oqto_pi::session_files::cwd_to_safe_dirname(&header.cwd)
                ))
                && path.file_name().is_some_and(|name| name
                    .to_string_lossy()
                    .ends_with(&format!("_{}.jsonl", header.id))),
            "Native header location mismatch"
        );
        if check_unique {
            let unique = oqto_pi::session_files::find_unique_session_file_in(
                self.session_root(),
                &header.id,
                &header.cwd,
            )?;
            ensure!(unique.as_deref() == Some(path), "Ambiguous native source");
        }
        let after = file.metadata()?;
        let published = std::fs::symlink_metadata(path)?;
        ensure!(
            (
                before.dev(),
                before.ino(),
                before.len(),
                before.mtime(),
                before.mtime_nsec()
            ) == (
                after.dev(),
                after.ino(),
                after.len(),
                after.mtime(),
                after.mtime_nsec()
            ) && (after.dev(), after.ino()) == (published.dev(), published.ino()),
            "Native header changed during discovery"
        );
        self.validate()?;
        ensure!(
            path.canonicalize()? == path
                && DirectoryIdentity::read(parent.to_owned())? == parent_identity,
            "Native header directory changed"
        );
        let legacy_identity = serde_json::to_string(&SourceIdentity {
            root: self.session.clone(),
            file: path.to_owned(),
            dev: before.dev(),
            ino: before.ino(),
            native_id: header.id.clone(),
            workspace: header.cwd.clone(),
        })?;
        let root_dir = std::fs::File::open(self.session_root())?;
        ensure!(
            root_dir.metadata()?.ino() == self.session.ino,
            "Native session root replaced"
        );
        let identity = serde_json::to_string(&SourcePin {
            pin: 2,
            root: PinnedDirectory {
                path: self.session.path.clone(),
                filesystem: oqto_personal::persistent_filesystem(&root_dir)?,
                ino: self.session.ino,
            },
            file: path.to_owned(),
            filesystem: oqto_personal::persistent_filesystem(&file)?,
            ino: before.ino(),
            native_id: header.id.clone(),
            workspace: header.cwd.clone(),
        })?;
        Ok(NativeHeader {
            external_id: header.id,
            workspace: header.cwd,
            path: path.to_owned(),
            identity,
            legacy_identity,
            dev: before.dev(),
            ino: before.ino(),
        })
    }
    pub(crate) fn headers(&self) -> Result<Vec<NativeHeader>> {
        self.validate()?;
        let mut scan = HeaderScan::default();
        while !scan.advance(self, usize::MAX, std::time::Duration::from_secs(5))? {}
        Ok(scan.headers)
    }
    pub(crate) async fn validate_binding(
        &self,
        binding: &crate::tui_rpc::ObservationBinding,
    ) -> Result<NativeHeader> {
        ensure!(
            binding.session_root == self.session.path
                && binding.socket_root == self.socket.path
                && self.workspace_roots().contains(&binding.workspace_root),
            "Native source binding denied"
        );
        let header = self.header(&binding.session_file)?;
        ensure!(
            header.external_id == binding.external_session_id
                && header.workspace == binding.work_directory,
            "Native source identity changed"
        );
        oqto_history::oqto_log::ops::native_source::validate(
            &self.home,
            &binding.public_session_id,
            &header.external_id,
            header
                .workspace
                .to_str()
                .context("Invalid native Workspace")?,
            &header.accepted_pins(),
        )
        .await?;
        Ok(header)
    }

    /// Owner-approved upgrade of version 1 pins that stopped matching only because
    /// macOS assigned a new device number after a remount. A pin is upgraded only
    /// when its file path, inode, native ID, Workspace and session-root path and
    /// inode all equal today's source; anything else is left for the owner to
    /// inspect. `apply` false reports without writing.
    pub(crate) async fn repair_pins(&self, apply: bool) -> Result<PinRepair> {
        self.repair_pins_with(apply, None).await
    }

    /// `accept_rewritten`: the owner states that Session files in this one
    /// Workspace were rewritten in place (new inode) by a trusted tool. A pin is
    /// then also upgraded when the file path, Workspace, session root and, above
    /// all, the native session id read from the file today still equal the
    /// pinned ones; only the file inode (and mount number) differ.
    pub(crate) async fn repair_pins_with(
        &self,
        apply: bool,
        accept_rewritten: Option<&Path>,
    ) -> Result<PinRepair> {
        let mut report = PinRepair::default();
        let mut pins: std::collections::HashMap<
            String,
            Vec<oqto_history::oqto_log::ops::native_source::NativePin>,
        > = std::collections::HashMap::new();
        for header in self.headers()? {
            let Some(workspace) = header.workspace.to_str() else {
                continue;
            };
            if !pins.contains_key(workspace) {
                let found =
                    oqto_history::oqto_log::ops::native_source::native_pins(&self.home, workspace)
                        .await?;
                pins.insert(workspace.to_owned(), found);
            }
            for row in pins[workspace]
                .iter()
                .filter(|row| row.external == header.external_id)
            {
                report.examined += 1;
                if header.accepted_pins().contains(&row.pin.as_str()) {
                    report.current += 1;
                    continue;
                }
                match pin_repair::classify(&header, &row.pin, accept_rewritten) {
                    pin_repair::Eligibility::Unrecognized => {
                        report.unrecognized += 1;
                        continue;
                    }
                    pin_repair::Eligibility::Mismatched => {
                        report.mismatched += 1;
                        continue;
                    }
                    pin_repair::Eligibility::TrustedRewrite => report.rewritten += 1,
                    pin_repair::Eligibility::SameSource => {}
                }
                if !apply {
                    report.upgradable += 1;
                    continue;
                }
                if oqto_history::oqto_log::ops::native_source::repin(
                    &self.home,
                    workspace,
                    &row.public,
                    &row.external,
                    &row.pin,
                    &header.identity,
                )
                .await?
                {
                    report.upgraded += 1;
                } else {
                    report.changed_concurrently += 1;
                }
            }
        }
        Ok(report)
    }
}

impl PersonalSource {
    /// Upgrade version 1 pins to version 2 while they still match today's mount.
    /// No approval is needed: the stored pin equals the current legacy identity,
    /// so it is provably the same file on the same mount. Done at startup so the
    /// next reboot cannot strand these Sessions (which `repair_pins` then has to
    /// recover with owner approval). Pins that no longer match are left alone.
    pub async fn upgrade_current_pins(&self) -> Result<usize> {
        let mut upgraded = 0;
        let mut pins: std::collections::HashMap<
            String,
            Vec<oqto_history::oqto_log::ops::native_source::NativePin>,
        > = std::collections::HashMap::new();
        for header in self.headers()? {
            if header.identity == header.legacy_identity {
                continue;
            }
            let Some(workspace) = header.workspace.to_str() else {
                continue;
            };
            if !pins.contains_key(workspace) {
                let found =
                    oqto_history::oqto_log::ops::native_source::native_pins(&self.home, workspace)
                        .await?;
                pins.insert(workspace.to_owned(), found);
            }
            for row in pins[workspace].iter().filter(|row| {
                row.external == header.external_id && row.pin == header.legacy_identity
            }) {
                if oqto_history::oqto_log::ops::native_source::repin(
                    &self.home,
                    workspace,
                    &row.public,
                    &row.external,
                    &row.pin,
                    &header.identity,
                )
                .await?
                {
                    upgraded += 1;
                }
            }
        }
        Ok(upgraded)
    }
}

/// Counts only; no paths or identities are reported.
#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct PinRepair {
    pub examined: usize,
    /// Owner-trusted rewrites (same session id, new inode) included in the
    /// upgradable/upgraded counts.
    pub rewritten: usize,
    pub current: usize,
    pub upgradable: usize,
    pub upgraded: usize,
    pub mismatched: usize,
    pub unrecognized: usize,
    pub changed_concurrently: usize,
}
/// A Session's working folder must be canonical. A folder that has since been
/// deleted (a removed checkout or worktree) cannot be canonicalized, but its
/// transcript is still the owner's history: it is accepted when the recorded path
/// is absolute and plain (no `.`/`..`) and its nearest existing ancestor
/// canonicalizes to itself, so no symlink can redirect it. Root containment is
/// checked separately by `eligible_workspace`, exactly as for existing folders.
pub(crate) fn canonical_or_vanished(cwd: &Path) -> Result<bool> {
    use std::path::Component;
    if !cwd.is_absolute() {
        return Ok(false);
    }
    match cwd.canonicalize() {
        Ok(canonical) => Ok(canonical == cwd),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if cwd
                .components()
                .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
            {
                return Ok(false);
            }
            let mut ancestor = cwd;
            while let Some(parent) = ancestor.parent() {
                ancestor = parent;
                match ancestor.canonicalize() {
                    Ok(canonical) => return Ok(canonical == ancestor),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(false)
        }
        Err(error) => Err(error.into()),
    }
}

fn eligible_workspace(workspace: &Path, root: &Path) -> bool {
    workspace.strip_prefix(root).is_ok_and(|relative| {
        relative.components().count() <= 4
            && relative.components().all(|part| {
                part.as_os_str().to_str().is_some_and(|name| {
                    !name.starts_with('.') && !matches!(name, "node_modules" | "target" | "vendor")
                })
            })
    })
}

#[cfg(test)]
mod tests;

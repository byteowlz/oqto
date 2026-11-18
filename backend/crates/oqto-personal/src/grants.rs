//! Local-owner grant policy. These types must never be accepted as enrollment input.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionAccess {
    #[default]
    None,
    Observe,
    /// Full host-user agent input authority; native terminal confirmation is still required.
    Control,
}

/// A file grant's root no longer proves to be the folder the owner selected.
/// Typed so callers answer "needs owner re-approval", never a generic fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrantRootChanged;
impl std::fmt::Display for GrantRootChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("file grant root identity changed; owner reapproval required")
    }
}
impl std::error::Error for GrantRootChanged {}

/// Stored grants keep their v1 JSON shape (`deny_unknown_fields`), so an older
/// Runner can still read them after a rollback. The persistent volume identity
/// lives beside them in `personal_grant_volumes` and is attached in memory only.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileGrant {
    id: String,
    path: PathBuf,
    writable: bool,
    /// Mount-instance number at selection. On macOS it changes when mount order
    /// changes (every reboot), so it is a fast path, not the identity.
    device: u64,
    inode: u64,
    /// Persistent volume identity (macOS volume UUID), when recorded.
    #[serde(skip)]
    volume: Option<String>,
}

impl FileGrant {
    /// Select a root locally. Subsequent file I/O must additionally use descriptor-relative confinement.
    pub fn select(path: &Path, writable: bool) -> Result<Self> {
        ensure!(path.is_absolute(), "file grant root must be absolute");
        let path = path
            .canonicalize()
            .context("resolve selected file grant root")?;
        let metadata = path
            .metadata()
            .context("inspect selected file grant root")?;
        ensure!(metadata.is_dir(), "file grant root must be a directory");
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            path,
            writable,
            device: metadata.dev(),
            inode: metadata.ino(),
            volume: None,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn writable(&self) -> bool {
        self.writable
    }
    /// Open and validate the actual root descriptor. Adapt this handle into
    /// capability-relative I/O; do not validate here and then reopen its path.
    ///
    /// The root is the selected folder when it is a directory with the selected
    /// inode on the same volume. The mount-instance number proves "same volume"
    /// while it is unchanged; after a remount only the recorded volume UUID can.
    pub fn open_root(&self) -> Result<std::fs::File> {
        let handle = self.open_path()?;
        let metadata = handle.metadata()?;
        if !metadata.is_dir() || metadata.ino() != self.inode {
            return Err(GrantRootChanged.into());
        }
        if metadata.dev() == self.device {
            return Ok(handle);
        }
        match &self.volume {
            Some(volume) if *volume == crate::persistent_filesystem(&handle)? => Ok(handle),
            _ => Err(GrantRootChanged.into()),
        }
    }
    fn open_path(&self) -> Result<std::fs::File> {
        use std::os::unix::fs::OpenOptionsExt;
        Ok(std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY)
            .open(&self.path)?)
    }
    pub(crate) fn inode(&self) -> u64 {
        self.inode
    }
    pub(crate) fn with_volume(&mut self, volume: String) {
        self.volume = Some(volume);
    }
    /// The volume identity to record without owner approval: only while the
    /// stored mount-instance number still matches, i.e. provably the same mount.
    pub(crate) fn same_mount_volume(&self) -> Result<Option<String>> {
        let handle = self.open_path()?;
        let metadata = handle.metadata()?;
        (metadata.is_dir() && metadata.ino() == self.inode && metadata.dev() == self.device)
            .then(|| crate::persistent_filesystem(&handle))
            .transpose()
    }
    /// The volume identity an owner may approve after a remount: same canonical
    /// path, still a directory, same inode; only the mount-instance number moved.
    pub(crate) fn remounted_volume(&self) -> Result<Option<String>> {
        if self.path.canonicalize()? != self.path {
            return Ok(None);
        }
        let handle = self.open_path()?;
        let metadata = handle.metadata()?;
        (metadata.is_dir() && metadata.ino() == self.inode && metadata.dev() != self.device)
            .then(|| crate::persistent_filesystem(&handle))
            .transpose()
    }
    pub fn validate_identity(&self) -> Result<()> {
        ensure!(
            self.path.is_absolute() && self.path.canonicalize()? == self.path,
            "file grant root moved"
        );
        let _handle = self.open_root()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grants {
    pub sessions: SessionAccess,
    pub files: Vec<FileGrant>,
    /// Full host-user shell authority, never a Files-root sandbox.
    pub full_user_terminal: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Access<'a> {
    /// Paired-device inventory only; never an execution or observation grant.
    Inventory,
    SessionObserve,
    SessionControl,
    FileRead(&'a str),
    FileWrite(&'a str),
    FullUserTerminal,
}

impl Grants {
    pub(crate) fn validate_selection(&self) -> Result<()> {
        ensure!(self.files.len() <= 32, "too many selected file roots");
        let mut ids = HashSet::new();
        for file in &self.files {
            ensure!(
                Uuid::parse_str(&file.id).is_ok() && ids.insert(&file.id),
                "invalid or duplicate file root identity"
            );
            file.validate_identity()?;
        }
        Ok(())
    }
    pub(crate) fn allows(&self, access: Access<'_>) -> bool {
        match access {
            Access::Inventory => true,
            Access::SessionObserve => self.sessions != SessionAccess::None,
            Access::SessionControl => self.sessions == SessionAccess::Control,
            Access::FullUserTerminal => self.full_user_terminal,
            Access::FileRead(id) => self.files.iter().any(|file| file.id == id),
            Access::FileWrite(id) => self.files.iter().any(|file| file.id == id && file.writable),
        }
    }
}

#[cfg(test)]
#[path = "grants_tests.rs"]
mod tests;

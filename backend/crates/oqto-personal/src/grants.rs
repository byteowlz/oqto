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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileGrant {
    id: String,
    path: PathBuf,
    writable: bool,
    device: u64,
    inode: u64,
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
    pub fn open_root(&self) -> Result<std::fs::File> {
        use std::os::unix::fs::OpenOptionsExt;
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY)
            .open(&self.path)?;
        let metadata = handle.metadata()?;
        ensure!(
            metadata.is_dir() && metadata.dev() == self.device && metadata.ino() == self.inode,
            "file grant root identity changed; owner reapproval required"
        );
        Ok(handle)
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
            Access::SessionObserve => self.sessions != SessionAccess::None,
            Access::SessionControl => self.sessions == SessionAccess::Control,
            Access::FullUserTerminal => self.full_user_terminal,
            Access::FileRead(id) => self.files.iter().any(|file| file.id == id),
            Access::FileWrite(id) => self.files.iter().any(|file| file.id == id && file.writable),
        }
    }
}

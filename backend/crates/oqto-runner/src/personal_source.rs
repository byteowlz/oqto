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
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SourceIdentity {
    root: DirectoryIdentity,
    file: PathBuf,
    dev: u64,
    ino: u64,
    native_id: String,
    workspace: PathBuf,
}
#[derive(Debug)]
pub(crate) struct NativeHeader {
    pub external_id: String,
    pub workspace: PathBuf,
    pub path: PathBuf,
    pub identity: String,
    pub dev: u64,
    pub ino: u64,
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
            header.cwd.is_absolute() && header.cwd.canonicalize()? == header.cwd,
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
        let identity = serde_json::to_string(&SourceIdentity {
            root: self.session.clone(),
            file: path.to_owned(),
            dev: before.dev(),
            ino: before.ino(),
            native_id: header.id.clone(),
            workspace: header.cwd.clone(),
        })?;
        Ok(NativeHeader {
            external_id: header.id,
            workspace: header.cwd,
            path: path.to_owned(),
            identity,
            dev: before.dev(),
            ino: before.ino(),
        })
    }
    pub(crate) fn headers(&self) -> Result<Vec<NativeHeader>> {
        self.validate()?;
        let mut headers = Vec::new();
        let mut count = 0;
        for (n, directory) in std::fs::read_dir(self.session_root())?.enumerate() {
            ensure!(
                n < MAX_DIRECTORIES,
                "Native directory discovery bound exceeded"
            );
            let directory = directory?;
            if !directory.file_type()?.is_dir() {
                continue;
            }
            // A symlink or foreign-owned directory is never traversed.
            if DirectoryIdentity::read(directory.path()).is_err() {
                continue;
            }
            for entry in std::fs::read_dir(directory.path())? {
                ensure!(
                    count < MAX_HEADERS,
                    "Native header discovery bound exceeded"
                );
                count += 1;
                let entry = entry?;
                if entry.file_type()?.is_file()
                    && entry.path().extension().is_some_and(|ext| ext == "jsonl")
                    && let Ok(header) = self.read_header(&entry.path(), false)
                {
                    headers.push(header);
                }
            }
        }
        Ok(headers)
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
            &header.identity,
        )
        .await?;
        Ok(header)
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

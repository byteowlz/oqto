//! Descriptor-rooted, no-follow snapshots of exact author sidecar names.
use super::Work;
use crate::personal_source::{NativeHeader, PersonalSource};
use anyhow::{Context, Result, ensure};
use rustix::fs::{Mode, OFlags, open, openat};
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::{File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Component, Path},
    sync::Arc,
};

const FILE_BYTES: usize = 64 * 1024;
const DIR: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const READ: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC)
    .union(OFlags::NONBLOCK);
type Identity = (u64, u64);
type Stamp = (u64, u64, u64, i64, i64, i64, i64);
fn identity(meta: &Metadata) -> Identity {
    (meta.dev(), meta.ino())
}
fn stamp(meta: &Metadata) -> Stamp {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    )
}
fn owned_file(meta: &Metadata) -> Result<()> {
    ensure!(
        meta.is_file()
            && meta.uid() == rustix::process::geteuid().as_raw()
            && meta.mode() & 0o022 == 0,
        "Unsafe relation hint file"
    );
    Ok(())
}
fn root(path: &Path) -> Result<File> {
    ensure!(
        path.is_absolute(),
        "Explicit canonical source root required"
    );
    let mut directory = File::from(open("/", DIR, Mode::empty())?);
    for component in path.components() {
        match component {
            Component::RootDir => (),
            Component::Normal(name) => {
                directory = File::from(openat(&directory, name, DIR, Mode::empty())?)
            }
            _ => anyhow::bail!("Noncanonical source component"),
        }
    }
    Ok(directory)
}
fn file(parent: &File, name: &OsStr) -> Result<Option<File>> {
    match openat(parent, name, READ, Mode::empty()) {
        Ok(fd) => Ok(Some(File::from(fd))),
        Err(rustix::io::Errno::NOENT) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Per-job descriptors only. Never stores HOME/source/authority globally.
#[derive(Default)]
pub(super) struct Directories {
    root: Option<Arc<File>>,
    parents: HashMap<OsString, Arc<File>>,
}
impl Directories {
    fn get(
        &mut self,
        source: &PersonalSource,
        directory: &OsStr,
        work: &mut Work,
    ) -> Result<(Arc<File>, Arc<File>)> {
        if self.root.is_none() {
            work.descriptor()?;
            self.root = Some(Arc::new(root(source.session_root())?));
        }
        let root = self
            .root
            .as_ref()
            .context("Relation root unavailable")?
            .clone();
        if !self.parents.contains_key(directory) {
            work.descriptor()?;
            let parent = Arc::new(File::from(openat(&*root, directory, DIR, Mode::empty())?));
            self.parents.insert(directory.to_owned(), parent);
        }
        let parent = self
            .parents
            .get(directory)
            .context("Relation directory unavailable")?
            .clone();
        Ok((root, parent))
    }
}

pub(super) struct Snapshot {
    root: Arc<File>,
    parent: Arc<File>,
    native: File,
    native_name: OsString,
    native_id: Identity,
    file: Option<File>,
    directory: OsString,
    name: OsString,
    root_id: Identity,
    parent_id: Identity,
    file_stamp: Option<Stamp>,
    pub(super) bytes: Option<Vec<u8>>,
}
impl Snapshot {
    pub(super) fn read(
        source: &PersonalSource,
        session_file: &Path,
        header: &NativeHeader,
        work: &mut Work,
        directories: &mut Directories,
    ) -> Result<Self> {
        source.validate()?;
        let directory = session_file
            .parent()
            .and_then(Path::file_name)
            .context("Native parent missing")?
            .to_owned();
        ensure!(
            session_file.parent().and_then(Path::parent) == Some(source.session_root()),
            "Unexpected native depth"
        );
        let (root, parent) = directories.get(source, &directory, work)?;
        let native_name = session_file.file_name().context("Native file missing")?;
        work.descriptor()?;
        let native = file(&parent, native_name)?.context("Native source vanished")?;
        let native_meta = native.metadata()?;
        ensure!(
            native_meta.is_file() && identity(&native_meta) == (header.dev, header.ino),
            "Native source replaced"
        );
        let mut name = native_name.to_owned();
        name.push(".relations.json");
        let root_id = identity(&root.metadata()?);
        let parent_id = identity(&parent.metadata()?);
        let mut snapshot = Self {
            root,
            parent,
            native,
            native_name: native_name.to_owned(),
            native_id: (header.dev, header.ino),
            file: None,
            directory,
            name,
            root_id,
            parent_id,
            file_stamp: None,
            bytes: None,
        };
        // Rebind the held descriptors to the still-selected source BEFORE
        // reading any optional bytes, not only after discarding bad output.
        snapshot.published_parent(source)?;
        let mut file = file(&snapshot.parent, &snapshot.name)?;
        if let Some(file) = &mut file {
            work.present()?;
            work.descriptor()?;
            let before = file.metadata()?;
            owned_file(&before)?;
            ensure!(
                before.len() <= FILE_BYTES as u64
                    && before.len() <= work.limits.bytes.saturating_sub(work.bytes) as u64,
                "Relation hint byte bound exceeded"
            );
            let capacity = FILE_BYTES.min(work.limits.bytes.saturating_sub(work.bytes));
            let mut content = Vec::new();
            (&mut *file)
                .take(capacity as u64)
                .read_to_end(&mut content)?;
            work.bytes(content.len())?;
            let after = file.metadata()?;
            owned_file(&after)?;
            ensure!(
                stamp(&before) == stamp(&after) && content.len() as u64 == before.len(),
                "Relation hint changed while reading"
            );
            snapshot.file_stamp = Some(stamp(&after));
            snapshot.bytes = Some(content);
        }
        snapshot.file = file;
        snapshot.revalidate(source)?;
        Ok(snapshot)
    }
    fn published_parent(&self, source: &PersonalSource) -> Result<File> {
        source.validate()?;
        let current_root = root(source.session_root())?;
        let current_parent =
            File::from(openat(&current_root, &self.directory, DIR, Mode::empty())?);
        ensure!(
            identity(&self.root.metadata()?) == self.root_id
                && identity(&current_root.metadata()?) == self.root_id
                && identity(&self.parent.metadata()?) == self.parent_id
                && identity(&current_parent.metadata()?) == self.parent_id,
            "Relation source directory replaced"
        );
        let current_native =
            file(&current_parent, &self.native_name)?.context("Native source vanished")?;
        ensure!(
            identity(&self.native.metadata()?) == self.native_id
                && identity(&current_native.metadata()?) == self.native_id,
            "Native author source replaced"
        );
        ensure!(
            current_parent.metadata()?.uid() == rustix::process::geteuid().as_raw(),
            "Native parent owner changed"
        );
        source.validate()?;
        Ok(current_parent)
    }
    pub(super) fn revalidate(&self, source: &PersonalSource) -> Result<()> {
        let current_parent = self.published_parent(source)?;
        let published = file(&current_parent, &self.name)?;
        match (&self.file, published, self.file_stamp) {
            (None, None, None) => (),
            (Some(held), Some(published), Some(expected)) => {
                let held = held.metadata()?;
                let published = published.metadata()?;
                owned_file(&held)?;
                owned_file(&published)?;
                ensure!(
                    stamp(&held) == expected && stamp(&published) == expected,
                    "Relation publication changed"
                );
            }
            _ => anyhow::bail!("Relation publication appeared or disappeared"),
        }
        Ok(())
    }
}

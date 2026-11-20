//! Preserve-first filesystem admission. No cleanup ever unlinks ownership locks.
use super::*;
use anyhow::ensure;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
};

const DB: &str = "catalog.sqlite3";
const MARKER: &str = "catalog-owner.json";
const PURPOSE: &str = "oqto.durable-catalog.v1";
const APPLICATION: u32 = 0x4f514443;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Identity {
    purpose: String,
    filesystem: String,
    inode: u64,
}
impl Identity {
    fn for_file(file: &File) -> Result<Self> {
        Ok(Self {
            purpose: PURPOSE.into(),
            filesystem: super::filesystem::identity(file)?,
            inode: file.metadata()?.ino(),
        })
    }
}

pub(super) struct Layout {
    root: PathBuf,
    root_file: File,
    control_file: File,
    data_file: File,
    locks_file: File,
    database_file: File,
}
pub(super) struct Prepared {
    pub(super) layout: Arc<Layout>,
    _initialization: File,
}
fn private(metadata: &fs::Metadata) -> Result<()> {
    private_owner(metadata, rustix::process::geteuid().as_raw())
}
fn private_owner(metadata: &fs::Metadata, uid: u32) -> Result<()> {
    ensure!(
        metadata.uid() == uid && metadata.mode() & 0o077 == 0,
        "Durable state must be private and owned by the current Principal"
    );
    Ok(())
}
fn parent_chain(parent: &Path) -> Result<()> {
    for path in parent.ancestors() {
        let meta = path.metadata()?;
        ensure!(meta.is_dir(), "Durable state ancestor is not a directory");
        let ancestor = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)?;
        super::acl::check(&ancestor)?;
        let uid = meta.uid();
        ensure!(
            uid == 0 || uid == rustix::process::geteuid().as_raw(),
            "Untrusted Durable state ancestor owner"
        );
        ensure!(
            meta.mode() & 0o022 == 0 || (uid == 0 && meta.mode() & 0o1000 != 0),
            "Writable Durable state ancestor"
        );
    }
    Ok(())
}
fn directory(path: &Path, create: bool) -> Result<File> {
    if create {
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error).context("create Durable state directory"),
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(meta.is_dir(), "Durable state is not a directory");
    private(&meta)?;
    super::acl::check(&file)?;
    Ok(file)
}
fn regular(path: &Path, create: bool) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.nlink() == 1,
        "Durable control file must be a single-link regular file"
    );
    private(&meta)?;
    super::acl::check(&file)?;
    Ok(file)
}
fn present(path: &Path) -> Result<bool> {
    Ok(crate::runtime_guard::no_follow_state_metadata(path)?.is_some())
}
fn names_only(path: &Path, allowed: &[&str]) -> Result<()> {
    for item in fs::read_dir(path)? {
        let name = item?.file_name();
        ensure!(
            name.to_str().is_some_and(|name| allowed.contains(&name)),
            "Unrecognized Durable control state; refusing adoption"
        );
    }
    Ok(())
}
fn same_file(path: &Path, expected: &File, is_directory: bool) -> Result<()> {
    let actual = if is_directory {
        directory(path, false)?
    } else {
        regular(path, false)?
    };
    ensure!(
        Identity::for_file(&actual)? == Identity::for_file(expected)?,
        "Durable state identity changed"
    );
    Ok(())
}
pub(super) fn same_directory(path: &Path, expected: &File) -> Result<()> {
    same_file(path, expected, true)
}
pub(super) fn same_lock(path: &Path, expected: &File) -> Result<()> {
    same_file(path, expected, false)
}

fn catalog(control: &Path, data: &Path) -> Result<File> {
    let path = control.join(DB);
    let marker = control.join(MARKER);
    let database_exists = present(&path)?;
    let marker_exists = present(&marker)?;
    ensure!(
        database_exists == marker_exists,
        "Incomplete Durable ownership state; explicit recovery required"
    );
    if !database_exists {
        ensure!(
            fs::read_dir(data)?.next().is_none(),
            "Existing Durable data has no catalog ownership record"
        );
        for name in [
            "catalog.sqlite3-wal",
            "catalog.sqlite3-shm",
            "catalog.sqlite3-journal",
        ] {
            ensure!(
                !present(&control.join(name))?,
                "Orphan Durable catalog sidecar"
            );
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)?;
        let mut owner = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&marker)?;
        owner.write_all(&serde_json::to_vec(&Identity::for_file(&file)?)?)?;
        owner.sync_all()?;
        file.sync_all()?;
        File::open(control)?.sync_all()?;
    }
    let mut file = regular(&path, false)?;
    let mut owner = regular(&marker, false)?;
    ensure!(
        owner.metadata()?.len() <= 512,
        "Oversized Durable ownership record"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut owner).take(513).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 512, "Oversized Durable ownership record");
    let identity: Identity =
        serde_json::from_slice(&bytes).context("Invalid Durable ownership record")?;
    ensure!(
        identity == Identity::for_file(&file)?,
        "Durable catalog ownership mismatch"
    );
    if file.metadata()?.len() == 0 {
        ensure!(
            !database_exists,
            "Empty existing Durable catalog; explicit recovery required"
        );
    } else {
        let mut header = [0_u8; 100];
        file.read_exact(&mut header)
            .context("Invalid Durable catalog header")?;
        ensure!(
            &header[..16] == b"SQLite format 3\0"
                && u32::from_be_bytes([header[68], header[69], header[70], header[71]])
                    == APPLICATION,
            "Foreign Durable catalog header"
        );
    }
    for name in [
        "catalog.sqlite3-wal",
        "catalog.sqlite3-shm",
        "catalog.sqlite3-journal",
    ] {
        if present(&control.join(name))? {
            regular(&control.join(name), false)?;
        }
    }
    Ok(file)
}

// Inspect existing state before creating locks, directories or opening SQLite.
fn preflight(root: &Path) -> Result<()> {
    let control = root.join("control");
    let data = root.join("data");
    let has_control = present(&control)?;
    let has_data = present(&data)?;
    ensure!(
        has_control == has_data,
        "Incomplete Durable directory layout; explicit recovery required"
    );
    if !has_control {
        return Ok(());
    }
    directory(&control, false)?;
    directory(&data, false)?;
    names_only(
        &control,
        &[
            "catalog.lock",
            DB,
            MARKER,
            "catalog.sqlite3-wal",
            "catalog.sqlite3-shm",
            "catalog.sqlite3-journal",
            "locks",
        ],
    )?;
    for name in [
        "catalog.lock",
        "catalog.sqlite3-wal",
        "catalog.sqlite3-shm",
        "catalog.sqlite3-journal",
    ] {
        if present(&control.join(name))? {
            regular(&control.join(name), false)?;
        }
    }
    if present(&control.join("locks"))? {
        directory(&control.join("locks"), false)?;
    }
    let database = present(&control.join(DB))?;
    let marker = present(&control.join(MARKER))?;
    ensure!(
        database == marker,
        "Incomplete Durable ownership state; explicit recovery required"
    );
    if database {
        ensure!(
            present(&control.join("catalog.lock"))? && present(&control.join("locks"))?,
            "Incomplete Durable control layout"
        );
        catalog(&control, &data)?;
    } else {
        ensure!(
            fs::read_dir(&data)?.next().is_none(),
            "Existing Durable data has no catalog ownership record"
        );
        for name in [
            "catalog.sqlite3-wal",
            "catalog.sqlite3-shm",
            "catalog.sqlite3-journal",
        ] {
            ensure!(
                !present(&control.join(name))?,
                "Orphan Durable catalog sidecar"
            );
        }
    }
    Ok(())
}

pub(super) fn prepare(path: &Path) -> Result<Prepared> {
    ensure!(
        path.is_absolute()
            && path.components().all(|p| matches!(
                p,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            )),
        "Durable state requires a normalized absolute path"
    );
    let parent = path
        .parent()
        .context("Durable root needs a parent")?
        .canonicalize()?;
    parent_chain(&parent)?;
    let root = parent.join(path.file_name().context("Durable root needs a leaf")?);
    if !present(&root)? {
        let parent_file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(&parent)?;
        super::filesystem::identity(&parent_file)?;
    }
    let root_file = directory(&root, true)?;
    super::filesystem::identity(&root_file)?;
    names_only(&root, &["control", "data"])?;
    preflight(&root)?;
    let control_file = directory(&root.join("control"), true)?;
    let data_file = directory(&root.join("data"), true)?;
    let control = root.join("control");
    names_only(
        &control,
        &[
            "catalog.lock",
            DB,
            MARKER,
            "catalog.sqlite3-wal",
            "catalog.sqlite3-shm",
            "catalog.sqlite3-journal",
            "locks",
        ],
    )?;
    let initialization = regular(&control.join("catalog.lock"), true)?;
    initialization
        .try_lock()
        .context("Durable bootstrap is already running")?;
    let locks_file = directory(&control.join("locks"), true)?;
    let database_file = catalog(&control, &root.join("data"))?;
    Ok(Prepared {
        layout: Arc::new(Layout {
            root,
            root_file,
            control_file,
            data_file,
            locks_file,
            database_file,
        }),
        _initialization: initialization,
    })
}
impl Layout {
    pub(super) fn database(&self) -> PathBuf {
        self.root.join("control").join(DB)
    }
    pub(super) fn lock_path(&self, id: &str) -> PathBuf {
        self.root.join("control/locks").join(format!("{id}.lock"))
    }
    pub(super) fn check(&self) -> Result<()> {
        parent_chain(self.root.parent().context("Durable state parent")?)?;
        same_directory(&self.root, &self.root_file)?;
        same_directory(&self.root.join("control"), &self.control_file)?;
        same_directory(&self.root.join("data"), &self.data_file)?;
        same_directory(&self.root.join("control/locks"), &self.locks_file)?;
        same_file(&self.database(), &self.database_file, false)?;
        // Also revalidate marker/header/sidecars before further catalog work.
        catalog(&self.root.join("control"), &self.root.join("data"))?;
        Ok(())
    }
    pub(super) fn lease(&self, id: &str) -> Result<(PathBuf, File, File)> {
        ensure!(
            uuid::Uuid::parse_str(id)?.to_string() == id,
            "Invalid Durable store identity"
        );
        self.check()?;
        let lock = regular(&self.lock_path(id), true)?;
        lock.try_lock()
            .context("Durable store already has an owner")?;
        let path = self.root.join("data").join(id);
        let directory = directory(&path, true)?;
        self.check()?;
        same_lock(&self.lock_path(id), &lock)?;
        Ok((path, directory, lock))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_check_uses_descriptor_metadata_and_rejects_a_different_principal() -> Result<()> {
        let root = tempfile::tempdir()?;
        let file = directory(&root.path().join("owned"), true)?;
        let metadata = file.metadata()?;
        assert_eq!(metadata.mode() & 0o077, 0);
        private(&metadata)?;
        assert!(private_owner(&metadata, metadata.uid().wrapping_add(1)).is_err());
        Ok(())
    }
}

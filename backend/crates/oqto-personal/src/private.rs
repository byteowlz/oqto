//! Preserve-first Unix private-state admission; never chmod unknown state into compliance.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

mod identity;
mod restore;
pub use identity::filesystem as persistent_filesystem;
pub(crate) use restore::restore_owner;

pub(crate) fn open_state(path: &Path) -> Result<(PathBuf, PathBuf, fs::File)> {
    open_state_impl(path, None)
}

pub(crate) fn repair_owner(path: &Path, expected_sha256: &str) -> Result<PathBuf> {
    ensure!(
        expected_sha256.len() == 64 && expected_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "expected marker SHA256 required"
    );
    let (root, _, _lock) = open_state_impl(path, Some(expected_sha256))?;
    Ok(root.join(format!("store-owner-{expected_sha256}.backup")))
}

fn open_state_impl(path: &Path, repair: Option<&str>) -> Result<(PathBuf, PathBuf, fs::File)> {
    if repair.is_some() {
        ensure!(
            path.is_dir()
                && path.join("personal.sqlite3").is_file()
                && path.join("store-owner.json").exists()
                && path.join("authority.lock").is_file(),
            "repair requires an existing private store and marker"
        );
    }
    ensure!(
        path.is_absolute(),
        "personal state directory must be absolute"
    );
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let leaf = fs::symlink_metadata(path)?;
    ensure!(
        !leaf.file_type().is_symlink(),
        "personal state directory cannot be a symlink"
    );
    let root = path.canonicalize()?;
    let uid = rustix::process::geteuid().as_raw();
    ensure!(
        leaf.is_dir() && leaf.uid() == uid && leaf.mode() & 0o077 == 0,
        "personal state must be owned by the host user with no group/other access"
    );
    for ancestor in root.ancestors().skip(1) {
        let metadata = ancestor.metadata()?;
        ensure!(
            metadata.uid() == uid || metadata.uid() == 0,
            "untrusted personal state ancestor owner"
        );
        ensure!(
            metadata.mode() & 0o022 == 0 || (metadata.uid() == 0 && metadata.mode() & 0o1000 != 0),
            "writable personal state ancestor"
        );
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root.join("authority.lock"))?;
    let metadata = lock.metadata()?;
    ensure!(
        metadata.is_file() && metadata.uid() == uid && metadata.mode() & 0o077 == 0,
        "authority lock must be private and owned"
    );
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .context("personal authority is already running")?;
    let database = root.join("personal.sqlite3");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&database)
    {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).context("create private device store"),
    }
    for name in [
        "personal.sqlite3",
        "personal.sqlite3-wal",
        "personal.sqlite3-shm",
        "personal.sqlite3-journal",
    ] {
        match root.join(name).symlink_metadata() {
            Ok(metadata) => ensure!(
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.uid() == uid
                    && metadata.mode() & 0o077 == 0,
                "device store files must be private owned regular files"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    // Establish durable ownership BEFORE the first SQLite write. A process can
    // die after SQLite makes its header but before migration sets application_id.
    // Only the exact inode recorded by this private marker may recover that state.
    let marker = root.join("store-owner.json");
    let metadata = database.metadata()?;
    let database_file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&database)?;
    ensure!(
        database_file.metadata()?.is_file(),
        "database must be a regular file"
    );
    let persistent = identity::StoreIdentity::for_file(&database_file)?;
    let ownership = persistent.encode()?;
    let mut recognized_header = metadata.len() == 0;
    if metadata.len() > 0 {
        use std::io::Read;
        let mut header = [0u8; 100];
        fs::File::open(&database)?.read_exact(&mut header)?;
        ensure!(
            &header[..16] == b"SQLite format 3\0",
            "unrecognized personal store; refusing to modify it"
        );
        recognized_header = header[68..72] == [0x4f, 0x51, 0x50, 0x50];
        ensure!(
            recognized_header || header[68..72] == [0, 0, 0, 0],
            "foreign personal store application identity"
        );
    }
    match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&marker)
    {
        Ok(mut file) => {
            use std::io::Read;
            let meta = file.metadata()?;
            ensure!(
                meta.is_file()
                    && meta.uid() == uid
                    && meta.mode() & 0o077 == 0
                    && meta.len() <= 256,
                "invalid store ownership marker"
            );
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            if let Some(expected) = repair {
                use sha2::{Digest, Sha256};
                ensure!(
                    format!("{:x}", Sha256::digest(contents.as_bytes())) == expected,
                    "marker changed since approval; refusing repair"
                );
                let legacy: identity::LegacyIdentity = serde_json::from_str(&contents)
                    .context("only legacy ownership markers can be repaired")?;
                ensure!(
                    legacy.purpose == "oqto.personal-store.v1" && legacy.ino == metadata.ino(),
                    "repair refuses foreign database inode or marker"
                );
                ensure!(
                    recognized_header && metadata.len() >= 100,
                    "repair requires a recognized initialized personal database"
                );
                let backup = root.join(format!("store-owner-{expected}.backup"));
                match fs::hard_link(&marker, &backup) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        // Resume only our exact hard-linked backup after an interrupted
                        // pre-publication migration. Unknown backup files are preserved.
                        let saved = fs::OpenOptions::new()
                            .read(true)
                            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                            .open(&backup)?;
                        let saved_meta = saved.metadata()?;
                        ensure!(
                            saved_meta.is_file()
                                && saved_meta.dev() == meta.dev()
                                && saved_meta.ino() == meta.ino(),
                            "existing ownership backup is not the original marker; preserving it"
                        );
                    }
                    Err(error) => return Err(error).context("preserve old ownership marker"),
                }
                fs::File::open(&root)?.sync_all()?;
                let staged = root.join(format!("store-owner-{}.new", uuid::Uuid::new_v4()));
                let result = (|| -> Result<()> {
                    use std::io::Write;
                    let mut output = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&staged)?;
                    output.write_all(ownership.as_bytes())?;
                    output.sync_all()?;
                    let current = fs::symlink_metadata(&marker)?;
                    ensure!(
                        current.dev() == meta.dev() && current.ino() == meta.ino(),
                        "marker replaced during repair"
                    );
                    let current_database = fs::OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(&database)?;
                    ensure!(
                        identity::StoreIdentity::for_file(&current_database)? == persistent,
                        "database replaced during repair"
                    );
                    let mut current_marker = fs::OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(&marker)?;
                    let mut current_contents = String::new();
                    std::io::Read::take(&mut current_marker, 257)
                        .read_to_string(&mut current_contents)?;
                    ensure!(
                        current_contents == contents,
                        "marker contents changed during repair"
                    );
                    fs::rename(&staged, &marker)?;
                    fs::File::open(&root)?.sync_all()?;
                    Ok(())
                })();
                if staged.try_exists()? {
                    fs::remove_file(&staged)?;
                }
                result?;
            } else {
                identity::validate(&contents, &persistent, metadata.dev())?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            use std::io::Write;
            ensure!(
                recognized_header,
                "unrecognized personal store; refusing to claim interrupted state"
            );
            // An interrupted write must never leave a half-valid final marker.
            // Link the fully synced private staging inode without clobbering.
            let staged = root.join(format!("store-owner-{}.new", uuid::Uuid::new_v4()));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&staged)?;
            file.write_all(ownership.as_bytes())?;
            file.sync_all()?;
            fs::hard_link(&staged, &marker)?;
            fs::File::open(&root)?.sync_all()?;
            fs::remove_file(staged)?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok((root, database, lock))
}

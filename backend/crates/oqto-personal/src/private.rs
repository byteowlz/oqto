//! Preserve-first Unix private-state admission; never chmod unknown state into compliance.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

pub(crate) fn open_state(path: &Path) -> Result<(PathBuf, PathBuf, fs::File)> {
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
    let ownership = format!(
        "{{\"purpose\":\"oqto.personal-store.v1\",\"dev\":{},\"ino\":{}}}\n",
        metadata.dev(),
        metadata.ino()
    );
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
            ensure!(
                contents == ownership,
                "store ownership marker does not match database inode"
            );
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

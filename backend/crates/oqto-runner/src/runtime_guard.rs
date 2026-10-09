//! Admission for private runner state, before config helpers or SQLite can act.
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// One default endpoint shared by the client and daemon. No template expansion.
pub fn default_socket_path() -> PathBuf {
    socket_path_in(
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .as_deref(),
    )
}

fn socket_path_in(runtime_dir: Option<&Path>) -> PathBuf {
    runtime_dir
        .unwrap_or_else(|| Path::new("/tmp"))
        .join("oqto-runner.sock")
}

pub fn validate_socket_path(path: &Path) -> Result<()> {
    anyhow::ensure!(
        path.is_absolute(),
        "runner socket must be an absolute path: {}",
        path.display()
    );
    anyhow::ensure!(
        !path.to_string_lossy().contains('{') && !path.to_string_lossy().contains('}'),
        "unresolved runner socket placeholder: {}",
        path.display()
    );
    Ok(())
}

#[cfg(unix)]
fn effective_uid() -> u32 {
    // SAFETY: geteuid has no arguments, memory access, or failure case.
    unsafe { libc::geteuid() }
}

fn existing_ancestor(path: &Path) -> Result<(&Path, std::fs::Metadata)> {
    for ancestor in path.ancestors() {
        match std::fs::metadata(ancestor) {
            Ok(metadata) => return Ok((ancestor, metadata)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspect runner state owner: {}", ancestor.display())
                });
            }
        }
    }
    bail!("no existing ancestor for runner state: {}", path.display())
}

#[cfg(unix)]
fn require_owner(path: &Path, actual_uid: u32, owner_uid: u32) -> Result<()> {
    anyhow::ensure!(
        actual_uid == owner_uid,
        "refusing wrong-UID runner state access: effective uid {actual_uid}, owner uid {owner_uid}, path {}. Start the runner as the owning user; do not repair this by recursively chowning user state",
        path.display()
    );
    Ok(())
}

/// Private writable state must belong to this process's actual OS principal.
/// Administrative multi-user runners may use administrative state; user state
/// must instead be opened by that user's runner. Never change process-wide UID.
pub fn ensure_private_state_owner(path: &Path) -> Result<()> {
    #[cfg(unix)]
    let uid = effective_uid();
    #[cfg(not(unix))]
    let uid = 0;
    audit_private_state_owner(path, uid)
}

/// Read-only doctor audit for an explicitly resolved OS identity; confers no access grant.
pub fn audit_private_state_owner(path: &Path, uid: u32) -> Result<()> {
    anyhow::ensure!(
        path.is_absolute(),
        "private runner state must be absolute: {}",
        path.display()
    );
    let (ancestor, metadata) = existing_ancestor(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        require_owner(ancestor, uid, metadata.uid())?;
    }
    #[cfg(not(unix))]
    let _ = (ancestor, metadata, uid);
    Ok(())
}

/// A user may create an endpoint in an administrative directory (e.g. /tmp),
/// but no runner may mutate another user's runtime directory, even as root.
pub fn ensure_socket_identity(path: &Path) -> Result<()> {
    validate_socket_path(path)?;
    let parent = path
        .parent()
        .context("runner socket requires a parent directory")?;
    let (ancestor, metadata) = existing_ancestor(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != 0 {
            require_owner(ancestor, effective_uid(), metadata.uid())?;
        }
    }
    #[cfg(not(unix))]
    let _ = (ancestor, metadata);
    Ok(())
}

/// Metadata-only ownership check; deliberately never opens SQLite, even read
/// only, because a readonly WAL connection can create sidecars as this UID.
pub fn ensure_history_state_owner(home: &Path) -> Result<()> {
    #[cfg(unix)]
    let uid = effective_uid();
    #[cfg(not(unix))]
    let uid = 0;
    audit_history_state_owner(home, uid)
}

/// Audit home/shards/DB/WAL/SHM by metadata only, including from an admin doctor.
pub fn audit_history_state_owner(home: &Path, uid: u32) -> Result<()> {
    audit_private_state_owner(home, uid)?;
    let root = home.join(".local/share/oqto/oqto-log");
    if !root.try_exists()? {
        return Ok(());
    }
    audit_private_state_owner(&root, uid)?;
    anyhow::ensure!(
        !root.is_symlink(),
        "history root must not be a symlink: {}",
        root.display()
    );
    for entry in std::fs::read_dir(&root).context("inspect history shard ownership")? {
        let shard = entry?.path();
        let metadata = std::fs::symlink_metadata(&shard)?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "history shard must not be a symlink: {}",
            shard.display()
        );
        if !metadata.is_dir() {
            continue;
        }
        audit_private_state_owner(&shard, uid)?;
        for name in [
            "oqto-log.sqlite",
            "oqto-log.sqlite-wal",
            "oqto-log.sqlite-shm",
        ] {
            let file = shard.join(name);
            match std::fs::symlink_metadata(&file) {
                Ok(metadata) => {
                    anyhow::ensure!(
                        metadata.is_file(),
                        "history SQLite state must be a regular file: {}",
                        file.display()
                    );
                    audit_private_state_owner(&file, uid)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("inspect history state: {}", file.display()));
                }
            }
        }
    }
    Ok(())
}

pub fn ensure_config_state_owner(config: &crate::daemon::config::RunnerUserConfig) -> Result<()> {
    ensure_private_state_owner(&config.pi_sessions_dir)?;
    ensure_private_state_owner(&config.memories_dir)?;
    if let Some(history) = &config.history_read {
        ensure_history_state_owner(&history.home)?;
    }
    Ok(())
}

/// Call before config loading (which invokes Pi), imports and worker startup,
/// and again after loading env files to validate newly introduced XDG roots.
pub fn ensure_launch_identity() -> Result<()> {
    let home =
        std::env::var_os("HOME").context("runner requires an explicit HOME for private state")?;
    ensure_history_state_owner(Path::new(&home))?;
    for key in ["XDG_DATA_HOME", "XDG_RUNTIME_DIR"] {
        if let Some(path) = std::env::var_os(key) {
            ensure_private_state_owner(Path::new(&path))
                .with_context(|| format!("validate {key}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn doctor_explicit_identity_audit_is_metadata_only_and_rejects_wrong_owner() -> Result<()> {
        let home = tempfile::tempdir()?;
        let shard = home.path().join(".local/share/oqto/oqto-log/example");
        std::fs::create_dir_all(&shard)?;
        let database = shard.join("oqto-log.sqlite");
        std::fs::write(&database, b"not opened as SQLite")?;
        audit_history_state_owner(home.path(), effective_uid())?;
        assert!(!shard.join("oqto-log.sqlite-wal").exists());
        assert!(!shard.join("oqto-log.sqlite-shm").exists());
        assert!(audit_history_state_owner(home.path(), effective_uid().wrapping_add(1)).is_err());
        assert_eq!(std::fs::read(database)?, b"not opened as SQLite");
        Ok(())
    }

    #[test]
    fn canonical_socket_has_no_literal_placeholder() {
        let path = socket_path_in(Some(Path::new("/run/user/1234")));
        assert_eq!(path, Path::new("/run/user/1234/oqto-runner.sock"));
        validate_socket_path(&path).expect("canonical socket");
        assert_eq!(socket_path_in(None), Path::new("/tmp/oqto-runner.sock"));
    }

    #[test]
    fn rejects_relative_and_unexpanded_endpoints() {
        for path in [
            "oqto-runner.sock",
            "/run/user/1000/{runtime_dir}/oqto-runner.sock",
            "/tmp/{uid}.sock",
        ] {
            assert!(validate_socket_path(Path::new(path)).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn wrong_uid_is_rejected_even_when_process_is_root() {
        let home = Path::new("/home/user");
        assert!(require_owner(home, 0, 1000).is_err());
        assert!(require_owner(home, 1001, 1000).is_err());
        require_owner(home, 1000, 1000).expect("own user state");
        require_owner(Path::new("/root"), 0, 0).expect("administrative state");
    }

    #[test]
    fn metadata_audit_never_opens_sqlite_or_creates_sidecars() {
        let home = tempfile::tempdir().expect("fixture");
        let shard = home.path().join(".local/share/oqto/oqto-log/shard");
        std::fs::create_dir_all(&shard).expect("shard");
        let db = shard.join("oqto-log.sqlite");
        std::fs::write(&db, b"not even a database").expect("fixture bytes");
        ensure_history_state_owner(home.path()).expect("metadata only");
        assert_eq!(
            std::fs::read(&db).expect("unchanged bytes"),
            b"not even a database"
        );
        assert!(!shard.join("oqto-log.sqlite-wal").exists());
        assert!(!shard.join("oqto-log.sqlite-shm").exists());
    }

    #[cfg(unix)]
    #[test]
    fn metadata_audit_rejects_symlink_sidecars_without_touching_target() {
        let home = tempfile::tempdir().expect("fixture");
        let shard = home.path().join(".local/share/oqto/oqto-log/shard");
        std::fs::create_dir_all(&shard).expect("shard");
        let target = home.path().join("protected");
        std::fs::write(&target, b"unchanged").expect("target");
        std::os::unix::fs::symlink(&target, shard.join("oqto-log.sqlite-shm"))
            .expect("sidecar symlink");
        assert!(ensure_history_state_owner(home.path()).is_err());
        assert_eq!(
            std::fs::read(target).expect("target unchanged"),
            b"unchanged"
        );
    }

    #[test]
    fn missing_private_state_is_checked_without_creating_it() {
        let dir = tempfile::tempdir().expect("fixture");
        let missing = dir.path().join("missing/history/shard/oqto-log.sqlite");
        ensure_private_state_owner(&missing).expect("owned nearest ancestor");
        assert!(!dir.path().join("missing").exists());
    }
}

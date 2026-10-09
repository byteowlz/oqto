//! Exclusive, incarnation-bound Unix listener ownership. Lock inodes persist.
use anyhow::{Context, Result, bail};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub(crate) struct SocketLease {
    _lock: File,
    path: PathBuf,
    bound: Option<(u64, u64)>,
}

impl SocketLease {
    pub(crate) fn acquire(path: &Path) -> Result<Self> {
        crate::runtime_guard::validate_socket_path(path)?;
        let lock_path = path.with_extension("sock.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&lock_path)
            .with_context(|| format!("open runner lifetime lock: {}", lock_path.display()))?;
        anyhow::ensure!(
            lock.metadata()?.is_file(),
            "runner lifetime lock is not a regular file"
        );
        crate::runtime_guard::ensure_private_state_owner(&lock_path)?;
        lock.try_lock()
            .with_context(|| format!("another runner owns endpoint {}", path.display()))?;
        Ok(Self {
            _lock: lock,
            path: path.to_path_buf(),
            bound: None,
        })
    }

    /// Only remove a proven refused stale socket, under the lifetime lock.
    /// A timeout, permission error, symlink or non-socket is not stale proof.
    pub(crate) async fn prepare(&self) -> Result<()> {
        use std::os::unix::fs::FileTypeExt;
        let metadata = match std::fs::symlink_metadata(&self.path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).context("inspect existing runner endpoint"),
        };
        anyhow::ensure!(
            metadata.file_type().is_socket(),
            "refusing to replace non-socket endpoint {}",
            self.path.display()
        );
        crate::runtime_guard::ensure_private_state_owner(&self.path)?;
        match tokio::time::timeout(
            std::time::Duration::from_millis(500),
            tokio::net::UnixStream::connect(&self.path),
        )
        .await
        {
            Ok(Ok(_)) => bail!(
                "runner endpoint already has a live listener: {}",
                self.path.display()
            ),
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                let current = std::fs::symlink_metadata(&self.path)?;
                anyhow::ensure!(
                    (current.dev(), current.ino()) == (metadata.dev(), metadata.ino()),
                    "runner endpoint changed during stale check"
                );
                std::fs::remove_file(&self.path).context("remove proven stale runner endpoint")?;
                Ok(())
            }
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(Err(error)) => Err(error).context("cannot prove existing runner endpoint is stale"),
            Err(_) => bail!("existing runner endpoint timed out; refusing to replace it"),
        }
    }

    pub(crate) fn record_bound(&mut self) -> Result<()> {
        use std::os::unix::fs::FileTypeExt;
        let metadata = std::fs::symlink_metadata(&self.path)?;
        anyhow::ensure!(
            metadata.file_type().is_socket(),
            "bound endpoint is no longer a socket"
        );
        self.bound = Some((metadata.dev(), metadata.ino()));
        Ok(())
    }
}

impl Drop for SocketLease {
    fn drop(&mut self) {
        if let Some(bound) = self.bound
            && let Ok(metadata) = std::fs::symlink_metadata(&self.path)
            && (metadata.dev(), metadata.ino()) == bound
            && let Err(error) = std::fs::remove_file(&self.path)
        {
            log::warn!(
                "cleanup owned runner endpoint {}: {error}",
                self.path.display()
            );
        }
        // Never unlink the lifetime lock. File close releases its lock while
        // retaining the same inode for the next incarnation's admission.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[tokio::test]
    async fn live_unmanaged_listener_is_never_unlinked() {
        let dir = tempfile::tempdir().expect("fixture");
        let path = dir.path().join("runner.sock");
        let listener = UnixListener::bind(&path).expect("unmanaged listener");
        let inode = path.metadata().expect("socket").ino();
        let lease = SocketLease::acquire(&path).expect("lock");
        assert!(lease.prepare().await.is_err());
        drop(lease);
        assert_eq!(path.metadata().expect("retained socket").ino(), inode);
        drop(listener);
    }

    #[tokio::test]
    async fn stale_socket_removed_but_files_and_symlinks_preserved() {
        let dir = tempfile::tempdir().expect("fixture");
        let path = dir.path().join("runner.sock");
        drop(UnixListener::bind(&path).expect("stale listener"));
        let lease = SocketLease::acquire(&path).expect("lock");
        lease.prepare().await.expect("stale proof");
        assert!(!path.exists());
        std::fs::write(&path, "do not delete").expect("file");
        assert!(lease.prepare().await.is_err());
        assert_eq!(
            std::fs::read_to_string(&path).expect("file"),
            "do not delete"
        );
        std::fs::remove_file(&path).expect("remove fixture");
        std::os::unix::fs::symlink(dir.path().join("target"), &path).expect("symlink");
        assert!(lease.prepare().await.is_err());
        assert!(path.is_symlink());
    }

    #[tokio::test]
    async fn lifetime_lock_and_cleanup_protect_newer_incarnation() {
        let dir = tempfile::tempdir().expect("fixture");
        let path = dir.path().join("runner.sock");
        let mut old = SocketLease::acquire(&path).expect("old lease");
        let listener = UnixListener::bind(&path).expect("old listener");
        old.record_bound().expect("bound");
        let lock_inode = path
            .with_extension("sock.lock")
            .metadata()
            .expect("lock")
            .ino();
        assert!(SocketLease::acquire(&path).is_err());
        std::fs::remove_file(&path).expect("simulate external replacement");
        let newer = UnixListener::bind(&path).expect("new listener");
        let new_inode = path.metadata().expect("new socket").ino();
        drop(old);
        assert_eq!(
            path.metadata().expect("new listener retained").ino(),
            new_inode
        );
        let next = SocketLease::acquire(&path).expect("released lock");
        assert_eq!(
            path.with_extension("sock.lock")
                .metadata()
                .expect("same lock inode")
                .ino(),
            lock_inode
        );
        assert!(next.prepare().await.is_err());
        drop(listener);
        drop(newer);
    }

    #[test]
    fn symlink_lifetime_lock_is_rejected() {
        let dir = tempfile::tempdir().expect("fixture");
        let path = dir.path().join("runner.sock");
        let target = dir.path().join("target");
        std::fs::write(&target, "unchanged").expect("target");
        std::os::unix::fs::symlink(&target, path.with_extension("sock.lock"))
            .expect("symlink lock");
        assert!(SocketLease::acquire(&path).is_err());
        assert_eq!(
            std::fs::read_to_string(target).expect("target"),
            "unchanged"
        );
    }
}

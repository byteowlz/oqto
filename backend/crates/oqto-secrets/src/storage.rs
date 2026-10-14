use crate::ProviderError;
use std::fs::File;
#[cfg(target_os = "linux")]
use std::fs::OpenOptions;
#[cfg(target_os = "linux")]
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub(crate) const MAX_VAULT_BYTES: usize = 2 * 1024 * 1024;

/// Exclusion is anchored to the directory inode, never a removable lock file.
/// All operations use its retained Linux proc-fd path, so a rename/replacement
/// cannot redirect an already-open provider's writes into a different vault.
pub(crate) struct Storage {
    configured_root: PathBuf,
    pinned_root: PathBuf,
    directory: File,
}

#[cfg(target_os = "linux")]
impl Storage {
    pub(crate) fn open(root: &Path) -> Result<Self, ProviderError> {
        use fs4::fs_std::FileExt;
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        if !root.is_absolute() {
            return Err(ProviderError::UnsafeStorage);
        }
        // Trusted runner config supplies the path; never an App. Do not
        // repair permissions or follow pre-existing symlink components.
        for ancestor in root.ancestors() {
            match std::fs::symlink_metadata(ancestor) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(ProviderError::UnsafeStorage);
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(ProviderError::Storage),
            }
        }
        match std::fs::symlink_metadata(root) {
            Ok(metadata) => check_permissions(&metadata, true)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::DirBuilder::new()
                    .mode(0o700)
                    .create(root)
                    .map_err(|_| ProviderError::Storage)?;
            }
            Err(_) => return Err(ProviderError::Storage),
        }
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(root)
            .map_err(|_| ProviderError::UnsafeStorage)?;
        check_permissions(
            &directory.metadata().map_err(|_| ProviderError::Storage)?,
            true,
        )?;
        if !directory
            .try_lock_exclusive()
            .map_err(|_| ProviderError::Storage)?
        {
            return Err(ProviderError::Busy);
        }
        // The descriptor is private, retained, and close-on-exec. This path is
        // generated internally, never accepted as an external capability/path.
        let pinned_root = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        std::fs::metadata(&pinned_root).map_err(|_| ProviderError::Storage)?;
        let store = Self {
            configured_root: root.to_owned(),
            pinned_root,
            directory,
        };
        store.exists()?;
        Ok(store)
    }

    pub(crate) fn ensure_anchor(&self) -> Result<(), ProviderError> {
        use std::os::unix::fs::MetadataExt;
        let pinned = self
            .directory
            .metadata()
            .map_err(|_| ProviderError::Storage)?;
        let configured = std::fs::symlink_metadata(&self.configured_root)
            .map_err(|_| ProviderError::UnsafeStorage)?;
        check_permissions(&pinned, true)?;
        check_permissions(&configured, true)?;
        if pinned.dev() != configured.dev() || pinned.ino() != configured.ino() {
            return Err(ProviderError::UnsafeStorage);
        }
        Ok(())
    }

    pub(crate) fn exists(&self) -> Result<bool, ProviderError> {
        self.ensure_anchor()?;
        match std::fs::symlink_metadata(self.pinned_root.join("vault.age")) {
            Ok(metadata) => {
                check_permissions(&metadata, false)?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(ProviderError::Storage),
        }
    }

    pub(crate) fn read(&self) -> Result<Vec<u8>, ProviderError> {
        use std::os::unix::fs::OpenOptionsExt;
        self.ensure_anchor()?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(self.pinned_root.join("vault.age"))
            .map_err(|_| ProviderError::Storage)?;
        let metadata = file.metadata().map_err(|_| ProviderError::Storage)?;
        check_permissions(&metadata, false)?;
        if metadata.len() > MAX_VAULT_BYTES as u64 {
            return Err(ProviderError::Integrity);
        }
        let mut bytes = Vec::new();
        file.take((MAX_VAULT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ProviderError::Storage)?;
        if bytes.len() > MAX_VAULT_BYTES {
            return Err(ProviderError::Integrity);
        }
        Ok(bytes)
    }

    pub(crate) fn replace(&self, ciphertext: &[u8]) -> Result<(), ProviderError> {
        if ciphertext.len() > MAX_VAULT_BYTES {
            return Err(ProviderError::InvalidInput);
        }
        self.exists()?;
        let mut staged = tempfile::NamedTempFile::new_in(&self.pinned_root)
            .map_err(|_| ProviderError::Storage)?;
        staged
            .write_all(ciphertext)
            .map_err(|_| ProviderError::Storage)?;
        staged
            .as_file()
            .sync_all()
            .map_err(|_| ProviderError::Storage)?;
        staged
            .persist(self.pinned_root.join("vault.age"))
            .map_err(|_| ProviderError::Storage)?;
        self.directory
            .sync_all()
            .map_err(|_| ProviderError::Storage)?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn check_permissions(metadata: &std::fs::Metadata, directory: bool) -> Result<(), ProviderError> {
    use std::os::unix::fs::MetadataExt;
    if metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
        || metadata.file_type().is_symlink()
        || (directory && !metadata.is_dir())
        || (!directory && (!metadata.is_file() || metadata.nlink() != 1))
    {
        return Err(ProviderError::UnsafeStorage);
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
impl Storage {
    pub(crate) fn open(_root: &Path) -> Result<Self, ProviderError> {
        Err(ProviderError::UnsupportedPlatform)
    }
    pub(crate) fn ensure_anchor(&self) -> Result<(), ProviderError> {
        Err(ProviderError::UnsupportedPlatform)
    }
    pub(crate) fn exists(&self) -> Result<bool, ProviderError> {
        Err(ProviderError::UnsupportedPlatform)
    }
    pub(crate) fn read(&self) -> Result<Vec<u8>, ProviderError> {
        Err(ProviderError::UnsupportedPlatform)
    }
    pub(crate) fn replace(&self, _ciphertext: &[u8]) -> Result<(), ProviderError> {
        Err(ProviderError::UnsupportedPlatform)
    }
}

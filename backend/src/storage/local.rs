//! Local filesystem storage implementation.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use log::debug;
use std::path::{Path, PathBuf};
use tokio::fs;

use super::{Storage, StorageError, StorageMetadata, StorageResult};

/// Local filesystem storage implementation.
#[derive(Debug, Clone)]
pub struct LocalStorage {
    /// Base directory for storage.
    base_path: PathBuf,
}

impl LocalStorage {
    /// Create a new local storage instance.
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Get the full path for a storage path.
    fn full_path(&self, path: &str) -> PathBuf {
        self.base_path.join(normalize_path(path))
    }

    /// Ensure the base directory exists.
    async fn ensure_base_dir(&self) -> StorageResult<()> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path).await?;
        }
        Ok(())
    }
}

/// Normalize a path by removing leading slashes and double slashes.
fn normalize_path(path: &str) -> &str {
    path.trim_start_matches('/')
}

/// Convert system time to chrono DateTime.
fn system_time_to_chrono(time: std::time::SystemTime) -> Option<DateTime<Utc>> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos()).unwrap_or_default())
}

#[async_trait]
impl Storage for LocalStorage {
    async fn exists(&self, path: &str) -> StorageResult<bool> {
        let full_path = self.full_path(path);
        Ok(full_path.exists())
    }

    async fn metadata(&self, path: &str) -> StorageResult<StorageMetadata> {
        let full_path = self.full_path(path);
        let meta = fs::metadata(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(path.to_string())
            } else {
                StorageError::Io(e)
            }
        })?;

        let mut storage_meta = if meta.is_dir() {
            StorageMetadata::directory(path)
        } else {
            StorageMetadata::file(path, meta.len())
        };

        if let Ok(modified) = meta.modified() {
            if let Some(dt) = system_time_to_chrono(modified) {
                storage_meta = storage_meta.with_modified(dt);
            }
        }

        // Guess content type from extension
        if !meta.is_dir() {
            if let Some(ext) = full_path.extension() {
                let content_type = match ext.to_str() {
                    Some("txt") => "text/plain",
                    Some("html") | Some("htm") => "text/html",
                    Some("css") => "text/css",
                    Some("js") => "application/javascript",
                    Some("json") => "application/json",
                    Some("xml") => "application/xml",
                    Some("png") => "image/png",
                    Some("jpg") | Some("jpeg") => "image/jpeg",
                    Some("gif") => "image/gif",
                    Some("svg") => "image/svg+xml",
                    Some("pdf") => "application/pdf",
                    Some("zip") => "application/zip",
                    Some("md") => "text/markdown",
                    Some("rs") => "text/x-rust",
                    Some("py") => "text/x-python",
                    Some("ts") | Some("tsx") => "text/typescript",
                    _ => "application/octet-stream",
                };
                storage_meta = storage_meta.with_content_type(content_type);
            }
        }

        Ok(storage_meta)
    }

    async fn read(&self, path: &str) -> StorageResult<Vec<u8>> {
        let full_path = self.full_path(path);
        fs::read(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(path.to_string())
            } else {
                StorageError::Io(e)
            }
        })
    }

    async fn write(&self, path: &str, data: &[u8]) -> StorageResult<()> {
        self.ensure_base_dir().await?;
        let full_path = self.full_path(path);

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(&full_path, data).await?;
        debug!("Wrote {} bytes to {}", data.len(), full_path.display());
        Ok(())
    }

    async fn delete(&self, path: &str) -> StorageResult<()> {
        let full_path = self.full_path(path);

        if !full_path.exists() {
            return Err(StorageError::NotFound(path.to_string()));
        }

        if full_path.is_dir() {
            fs::remove_dir_all(&full_path).await?;
        } else {
            fs::remove_file(&full_path).await?;
        }

        debug!("Deleted {}", full_path.display());
        Ok(())
    }

    async fn list(&self, prefix: &str) -> StorageResult<Vec<StorageMetadata>> {
        let full_path = self.full_path(prefix);

        if !full_path.exists() {
            return Ok(vec![]);
        }

        if !full_path.is_dir() {
            return Err(StorageError::InvalidPath(format!(
                "{} is not a directory",
                prefix
            )));
        }

        let mut entries = vec![];
        let mut read_dir = fs::read_dir(&full_path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let entry_path = entry.path();
            let relative_path = entry_path
                .strip_prefix(&self.base_path)
                .unwrap_or(&entry_path)
                .to_string_lossy()
                .to_string();

            let meta = entry.metadata().await?;
            let mut storage_meta = if meta.is_dir() {
                StorageMetadata::directory(&relative_path)
            } else {
                StorageMetadata::file(&relative_path, meta.len())
            };

            if let Ok(modified) = meta.modified() {
                if let Some(dt) = system_time_to_chrono(modified) {
                    storage_meta = storage_meta.with_modified(dt);
                }
            }

            entries.push(storage_meta);
        }

        Ok(entries)
    }

    async fn create_dir(&self, path: &str) -> StorageResult<()> {
        self.ensure_base_dir().await?;
        let full_path = self.full_path(path);
        fs::create_dir_all(&full_path).await?;
        debug!("Created directory {}", full_path.display());
        Ok(())
    }

    async fn copy(&self, src: &str, dst: &str) -> StorageResult<()> {
        let src_path = self.full_path(src);
        let dst_path = self.full_path(dst);

        if !src_path.exists() {
            return Err(StorageError::NotFound(src.to_string()));
        }

        // Ensure parent directory exists
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path).await?;
        } else {
            fs::copy(&src_path, &dst_path).await?;
        }

        debug!("Copied {} to {}", src_path.display(), dst_path.display());
        Ok(())
    }

    async fn rename(&self, src: &str, dst: &str) -> StorageResult<()> {
        let src_path = self.full_path(src);
        let dst_path = self.full_path(dst);

        if !src_path.exists() {
            return Err(StorageError::NotFound(src.to_string()));
        }

        // Ensure parent directory exists
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&src_path, &dst_path).await?;
        debug!("Renamed {} to {}", src_path.display(), dst_path.display());
        Ok(())
    }

    async fn sync_to_storage(&self, local_path: &Path, storage_path: &str) -> StorageResult<()> {
        if !local_path.exists() {
            return Err(StorageError::NotFound(local_path.display().to_string()));
        }

        let dst_path = self.full_path(storage_path);

        // Ensure parent directory exists
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        if local_path.is_dir() {
            copy_dir_recursive(local_path, &dst_path).await?;
        } else {
            fs::copy(local_path, &dst_path).await?;
        }

        debug!(
            "Synced {} to storage {}",
            local_path.display(),
            storage_path
        );
        Ok(())
    }

    async fn sync_from_storage(&self, storage_path: &str, local_path: &Path) -> StorageResult<()> {
        let src_path = self.full_path(storage_path);

        if !src_path.exists() {
            return Err(StorageError::NotFound(storage_path.to_string()));
        }

        // Ensure parent directory exists
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, local_path).await?;
        } else {
            fs::copy(&src_path, local_path).await?;
        }

        debug!(
            "Synced storage {} to {}",
            storage_path,
            local_path.display()
        );
        Ok(())
    }
}

/// Recursively copy a directory.
async fn copy_dir_recursive(src: &Path, dst: &Path) -> StorageResult<()> {
    fs::create_dir_all(dst).await?;

    let mut entries = fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dst_path)).await?;
        } else {
            fs::copy(&src_path, &dst_path).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_storage() -> (LocalStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path());
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let (storage, _dir) = create_test_storage().await;

        storage.write("test.txt", b"hello world").await.unwrap();
        let content = storage.read("test.txt").await.unwrap();

        assert_eq!(content, b"hello world");
    }

    #[tokio::test]
    async fn test_read_string() {
        let (storage, _dir) = create_test_storage().await;

        storage
            .write_string("test.txt", "hello world")
            .await
            .unwrap();
        let content = storage.read_string("test.txt").await.unwrap();

        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_exists() {
        let (storage, _dir) = create_test_storage().await;

        assert!(!storage.exists("test.txt").await.unwrap());
        storage.write("test.txt", b"test").await.unwrap();
        assert!(storage.exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let (storage, _dir) = create_test_storage().await;

        storage.write("test.txt", b"test").await.unwrap();
        assert!(storage.exists("test.txt").await.unwrap());

        storage.delete("test.txt").await.unwrap();
        assert!(!storage.exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_metadata() {
        let (storage, _dir) = create_test_storage().await;

        storage.write("test.txt", b"hello").await.unwrap();
        let meta = storage.metadata("test.txt").await.unwrap();

        assert_eq!(meta.path, "test.txt");
        assert_eq!(meta.size, 5);
        assert!(!meta.is_dir);
        assert_eq!(meta.content_type, Some("text/plain".to_string()));
    }

    #[tokio::test]
    async fn test_create_dir_and_list() {
        let (storage, _dir) = create_test_storage().await;

        storage.create_dir("subdir").await.unwrap();
        storage.write("subdir/file1.txt", b"one").await.unwrap();
        storage.write("subdir/file2.txt", b"two").await.unwrap();

        let entries = storage.list("subdir").await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_copy() {
        let (storage, _dir) = create_test_storage().await;

        storage.write("source.txt", b"original").await.unwrap();
        storage.copy("source.txt", "copy.txt").await.unwrap();

        let content = storage.read("copy.txt").await.unwrap();
        assert_eq!(content, b"original");
    }

    #[tokio::test]
    async fn test_rename() {
        let (storage, _dir) = create_test_storage().await;

        storage.write("old.txt", b"content").await.unwrap();
        storage.rename("old.txt", "new.txt").await.unwrap();

        assert!(!storage.exists("old.txt").await.unwrap());
        assert!(storage.exists("new.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_nested_directories() {
        let (storage, _dir) = create_test_storage().await;

        storage
            .write("a/b/c/deep.txt", b"deep content")
            .await
            .unwrap();

        assert!(storage.exists("a/b/c/deep.txt").await.unwrap());
        let content = storage.read("a/b/c/deep.txt").await.unwrap();
        assert_eq!(content, b"deep content");
    }
}

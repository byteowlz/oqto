//! Storage trait definitions.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::StorageResult;

/// Metadata about a stored object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetadata {
    /// Object path/key.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// Content type (MIME).
    pub content_type: Option<String>,
    /// Last modified time.
    pub modified: Option<DateTime<Utc>>,
    /// Created time.
    pub created: Option<DateTime<Utc>>,
    /// Whether this is a directory.
    pub is_dir: bool,
}

impl StorageMetadata {
    /// Create metadata for a file.
    pub fn file(path: impl Into<String>, size: u64) -> Self {
        Self {
            path: path.into(),
            size,
            content_type: None,
            modified: None,
            created: None,
            is_dir: false,
        }
    }

    /// Create metadata for a directory.
    pub fn directory(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            size: 0,
            content_type: None,
            modified: None,
            created: None,
            is_dir: true,
        }
    }

    /// Set the modified time.
    pub fn with_modified(mut self, time: DateTime<Utc>) -> Self {
        self.modified = Some(time);
        self
    }

    /// Set the content type.
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }
}

/// Storage trait for file operations.
///
/// Implementations provide access to stored files, whether local or remote.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Check if a path exists.
    async fn exists(&self, path: &str) -> StorageResult<bool>;

    /// Get metadata for a path.
    async fn metadata(&self, path: &str) -> StorageResult<StorageMetadata>;

    /// Read a file's contents.
    async fn read(&self, path: &str) -> StorageResult<Vec<u8>>;

    /// Read a file as string.
    async fn read_string(&self, path: &str) -> StorageResult<String> {
        let bytes = self.read(path).await?;
        String::from_utf8(bytes)
            .map_err(|e| super::StorageError::Backend(format!("invalid UTF-8: {}", e)))
    }

    /// Write data to a file.
    async fn write(&self, path: &str, data: &[u8]) -> StorageResult<()>;

    /// Write a string to a file.
    async fn write_string(&self, path: &str, content: &str) -> StorageResult<()> {
        self.write(path, content.as_bytes()).await
    }

    /// Delete a file or directory.
    async fn delete(&self, path: &str) -> StorageResult<()>;

    /// List files in a directory.
    async fn list(&self, prefix: &str) -> StorageResult<Vec<StorageMetadata>>;

    /// Create a directory.
    async fn create_dir(&self, path: &str) -> StorageResult<()>;

    /// Copy a file.
    async fn copy(&self, src: &str, dst: &str) -> StorageResult<()>;

    /// Move/rename a file.
    async fn rename(&self, src: &str, dst: &str) -> StorageResult<()>;

    /// Sync a local directory to storage.
    async fn sync_to_storage(&self, local_path: &Path, storage_path: &str) -> StorageResult<()>;

    /// Sync storage to a local directory.
    async fn sync_from_storage(&self, storage_path: &str, local_path: &Path) -> StorageResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_metadata_file() {
        let meta = StorageMetadata::file("test.txt", 100);
        assert_eq!(meta.path, "test.txt");
        assert_eq!(meta.size, 100);
        assert!(!meta.is_dir);
    }

    #[test]
    fn test_storage_metadata_directory() {
        let meta = StorageMetadata::directory("test/");
        assert_eq!(meta.path, "test/");
        assert!(meta.is_dir);
    }
}

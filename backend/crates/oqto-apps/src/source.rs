use std::path::Path;

use async_trait::async_trait;
use thiserror::Error;

/// Directory metadata supplied by an authorized work-directory adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirEntry {
    pub name: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified_at: i64,
}

/// File metadata supplied by an authorized work-directory adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppFileStat {
    pub exists: bool,
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified_at: i64,
}

/// Source failures stay adapter-neutral and must not contain credentials.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct AppSourceError {
    pub message: String,
}

impl AppSourceError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Minimal filesystem seam required for App discovery and publication.
///
/// Paths are relative to an authorization-bound work-directory root. The
/// adapter must reject escape before touching its underlying filesystem.
#[async_trait]
pub trait AppFileSource: Send + Sync {
    /// Returns `None` when the directory does not exist.
    async fn list_directory(
        &self,
        relative_path: &Path,
    ) -> Result<Option<Vec<AppDirEntry>>, AppSourceError>;

    /// Returns `None` when the path does not exist.
    async fn stat(&self, relative_path: &Path) -> Result<Option<AppFileStat>, AppSourceError>;

    /// Reads at most `max_bytes`; adapters must fail rather than truncate.
    async fn read_file(
        &self,
        relative_path: &Path,
        max_bytes: u64,
    ) -> Result<Option<Vec<u8>>, AppSourceError>;
}

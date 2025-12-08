//! Storage error types.

use thiserror::Error;

/// Result type for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

/// Errors that can occur during storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// File or directory not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Path already exists.
    #[error("already exists: {0}")]
    AlreadyExists(String),

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid path.
    #[error("invalid path: {0}")]
    InvalidPath(String),

    /// Storage backend error.
    #[error("backend error: {0}")]
    Backend(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = StorageError::NotFound("file.txt".to_string());
        assert_eq!(err.to_string(), "not found: file.txt");
    }
}

//! Container runtime error types.

use thiserror::Error;

/// Result type for container operations.
pub type ContainerResult<T> = Result<T, ContainerError>;

/// Errors that can occur during container operations.
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum ContainerError {
    /// The container command failed.
    #[error("container {command} failed: {message}")]
    CommandFailed { command: String, message: String },

    /// Container was not found.
    #[error("container not found: {0}")]
    ContainerNotFound(String),

    /// Image was not found.
    #[error("image not found: {0}")]
    ImageNotFound(String),

    /// Failed to parse container output.
    #[error("failed to parse container output: {0}")]
    ParseError(String),

    /// No container runtime available.
    #[error("no container runtime available (docker or podman)")]
    NoRuntimeAvailable,

    /// Invalid input provided.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Generic IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

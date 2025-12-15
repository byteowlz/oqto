//! Podman error types.

use thiserror::Error;

/// Result type for Podman operations.
pub type PodmanResult<T> = Result<T, PodmanError>;

/// Errors that can occur during Podman operations.
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum PodmanError {
    /// The podman command failed.
    #[error("podman {command} failed: {message}")]
    CommandFailed { command: String, message: String },

    /// Container was not found.
    #[error("container not found: {0}")]
    ContainerNotFound(String),

    /// Image was not found.
    #[error("image not found: {0}")]
    ImageNotFound(String),

    /// Failed to parse podman output.
    #[error("failed to parse podman output: {0}")]
    ParseError(String),

    /// Generic IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

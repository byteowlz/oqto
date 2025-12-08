//! EAVS client error types.

use thiserror::Error;

/// Result type for EAVS operations.
pub type EavsResult<T> = Result<T, EavsError>;

/// Errors that can occur during EAVS operations.
#[derive(Debug, Error)]
pub enum EavsError {
    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    /// EAVS returned an error response.
    #[error("EAVS error: {message} (code: {code})")]
    ApiError { message: String, code: String },

    /// Key not found.
    #[error("Key not found: {0}")]
    KeyNotFound(String),

    /// Unauthorized (invalid master key).
    #[error("Unauthorized: invalid master key")]
    Unauthorized,

    /// EAVS keys feature is disabled.
    #[error("EAVS keys feature is disabled")]
    KeysDisabled,

    /// Failed to parse response.
    #[error("Failed to parse response: {0}")]
    ParseError(String),

    /// Connection failed.
    #[error("Failed to connect to EAVS at {url}: {message}")]
    ConnectionFailed { url: String, message: String },
}

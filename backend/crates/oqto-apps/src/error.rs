use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable machine-readable rejection reason for an App package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppPackageErrorCode {
    SourceUnavailable,
    TooManyPackages,
    ManifestMissing,
    ManifestTooLarge,
    ManifestInvalidUtf8,
    ManifestInvalidToml,
    UnsupportedSchema,
    InvalidAppId,
    PackageIdMismatch,
    InvalidVersion,
    InvalidTitle,
    UnsupportedPresentation,
    InvalidPresentation,
    CapabilitiesNotSupported,
    UnknownCapability,
    MissingCapabilityTable,
    OrphanCapabilityTable,
    InvalidCapabilityRequest,
    CapabilityResourceRejected,
    OperationsTableMissing,
    OperationsTableInvalid,
    OperationMissing,
    OperationExecutableMissing,
    UnsupportedBinding,
    InvalidRelativePath,
    SymlinkRejected,
    EntryMissing,
    TooManyFiles,
    FileTooLarge,
    BundleTooLarge,
    FileChangedWhileReading,
}

/// A fail-loud App package rejection without leaking an absolute host path.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code:?}: {message}")]
pub struct AppPackageError {
    pub code: AppPackageErrorCode,
    pub message: String,
    pub relative_path: Option<PathBuf>,
}

impl AppPackageError {
    #[must_use]
    pub fn new(code: AppPackageErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            relative_path: None,
        }
    }

    #[must_use]
    pub fn at(mut self, relative_path: impl Into<PathBuf>) -> Self {
        self.relative_path = Some(relative_path.into());
        self
    }

    /// Re-code a lower-level rejection so callers report the policy that
    /// actually failed while keeping the precise diagnostic message.
    #[must_use]
    pub fn with_code(mut self, code: AppPackageErrorCode) -> Self {
        self.code = code;
        self
    }
}

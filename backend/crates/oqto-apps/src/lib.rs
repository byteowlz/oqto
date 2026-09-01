//! Canonical, host-neutral Oqto App package validation.
//!
//! This crate understands workspace source packages but owns no filesystem or
//! authorization mechanism. Callers provide an [`AppFileSource`] already bound
//! to an authorized work directory and publish the returned immutable snapshot
//! into a trusted artifact store.

mod capability;
mod digest;
mod discovery;
mod error;
mod manifest;
mod operations;
mod path;
mod source;
#[cfg(test)]
mod test_source;

pub use capability::{
    AppCapabilityKind, AppCapabilityRequest, AppFileAccess, AppFileResourceRequest,
    AppFilesCapability, AppOperationsCapability, CAPABILITY_FILES, CAPABILITY_KV,
    CAPABILITY_OPERATIONS, CAPABILITY_THEME, DEFAULT_OPERATIONS_TABLE, OPERATIONS_DIR,
    RawCapabilityTable, RawFileResource, RawFilesCapability, RawOperationsCapability,
};
pub use digest::{ContentDigest, digest_bundle};
pub use discovery::{
    AppCandidate, AppCandidateStatus, BundleFile, BundleLimits, BundleSnapshot,
    discover_candidates, snapshot_bundle,
};
pub use error::{AppPackageError, AppPackageErrorCode};
pub use manifest::{
    AppBindingKind, AppManifestV0, AppTitle, PresentationKind, PresentationMode,
    SandboxedWebPresentation, ValidatedManifest, parse_manifest,
};
pub use operations::{
    OPERATIONS_SCHEMA_V0_DRAFT, OperationDefinition, OperationsTable, ResolvedOperation,
    parse_operations_table,
};
pub use source::{AppDirEntry, AppFileSource, AppFileStat, AppSourceError};

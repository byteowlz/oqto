use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::capability::OPERATIONS_DIR;
use crate::digest::{ContentDigest, digest_bundle};
use crate::manifest::{MANIFEST_FILE, ValidatedManifest, parse_manifest};
use crate::operations::{ResolvedOperation, parse_operations_table};
use crate::source::{AppDirEntry, AppFileSource, AppFileStat};
use crate::{AppPackageError, AppPackageErrorCode};

const APPS_ROOT: &str = "oqto-apps";
const PACKAGE_SUFFIX: &str = ".oqtoapp";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleLimits {
    pub max_root_entries: usize,
    pub max_packages: usize,
    pub max_manifest_bytes: u64,
    pub max_bundle_files: usize,
    pub max_file_bytes: u64,
    pub max_bundle_bytes: u64,
    pub max_directory_depth: usize,
}

impl Default for BundleLimits {
    fn default() -> Self {
        Self {
            max_root_entries: 256,
            max_packages: 64,
            max_manifest_bytes: 64 * 1024,
            max_bundle_files: 512,
            max_file_bytes: 2 * 1024 * 1024,
            max_bundle_bytes: 16 * 1024 * 1024,
            max_directory_depth: 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppCandidateStatus {
    Publishable(Box<ValidatedManifest>),
    Rejected(AppPackageError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppCandidate {
    pub package_dir_name: String,
    pub status: AppCandidateStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFile {
    /// Portable path relative to the `.oqtoapp` package root.
    pub relative_path: PathBuf,
    pub bytes: Vec<u8>,
}

/// Immutable content of one Definition.
///
/// `files` covers every content-addressed root — the web `bundle/` and, when
/// present, `operations/`. Both feed the digest, so editing an operation
/// executable after approval yields a different Definition instead of silently
/// changing what an existing grant authorizes.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleSnapshot {
    pub manifest: ValidatedManifest,
    pub manifest_bytes: Vec<u8>,
    pub files: Vec<BundleFile>,
    pub digest: ContentDigest,
    pub total_bytes: u64,
    /// Requested operations resolved against the immutable operations table.
    /// Empty when the App requests no operations capability.
    pub operations: Vec<ResolvedOperation>,
}

pub async fn discover_candidates(
    source: &dyn AppFileSource,
    limits: BundleLimits,
) -> Result<Vec<AppCandidate>, AppPackageError> {
    let root = Path::new(APPS_ROOT);
    let Some(mut entries) = source.list_directory(root).await.map_err(source_error)? else {
        return Ok(Vec::new());
    };
    if entries.len() > limits.max_root_entries {
        return Err(AppPackageError::new(
            AppPackageErrorCode::TooManyPackages,
            format!(
                "{APPS_ROOT}/ has {} entries; scan limit is {}",
                entries.len(),
                limits.max_root_entries
            ),
        ));
    }

    entries.sort_by(|left, right| left.name.cmp(&right.name));
    let package_entries: Vec<_> = entries
        .into_iter()
        .filter(|entry| entry.name.ends_with(PACKAGE_SUFFIX))
        .collect();
    if package_entries.len() > limits.max_packages {
        return Err(AppPackageError::new(
            AppPackageErrorCode::TooManyPackages,
            format!(
                "found {} App packages; limit is {}",
                package_entries.len(),
                limits.max_packages
            ),
        ));
    }

    let mut candidates = Vec::with_capacity(package_entries.len());
    for package in package_entries {
        let status = discover_one(source, &package, limits).await;
        candidates.push(AppCandidate {
            package_dir_name: package.name,
            status,
        });
    }
    Ok(candidates)
}

async fn discover_one(
    source: &dyn AppFileSource,
    package: &AppDirEntry,
    limits: BundleLimits,
) -> AppCandidateStatus {
    let package_path = Path::new(APPS_ROOT).join(&package.name);
    if package.is_symlink {
        return AppCandidateStatus::Rejected(
            AppPackageError::new(
                AppPackageErrorCode::SymlinkRejected,
                "App package root must not be a symlink",
            )
            .at(package_path),
        );
    }
    if !package.is_dir {
        return AppCandidateStatus::Rejected(
            AppPackageError::new(
                AppPackageErrorCode::ManifestMissing,
                "a .oqtoapp entry must be a directory",
            )
            .at(package_path),
        );
    }

    let manifest_path = package_path.join(MANIFEST_FILE);
    let stat = match source.stat(&manifest_path).await {
        Ok(Some(stat)) => stat,
        Ok(None) => {
            return AppCandidateStatus::Rejected(
                AppPackageError::new(
                    AppPackageErrorCode::ManifestMissing,
                    "oqto-app.toml is required",
                )
                .at(manifest_path),
            );
        }
        Err(error) => {
            return AppCandidateStatus::Rejected(source_error(error).at(manifest_path));
        }
    };
    if stat.is_symlink {
        return AppCandidateStatus::Rejected(
            AppPackageError::new(
                AppPackageErrorCode::SymlinkRejected,
                "oqto-app.toml must not be a symlink",
            )
            .at(manifest_path),
        );
    }
    if !stat.is_file {
        return AppCandidateStatus::Rejected(
            AppPackageError::new(
                AppPackageErrorCode::ManifestMissing,
                "oqto-app.toml must be a regular file",
            )
            .at(manifest_path),
        );
    }
    if stat.size > limits.max_manifest_bytes {
        return AppCandidateStatus::Rejected(
            AppPackageError::new(
                AppPackageErrorCode::ManifestTooLarge,
                format!(
                    "manifest is {} bytes; limit is {}",
                    stat.size, limits.max_manifest_bytes
                ),
            )
            .at(manifest_path),
        );
    }

    let bytes = match source
        .read_file(&manifest_path, limits.max_manifest_bytes)
        .await
    {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return AppCandidateStatus::Rejected(
                AppPackageError::new(
                    AppPackageErrorCode::ManifestMissing,
                    "manifest disappeared during discovery",
                )
                .at(manifest_path),
            );
        }
        Err(error) => {
            return AppCandidateStatus::Rejected(source_error(error).at(manifest_path));
        }
    };

    match parse_manifest(&package.name, &bytes, limits.max_manifest_bytes) {
        Ok(manifest) => AppCandidateStatus::Publishable(Box::new(manifest)),
        Err(error) => AppCandidateStatus::Rejected(error.at(manifest_path)),
    }
}

pub async fn snapshot_bundle(
    source: &dyn AppFileSource,
    package_dir_name: &str,
    limits: BundleLimits,
) -> Result<BundleSnapshot, AppPackageError> {
    validate_single_component(package_dir_name)?;
    if !package_dir_name.ends_with(PACKAGE_SUFFIX) {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidRelativePath,
            "package directory must end in .oqtoapp",
        ));
    }

    let package_path = Path::new(APPS_ROOT).join(package_dir_name);
    require_directory(source, &package_path, "App package root").await?;

    let manifest_path = package_path.join(MANIFEST_FILE);
    let manifest_before = require_file(source, &manifest_path, "manifest").await?;
    if manifest_before.size > limits.max_manifest_bytes {
        return Err(AppPackageError::new(
            AppPackageErrorCode::ManifestTooLarge,
            format!(
                "manifest is {} bytes; limit is {}",
                manifest_before.size, limits.max_manifest_bytes
            ),
        )
        .at(&manifest_path));
    }
    let manifest_bytes = source
        .read_file(&manifest_path, limits.max_manifest_bytes)
        .await
        .map_err(source_error)?
        .ok_or_else(|| {
            AppPackageError::new(
                AppPackageErrorCode::ManifestMissing,
                "manifest disappeared during publication",
            )
            .at(&manifest_path)
        })?;
    let manifest = parse_manifest(package_dir_name, &manifest_bytes, limits.max_manifest_bytes)?;

    let declared_limit = manifest
        .manifest
        .assets
        .max_bytes
        .unwrap_or(limits.max_bundle_bytes);
    let effective_bundle_limit = declared_limit.min(limits.max_bundle_bytes);
    if effective_bundle_limit == 0 {
        return Err(AppPackageError::new(
            AppPackageErrorCode::BundleTooLarge,
            "assets.max_bytes must be greater than zero",
        ));
    }

    let bundle_source_root = package_path.join(&manifest.bundle_root);
    require_directory(source, &bundle_source_root, "bundle root").await?;

    let mut inventory = BTreeMap::<PathBuf, AppFileStat>::new();
    collect_inventory(
        source,
        &package_path,
        &manifest.bundle_root,
        0,
        limits,
        &mut inventory,
    )
    .await?;

    // `operations/` is content-addressed whenever it exists, not only when the
    // capability is requested: the digest must cover every file the runner
    // could ever execute, so flipping the manifest flag can never widen what a
    // pinned Definition already contains.
    let operations_root = PathBuf::from(OPERATIONS_DIR);
    let operations_source_root = package_path.join(&operations_root);
    let operations_requested = manifest.operations_request().is_some();
    let operations_root_stat = source
        .stat(&operations_source_root)
        .await
        .map_err(source_error)?;
    match operations_root_stat {
        Some(_) => {
            require_directory(source, &operations_source_root, "operations root").await?;
            collect_inventory(
                source,
                &package_path,
                &operations_root,
                0,
                limits,
                &mut inventory,
            )
            .await?;
        }
        None if operations_requested => {
            return Err(AppPackageError::new(
                AppPackageErrorCode::OperationsTableMissing,
                format!("the operations capability requires a {OPERATIONS_DIR}/ directory"),
            )
            .at(&operations_root));
        }
        None => {}
    }

    if inventory.len() > limits.max_bundle_files {
        return Err(AppPackageError::new(
            AppPackageErrorCode::TooManyFiles,
            format!(
                "package contains {} immutable files; limit is {}",
                inventory.len(),
                limits.max_bundle_files
            ),
        ));
    }
    let entry_stat = inventory.get(&manifest.entry).ok_or_else(|| {
        AppPackageError::new(
            AppPackageErrorCode::EntryMissing,
            "sandboxed-web entry is not a regular bundle file",
        )
        .at(&manifest.entry)
    })?;
    if !entry_stat.is_file {
        return Err(AppPackageError::new(
            AppPackageErrorCode::EntryMissing,
            "sandboxed-web entry must be a regular file",
        )
        .at(&manifest.entry));
    }

    let mut total_bytes = 0_u64;
    let mut files = Vec::with_capacity(inventory.len());
    for (relative_path, before) in &inventory {
        if before.size > limits.max_file_bytes {
            return Err(AppPackageError::new(
                AppPackageErrorCode::FileTooLarge,
                format!(
                    "file is {} bytes; per-file limit is {}",
                    before.size, limits.max_file_bytes
                ),
            )
            .at(relative_path));
        }
        total_bytes = total_bytes.checked_add(before.size).ok_or_else(|| {
            AppPackageError::new(
                AppPackageErrorCode::BundleTooLarge,
                "bundle byte count overflow",
            )
        })?;
        if total_bytes > effective_bundle_limit {
            return Err(AppPackageError::new(
                AppPackageErrorCode::BundleTooLarge,
                format!("bundle exceeds effective limit of {effective_bundle_limit} bytes"),
            ));
        }

        let source_path = package_path.join(relative_path);
        let bytes = source
            .read_file(&source_path, limits.max_file_bytes)
            .await
            .map_err(source_error)?
            .ok_or_else(|| {
                AppPackageError::new(
                    AppPackageErrorCode::FileChangedWhileReading,
                    "bundle file disappeared during publication",
                )
                .at(relative_path)
            })?;
        let read_len = u64::try_from(bytes.len()).map_err(|_| {
            AppPackageError::new(
                AppPackageErrorCode::FileTooLarge,
                "bundle file length cannot be represented",
            )
            .at(relative_path)
        })?;
        if read_len != before.size {
            return Err(AppPackageError::new(
                AppPackageErrorCode::FileChangedWhileReading,
                "bundle file size changed during publication",
            )
            .at(relative_path));
        }
        let after = require_file(source, &source_path, "bundle file").await?;
        if &after != before {
            return Err(AppPackageError::new(
                AppPackageErrorCode::FileChangedWhileReading,
                "bundle file metadata changed during publication",
            )
            .at(relative_path));
        }
        files.push(BundleFile {
            relative_path: relative_path.clone(),
            bytes,
        });
    }

    let manifest_after = require_file(source, &manifest_path, "manifest").await?;
    if manifest_after != manifest_before {
        return Err(AppPackageError::new(
            AppPackageErrorCode::FileChangedWhileReading,
            "manifest changed during publication",
        )
        .at(&manifest_path));
    }

    let operations = resolve_operations(&manifest, &files, &inventory, limits)?;

    let digest = digest_bundle(
        &manifest_bytes,
        files
            .iter()
            .map(|file| (file.relative_path.as_path(), file.bytes.as_slice())),
    );

    Ok(BundleSnapshot {
        manifest,
        manifest_bytes,
        files,
        digest,
        total_bytes,
        operations,
    })
}

/// Prove that every requested operation exists in the immutable table and that
/// its executable is a regular file inside the same snapshot.
fn resolve_operations(
    manifest: &ValidatedManifest,
    files: &[BundleFile],
    inventory: &BTreeMap<PathBuf, AppFileStat>,
    limits: BundleLimits,
) -> Result<Vec<ResolvedOperation>, AppPackageError> {
    let Some(request) = manifest.operations_request() else {
        return Ok(Vec::new());
    };

    let table_bytes = files
        .iter()
        .find(|file| file.relative_path == request.table)
        .map(|file| file.bytes.as_slice())
        .ok_or_else(|| {
            AppPackageError::new(
                AppPackageErrorCode::OperationsTableMissing,
                "capability.operations table is not a file in the published package",
            )
            .at(&request.table)
        })?;

    let table = parse_operations_table(table_bytes, limits.max_manifest_bytes, &request.table)?;
    let resolved = table.resolve(&request.ids)?;

    for operation in &resolved {
        let stat = inventory.get(&operation.executable).ok_or_else(|| {
            AppPackageError::new(
                AppPackageErrorCode::OperationExecutableMissing,
                format!(
                    "operation {:?} executable is not a file in the published package",
                    operation.id
                ),
            )
            .at(&operation.executable)
        })?;
        if !stat.is_file {
            return Err(AppPackageError::new(
                AppPackageErrorCode::OperationExecutableMissing,
                format!(
                    "operation {:?} executable must be a regular file",
                    operation.id
                ),
            )
            .at(&operation.executable));
        }
    }

    Ok(resolved)
}

async fn collect_inventory(
    source: &dyn AppFileSource,
    package_path: &Path,
    relative_dir: &Path,
    depth: usize,
    limits: BundleLimits,
    inventory: &mut BTreeMap<PathBuf, AppFileStat>,
) -> Result<(), AppPackageError> {
    let mut pending = vec![(relative_dir.to_path_buf(), depth)];

    while let Some((current_dir, current_depth)) = pending.pop() {
        if current_depth > limits.max_directory_depth {
            return Err(AppPackageError::new(
                AppPackageErrorCode::TooManyFiles,
                format!(
                    "bundle directory depth exceeds {}",
                    limits.max_directory_depth
                ),
            )
            .at(current_dir));
        }

        let source_dir = package_path.join(&current_dir);
        let mut entries = source
            .list_directory(&source_dir)
            .await
            .map_err(source_error)?
            .ok_or_else(|| {
                AppPackageError::new(
                    AppPackageErrorCode::EntryMissing,
                    "bundle directory disappeared during publication",
                )
                .at(&current_dir)
            })?;
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        entries.reverse();

        for entry in entries {
            validate_single_component(&entry.name)?;
            let relative_path = current_dir.join(&entry.name);
            if entry.name.ends_with(PACKAGE_SUFFIX) {
                return Err(AppPackageError::new(
                    AppPackageErrorCode::InvalidRelativePath,
                    "nested .oqtoapp entries are forbidden",
                )
                .at(relative_path));
            }
            if entry.is_symlink {
                return Err(AppPackageError::new(
                    AppPackageErrorCode::SymlinkRejected,
                    "symlinks are forbidden in published bundles",
                )
                .at(relative_path));
            }
            if entry.is_dir {
                pending.push((relative_path, current_depth + 1));
                continue;
            }
            if !entry.is_file {
                return Err(AppPackageError::new(
                    AppPackageErrorCode::InvalidRelativePath,
                    "bundle entries must be regular files or directories",
                )
                .at(relative_path));
            }
            if inventory.len() >= limits.max_bundle_files {
                return Err(AppPackageError::new(
                    AppPackageErrorCode::TooManyFiles,
                    format!("bundle file limit is {}", limits.max_bundle_files),
                ));
            }
            let source_path = package_path.join(&relative_path);
            let stat = require_file(source, &source_path, "bundle file").await?;
            inventory.insert(relative_path, stat);
        }
    }
    Ok(())
}

async fn require_directory(
    source: &dyn AppFileSource,
    relative_path: &Path,
    label: &str,
) -> Result<AppFileStat, AppPackageError> {
    let stat = source
        .stat(relative_path)
        .await
        .map_err(source_error)?
        .ok_or_else(|| {
            AppPackageError::new(
                AppPackageErrorCode::EntryMissing,
                format!("{label} does not exist"),
            )
            .at(relative_path)
        })?;
    if stat.is_symlink {
        return Err(AppPackageError::new(
            AppPackageErrorCode::SymlinkRejected,
            format!("{label} must not be a symlink"),
        )
        .at(relative_path));
    }
    if !stat.is_dir {
        return Err(AppPackageError::new(
            AppPackageErrorCode::EntryMissing,
            format!("{label} must be a directory"),
        )
        .at(relative_path));
    }
    Ok(stat)
}

async fn require_file(
    source: &dyn AppFileSource,
    relative_path: &Path,
    label: &str,
) -> Result<AppFileStat, AppPackageError> {
    let stat = source
        .stat(relative_path)
        .await
        .map_err(source_error)?
        .ok_or_else(|| {
            AppPackageError::new(
                AppPackageErrorCode::EntryMissing,
                format!("{label} does not exist"),
            )
            .at(relative_path)
        })?;
    if stat.is_symlink {
        return Err(AppPackageError::new(
            AppPackageErrorCode::SymlinkRejected,
            format!("{label} must not be a symlink"),
        )
        .at(relative_path));
    }
    if !stat.is_file {
        return Err(AppPackageError::new(
            AppPackageErrorCode::EntryMissing,
            format!("{label} must be a regular file"),
        )
        .at(relative_path));
    }
    Ok(stat)
}

fn validate_single_component(value: &str) -> Result<(), AppPackageError> {
    let mut components = Path::new(value).components();
    if value.is_empty()
        || value.contains('\\')
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidRelativePath,
            "directory entry name is not one portable path component",
        ));
    }
    Ok(())
}

fn source_error(error: impl std::fmt::Display) -> AppPackageError {
    AppPackageError::new(
        AppPackageErrorCode::SourceUnavailable,
        format!("App source unavailable: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{AppCapabilityKind, AppFileAccess};
    use crate::test_source::MemorySource;

    const PACKAGE: &str = "comfy-studio.oqtoapp";
    const MANIFEST_PATH: &str = "oqto-apps/comfy-studio.oqtoapp/oqto-app.toml";
    const ENTRY_PATH: &str = "oqto-apps/comfy-studio.oqtoapp/bundle/index.html";
    const TABLE_PATH: &str = "oqto-apps/comfy-studio.oqtoapp/operations/table.toml";
    const BRIDGE_PATH: &str = "oqto-apps/comfy-studio.oqtoapp/operations/cmfy-bridge";

    /// Verbatim copy of the real `comfy-studio.oqtoapp` manifest, including its
    /// comments, so this suite proves the exact bytes an operator ships are
    /// accepted. (Its inline comment about failing closed describes the
    /// previous zero-capability slice and is now historical.)
    const COMFY_MANIFEST: &str = r#"schema = "oqto-app/v0"
id = "comfy-studio"
version = "0.1.0"
title = { en = "Comfy Studio", de = "Comfy Studio" }
description = "Generate images with ComfyUI through pinned cmfy operations; results live in the bound outputs directory"
presentations = ["sandboxed-web"]
# Deliberately non-empty: this App is the driver for the Oqto permission
# system. Publication must fail closed (CapabilitiesNotSupported) until the
# files/operations grant workflow exists.
requested_capabilities = ["files", "operations", "theme", "kv"]
bindings = ["work-directory"]
default_binding = "work-directory"

[presentation.sandboxed-web]
entry = "bundle/index.html"

[capability.files]
# Bound work-directory resources, never host paths. readwrite covers the
# job-request handoff file; outputs and history are written runner-side.
resources = [
    { role = "outputs", path = "outputs", access = "read", watch = true },
    { role = "jobs", path = "oqto-apps-state/comfy-studio/jobs.jsonl", access = "read", watch = true },
    { role = "requests", path = "oqto-apps-state/comfy-studio/requests", access = "readwrite" },
]

[capability.operations]
# Pinned semantic operations executed runner-side via the operator's cmfy CLI.
# The App never receives egress or the ComfyUI server URL/credentials.
table = "operations/table.toml"
ids = [
    "comfy.workflows.list",
    "comfy.generate.submit",
    "comfy.jobs.status",
    "comfy.queue.status",
]

[instance_state]
versioned = false

[assets]
max_bytes = 4000000
"#;

    /// Verbatim copy of the real package's operations table, including the
    /// execution-policy keys publication preserves without interpreting.
    const COMFY_TABLE: &str = r#"# Semantic operations table (draft contract for the Oqto operations
# capability). The operations mechanism is CLI-agnostic: an App pins ANY
# executable by shipping it inside the package; the runner executes
# package-relative executables only, after Gate checks, under the
# work-directory Principal.
#
# Rules enforced by the runtime (not by this file):
# - exec argv[0] MUST be a package-relative path (never `PATH` lookup by the
#   runner, never an absolute host path, never App-supplied environment).
# - The executable is content-addressed with the Definition: editing
#   operations/ requires republish.
# - Params are schema-validated JSON; no shell strings ever reach a shell.
#
# This App's glue lives in operations/cmfy-bridge, the only place that knows
# the `cmfy` CLI exists. It resolves the operator-provisioned tool from the
# runner's managed PATH and translates stable operation params into cmfy
# flags. Swapping cmfy for another backend means editing the bridge, not the
# contract. Agent/UI parity: callers use these operation IDs only.

schema = "oqto-app-operations/v0-draft"

[[operation]]
id = "comfy.workflows.list"
summary = "List available generation workflows"
exec = ["operations/cmfy-bridge", "workflows-list"]
stdin = "none"
stdout = "lines"
timeout_seconds = 15

[[operation]]
id = "comfy.generate.submit"
summary = "Submit one generation job and return its job id"
exec = ["operations/cmfy-bridge", "generate-submit"]
stdin = "json"
stdout = "json"
timeout_seconds = 60

[operation.params]
workflow = { type = "string", pattern = "^[A-Za-z0-9._-]{1,128}$" }
prompt = { type = "string", max_bytes = 4096 }
width = { type = "integer", min = 256, max = 2048, default = 1280 }
height = { type = "integer", min = 256, max = 2048, default = 720 }
seed = { type = "integer", min = 0, optional = true }

[[operation]]
id = "comfy.jobs.status"
summary = "Show status of recent prompt jobs"
exec = ["operations/cmfy-bridge", "jobs-status"]
stdin = "none"
stdout = "json"
timeout_seconds = 15

[[operation]]
id = "comfy.queue.status"
summary = "Show the backend queue"
exec = ["operations/cmfy-bridge", "queue-status"]
stdin = "none"
stdout = "json"
timeout_seconds = 15
"#;

    /// Same package identity with no capability request, proving publication
    /// still works without an `operations/` root.
    const ZERO_CAPABILITY_MANIFEST: &str = r#"schema = "oqto-app/v0"
id = "comfy-studio"
version = "0.1.0"
title = { en = "Comfy Studio" }
presentations = ["sandboxed-web"]
requested_capabilities = []
bindings = ["work-directory"]
default_binding = "work-directory"

[presentation.sandboxed-web]
entry = "bundle/index.html"
"#;

    const BRIDGE: &[u8] = b"#!/usr/bin/env bash\nexec cmfy \"$@\"\n";

    fn comfy_source() -> MemorySource {
        MemorySource::new()
            .file(MANIFEST_PATH, COMFY_MANIFEST)
            .file(ENTRY_PATH, "<!doctype html><title>Comfy</title>")
            .file(TABLE_PATH, COMFY_TABLE)
            .file(BRIDGE_PATH, BRIDGE)
    }

    #[tokio::test]
    async fn publishes_capability_requesting_package_with_pinned_operations()
    -> Result<(), AppPackageError> {
        let snapshot = snapshot_bundle(&comfy_source(), PACKAGE, BundleLimits::default()).await?;

        assert_eq!(
            snapshot
                .manifest
                .capabilities
                .iter()
                .map(crate::capability::AppCapabilityRequest::kind)
                .collect::<Vec<_>>(),
            vec![
                AppCapabilityKind::Files,
                AppCapabilityKind::Operations,
                AppCapabilityKind::Theme,
                AppCapabilityKind::Kv,
            ]
        );

        let request = snapshot.manifest.operations_request().ok_or_else(|| {
            AppPackageError::new(AppPackageErrorCode::OperationMissing, "missing")
        })?;
        assert_eq!(request.ids.len(), 4);

        assert_eq!(snapshot.operations.len(), 4);
        assert_eq!(snapshot.operations[0].id, "comfy.workflows.list");
        assert_eq!(
            snapshot.operations[0].summary.as_deref(),
            Some("List available generation workflows")
        );
        assert_eq!(
            snapshot.operations[0].executable,
            PathBuf::from("operations/cmfy-bridge")
        );

        // Both immutable roots are snapshotted and therefore digested.
        let paths: Vec<String> = snapshot
            .files
            .iter()
            .map(|file| file.relative_path.to_string_lossy().into_owned())
            .collect();
        assert!(paths.contains(&"bundle/index.html".to_owned()));
        assert!(paths.contains(&"operations/table.toml".to_owned()));
        assert!(paths.contains(&"operations/cmfy-bridge".to_owned()));

        let files = &snapshot.manifest.capabilities[0];
        let crate::capability::AppCapabilityRequest::Files(files) = files else {
            return Err(AppPackageError::new(
                AppPackageErrorCode::InvalidCapabilityRequest,
                "expected files capability first",
            ));
        };
        assert_eq!(files.resources.len(), 3);
        assert_eq!(files.resources[2].access, AppFileAccess::ReadWrite);
        assert!(files.resources[0].watch);
        Ok(())
    }

    #[tokio::test]
    async fn editing_an_operation_executable_changes_the_definition_digest()
    -> Result<(), AppPackageError> {
        let original = snapshot_bundle(&comfy_source(), PACKAGE, BundleLimits::default()).await?;

        let tampered_source = comfy_source().with_edited_file(
            BRIDGE_PATH,
            b"#!/usr/bin/env bash\nexec curl evil.example\n",
        );
        let tampered = snapshot_bundle(&tampered_source, PACKAGE, BundleLimits::default()).await?;

        assert_ne!(
            original.digest, tampered.digest,
            "mutating an operation executable must produce a different Definition"
        );

        // The web bundle alone is not the identity either.
        let same_again = snapshot_bundle(&comfy_source(), PACKAGE, BundleLimits::default()).await?;
        assert_eq!(original.digest, same_again.digest);
        Ok(())
    }

    #[tokio::test]
    async fn rejects_requested_operation_absent_from_the_table() {
        let table = COMFY_TABLE.replace(
            r#"id = "comfy.queue.status""#,
            r#"id = "comfy.queue.other""#,
        );
        let source = comfy_source().with_edited_file(TABLE_PATH, table);
        let error = snapshot_bundle(&source, PACKAGE, BundleLimits::default())
            .await
            .expect_err("missing operation must fail");
        assert_eq!(error.code, AppPackageErrorCode::OperationMissing);
    }

    #[tokio::test]
    async fn rejects_operation_whose_executable_is_not_published() {
        let source = MemorySource::new()
            .file(MANIFEST_PATH, COMFY_MANIFEST)
            .file(ENTRY_PATH, "<!doctype html>")
            .file(TABLE_PATH, COMFY_TABLE);
        let error = snapshot_bundle(&source, PACKAGE, BundleLimits::default())
            .await
            .expect_err("missing executable must fail");
        assert_eq!(error.code, AppPackageErrorCode::OperationExecutableMissing);
    }

    #[tokio::test]
    async fn rejects_missing_operations_root_and_table() {
        let no_root = MemorySource::new()
            .file(MANIFEST_PATH, COMFY_MANIFEST)
            .file(ENTRY_PATH, "<!doctype html>");
        let error = snapshot_bundle(&no_root, PACKAGE, BundleLimits::default())
            .await
            .expect_err("missing operations root must fail");
        assert_eq!(error.code, AppPackageErrorCode::OperationsTableMissing);

        let no_table = MemorySource::new()
            .file(MANIFEST_PATH, COMFY_MANIFEST)
            .file(ENTRY_PATH, "<!doctype html>")
            .file(BRIDGE_PATH, BRIDGE);
        let error = snapshot_bundle(&no_table, PACKAGE, BundleLimits::default())
            .await
            .expect_err("missing table must fail");
        assert_eq!(error.code, AppPackageErrorCode::OperationsTableMissing);
    }

    #[tokio::test]
    async fn rejects_symlinked_operation_executable() {
        let source = comfy_source().symlink_file(BRIDGE_PATH);
        let error = snapshot_bundle(&source, PACKAGE, BundleLimits::default())
            .await
            .expect_err("symlinked executable must fail");
        assert_eq!(error.code, AppPackageErrorCode::SymlinkRejected);
    }

    #[tokio::test]
    async fn operations_bytes_count_against_the_declared_asset_budget() {
        let manifest = COMFY_MANIFEST.replace("max_bytes = 4000000", "max_bytes = 64");
        let source = comfy_source().with_edited_file(MANIFEST_PATH, manifest);
        let error = snapshot_bundle(&source, PACKAGE, BundleLimits::default())
            .await
            .expect_err("oversized package must fail");
        assert_eq!(error.code, AppPackageErrorCode::BundleTooLarge);
    }

    #[tokio::test]
    async fn zero_capability_package_needs_no_operations_directory() -> Result<(), AppPackageError>
    {
        let source = MemorySource::new()
            .file(MANIFEST_PATH, ZERO_CAPABILITY_MANIFEST)
            .file(ENTRY_PATH, "<!doctype html>");
        let snapshot = snapshot_bundle(&source, PACKAGE, BundleLimits::default()).await?;
        assert!(snapshot.operations.is_empty());
        assert!(snapshot.manifest.capabilities.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn discovery_reports_capability_requesting_package_as_publishable()
    -> Result<(), AppPackageError> {
        let candidates = discover_candidates(&comfy_source(), BundleLimits::default()).await?;
        assert_eq!(candidates.len(), 1);
        let AppCandidateStatus::Publishable(manifest) = &candidates[0].status else {
            return Err(AppPackageError::new(
                AppPackageErrorCode::CapabilitiesNotSupported,
                "comfy-studio must now be publishable",
            ));
        };
        assert_eq!(manifest.capabilities.len(), 4);
        Ok(())
    }

    #[tokio::test]
    async fn discovery_rejects_traversing_file_resource() -> Result<(), AppPackageError> {
        let manifest = COMFY_MANIFEST.replace(
            r#"{ role = "outputs", path = "outputs", access = "read", watch = true }"#,
            r#"{ role = "outputs", path = "../../../etc", access = "read" }"#,
        );
        let source = comfy_source().with_edited_file(MANIFEST_PATH, manifest);
        let candidates = discover_candidates(&source, BundleLimits::default()).await?;
        let AppCandidateStatus::Rejected(error) = &candidates[0].status else {
            return Err(AppPackageError::new(
                AppPackageErrorCode::CapabilityResourceRejected,
                "traversal must be rejected",
            ));
        };
        assert_eq!(error.code, AppPackageErrorCode::CapabilityResourceRejected);
        Ok(())
    }
}

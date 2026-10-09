//! Typed, fail-closed App capability requests.
//!
//! A manifest names capabilities twice: once as a flat `requested_capabilities`
//! list and once as `[capability.<name>]` configuration. Both must agree
//! exactly, because a grant is written from the *typed* request while a human
//! approves what the list implies. Unknown names, orphan tables, and missing
//! tables are rejected rather than ignored, so no authority can reach a
//! Definition without appearing in the request the user reviews.
//!
//! These types carry no host path, Principal, or grant state. They describe
//! only what a package asks for.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::path::{PathLimits, bounded_relative_path, is_beneath, portable_display};
use crate::{AppPackageError, AppPackageErrorCode};

pub const CAPABILITY_FILES: &str = "files";
pub const CAPABILITY_OPERATIONS: &str = "operations";
pub const CAPABILITY_THEME: &str = "theme";
pub const CAPABILITY_KV: &str = "kv";
pub const CAPABILITY_AGENT_CONTEXT: &str = "agent_context";

/// Package-relative root holding immutable, content-addressed operation files.
pub const OPERATIONS_DIR: &str = "operations";
/// Conventional operations table path; any safe path under `operations/` works.
pub const DEFAULT_OPERATIONS_TABLE: &str = "operations/table.toml";
pub const CONTEXT_DIR: &str = "context";
pub const DEFAULT_CONTEXT_CATALOG: &str = "context/catalog.toml";

const MAX_REQUESTED_CAPABILITIES: usize = 8;
const MAX_FILE_RESOURCES: usize = 32;
const MAX_ROLE_BYTES: usize = 64;
const MAX_OPERATION_IDS: usize = 64;
pub(crate) const MAX_OPERATION_ID_BYTES: usize = 128;
pub(crate) const MAX_OPERATION_ID_SEGMENTS: usize = 8;

const RESOURCE_PATH_LIMITS: PathLimits = PathLimits {
    max_bytes: 256,
    max_components: 16,
};
pub(crate) const PACKAGE_PATH_LIMITS: PathLimits = PathLimits {
    max_bytes: 256,
    max_components: 8,
};

/// Work-directory components an App may never bind through a files grant.
///
/// `.git` and `.oqto` hold repository and Oqto control state; `oqto-apps` holds
/// App source, so binding it would let one App read or rewrite another App's
/// package (including its own operations) after approval.
const RESERVED_RESOURCE_COMPONENTS: [&str; 3] = [".git", ".oqto", "oqto-apps"];

/// Capability vocabulary understood by the v0 runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCapabilityKind {
    Files,
    Operations,
    Theme,
    Kv,
    AgentContext,
}

impl AppCapabilityKind {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            CAPABILITY_FILES => Some(Self::Files),
            CAPABILITY_OPERATIONS => Some(Self::Operations),
            CAPABILITY_THEME => Some(Self::Theme),
            CAPABILITY_KV => Some(Self::Kv),
            CAPABILITY_AGENT_CONTEXT => Some(Self::AgentContext),
            _ => None,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Files => CAPABILITY_FILES,
            Self::Operations => CAPABILITY_OPERATIONS,
            Self::Theme => CAPABILITY_THEME,
            Self::Kv => CAPABILITY_KV,
            Self::AgentContext => CAPABILITY_AGENT_CONTEXT,
        }
    }

    /// True when the capability carries authority that must be spelled out in
    /// its own `[capability.<name>]` table.
    #[must_use]
    pub fn requires_table(self) -> bool {
        matches!(self, Self::Files | Self::Operations | Self::AgentContext)
    }
}

/// Access level requested for one bound work-directory resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppFileAccess {
    Read,
    ReadWrite,
}

impl AppFileAccess {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::ReadWrite => "readwrite",
        }
    }
}

/// One work-directory resource an App asks to bind.
///
/// `path` is relative to the bound work directory, never a host path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppFileResourceRequest {
    pub role: String,
    pub path: PathBuf,
    pub access: AppFileAccess,
    pub watch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppFilesCapability {
    pub resources: Vec<AppFileResourceRequest>,
}

/// App-defined Agent Context catalog shipped inside the immutable package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppAgentContextCapability {
    pub catalog: PathBuf,
}

/// Pinned semantic operations an App asks to invoke.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppOperationsCapability {
    /// Package-relative operations table, always beneath `operations/`.
    pub table: PathBuf,
    pub ids: Vec<String>,
}

/// One validated capability request in canonical form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "capability")]
pub enum AppCapabilityRequest {
    Files(AppFilesCapability),
    Operations(AppOperationsCapability),
    Theme,
    Kv,
    AgentContext(AppAgentContextCapability),
}

impl AppCapabilityRequest {
    #[must_use]
    pub fn kind(&self) -> AppCapabilityKind {
        match self {
            Self::Files(_) => AppCapabilityKind::Files,
            Self::Operations(_) => AppCapabilityKind::Operations,
            Self::Theme => AppCapabilityKind::Theme,
            Self::Kv => AppCapabilityKind::Kv,
            Self::AgentContext(_) => AppCapabilityKind::AgentContext,
        }
    }

    /// Operations request, when this is one. Used by publication to resolve
    /// requested ids against the immutable operations table.
    #[must_use]
    pub fn as_operations(&self) -> Option<&AppOperationsCapability> {
        match self {
            Self::Operations(operations) => Some(operations),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Raw manifest shapes
// ---------------------------------------------------------------------------

/// Raw `[capability]` table. Unknown capability names are captured so they can
/// be rejected rather than silently dropped.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawCapabilityTable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<RawFilesCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operations: Option<RawOperationsCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<toml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kv: Option<toml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_context: Option<RawAgentContextCapability>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFilesCapability {
    pub resources: Vec<RawFileResource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFileResource {
    pub role: String,
    pub path: String,
    pub access: String,
    #[serde(default)]
    pub watch: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawAgentContextCapability {
    pub catalog: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawOperationsCapability {
    pub table: String,
    pub ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate the manifest's capability request in one fail-closed pass.
///
/// Returns requests in canonical kind order so grants, digests, and permission
/// dialogs stay stable regardless of manifest ordering.
pub(crate) fn validate_capabilities(
    requested: &[String],
    table: &RawCapabilityTable,
) -> Result<Vec<AppCapabilityRequest>, AppPackageError> {
    if let Some(name) = table.unknown.keys().next() {
        return Err(AppPackageError::new(
            AppPackageErrorCode::UnknownCapability,
            format!("[capability.{name}] is not a known capability"),
        ));
    }
    if requested.len() > MAX_REQUESTED_CAPABILITIES {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidCapabilityRequest,
            format!(
                "requested_capabilities has {} entries; limit is {MAX_REQUESTED_CAPABILITIES}",
                requested.len()
            ),
        ));
    }

    let mut requested_kinds = BTreeSet::new();
    for name in requested {
        let kind = AppCapabilityKind::parse(name).ok_or_else(|| {
            AppPackageError::new(
                AppPackageErrorCode::UnknownCapability,
                format!("requested capability {name:?} is not a known capability"),
            )
        })?;
        if !requested_kinds.insert(kind) {
            return Err(AppPackageError::new(
                AppPackageErrorCode::InvalidCapabilityRequest,
                format!("capability {name:?} is requested more than once"),
            ));
        }
    }

    let declared: BTreeSet<AppCapabilityKind> = [
        table.files.is_some().then_some(AppCapabilityKind::Files),
        table
            .operations
            .is_some()
            .then_some(AppCapabilityKind::Operations),
        table.theme.is_some().then_some(AppCapabilityKind::Theme),
        table.kv.is_some().then_some(AppCapabilityKind::Kv),
        table
            .agent_context
            .is_some()
            .then_some(AppCapabilityKind::AgentContext),
    ]
    .into_iter()
    .flatten()
    .collect();

    if let Some(kind) = declared.difference(&requested_kinds).next() {
        return Err(AppPackageError::new(
            AppPackageErrorCode::OrphanCapabilityTable,
            format!(
                "[capability.{}] is declared but {:?} is not in requested_capabilities",
                kind.name(),
                kind.name()
            ),
        ));
    }
    for kind in &requested_kinds {
        if kind.requires_table() && !declared.contains(kind) {
            return Err(AppPackageError::new(
                AppPackageErrorCode::MissingCapabilityTable,
                format!(
                    "capability {:?} requires a [capability.{}] table describing its exact authority",
                    kind.name(),
                    kind.name()
                ),
            ));
        }
        if !kind.requires_table() && declared.contains(kind) {
            return Err(AppPackageError::new(
                AppPackageErrorCode::InvalidCapabilityRequest,
                format!(
                    "capability {:?} takes no configuration; remove [capability.{}]",
                    kind.name(),
                    kind.name()
                ),
            ));
        }
    }

    let mut validated = Vec::with_capacity(requested_kinds.len());
    for kind in requested_kinds {
        let request = match kind {
            AppCapabilityKind::Files => {
                let raw = table.files.as_ref().ok_or_else(missing_files_table)?;
                AppCapabilityRequest::Files(validate_files(raw)?)
            }
            AppCapabilityKind::Operations => {
                let raw = table
                    .operations
                    .as_ref()
                    .ok_or_else(missing_operations_table)?;
                AppCapabilityRequest::Operations(validate_operations(raw)?)
            }
            AppCapabilityKind::Theme => AppCapabilityRequest::Theme,
            AppCapabilityKind::Kv => AppCapabilityRequest::Kv,
            AppCapabilityKind::AgentContext => {
                let raw = table.agent_context.as_ref().ok_or_else(|| {
                    AppPackageError::new(
                        AppPackageErrorCode::MissingCapabilityTable,
                        "[capability.agent_context] is required",
                    )
                })?;
                let catalog =
                    bounded_relative_path(&raw.catalog, PACKAGE_PATH_LIMITS).map_err(|error| {
                        error.with_code(AppPackageErrorCode::InvalidCapabilityRequest)
                    })?;
                if !is_beneath(&catalog, CONTEXT_DIR) {
                    return Err(AppPackageError::new(
                        AppPackageErrorCode::InvalidCapabilityRequest,
                        "capability.agent_context catalog must be a package-relative file under context/",
                    )
                    .at(&catalog));
                }
                AppCapabilityRequest::AgentContext(AppAgentContextCapability { catalog })
            }
        };
        validated.push(request);
    }
    Ok(validated)
}

fn missing_files_table() -> AppPackageError {
    AppPackageError::new(
        AppPackageErrorCode::MissingCapabilityTable,
        "[capability.files] is required",
    )
}

fn missing_operations_table() -> AppPackageError {
    AppPackageError::new(
        AppPackageErrorCode::MissingCapabilityTable,
        "[capability.operations] is required",
    )
}

fn validate_files(raw: &RawFilesCapability) -> Result<AppFilesCapability, AppPackageError> {
    if raw.resources.is_empty() {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidCapabilityRequest,
            "capability.files must request at least one resource",
        ));
    }
    if raw.resources.len() > MAX_FILE_RESOURCES {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidCapabilityRequest,
            format!(
                "capability.files requests {} resources; limit is {MAX_FILE_RESOURCES}",
                raw.resources.len()
            ),
        ));
    }

    let mut roles = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut resources = Vec::with_capacity(raw.resources.len());
    for resource in &raw.resources {
        if !is_valid_role(&resource.role) {
            return Err(AppPackageError::new(
                AppPackageErrorCode::InvalidCapabilityRequest,
                format!(
                    "resource role {:?} must be 1-{MAX_ROLE_BYTES} lowercase kebab-case characters",
                    resource.role
                ),
            ));
        }
        if !roles.insert(resource.role.clone()) {
            return Err(AppPackageError::new(
                AppPackageErrorCode::InvalidCapabilityRequest,
                format!(
                    "resource role {:?} is declared more than once",
                    resource.role
                ),
            ));
        }

        let access = match resource.access.as_str() {
            "read" => AppFileAccess::Read,
            "readwrite" => AppFileAccess::ReadWrite,
            other => {
                return Err(AppPackageError::new(
                    AppPackageErrorCode::InvalidCapabilityRequest,
                    format!("resource access {other:?} must be \"read\" or \"readwrite\""),
                ));
            }
        };

        let path = validate_resource_path(&resource.path)?;
        if !paths.insert(path.clone()) {
            return Err(AppPackageError::new(
                AppPackageErrorCode::InvalidCapabilityRequest,
                format!(
                    "resource path {:?} is bound more than once",
                    portable_display(&path)
                ),
            ));
        }

        resources.push(AppFileResourceRequest {
            role: resource.role.clone(),
            path,
            access,
            watch: resource.watch,
        });
    }

    Ok(AppFilesCapability { resources })
}

/// Validate one work-directory-relative resource path against portable-path
/// rules plus the reserved-component policy.
fn validate_resource_path(value: &str) -> Result<PathBuf, AppPackageError> {
    let path = bounded_relative_path(value, RESOURCE_PATH_LIMITS)
        .map_err(|error| error.with_code(AppPackageErrorCode::CapabilityResourceRejected))?;
    for component in path.components() {
        let component = component.as_os_str().to_string_lossy();
        if RESERVED_RESOURCE_COMPONENTS
            .iter()
            .any(|reserved| component.eq_ignore_ascii_case(reserved))
        {
            return Err(AppPackageError::new(
                AppPackageErrorCode::CapabilityResourceRejected,
                format!("resource path may not traverse the reserved {component:?} directory"),
            )
            .at(&path));
        }
    }
    Ok(path)
}

fn validate_operations(
    raw: &RawOperationsCapability,
) -> Result<AppOperationsCapability, AppPackageError> {
    let table = bounded_relative_path(&raw.table, PACKAGE_PATH_LIMITS)
        .map_err(|error| error.with_code(AppPackageErrorCode::InvalidCapabilityRequest))?;
    if !is_beneath(&table, OPERATIONS_DIR) {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidCapabilityRequest,
            format!(
                "capability.operations table must be a package-relative file under {OPERATIONS_DIR}/"
            ),
        )
        .at(&table));
    }

    if raw.ids.is_empty() {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidCapabilityRequest,
            "capability.operations must request at least one operation id",
        ));
    }
    if raw.ids.len() > MAX_OPERATION_IDS {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidCapabilityRequest,
            format!(
                "capability.operations requests {} ids; limit is {MAX_OPERATION_IDS}",
                raw.ids.len()
            ),
        ));
    }

    let mut seen = BTreeSet::new();
    for id in &raw.ids {
        if !is_valid_operation_id(id) {
            return Err(AppPackageError::new(
                AppPackageErrorCode::InvalidCapabilityRequest,
                format!("operation id {id:?} is not a valid dotted identifier"),
            ));
        }
        if !seen.insert(id.clone()) {
            return Err(AppPackageError::new(
                AppPackageErrorCode::InvalidCapabilityRequest,
                format!("operation id {id:?} is requested more than once"),
            ));
        }
    }

    Ok(AppOperationsCapability {
        table,
        ids: raw.ids.clone(),
    })
}

fn is_valid_role(value: &str) -> bool {
    is_kebab_identifier(value, MAX_ROLE_BYTES)
}

/// Strict dotted identifier such as `comfy.generate.submit`.
#[must_use]
pub(crate) fn is_valid_operation_id(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_OPERATION_ID_BYTES {
        return false;
    }
    let segments: Vec<&str> = value.split('.').collect();
    if segments.is_empty() || segments.len() > MAX_OPERATION_ID_SEGMENTS {
        return false;
    }
    segments
        .iter()
        .all(|segment| is_kebab_identifier(segment, MAX_OPERATION_ID_BYTES))
}

/// Lowercase kebab-case identifier: starts alphabetic, no doubled or trailing
/// dashes, ASCII only.
fn is_kebab_identifier(value: &str, max_bytes: usize) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > max_bytes || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    if bytes.last() == Some(&b'-') {
        return false;
    }
    let mut previous_dash = false;
    for byte in bytes {
        let is_dash = *byte == b'-';
        if !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && !is_dash {
            return false;
        }
        if is_dash && previous_dash {
            return false;
        }
        previous_dash = is_dash;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files_table(resources: Vec<RawFileResource>) -> RawCapabilityTable {
        RawCapabilityTable {
            files: Some(RawFilesCapability { resources }),
            ..RawCapabilityTable::default()
        }
    }

    fn resource(role: &str, path: &str, access: &str) -> RawFileResource {
        RawFileResource {
            role: role.to_owned(),
            path: path.to_owned(),
            access: access.to_owned(),
            watch: false,
        }
    }

    fn requested(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn accepts_files_operations_theme_and_kv() -> Result<(), AppPackageError> {
        let table = RawCapabilityTable {
            files: Some(RawFilesCapability {
                resources: vec![
                    RawFileResource {
                        role: "outputs".to_owned(),
                        path: "outputs".to_owned(),
                        access: "read".to_owned(),
                        watch: true,
                    },
                    resource(
                        "requests",
                        "oqto-apps-state/comfy-studio/requests",
                        "readwrite",
                    ),
                ],
            }),
            operations: Some(RawOperationsCapability {
                table: DEFAULT_OPERATIONS_TABLE.to_owned(),
                ids: vec!["comfy.workflows.list".to_owned()],
            }),
            theme: None,
            kv: None,
            agent_context: None,
            unknown: BTreeMap::new(),
        };
        let validated =
            validate_capabilities(&requested(&["files", "operations", "theme", "kv"]), &table)?;

        assert_eq!(
            validated
                .iter()
                .map(AppCapabilityRequest::kind)
                .collect::<Vec<_>>(),
            vec![
                AppCapabilityKind::Files,
                AppCapabilityKind::Operations,
                AppCapabilityKind::Theme,
                AppCapabilityKind::Kv,
            ]
        );
        let AppCapabilityRequest::Files(files) = &validated[0] else {
            panic!("expected files capability");
        };
        assert_eq!(files.resources[0].access, AppFileAccess::Read);
        assert!(files.resources[0].watch);
        assert_eq!(files.resources[1].access, AppFileAccess::ReadWrite);
        Ok(())
    }

    #[test]
    fn canonical_order_is_independent_of_manifest_order() -> Result<(), AppPackageError> {
        let table = RawCapabilityTable {
            theme: Some(toml::Value::Table(toml::map::Map::new())),
            ..RawCapabilityTable::default()
        };
        // theme declares a table, which is rejected; use the parameterless form.
        assert_eq!(
            validate_capabilities(&requested(&["theme"]), &table)
                .expect_err("configured theme must fail")
                .code,
            AppPackageErrorCode::InvalidCapabilityRequest
        );

        let plain = RawCapabilityTable::default();
        let validated = validate_capabilities(&requested(&["kv", "theme"]), &plain)?;
        assert_eq!(
            validated
                .iter()
                .map(AppCapabilityRequest::kind)
                .collect::<Vec<_>>(),
            vec![AppCapabilityKind::Theme, AppCapabilityKind::Kv]
        );
        Ok(())
    }

    #[test]
    fn rejects_unknown_capability_name_and_table() {
        assert_eq!(
            validate_capabilities(&requested(&["network"]), &RawCapabilityTable::default())
                .expect_err("unknown name must fail")
                .code,
            AppPackageErrorCode::UnknownCapability
        );

        let mut unknown = BTreeMap::new();
        unknown.insert("network".to_owned(), toml::Value::Boolean(true));
        let table = RawCapabilityTable {
            unknown,
            ..RawCapabilityTable::default()
        };
        assert_eq!(
            validate_capabilities(&requested(&["theme"]), &table)
                .expect_err("unknown table must fail")
                .code,
            AppPackageErrorCode::UnknownCapability
        );
    }

    #[test]
    fn rejects_orphan_and_missing_capability_tables() {
        let orphan = files_table(vec![resource("outputs", "outputs", "read")]);
        assert_eq!(
            validate_capabilities(&requested(&["theme"]), &orphan)
                .expect_err("orphan table must fail")
                .code,
            AppPackageErrorCode::OrphanCapabilityTable
        );

        assert_eq!(
            validate_capabilities(&requested(&["files"]), &RawCapabilityTable::default())
                .expect_err("missing table must fail")
                .code,
            AppPackageErrorCode::MissingCapabilityTable
        );
        assert_eq!(
            validate_capabilities(&requested(&["operations"]), &RawCapabilityTable::default())
                .expect_err("missing table must fail")
                .code,
            AppPackageErrorCode::MissingCapabilityTable
        );
    }

    #[test]
    fn rejects_duplicate_requested_capability() {
        assert_eq!(
            validate_capabilities(
                &requested(&["theme", "theme"]),
                &RawCapabilityTable::default()
            )
            .expect_err("duplicate must fail")
            .code,
            AppPackageErrorCode::InvalidCapabilityRequest
        );
    }

    #[test]
    fn rejects_reserved_and_escaping_resource_paths() {
        for path in [
            "../outside",
            "/etc/passwd",
            ".git/config",
            ".oqto/apps",
            "oqto-apps/other.oqtoapp/oqto-app.toml",
            "nested/.git/config",
        ] {
            let table = files_table(vec![resource("role", path, "read")]);
            let error = validate_capabilities(&requested(&["files"]), &table)
                .expect_err("unsafe resource path must fail");
            assert_eq!(
                error.code,
                AppPackageErrorCode::CapabilityResourceRejected,
                "path={path}"
            );
        }
    }

    #[test]
    fn rejects_invalid_role_access_and_duplicates() {
        let bad_role = files_table(vec![resource("Bad_Role", "outputs", "read")]);
        assert_eq!(
            validate_capabilities(&requested(&["files"]), &bad_role)
                .expect_err("role must fail")
                .code,
            AppPackageErrorCode::InvalidCapabilityRequest
        );

        let bad_access = files_table(vec![resource("outputs", "outputs", "write")]);
        assert_eq!(
            validate_capabilities(&requested(&["files"]), &bad_access)
                .expect_err("access must fail")
                .code,
            AppPackageErrorCode::InvalidCapabilityRequest
        );

        let duplicate_role = files_table(vec![
            resource("outputs", "outputs", "read"),
            resource("outputs", "other", "read"),
        ]);
        assert_eq!(
            validate_capabilities(&requested(&["files"]), &duplicate_role)
                .expect_err("duplicate role must fail")
                .code,
            AppPackageErrorCode::InvalidCapabilityRequest
        );

        let duplicate_path = files_table(vec![
            resource("outputs", "outputs", "read"),
            resource("also-outputs", "outputs", "readwrite"),
        ]);
        assert_eq!(
            validate_capabilities(&requested(&["files"]), &duplicate_path)
                .expect_err("duplicate path must fail")
                .code,
            AppPackageErrorCode::InvalidCapabilityRequest
        );
    }

    #[test]
    fn rejects_operations_table_outside_operations_root() {
        for table_path in ["table.toml", "bundle/table.toml", "operations"] {
            let table = RawCapabilityTable {
                operations: Some(RawOperationsCapability {
                    table: table_path.to_owned(),
                    ids: vec!["a.b".to_owned()],
                }),
                ..RawCapabilityTable::default()
            };
            assert_eq!(
                validate_capabilities(&requested(&["operations"]), &table)
                    .expect_err("table path must fail")
                    .code,
                AppPackageErrorCode::InvalidCapabilityRequest,
                "table={table_path}"
            );
        }
    }

    #[test]
    fn rejects_malformed_operation_ids() {
        for id in [
            "",
            "Comfy.List",
            "comfy..list",
            "comfy.",
            "comfy list",
            "-a.b",
        ] {
            let table = RawCapabilityTable {
                operations: Some(RawOperationsCapability {
                    table: DEFAULT_OPERATIONS_TABLE.to_owned(),
                    ids: vec![id.to_owned()],
                }),
                ..RawCapabilityTable::default()
            };
            assert_eq!(
                validate_capabilities(&requested(&["operations"]), &table)
                    .expect_err("id must fail")
                    .code,
                AppPackageErrorCode::InvalidCapabilityRequest,
                "id={id}"
            );
        }
        assert!(is_valid_operation_id("comfy.generate.submit"));
        assert!(is_valid_operation_id("comfy.workflows.list"));
    }
}

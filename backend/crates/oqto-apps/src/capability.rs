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
pub const CAPABILITY_CONNECTION: &str = "connection";
/// The only mediated HTTP profile currently supported.
pub const CONNECTION_PROFILE_HTTPS_GET_V1: &str = "https_get_v1";
const MAX_CONNECTION_SLOTS: usize = 32;

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
    Connection,
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
            CAPABILITY_CONNECTION => Some(Self::Connection),
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
            Self::Connection => CAPABILITY_CONNECTION,
        }
    }

    /// True when the capability carries authority that must be spelled out in
    /// its own `[capability.<name>]` table.
    #[must_use]
    pub fn requires_table(self) -> bool {
        self.table_shape().is_some()
    }

    /// TOML shape of the required `[capability.<name>]` table, quoted in
    /// validation errors so an author can fix the manifest without reading
    /// resolver source. `None` for capabilities that take no configuration.
    #[must_use]
    pub fn table_shape(self) -> Option<&'static str> {
        match self {
            Self::Files => Some(
                "[[capability.files.resources]] with role = \"<name>\", \
                 path = \"<work-directory-relative path>\", \
                 access = \"read\" | \"readwrite\", optional watch = true|false",
            ),
            Self::Operations => Some(
                "[capability.operations] with table = \"<package-relative operations file>\", \
                 ids = [\"<operation id>\", ...]",
            ),
            Self::AgentContext => Some(
                "[capability.agent_context] with catalog = \"<package-relative catalog file>\"",
            ),
            Self::Connection => Some("[capability.connection] with named connection slots"),
            Self::Theme | Self::Kv => None,
        }
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

/// One named connection request, matching the canonical protocol slot fields.
/// This contains no credential, provider path, binding, or grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConnectionSlot {
    pub slot: String,
    pub expected_origin: String,
    pub profile: String,
}

/// Explicit bounded connection slots; configuration is never a grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConnectionCapability {
    pub slots: Vec<AppConnectionSlot>,
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
    Connection(AppConnectionCapability),
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
            Self::Connection(_) => AppCapabilityKind::Connection,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<RawConnectionCapability>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawConnectionCapability {
    pub slots: Vec<AppConnectionSlot>,
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
        table
            .connection
            .is_some()
            .then_some(AppCapabilityKind::Connection),
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
        if let Some(shape) = kind.table_shape()
            && !declared.contains(kind)
        {
            return Err(AppPackageError::new(
                AppPackageErrorCode::MissingCapabilityTable,
                format!(
                    "capability {:?} requires a [capability.{}] table describing its exact authority; expected {shape}",
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
            AppCapabilityKind::Connection => {
                let raw = table.connection.as_ref().ok_or_else(|| {
                    AppPackageError::new(
                        AppPackageErrorCode::MissingCapabilityTable,
                        "[capability.connection] is required",
                    )
                })?;
                AppCapabilityRequest::Connection(validate_connections(raw)?)
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

fn validate_connections(
    raw: &RawConnectionCapability,
) -> Result<AppConnectionCapability, AppPackageError> {
    let invalid =
        |message| AppPackageError::new(AppPackageErrorCode::InvalidCapabilityRequest, message);
    if raw.slots.is_empty() || raw.slots.len() > MAX_CONNECTION_SLOTS {
        return Err(invalid(format!(
            "capability.connection must declare 1-{MAX_CONNECTION_SLOTS} slots"
        )));
    }
    let mut seen = BTreeSet::new();
    let mut slots = Vec::with_capacity(raw.slots.len());
    for entry in &raw.slots {
        if !is_kebab_identifier(&entry.slot, MAX_ROLE_BYTES) || !seen.insert(&entry.slot) {
            return Err(invalid(
                "connection slots must be unique 1-64 lowercase kebab-case identifiers".to_owned(),
            ));
        }
        if entry.profile != CONNECTION_PROFILE_HTTPS_GET_V1 {
            return Err(invalid(
                "connection profile must be https_get_v1".to_owned(),
            ));
        }
        let expected_origin = canonical_connection_origin(&entry.expected_origin).ok_or_else(|| {
            invalid("connection expected_origin must be an HTTPS DNS origin on port 443 without path, query, fragment, or userinfo".to_owned())
        })?;
        slots.push(AppConnectionSlot {
            slot: entry.slot.clone(),
            expected_origin,
            profile: entry.profile.clone(),
        });
    }
    slots.sort_by(|left, right| left.slot.cmp(&right.slot));
    Ok(AppConnectionCapability { slots })
}

/// Strict ASCII DNS authority parsing, not URL parsing: URL parsers may silently
/// normalize paths, userinfo, backslashes and nonstandard IP literals. Runtime
/// mediation must additionally validate resolved addresses and deny redirects.
fn canonical_connection_origin(value: &str) -> Option<String> {
    if value.len() > 265 {
        return None;
    }
    let authority = value.strip_prefix("https://")?;
    let host = authority.strip_suffix(":443").unwrap_or(authority);
    if host.len() > 253 || !host.contains('.') || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    // Reject numeric terminal labels, including URL-style alternative IPv4
    // forms (decimal, octal, and hexadecimal), rather than treating them as DNS.
    let last = host.rsplit('.').next()?;
    if last.bytes().all(|byte| byte.is_ascii_digit())
        || last.to_ascii_lowercase().starts_with("0x")
        || host.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || !label.as_bytes()[0].is_ascii_alphanumeric()
                || !label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return None;
    }
    Some(format!("https://{}", host.to_ascii_lowercase()))
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

    fn connection_slot(slot: &str, origin: &str) -> AppConnectionSlot {
        AppConnectionSlot {
            slot: slot.to_owned(),
            expected_origin: origin.to_owned(),
            profile: CONNECTION_PROFILE_HTTPS_GET_V1.to_owned(),
        }
    }

    fn connection_table(slots: Vec<AppConnectionSlot>) -> RawCapabilityTable {
        RawCapabilityTable {
            connection: Some(RawConnectionCapability { slots }),
            ..RawCapabilityTable::default()
        }
    }

    #[test]
    fn connection_slots_are_canonical_and_protocol_shaped() -> Result<(), AppPackageError> {
        let table = connection_table(vec![
            connection_slot("z-api", "https://API.Example.com:443"),
            connection_slot("a-api", "https://api.example.com"),
        ]);
        let result = validate_capabilities(&requested(&["connection"]), &table)?;
        let AppCapabilityRequest::Connection(connection) = &result[0] else {
            panic!("expected connection request");
        };
        assert_eq!(connection.slots[0].slot, "a-api");
        assert_eq!(
            connection.slots[1].expected_origin,
            "https://api.example.com"
        );
        assert_eq!(
            serde_json::to_value(&connection.slots[0]).expect("serialize slot"),
            serde_json::json!({"slot":"a-api", "expected_origin":"https://api.example.com", "profile":"https_get_v1"})
        );
        let reverse = connection_table(
            table
                .connection
                .expect("connection")
                .slots
                .into_iter()
                .rev()
                .collect(),
        );
        assert_eq!(
            result,
            validate_capabilities(&requested(&["connection"]), &reverse)?
        );
        Ok(())
    }

    #[test]
    fn connection_requires_exact_declaration() {
        for (names, table, code) in [
            (
                vec!["connection"],
                RawCapabilityTable::default(),
                AppPackageErrorCode::MissingCapabilityTable,
            ),
            (
                vec!["theme"],
                connection_table(vec![connection_slot("api", "https://api.example.com")]),
                AppPackageErrorCode::OrphanCapabilityTable,
            ),
            (
                vec!["connection", "connection"],
                connection_table(vec![connection_slot("api", "https://api.example.com")]),
                AppPackageErrorCode::InvalidCapabilityRequest,
            ),
        ] {
            assert_eq!(
                validate_capabilities(&requested(&names), &table)
                    .expect_err("must fail")
                    .code,
                code
            );
        }
    }

    #[test]
    fn connection_slots_are_explicit_unique_and_bounded() {
        let valid = connection_slot("api", "https://api.example.com");
        let mut cases = vec![vec![], vec![valid.clone(), valid.clone()]];
        for slot in [
            "",
            "API",
            "api_key",
            "-api",
            "api-",
            "api--key",
            "*",
            &"a".repeat(65),
        ] {
            cases.push(vec![connection_slot(slot, "https://api.example.com")]);
        }
        let boundary: Vec<_> = (0..MAX_CONNECTION_SLOTS)
            .map(|i| connection_slot(&format!("api-{i}"), "https://api.example.com"))
            .collect();
        assert!(
            validate_capabilities(
                &requested(&["connection"]),
                &connection_table(boundary.clone())
            )
            .is_ok()
        );
        let mut excess = boundary;
        excess.push(connection_slot("another-api", "https://api.example.com"));
        cases.push(excess);
        let mut unsupported = valid;
        unsupported.profile = "https_post_v1".to_owned();
        cases.push(vec![unsupported]);
        for slots in cases {
            assert_eq!(
                validate_capabilities(&requested(&["connection"]), &connection_table(slots))
                    .expect_err("must fail")
                    .code,
                AppPackageErrorCode::InvalidCapabilityRequest
            );
        }
    }

    #[test]
    fn rejects_non_origin_urls_and_non_dns_hosts() {
        for origin in [
            "",
            "http://api.example.com",
            "HTTPS://api.example.com",
            "https://api.example.com/",
            "https://api.example.com/path",
            "https://api.example.com/..",
            "https://api.example.com?",
            "https://api.example.com?q=x",
            "https://api.example.com#",
            "https://api.example.com:80",
            "https://api.example.com:0443",
            "https://user@api.example.com",
            "https://@api.example.com",
            "https://user:pass@api.example.com",
            "https://127.0.0.1",
            "https://[::1]",
            "https://127.1",
            "https://0x7f.1",
            "https://localhost",
            "https://*.example.com",
            "https://api.example.com.",
            "https://api..example.com",
            "https://-api.example.com",
            "https://api_.example.com",
            "https://api.example.com\\evil",
            "https://api.example.com\n",
            "https://é.example.com",
            "https://%61pi.example.com",
            "https://api.example.0x7f",
        ] {
            assert!(
                canonical_connection_origin(origin).is_none(),
                "origin={origin:?}"
            );
        }
        assert!(
            canonical_connection_origin(&format!("https://{}.example.com", "a".repeat(64)))
                .is_none()
        );
        assert!(
            canonical_connection_origin(&format!(
                "https://{}.{}.{}.com",
                "a".repeat(63),
                "b".repeat(63),
                "c".repeat(63)
            ))
            .is_some()
        );
        assert!(
            canonical_connection_origin(&format!(
                "https://{}.{}.{}.{}.com",
                "a".repeat(63),
                "b".repeat(63),
                "c".repeat(63),
                "d".repeat(63)
            ))
            .is_none()
        );
    }

    #[test]
    fn parses_connection_toml_and_rejects_implicit_or_extra_authority() {
        let valid = "[[connection.slots]]\nslot = 'api'\nexpected_origin = 'https://api.example.com'\nprofile = 'https_get_v1'\n";
        let table: RawCapabilityTable = toml::from_str(valid).expect("parse TOML");
        assert!(validate_capabilities(&requested(&["connection"]), &table).is_ok());
        for extra in [
            "credential = 'secret'\n",
            "provider = 'kyz'\n",
            "origin = 'https://other.example.com'\n",
        ] {
            assert!(toml::from_str::<RawCapabilityTable>(&format!("{valid}{extra}")).is_err());
        }
        for invalid in [
            "[connection]\n",
            "[connection]\nslots = ['api']\n",
            "[[connection.slots]]\nslot = 'api'\nexpected_origin = 'https://api.example.com'\n",
        ] {
            assert!(toml::from_str::<RawCapabilityTable>(invalid).is_err());
        }
    }

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
            connection: None,
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
    fn missing_table_error_names_the_expected_shape() {
        let files = validate_capabilities(&requested(&["files"]), &RawCapabilityTable::default())
            .expect_err("missing files table must fail");
        for key in [
            "[[capability.files.resources]]",
            "role",
            "path",
            "access",
            "readwrite",
            "watch",
        ] {
            assert!(
                files.message.contains(key),
                "files error should name {key:?}: {}",
                files.message
            );
        }

        let operations =
            validate_capabilities(&requested(&["operations"]), &RawCapabilityTable::default())
                .expect_err("missing operations table must fail");
        assert!(
            operations.message.contains("table ="),
            "{}",
            operations.message
        );
        assert!(
            operations.message.contains("ids ="),
            "{}",
            operations.message
        );

        let context = validate_capabilities(
            &requested(&["agent_context"]),
            &RawCapabilityTable::default(),
        )
        .expect_err("missing agent_context table must fail");
        assert!(context.message.contains("catalog ="), "{}", context.message);

        for kind in [AppCapabilityKind::Theme, AppCapabilityKind::Kv] {
            assert!(!kind.requires_table());
        }
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

use std::collections::BTreeMap;
use std::path::{Component, PathBuf};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::capability::{AppCapabilityRequest, RawCapabilityTable, validate_capabilities};
use crate::path::portable_relative_path;
use crate::{AppPackageError, AppPackageErrorCode};

pub const APP_SCHEMA_V0: &str = "oqto-app/v0";
pub const MANIFEST_FILE: &str = "oqto-app.toml";

/// Presentation kinds understood by the v0 host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresentationKind {
    Declarative,
    SandboxedWeb,
}

/// Semantic fidelity allocated by the host, never a raw pixel contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationMode {
    Compact,
    Standard,
    Expanded,
    Focused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppBindingKind {
    WorkDirectory,
    Workspace,
    Account,
    Deployment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppTitle {
    pub en: String,
    #[serde(default)]
    pub de: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxedWebPresentation {
    pub entry: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PresentationTable {
    #[serde(default, rename = "sandboxed-web")]
    pub sandboxed_web: Option<SandboxedWebPresentation>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InstanceStateManifest {
    #[serde(default)]
    pub versioned: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AssetManifest {
    #[serde(default)]
    pub max_bytes: Option<u64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

/// Parsed v0 source manifest. Unknown fields are retained for lossless
/// inspection and future schema migration; they confer no behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppManifestV0 {
    pub schema: String,
    pub id: String,
    pub version: String,
    pub title: AppTitle,
    #[serde(default)]
    pub description: Option<String>,
    pub presentations: Vec<String>,
    #[serde(default)]
    pub requested_capabilities: Vec<String>,
    pub bindings: Vec<String>,
    pub default_binding: String,
    #[serde(default)]
    pub presentation: PresentationTable,
    /// Raw `[capability.*]` configuration. The validated form on
    /// [`ValidatedManifest::capabilities`] is the authority callers use.
    #[serde(default)]
    pub capability: RawCapabilityTable,
    #[serde(default)]
    pub instance_state: InstanceStateManifest,
    #[serde(default)]
    pub assets: AssetManifest,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

/// A manifest whose schema, identity, binding, presentation, and capability
/// request have passed v0 validation.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedManifest {
    pub manifest: AppManifestV0,
    pub raw: toml::Value,
    pub package_dir_name: String,
    pub entry: PathBuf,
    pub bundle_root: PathBuf,
    /// Requested capabilities in canonical kind order.
    pub capabilities: Vec<AppCapabilityRequest>,
}

impl ValidatedManifest {
    /// Operations request, when the manifest asks for the operations
    /// capability. Publication uses it to resolve ids against the immutable
    /// operations table.
    #[must_use]
    pub fn operations_request(&self) -> Option<&crate::capability::AppOperationsCapability> {
        self.capabilities
            .iter()
            .find_map(AppCapabilityRequest::as_operations)
    }
}

pub fn parse_manifest(
    package_dir_name: &str,
    bytes: &[u8],
    max_manifest_bytes: u64,
) -> Result<ValidatedManifest, AppPackageError> {
    let byte_len = u64::try_from(bytes.len()).map_err(|_| {
        AppPackageError::new(
            AppPackageErrorCode::ManifestTooLarge,
            "manifest length cannot be represented",
        )
    })?;
    if byte_len > max_manifest_bytes {
        return Err(AppPackageError::new(
            AppPackageErrorCode::ManifestTooLarge,
            format!("manifest is {byte_len} bytes; limit is {max_manifest_bytes}"),
        ));
    }

    let text = std::str::from_utf8(bytes).map_err(|_| {
        AppPackageError::new(
            AppPackageErrorCode::ManifestInvalidUtf8,
            "manifest must be UTF-8",
        )
    })?;
    let raw = text.parse::<toml::Value>().map_err(|error| {
        AppPackageError::new(
            AppPackageErrorCode::ManifestInvalidToml,
            format!("invalid TOML: {error}"),
        )
    })?;
    let manifest = raw.clone().try_into::<AppManifestV0>().map_err(|error| {
        AppPackageError::new(
            AppPackageErrorCode::ManifestInvalidToml,
            format!("invalid v0 manifest shape: {error}"),
        )
    })?;

    validate_manifest(package_dir_name, manifest, raw)
}

fn validate_manifest(
    package_dir_name: &str,
    manifest: AppManifestV0,
    raw: toml::Value,
) -> Result<ValidatedManifest, AppPackageError> {
    if manifest.schema != APP_SCHEMA_V0 {
        return Err(AppPackageError::new(
            AppPackageErrorCode::UnsupportedSchema,
            format!("unsupported schema {:?}", manifest.schema),
        ));
    }
    if !is_valid_app_id(&manifest.id) {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidAppId,
            "id must be 1-64 lowercase kebab-case ASCII characters",
        ));
    }

    let expected_dir = format!("{}.oqtoapp", manifest.id);
    if package_dir_name != expected_dir {
        return Err(AppPackageError::new(
            AppPackageErrorCode::PackageIdMismatch,
            format!("package directory must be {expected_dir}"),
        ));
    }

    Version::parse(&manifest.version).map_err(|error| {
        AppPackageError::new(
            AppPackageErrorCode::InvalidVersion,
            format!("version must be SemVer: {error}"),
        )
    })?;

    if manifest.title.en.trim().is_empty() || manifest.title.en.len() > 128 {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidTitle,
            "title.en must contain 1-128 bytes",
        ));
    }
    if manifest
        .title
        .de
        .as_ref()
        .is_some_and(|title| title.trim().is_empty() || title.len() > 128)
    {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidTitle,
            "title.de must contain 1-128 bytes when present",
        ));
    }
    if manifest
        .description
        .as_ref()
        .is_some_and(|description| description.len() > 2_048)
    {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidTitle,
            "description must not exceed 2048 bytes",
        ));
    }

    let capabilities =
        validate_capabilities(&manifest.requested_capabilities, &manifest.capability)?;

    if !manifest
        .bindings
        .iter()
        .any(|binding| binding == "work-directory")
        || manifest.default_binding != "work-directory"
    {
        return Err(AppPackageError::new(
            AppPackageErrorCode::UnsupportedBinding,
            "the first runtime slice requires a work-directory binding",
        ));
    }
    if !manifest
        .presentations
        .iter()
        .any(|kind| kind == "sandboxed-web")
    {
        return Err(AppPackageError::new(
            AppPackageErrorCode::UnsupportedPresentation,
            "a sandboxed-web presentation is required",
        ));
    }

    let web = manifest
        .presentation
        .sandboxed_web
        .as_ref()
        .ok_or_else(|| {
            AppPackageError::new(
                AppPackageErrorCode::InvalidPresentation,
                "presentation.sandboxed-web is required",
            )
        })?;
    let entry = portable_relative_path(&web.entry)?;
    let mut components = entry.components();
    if components.next() != Some(Component::Normal("bundle".as_ref()))
        || components.next().is_none()
    {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidPresentation,
            "sandboxed-web entry must be a file beneath bundle/",
        ));
    }

    Ok(ValidatedManifest {
        manifest,
        raw,
        package_dir_name: package_dir_name.to_owned(),
        entry,
        bundle_root: PathBuf::from("bundle"),
        capabilities,
    })
}

fn is_valid_app_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 || !bytes[0].is_ascii_lowercase() {
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
    use crate::capability::AppCapabilityKind;

    const VALID: &str = r#"
schema = "oqto-app/v0"
id = "hello-oqto"
version = "0.1.0"
title = { en = "Hello", de = "Hallo", future = "kept" }
presentations = ["sandboxed-web"]
requested_capabilities = []
bindings = ["work-directory"]
default_binding = "work-directory"
future_key = "kept"

[presentation.sandboxed-web]
entry = "bundle/index.html"
future = true

[instance_state]
versioned = false

[assets]
max_bytes = 100000
"#;

    #[test]
    fn parses_v0_and_preserves_unknown_fields() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_manifest("hello-oqto.oqtoapp", VALID.as_bytes(), 65_536)?;
        assert_eq!(parsed.manifest.id, "hello-oqto");
        assert!(parsed.manifest.extra.contains_key("future_key"));
        assert!(parsed.manifest.title.extra.contains_key("future"));
        assert!(
            parsed
                .manifest
                .presentation
                .sandboxed_web
                .as_ref()
                .is_some_and(|web| web.extra.contains_key("future"))
        );
        assert_eq!(parsed.entry, PathBuf::from("bundle/index.html"));
        Ok(())
    }

    #[test]
    fn rejects_directory_id_mismatch() {
        let error = parse_manifest("other.oqtoapp", VALID.as_bytes(), 65_536)
            .expect_err("mismatch must fail");
        assert_eq!(error.code, AppPackageErrorCode::PackageIdMismatch);
    }

    #[test]
    fn parses_typed_capability_request() -> Result<(), Box<dyn std::error::Error>> {
        let input = format!(
            "{}\n[capability.files]\nresources = [\
             {{ role = \"outputs\", path = \"outputs\", access = \"read\", watch = true }}]\n",
            VALID.replace(
                "requested_capabilities = []",
                r#"requested_capabilities = ["files", "theme"]"#,
            )
        );
        let parsed = parse_manifest("hello-oqto.oqtoapp", input.as_bytes(), 65_536)?;
        assert_eq!(
            parsed
                .capabilities
                .iter()
                .map(AppCapabilityRequest::kind)
                .collect::<Vec<_>>(),
            vec![AppCapabilityKind::Files, AppCapabilityKind::Theme]
        );
        assert!(parsed.operations_request().is_none());
        Ok(())
    }

    #[test]
    fn rejects_capability_requested_without_its_table() {
        let input = VALID.replace(
            "requested_capabilities = []",
            "requested_capabilities = [\"files\"]",
        );
        let error = parse_manifest("hello-oqto.oqtoapp", input.as_bytes(), 65_536)
            .expect_err("missing table must fail");
        assert_eq!(error.code, AppPackageErrorCode::MissingCapabilityTable);
    }

    #[test]
    fn rejects_unknown_capability_name() {
        let input = VALID.replace(
            "requested_capabilities = []",
            "requested_capabilities = [\"network\"]",
        );
        let error = parse_manifest("hello-oqto.oqtoapp", input.as_bytes(), 65_536)
            .expect_err("unknown capability must fail");
        assert_eq!(error.code, AppPackageErrorCode::UnknownCapability);
    }

    #[test]
    fn rejects_orphan_capability_table() {
        let input = format!(
            "{VALID}\n[capability.files]\nresources = [\
             {{ role = \"outputs\", path = \"outputs\", access = \"read\" }}]\n"
        );
        let error = parse_manifest("hello-oqto.oqtoapp", input.as_bytes(), 65_536)
            .expect_err("orphan table must fail");
        assert_eq!(error.code, AppPackageErrorCode::OrphanCapabilityTable);
    }

    #[test]
    fn zero_capability_manifest_stays_valid() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_manifest("hello-oqto.oqtoapp", VALID.as_bytes(), 65_536)?;
        assert!(parsed.capabilities.is_empty());
        Ok(())
    }

    #[test]
    fn rejects_traversal_entry() {
        let input = VALID.replace("bundle/index.html", "bundle/../secret");
        let error = parse_manifest("hello-oqto.oqtoapp", input.as_bytes(), 65_536)
            .expect_err("traversal must fail");
        assert_eq!(error.code, AppPackageErrorCode::InvalidRelativePath);
    }

    #[test]
    fn rejects_non_kebab_ids() {
        for id in ["Hello", "hello_oqto", "hello--oqto", "hello-", "-hello", ""] {
            let input = VALID.replace("id = \"hello-oqto\"", &format!("id = \"{id}\""));
            let error = parse_manifest(&format!("{id}.oqtoapp"), input.as_bytes(), 65_536)
                .expect_err("invalid id must fail");
            assert_eq!(error.code, AppPackageErrorCode::InvalidAppId, "id={id}");
        }
    }
}

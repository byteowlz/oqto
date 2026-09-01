use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredBundleFile {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DefinitionInsert<'a> {
    pub id: &'a str,
    pub content_digest: &'a str,
    pub app_id: &'a str,
    pub version: &'a str,
    pub title_json: &'a str,
    pub description: Option<&'a str>,
    pub schema_tag: &'a str,
    pub manifest_toml: &'a str,
    pub web_entry_path: &'a str,
    pub file_index_json: &'a str,
    /// Canonical validated capability request pinned with this Definition.
    pub requested_capabilities_json: &'a str,
    pub total_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct PublicationInsert<'a> {
    pub acting_account_id: &'a str,
    pub work_directory_id: &'a str,
    pub package_name: &'a str,
    pub app_id: &'a str,
    pub definition: DefinitionInsert<'a>,
    /// Status a *newly created* Instance starts in. Reused Instances keep
    /// their existing status so republishing can never re-activate one that
    /// was denied or revoked.
    pub initial_instance_status: &'a str,
    /// Terminal status for this publish workflow.
    pub workflow_status: &'a str,
}

/// One current permission decision for an Instance.
#[derive(Debug, Clone, FromRow)]
pub struct CapabilityGrantRow {
    pub instance_id: String,
    pub definition_id: String,
    pub content_digest: String,
    pub request_json: String,
    pub decision: String,
    pub decided_by_account_id: String,
    pub decided_at: String,
    pub revoked_at: Option<String>,
}

/// Definition fields the permission flow needs.
#[derive(Debug, Clone, FromRow)]
pub struct DefinitionPermissionRow {
    pub definition_id: String,
    pub content_digest: String,
    pub app_id: String,
    pub version: String,
    pub title_json: String,
    pub requested_capabilities_json: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct AppInstanceRow {
    pub instance_id: String,
    pub definition_id: String,
    pub installation_id: String,
    pub app_id: String,
    pub version: String,
    pub title_json: String,
    pub content_digest: String,
    pub installation_owner_kind: String,
    pub binding_kind: String,
    pub status: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct DefinitionAssetRow {
    pub definition_id: String,
    pub content_digest: String,
    pub web_entry_path: String,
    pub file_index_json: String,
}

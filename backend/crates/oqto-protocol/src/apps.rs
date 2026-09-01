//! Canonical harness- and host-neutral Oqto App lifecycle contracts.
//!
//! Work-directory authority is supplied by the authenticated runner/backend
//! context. Agents and App frames never provide Account, Principal, grant, or
//! host-path identities through these contracts.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppLocalizedTitle {
    pub en: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub de: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppPresentationKind {
    Declarative,
    SandboxedWeb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppOwnerKind {
    WorkDirectory,
    Workspace,
    Account,
    Deployment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppCandidateState {
    Publishable,
    Rejected,
}

// ---------------------------------------------------------------------------
// Capability requests
// ---------------------------------------------------------------------------

/// Access an App asks for on one bound resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppFileAccess {
    Read,
    ReadWrite,
}

/// One resource an App asks to bind, named in work-directory-relative terms.
///
/// `path` is always relative to the bound work directory. Absolute host paths
/// are never public identity and must never appear here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppFileResourceRequest {
    /// Stable semantic name the App uses for this resource, such as `outputs`.
    pub role: String,
    pub path: String,
    pub access: AppFileAccess,
    /// True when the App asks to observe changes, not only read once.
    pub watch: bool,
}

/// One pinned semantic operation an App asks to invoke.
///
/// The summary is the package author's own description, shown to the person
/// deciding. The implementing executable stays a package-internal detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppOperationRequest {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppContextTopicRequest {
    pub id: String,
    pub title: String,
    pub disclosure: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppContextActionRequest {
    pub id: String,
    pub title: String,
    pub requires_user_activation: bool,
}

/// One capability an App asks for, in the exact shape a person approves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "capability", rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppCapabilityRequest {
    /// Read or write specific bound work-directory resources.
    Files {
        resources: Vec<AppFileResourceRequest>,
    },
    /// Invoke specific pinned operations that run outside the App sandbox.
    Operations {
        operations: Vec<AppOperationRequest>,
    },
    /// Read the host's current theme.
    Theme,
    /// Store the App's own settings, private to this Instance.
    Kv,
    /// Publish declared semantic context for authorized Agents and expose
    /// revision-bound contextual actions.
    AgentContext {
        topics: Vec<AppContextTopicRequest>,
        actions: Vec<AppContextActionRequest>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppRejectionCode {
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
    WorkDirectoryUnavailable,
    Unauthorized,
    ArtifactWriteFailed,
    WorkflowFailed,
    InstanceUnavailable,
    LaunchUnavailable,
    DataCopyUnsupported,
    /// The reviewed Definition no longer matches the Instance's pinned one.
    PermissionRequestStale,
    /// The Instance is not in a state where this permission action applies.
    PermissionStateInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppRejection {
    pub code: AppRejectionCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub relative_path: Option<String>,
}

/// One direct `.oqtoapp` source candidate in the authenticated work directory.
/// It intentionally exposes no absolute host path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppCandidateSummary {
    pub app_id: String,
    pub title: AppLocalizedTitle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub version: String,
    pub presentations: Vec<AppPresentationKind>,
    /// Exactly what the package asks for, already validated against its
    /// `[capability.*]` configuration. Never a bare label list.
    pub requested_capabilities: Vec<AppCapabilityRequest>,
    pub state: AppCandidateState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rejection: Option<AppRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppCandidateList {
    pub work_directory_id: String,
    pub candidates: Vec<AppCandidateSummary>,
}

/// Publish the current source candidate into a pinned Definition and a
/// work-directory Installation. The authenticated execution context supplies
/// the target; v0 intentionally has no data-copy option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppPublishRequest {
    pub app_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppPublishStatus {
    Validating,
    Materializing,
    AwaitingPermission,
    Approved,
    Denied,
    InstanceReady,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppInstanceStatus {
    Active,
    /// Published and pinned, but holding no authority until someone decides.
    AwaitingPermission,
    Suspended,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppPresentationSummary {
    pub presentation_id: String,
    pub kind: AppPresentationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppInstanceSummary {
    pub instance_id: String,
    pub definition_id: String,
    pub installation_id: String,
    pub app_id: String,
    pub title: AppLocalizedTitle,
    pub version: String,
    pub content_digest: String,
    pub installation_owner_kind: AppOwnerKind,
    pub binding_kind: AppOwnerKind,
    pub status: AppInstanceStatus,
    pub presentations: Vec<AppPresentationSummary>,
}

// ---------------------------------------------------------------------------
// Permission lifecycle
// ---------------------------------------------------------------------------

/// Exactly what one Instance asks for, pinned to the Definition a person
/// reviews. Approving this request approves nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppPermissionRequest {
    pub instance_id: String,
    pub definition_id: String,
    /// Immutable Definition digest the decision is bound to.
    pub content_digest: String,
    pub app_id: String,
    pub title: AppLocalizedTitle,
    pub version: String,
    pub capabilities: Vec<AppCapabilityRequest>,
}

/// Where an Instance stands in the permission lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppPermissionState {
    /// The App asks for nothing, so there is nothing to decide.
    NotRequired,
    AwaitingDecision,
    Allowed,
    Denied,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum AppPermissionDecision {
    Allow,
    Deny,
}

/// Decide one Instance's pending request.
///
/// `reviewed_content_digest` is the Definition the person actually saw. The
/// backend refuses the decision when it no longer matches the pinned
/// Definition, so an approval can never land on republished code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppPermissionDecisionRequest {
    pub decision: AppPermissionDecision,
    pub reviewed_content_digest: String,
}

/// Current permission standing of one Instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppPermissionStatus {
    pub request: AppPermissionRequest,
    pub state: AppPermissionState,
    pub instance_status: AppInstanceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub decided_by_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub decided_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppPublishResult {
    pub workflow_id: String,
    pub status: AppPublishStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance: Option<AppInstanceSummary>,
    /// Present when the Instance is awaiting a permission decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub permission_request: Option<AppPermissionRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rejection: Option<AppRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppInstanceList {
    pub work_directory_id: String,
    pub instances: Vec<AppInstanceSummary>,
}

/// One requester-local App presentation, delivered over the authenticated
/// API as a self-contained document and mounted in an opaque-origin
/// sandboxed frame. No App content URL exists, so there is no bearer token,
/// cookie, hostname, port, or DNS requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppKvGetResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppKvSetRequest {
    pub workspace_path: String,
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppKvDeleteRequest {
    pub workspace_path: String,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppPresentationDocument {
    pub instance_id: String,
    pub presentation_id: String,
    pub definition_id: String,
    pub content_digest: String,
    /// Complete inlined HTML for `iframe srcdoc` with a scripts-only sandbox.
    pub html: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_publish_request_has_no_data_copy_switch() {
        let request = serde_json::to_value(AppPublishRequest {
            app_id: "hello-oqto".to_owned(),
        })
        .expect("serialize App publish request");
        assert_eq!(request, serde_json::json!({ "app_id": "hello-oqto" }));
        assert!(request.get("include_data").is_none());
    }

    #[test]
    fn capability_request_is_a_tagged_union_of_exact_authority() {
        let capabilities = vec![
            AppCapabilityRequest::Files {
                resources: vec![AppFileResourceRequest {
                    role: "outputs".to_owned(),
                    path: "outputs".to_owned(),
                    access: AppFileAccess::Read,
                    watch: true,
                }],
            },
            AppCapabilityRequest::Operations {
                operations: vec![AppOperationRequest {
                    id: "comfy.generate.submit".to_owned(),
                    summary: Some("Submit one generation job".to_owned()),
                }],
            },
            AppCapabilityRequest::Theme,
            AppCapabilityRequest::Kv,
        ];
        let json = serde_json::to_value(&capabilities).expect("serialize capabilities");
        assert_eq!(json[0]["capability"], "files");
        assert_eq!(json[0]["resources"][0]["access"], "read");
        assert_eq!(json[0]["resources"][0]["watch"], true);
        assert_eq!(json[1]["capability"], "operations");
        assert_eq!(
            json[1]["operations"][0]["summary"],
            "Submit one generation job"
        );
        assert_eq!(json[2]["capability"], "theme");
        assert_eq!(json[3]["capability"], "kv");

        let restored: Vec<AppCapabilityRequest> =
            serde_json::from_value(json).expect("round-trip capabilities");
        assert_eq!(restored, capabilities);
    }

    #[test]
    fn permission_request_names_resources_semantically_without_host_paths() {
        let request = AppPermissionRequest {
            instance_id: "appinstance_1".to_owned(),
            definition_id: "appdef_1".to_owned(),
            content_digest: "a".repeat(64),
            app_id: "comfy-studio".to_owned(),
            title: AppLocalizedTitle {
                en: "Comfy Studio".to_owned(),
                de: None,
            },
            version: "0.1.0".to_owned(),
            capabilities: vec![AppCapabilityRequest::Files {
                resources: vec![AppFileResourceRequest {
                    role: "jobs".to_owned(),
                    path: "oqto-apps-state/comfy-studio/jobs.jsonl".to_owned(),
                    access: AppFileAccess::Read,
                    watch: true,
                }],
            }],
        };
        let json = serde_json::to_string(&request).expect("serialize permission request");
        assert!(!json.contains("/home/"));
        assert!(!json.contains("workspace_path"));
        assert!(!json.contains("principal"));
        // The bound resource stays work-directory-relative.
        assert!(!json.contains("\"/oqto-apps-state"));
    }

    #[test]
    fn rejected_candidate_carries_typed_diagnostic_without_host_path() {
        let candidate = AppCandidateSummary {
            app_id: "broken".to_owned(),
            title: AppLocalizedTitle {
                en: "broken".to_owned(),
                de: None,
            },
            description: None,
            version: "0.0.0".to_owned(),
            presentations: Vec::new(),
            requested_capabilities: Vec::new(),
            state: AppCandidateState::Rejected,

            rejection: Some(AppRejection {
                code: AppRejectionCode::ManifestMissing,
                message: "oqto-app.toml is required".to_owned(),
                relative_path: Some("oqto-apps/broken.oqtoapp/oqto-app.toml".to_owned()),
            }),
        };
        let json = serde_json::to_string(&candidate).expect("serialize candidate");
        assert!(!json.contains("/home/"));
        assert!(!json.contains("workspace_path"));
    }
}

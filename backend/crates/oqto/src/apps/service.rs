use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use oqto_apps::{
    AppCandidateStatus, AppPackageError, AppPackageErrorCode, BundleLimits, ResolvedOperation,
    ValidatedManifest, discover_candidates, snapshot_bundle,
};
use oqto_protocol::apps::{
    AppCandidateList, AppCandidateState, AppCandidateSummary, AppCapabilityRequest, AppFileAccess,
    AppFileResourceRequest, AppInstanceList, AppInstanceStatus, AppInstanceSummary,
    AppLocalizedTitle, AppOperationRequest, AppOwnerKind, AppPermissionDecision,
    AppPermissionRequest, AppPermissionState, AppPermissionStatus, AppPresentationKind,
    AppPresentationSummary, AppPublishResult, AppPublishStatus, AppRejection, AppRejectionCode,
};
use sha2::{Digest, Sha256};

use crate::user_plane::UserPlane;

use super::artifact::AppArtifactStore;
use super::models::{DefinitionInsert, PublicationInsert, StoredBundleFile};
use super::repository::{AppRepository, PermissionDecisionInsert};
use super::source::UserPlaneAppSource;

/// Instance status strings used by the durable schema.
const STATUS_ACTIVE: &str = "active";
const STATUS_AWAITING_PERMISSION: &str = "awaiting_permission";
const STATUS_SUSPENDED: &str = "suspended";

/// Typed outcome of a permission action, so callers map one enum instead of
/// re-deriving policy from strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppPermissionOutcome {
    Applied(Box<AppPermissionStatus>),
    /// The Instance asks for nothing, so there is nothing to decide.
    NotRequired(Box<AppPermissionStatus>),
    /// The reviewed Definition is not the one the Instance pins.
    Stale {
        pinned_content_digest: String,
    },
    /// The Instance is not in a state where this action applies.
    InvalidState {
        instance_status: AppInstanceStatus,
    },
}

/// Whether a decision may be recorded, decided without touching the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecisionGate {
    Proceed,
    NotRequired,
    Stale,
    InvalidState,
}

/// Pure admission policy for a permission decision.
///
/// A decision may only land when the App actually asks for something, the
/// deciding Account reviewed the Definition the Instance currently pins, and
/// the Instance is genuinely undecided or previously withdrawn.
fn decision_gate(
    requests_capabilities: bool,
    reviewed_content_digest: &str,
    pinned_content_digest: &str,
    instance_status: AppInstanceStatus,
) -> DecisionGate {
    if !requests_capabilities {
        return DecisionGate::NotRequired;
    }
    if reviewed_content_digest != pinned_content_digest {
        return DecisionGate::Stale;
    }
    match instance_status {
        AppInstanceStatus::AwaitingPermission | AppInstanceStatus::Suspended => {
            DecisionGate::Proceed
        }
        AppInstanceStatus::Active | AppInstanceStatus::Unavailable => DecisionGate::InvalidState,
    }
}

/// Pure projection of a stored grant into the state a person sees.
fn permission_state(
    requests_capabilities: bool,
    decision: Option<(&str, Option<&str>)>,
) -> AppPermissionState {
    if !requests_capabilities {
        return AppPermissionState::NotRequired;
    }
    match decision {
        None => AppPermissionState::AwaitingDecision,
        Some(("allowed", None)) => AppPermissionState::Allowed,
        Some(("allowed", Some(_))) => AppPermissionState::Revoked,
        Some(_) => AppPermissionState::Denied,
    }
}

#[derive(Clone)]
pub struct AuthorizedWorkDirectory {
    pub id: String,
    pub owner_kind: AppOwnerKind,
    pub owner_id: String,
    pub root: PathBuf,
    pub plane: Arc<dyn UserPlane>,
}

impl std::fmt::Debug for AuthorizedWorkDirectory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthorizedWorkDirectory")
            .field("id", &self.id)
            .field("owner_kind", &self.owner_kind)
            .field("owner_id", &"<authorized>")
            .field("root", &"<authorized>")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct AppRuntimeService {
    repository: AppRepository,
    artifacts: AppArtifactStore,
    limits: BundleLimits,
}

impl AppRuntimeService {
    #[must_use]
    pub fn new(repository: AppRepository, artifacts: AppArtifactStore) -> Self {
        Self {
            repository,
            artifacts,
            limits: BundleLimits::default(),
        }
    }

    #[must_use]
    pub fn repository(&self) -> &AppRepository {
        &self.repository
    }

    #[must_use]
    pub fn artifacts(&self) -> &AppArtifactStore {
        &self.artifacts
    }

    pub async fn list_candidates(
        &self,
        work_directory: &AuthorizedWorkDirectory,
    ) -> Result<AppCandidateList> {
        let source =
            UserPlaneAppSource::new(work_directory.plane.clone(), work_directory.root.clone());
        let candidates = discover_candidates(&source, self.limits)
            .await
            .map_err(|error| anyhow!(error))?
            .into_iter()
            .map(|candidate| match candidate.status {
                AppCandidateStatus::Publishable(manifest) => AppCandidateSummary {
                    app_id: manifest.manifest.id.clone(),
                    title: AppLocalizedTitle {
                        en: manifest.manifest.title.en.clone(),
                        de: manifest.manifest.title.de.clone(),
                    },
                    description: manifest.manifest.description.clone(),
                    version: manifest.manifest.version.clone(),
                    presentations: vec![AppPresentationKind::SandboxedWeb],
                    // Pre-publication browsing shows the request without
                    // operation summaries; those live in the immutable
                    // operations table resolved at publication.
                    requested_capabilities: capability_requests(&manifest, &[]),
                    state: AppCandidateState::Publishable,
                    rejection: None,
                },
                AppCandidateStatus::Rejected(error) => {
                    let fallback_id = candidate
                        .package_dir_name
                        .strip_suffix(".oqtoapp")
                        .unwrap_or(&candidate.package_dir_name)
                        .to_owned();
                    AppCandidateSummary {
                        app_id: fallback_id.clone(),
                        title: AppLocalizedTitle {
                            en: fallback_id,
                            de: None,
                        },
                        description: None,
                        version: "0.0.0".to_owned(),
                        presentations: Vec::new(),
                        requested_capabilities: Vec::new(),
                        state: AppCandidateState::Rejected,
                        rejection: Some(rejection_from_package(&error)),
                    }
                }
            })
            .collect();

        Ok(AppCandidateList {
            work_directory_id: work_directory.id.clone(),
            candidates,
        })
    }

    pub async fn publish(
        &self,
        acting_account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        app_id: &str,
    ) -> Result<AppPublishResult> {
        if !valid_app_id(app_id) {
            return Ok(AppPublishResult {
                workflow_id: String::new(),
                status: AppPublishStatus::Failed,
                instance: None,
                permission_request: None,
                rejection: Some(AppRejection {
                    code: AppRejectionCode::InvalidAppId,
                    message: "invalid App id".to_owned(),
                    relative_path: None,
                }),
            });
        }

        let workflow_id = self
            .repository
            .create_workflow(acting_account_id, &work_directory.id, app_id)
            .await?;
        let source =
            UserPlaneAppSource::new(work_directory.plane.clone(), work_directory.root.clone());
        let package_name = format!("{app_id}.oqtoapp");

        let snapshot = match snapshot_bundle(&source, &package_name, self.limits).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let rejection = rejection_from_package(&error);
                self.repository
                    .fail_workflow(
                        &workflow_id,
                        rejection_code_name(rejection.code),
                        &rejection.message,
                    )
                    .await?;
                return Ok(AppPublishResult {
                    workflow_id,
                    status: AppPublishStatus::Failed,
                    instance: None,
                    permission_request: None,
                    rejection: Some(rejection),
                });
            }
        };

        self.repository
            .update_workflow_status(&workflow_id, "materializing")
            .await?;
        if let Err(error) = self.artifacts.materialize(&snapshot).await {
            let message = format!("materializing immutable App artifact: {error:#}");
            self.repository
                .fail_workflow(&workflow_id, "artifact_write_failed", &message)
                .await?;
            return Ok(AppPublishResult {
                workflow_id,
                status: AppPublishStatus::Failed,
                instance: None,
                permission_request: None,
                rejection: Some(AppRejection {
                    code: AppRejectionCode::ArtifactWriteFailed,
                    message,
                    relative_path: None,
                }),
            });
        }

        let definition_id = format!("appdef_{}", snapshot.digest.as_str());
        let title_json = serde_json::to_string(&AppLocalizedTitle {
            en: snapshot.manifest.manifest.title.en.clone(),
            de: snapshot.manifest.manifest.title.de.clone(),
        })
        .context("serializing App title")?;
        let manifest_toml = std::str::from_utf8(&snapshot.manifest_bytes)
            .context("validated App manifest unexpectedly not UTF-8")?;
        let file_index = snapshot
            .files
            .iter()
            .map(|file| StoredBundleFile {
                relative_path: portable_path(&file.relative_path),
                sha256: hex::encode(Sha256::digest(&file.bytes)),
                bytes: u64::try_from(file.bytes.len()).unwrap_or(u64::MAX),
            })
            .collect::<Vec<_>>();
        let file_index_json =
            serde_json::to_string(&file_index).context("serializing App file index")?;
        let web_entry_path = portable_path(&snapshot.manifest.entry);
        let total_bytes = i64::try_from(snapshot.total_bytes)
            .context("App bundle size exceeds durable integer range")?;

        // The canonical request is computed once, pinned with the Definition,
        // and later approved verbatim. Nothing re-derives it from source.
        let capabilities = capability_requests(&snapshot.manifest, &snapshot.operations);
        let requested_capabilities_json = serde_json::to_string(&capabilities)
            .context("serializing the App capability request")?;
        let requires_permission = !capabilities.is_empty();
        let (initial_instance_status, workflow_status) = if requires_permission {
            (STATUS_AWAITING_PERMISSION, "awaiting_permission")
        } else {
            (STATUS_ACTIVE, "instance_ready")
        };

        let row = self
            .repository
            .commit_publication(
                &workflow_id,
                &PublicationInsert {
                    acting_account_id,
                    work_directory_id: &work_directory.id,
                    package_name: &package_name,
                    app_id,
                    definition: DefinitionInsert {
                        id: &definition_id,
                        content_digest: snapshot.digest.as_str(),
                        app_id,
                        version: &snapshot.manifest.manifest.version,
                        title_json: &title_json,
                        description: snapshot.manifest.manifest.description.as_deref(),
                        schema_tag: &snapshot.manifest.manifest.schema,
                        manifest_toml,
                        web_entry_path: &web_entry_path,
                        file_index_json: &file_index_json,
                        requested_capabilities_json: &requested_capabilities_json,
                        total_bytes,
                    },
                    initial_instance_status,
                    workflow_status,
                },
            )
            .await?;

        let instance = instance_summary(row)?;
        // Report the Instance's real state: a reused Instance that was denied
        // stays suspended even though this publication succeeded.
        let status = match instance.status {
            AppInstanceStatus::Active => AppPublishStatus::InstanceReady,
            AppInstanceStatus::AwaitingPermission => AppPublishStatus::AwaitingPermission,
            AppInstanceStatus::Suspended | AppInstanceStatus::Unavailable => {
                AppPublishStatus::Denied
            }
        };
        let permission_request = (instance.status == AppInstanceStatus::AwaitingPermission)
            .then(|| permission_request(&instance, capabilities));

        Ok(AppPublishResult {
            workflow_id,
            status,
            instance: Some(instance),
            permission_request,
            rejection: None,
        })
    }

    /// Current permission standing of one Instance in the authorized work
    /// directory.
    pub async fn permission_status(
        &self,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
    ) -> Result<Option<AppPermissionStatus>> {
        let Some(row) = self
            .repository
            .get_instance_for_work_directory(instance_id, &work_directory.id)
            .await?
        else {
            return Ok(None);
        };
        let instance = instance_summary(row)?;
        self.permission_status_for(&instance).await.map(Some)
    }

    /// Apply one allow/deny decision.
    ///
    /// The decision is bound to the Definition the deciding Account reviewed:
    /// if the Instance now pins a different Definition, the decision is refused
    /// rather than silently applied to different code.
    pub async fn decide_permissions(
        &self,
        acting_account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
        decision: AppPermissionDecision,
        reviewed_content_digest: &str,
    ) -> Result<Option<AppPermissionOutcome>> {
        let Some(row) = self
            .repository
            .get_instance_for_work_directory(instance_id, &work_directory.id)
            .await?
        else {
            return Ok(None);
        };
        let instance = instance_summary(row)?;

        let definition = self
            .repository
            .get_definition_permissions(&instance.definition_id)
            .await?
            .context("pinned App Definition missing while deciding permissions")?;
        let capabilities = stored_capabilities(&definition.requested_capabilities_json)?;

        match decision_gate(
            !capabilities.is_empty(),
            reviewed_content_digest,
            &definition.content_digest,
            instance.status,
        ) {
            DecisionGate::Proceed => {}
            DecisionGate::NotRequired => {
                return Ok(Some(AppPermissionOutcome::NotRequired(Box::new(
                    self.permission_status_for(&instance).await?,
                ))));
            }
            DecisionGate::Stale => {
                return Ok(Some(AppPermissionOutcome::Stale {
                    pinned_content_digest: definition.content_digest,
                }));
            }
            DecisionGate::InvalidState => {
                return Ok(Some(AppPermissionOutcome::InvalidState {
                    instance_status: instance.status,
                }));
            }
        }

        let (decision_name, instance_status, workflow_status) = match decision {
            AppPermissionDecision::Allow => ("allowed", STATUS_ACTIVE, "approved"),
            AppPermissionDecision::Deny => ("denied", STATUS_SUSPENDED, "denied"),
        };

        self.repository
            .record_permission_decision(&PermissionDecisionInsert {
                instance_id: &instance.instance_id,
                definition_id: &instance.definition_id,
                content_digest: &definition.content_digest,
                // Store the pinned request verbatim, not a re-serialization of
                // whatever the client believed it was approving.
                request_json: &definition.requested_capabilities_json,
                decision: decision_name,
                decided_by_account_id: acting_account_id,
                instance_status,
                workflow_status,
            })
            .await?;

        let refreshed = self
            .repository
            .get_instance(&instance.instance_id)
            .await?
            .context("App Instance missing after a permission decision")?;
        let refreshed = instance_summary(refreshed)?;
        Ok(Some(AppPermissionOutcome::Applied(Box::new(
            self.permission_status_for(&refreshed).await?,
        ))))
    }

    /// Revoke a live grant. The Instance suspends immediately, so the next
    /// presentation request fails closed.
    pub async fn revoke_permissions(
        &self,
        acting_account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
    ) -> Result<Option<AppPermissionOutcome>> {
        let Some(row) = self
            .repository
            .get_instance_for_work_directory(instance_id, &work_directory.id)
            .await?
        else {
            return Ok(None);
        };
        let instance = instance_summary(row)?;

        if !self
            .repository
            .revoke_capability_grant(&instance.instance_id, acting_account_id)
            .await?
        {
            return Ok(Some(AppPermissionOutcome::InvalidState {
                instance_status: instance.status,
            }));
        }

        let refreshed = self
            .repository
            .get_instance(&instance.instance_id)
            .await?
            .context("App Instance missing after grant revocation")?;
        let refreshed = instance_summary(refreshed)?;
        Ok(Some(AppPermissionOutcome::Applied(Box::new(
            self.permission_status_for(&refreshed).await?,
        ))))
    }

    async fn permission_status_for(
        &self,
        instance: &AppInstanceSummary,
    ) -> Result<AppPermissionStatus> {
        let definition = self
            .repository
            .get_definition_permissions(&instance.definition_id)
            .await?
            .context("pinned App Definition missing while reading permissions")?;
        let capabilities = stored_capabilities(&definition.requested_capabilities_json)?;
        let grant = self
            .repository
            .get_capability_grant(&instance.instance_id)
            .await?;

        let state = permission_state(
            !capabilities.is_empty(),
            grant
                .as_ref()
                .map(|grant| (grant.decision.as_str(), grant.revoked_at.as_deref())),
        );

        Ok(AppPermissionStatus {
            request: permission_request(instance, capabilities),
            state,
            instance_status: instance.status,
            decided_by_account_id: grant
                .as_ref()
                .map(|grant| grant.decided_by_account_id.clone()),
            decided_at: grant.as_ref().map(|grant| grant.decided_at.clone()),
            revoked_at: grant.and_then(|grant| grant.revoked_at),
        })
    }

    pub async fn list_instances(
        &self,
        work_directory: &AuthorizedWorkDirectory,
    ) -> Result<AppInstanceList> {
        let rows = self
            .repository
            .list_instances_for_work_directory(&work_directory.id)
            .await?;
        let instances = rows
            .into_iter()
            .map(instance_summary)
            .collect::<Result<Vec<_>>>()?;
        Ok(AppInstanceList {
            work_directory_id: work_directory.id.clone(),
            instances,
        })
    }

    /// Render the App Instance presentation as one self-contained HTML
    /// document from the immutable artifact. Delivery rides the normal
    /// authenticated API; no unauthenticated URL for App content exists.
    pub async fn presentation_document(
        &self,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
    ) -> Result<Option<String>> {
        let Some(instance) = self
            .repository
            .get_instance_for_work_directory(instance_id, &work_directory.id)
            .await?
        else {
            return Ok(None);
        };
        // Only an active Instance may present. An Instance awaiting a decision,
        // denied, or revoked fails closed here with no separate check needed.
        if instance.status != STATUS_ACTIVE {
            return Ok(None);
        }
        let Some(definition) = self
            .repository
            .get_definition_assets(&instance.definition_id)
            .await?
        else {
            return Ok(None);
        };
        let document = super::inline::render_inlined_document(&self.artifacts, &definition)
            .await
            .context("rendering inlined App presentation document")?;
        Ok(Some(document))
    }
}

/// Translate the package crate's validated request into the canonical
/// protocol request, attaching human summaries from the immutable operations
/// table when publication has resolved them.
fn capability_requests(
    manifest: &ValidatedManifest,
    operations: &[ResolvedOperation],
) -> Vec<AppCapabilityRequest> {
    manifest
        .capabilities
        .iter()
        .map(|capability| match capability {
            oqto_apps::AppCapabilityRequest::Files(files) => AppCapabilityRequest::Files {
                resources: files
                    .resources
                    .iter()
                    .map(|resource| AppFileResourceRequest {
                        role: resource.role.clone(),
                        path: portable_path(&resource.path),
                        access: match resource.access {
                            oqto_apps::AppFileAccess::Read => AppFileAccess::Read,
                            oqto_apps::AppFileAccess::ReadWrite => AppFileAccess::ReadWrite,
                        },
                        watch: resource.watch,
                    })
                    .collect(),
            },
            oqto_apps::AppCapabilityRequest::Operations(request) => {
                AppCapabilityRequest::Operations {
                    operations: request
                        .ids
                        .iter()
                        .map(|id| AppOperationRequest {
                            id: id.clone(),
                            summary: operations
                                .iter()
                                .find(|operation| &operation.id == id)
                                .and_then(|operation| operation.summary.clone()),
                        })
                        .collect(),
                }
            }
            oqto_apps::AppCapabilityRequest::Theme => AppCapabilityRequest::Theme,
            oqto_apps::AppCapabilityRequest::Kv => AppCapabilityRequest::Kv,
        })
        .collect()
}

fn stored_capabilities(json: &str) -> Result<Vec<AppCapabilityRequest>> {
    serde_json::from_str(json).context("decoding the pinned App capability request")
}

fn permission_request(
    instance: &AppInstanceSummary,
    capabilities: Vec<AppCapabilityRequest>,
) -> AppPermissionRequest {
    AppPermissionRequest {
        instance_id: instance.instance_id.clone(),
        definition_id: instance.definition_id.clone(),
        content_digest: instance.content_digest.clone(),
        app_id: instance.app_id.clone(),
        title: instance.title.clone(),
        version: instance.version.clone(),
        capabilities,
    }
}

fn instance_summary(row: super::models::AppInstanceRow) -> Result<AppInstanceSummary> {
    let title = serde_json::from_str::<AppLocalizedTitle>(&row.title_json)
        .context("decoding stored App title")?;
    Ok(AppInstanceSummary {
        instance_id: row.instance_id,
        definition_id: row.definition_id,
        installation_id: row.installation_id,
        app_id: row.app_id,
        title,
        version: row.version,
        content_digest: row.content_digest,
        installation_owner_kind: owner_kind(&row.installation_owner_kind)?,
        binding_kind: owner_kind(&row.binding_kind)?,
        status: match row.status.as_str() {
            STATUS_ACTIVE => AppInstanceStatus::Active,
            STATUS_AWAITING_PERMISSION => AppInstanceStatus::AwaitingPermission,
            STATUS_SUSPENDED => AppInstanceStatus::Suspended,
            "unavailable" => AppInstanceStatus::Unavailable,
            other => return Err(anyhow!("unknown stored App Instance status {other:?}")),
        },
        presentations: vec![AppPresentationSummary {
            presentation_id: "main".to_owned(),
            kind: AppPresentationKind::SandboxedWeb,
        }],
    })
}

fn owner_kind(value: &str) -> Result<AppOwnerKind> {
    match value {
        "work_directory" => Ok(AppOwnerKind::WorkDirectory),
        "workspace" => Ok(AppOwnerKind::Workspace),
        "account" => Ok(AppOwnerKind::Account),
        "deployment" => Ok(AppOwnerKind::Deployment),
        other => Err(anyhow!("unknown stored App owner kind {other:?}")),
    }
}

fn rejection_from_package(error: &AppPackageError) -> AppRejection {
    AppRejection {
        code: rejection_code(error.code),
        message: error.message.clone(),
        relative_path: error.relative_path.as_deref().map(portable_path),
    }
}

fn rejection_code(code: AppPackageErrorCode) -> AppRejectionCode {
    match code {
        AppPackageErrorCode::SourceUnavailable => AppRejectionCode::SourceUnavailable,
        AppPackageErrorCode::TooManyPackages => AppRejectionCode::TooManyPackages,
        AppPackageErrorCode::ManifestMissing => AppRejectionCode::ManifestMissing,
        AppPackageErrorCode::ManifestTooLarge => AppRejectionCode::ManifestTooLarge,
        AppPackageErrorCode::ManifestInvalidUtf8 => AppRejectionCode::ManifestInvalidUtf8,
        AppPackageErrorCode::ManifestInvalidToml => AppRejectionCode::ManifestInvalidToml,
        AppPackageErrorCode::UnsupportedSchema => AppRejectionCode::UnsupportedSchema,
        AppPackageErrorCode::InvalidAppId => AppRejectionCode::InvalidAppId,
        AppPackageErrorCode::PackageIdMismatch => AppRejectionCode::PackageIdMismatch,
        AppPackageErrorCode::InvalidVersion => AppRejectionCode::InvalidVersion,
        AppPackageErrorCode::InvalidTitle => AppRejectionCode::InvalidTitle,
        AppPackageErrorCode::UnsupportedPresentation => AppRejectionCode::UnsupportedPresentation,
        AppPackageErrorCode::InvalidPresentation => AppRejectionCode::InvalidPresentation,
        AppPackageErrorCode::CapabilitiesNotSupported => AppRejectionCode::CapabilitiesNotSupported,
        AppPackageErrorCode::UnknownCapability => AppRejectionCode::UnknownCapability,
        AppPackageErrorCode::MissingCapabilityTable => AppRejectionCode::MissingCapabilityTable,
        AppPackageErrorCode::OrphanCapabilityTable => AppRejectionCode::OrphanCapabilityTable,
        AppPackageErrorCode::InvalidCapabilityRequest => AppRejectionCode::InvalidCapabilityRequest,
        AppPackageErrorCode::CapabilityResourceRejected => {
            AppRejectionCode::CapabilityResourceRejected
        }
        AppPackageErrorCode::OperationsTableMissing => AppRejectionCode::OperationsTableMissing,
        AppPackageErrorCode::OperationsTableInvalid => AppRejectionCode::OperationsTableInvalid,
        AppPackageErrorCode::OperationMissing => AppRejectionCode::OperationMissing,
        AppPackageErrorCode::OperationExecutableMissing => {
            AppRejectionCode::OperationExecutableMissing
        }
        AppPackageErrorCode::UnsupportedBinding => AppRejectionCode::UnsupportedBinding,
        AppPackageErrorCode::InvalidRelativePath => AppRejectionCode::InvalidRelativePath,
        AppPackageErrorCode::SymlinkRejected => AppRejectionCode::SymlinkRejected,
        AppPackageErrorCode::EntryMissing => AppRejectionCode::EntryMissing,
        AppPackageErrorCode::TooManyFiles => AppRejectionCode::TooManyFiles,
        AppPackageErrorCode::FileTooLarge => AppRejectionCode::FileTooLarge,
        AppPackageErrorCode::BundleTooLarge => AppRejectionCode::BundleTooLarge,
        AppPackageErrorCode::FileChangedWhileReading => AppRejectionCode::FileChangedWhileReading,
    }
}

fn rejection_code_name(code: AppRejectionCode) -> &'static str {
    match code {
        AppRejectionCode::SourceUnavailable => "source_unavailable",
        AppRejectionCode::TooManyPackages => "too_many_packages",
        AppRejectionCode::ManifestMissing => "manifest_missing",
        AppRejectionCode::ManifestTooLarge => "manifest_too_large",
        AppRejectionCode::ManifestInvalidUtf8 => "manifest_invalid_utf8",
        AppRejectionCode::ManifestInvalidToml => "manifest_invalid_toml",
        AppRejectionCode::UnsupportedSchema => "unsupported_schema",
        AppRejectionCode::InvalidAppId => "invalid_app_id",
        AppRejectionCode::PackageIdMismatch => "package_id_mismatch",
        AppRejectionCode::InvalidVersion => "invalid_version",
        AppRejectionCode::InvalidTitle => "invalid_title",
        AppRejectionCode::UnsupportedPresentation => "unsupported_presentation",
        AppRejectionCode::InvalidPresentation => "invalid_presentation",
        AppRejectionCode::CapabilitiesNotSupported => "capabilities_not_supported",
        AppRejectionCode::UnknownCapability => "unknown_capability",
        AppRejectionCode::MissingCapabilityTable => "missing_capability_table",
        AppRejectionCode::OrphanCapabilityTable => "orphan_capability_table",
        AppRejectionCode::InvalidCapabilityRequest => "invalid_capability_request",
        AppRejectionCode::CapabilityResourceRejected => "capability_resource_rejected",
        AppRejectionCode::OperationsTableMissing => "operations_table_missing",
        AppRejectionCode::OperationsTableInvalid => "operations_table_invalid",
        AppRejectionCode::OperationMissing => "operation_missing",
        AppRejectionCode::OperationExecutableMissing => "operation_executable_missing",
        AppRejectionCode::PermissionRequestStale => "permission_request_stale",
        AppRejectionCode::PermissionStateInvalid => "permission_state_invalid",
        AppRejectionCode::UnsupportedBinding => "unsupported_binding",
        AppRejectionCode::InvalidRelativePath => "invalid_relative_path",
        AppRejectionCode::SymlinkRejected => "symlink_rejected",
        AppRejectionCode::EntryMissing => "entry_missing",
        AppRejectionCode::TooManyFiles => "too_many_files",
        AppRejectionCode::FileTooLarge => "file_too_large",
        AppRejectionCode::BundleTooLarge => "bundle_too_large",
        AppRejectionCode::FileChangedWhileReading => "file_changed_while_reading",
        AppRejectionCode::WorkDirectoryUnavailable => "work_directory_unavailable",
        AppRejectionCode::Unauthorized => "unauthorized",
        AppRejectionCode::ArtifactWriteFailed => "artifact_write_failed",
        AppRejectionCode::WorkflowFailed => "workflow_failed",
        AppRejectionCode::InstanceUnavailable => "instance_unavailable",
        AppRejectionCode::LaunchUnavailable => "launch_unavailable",
        AppRejectionCode::DataCopyUnsupported => "data_copy_unsupported",
    }
}

fn portable_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn valid_app_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && bytes.last() != Some(&b'-')
        && !value.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PINNED: &str = "pinned-digest";

    #[test]
    fn zero_capability_apps_have_nothing_to_decide() {
        assert_eq!(
            decision_gate(false, PINNED, PINNED, AppInstanceStatus::Active),
            DecisionGate::NotRequired
        );
        assert_eq!(
            permission_state(false, None),
            AppPermissionState::NotRequired
        );
    }

    #[test]
    fn a_decision_must_name_the_definition_that_is_pinned() {
        assert_eq!(
            decision_gate(
                true,
                "reviewed-something-else",
                PINNED,
                AppInstanceStatus::AwaitingPermission
            ),
            DecisionGate::Stale,
            "approving a Definition the person never saw must be refused"
        );
        assert_eq!(
            decision_gate(true, PINNED, PINNED, AppInstanceStatus::AwaitingPermission),
            DecisionGate::Proceed
        );
    }

    #[test]
    fn only_undecided_or_withdrawn_instances_accept_a_decision() {
        assert_eq!(
            decision_gate(true, PINNED, PINNED, AppInstanceStatus::AwaitingPermission),
            DecisionGate::Proceed
        );
        // Re-granting after a denial or revocation is legitimate.
        assert_eq!(
            decision_gate(true, PINNED, PINNED, AppInstanceStatus::Suspended),
            DecisionGate::Proceed
        );
        assert_eq!(
            decision_gate(true, PINNED, PINNED, AppInstanceStatus::Active),
            DecisionGate::InvalidState
        );
        assert_eq!(
            decision_gate(true, PINNED, PINNED, AppInstanceStatus::Unavailable),
            DecisionGate::InvalidState
        );
    }

    #[test]
    fn grant_rows_project_to_the_state_a_person_sees() {
        assert_eq!(
            permission_state(true, None),
            AppPermissionState::AwaitingDecision
        );
        assert_eq!(
            permission_state(true, Some(("allowed", None))),
            AppPermissionState::Allowed
        );
        assert_eq!(
            permission_state(true, Some(("allowed", Some("2026-09-01T00:00:00Z")))),
            AppPermissionState::Revoked
        );
        assert_eq!(
            permission_state(true, Some(("denied", None))),
            AppPermissionState::Denied
        );
    }

    #[test]
    fn stored_capabilities_round_trip_the_pinned_request() -> Result<()> {
        let capabilities = vec![
            AppCapabilityRequest::Files {
                resources: vec![AppFileResourceRequest {
                    role: "outputs".to_owned(),
                    path: "outputs".to_owned(),
                    access: AppFileAccess::Read,
                    watch: true,
                }],
            },
            AppCapabilityRequest::Theme,
        ];
        let json = serde_json::to_string(&capabilities)?;
        assert_eq!(stored_capabilities(&json)?, capabilities);
        assert!(stored_capabilities("[]")?.is_empty());
        Ok(())
    }
}

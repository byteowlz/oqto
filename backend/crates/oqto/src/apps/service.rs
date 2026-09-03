use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use oqto_apps::{
    AppCandidateStatus, AppPackageError, AppPackageErrorCode, BundleLimits, ResolvedOperation,
    ValidatedManifest, discover_candidates, parse_manifest, parse_operations_table,
    snapshot_bundle,
};
use oqto_protocol::apps::{
    AppCandidateList, AppCandidateState, AppCandidateSummary, AppCapabilityRequest,
    AppContextActionRequest, AppContextTopicRequest, AppFileAccess, AppFileResourceRequest,
    AppInstanceList, AppInstanceStatus, AppInstanceSummary, AppLocalizedTitle, AppOperationRequest,
    AppOperationResult, AppOwnerKind, AppPermissionDecision, AppPermissionRequest,
    AppPermissionState, AppPermissionStatus, AppPresentationKind, AppPresentationSummary,
    AppPublishResult, AppPublishStatus, AppRejection, AppRejectionCode,
};
use sha2::{Digest, Sha256};

use crate::user_plane::{AppOperationExecution, UserPlane};

use super::artifact::AppArtifactStore;
use super::models::{DefinitionInsert, PublicationInsert, StoredBundleFile};
use super::repository::{AppRepository, PermissionDecisionInsert};
use super::source::UserPlaneAppSource;

/// Instance status strings used by the durable schema.
const STATUS_ACTIVE: &str = "active";
const STATUS_AWAITING_PERMISSION: &str = "awaiting_permission";
const STATUS_SUSPENDED: &str = "suspended";

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppGrantedFileResource {
    pub role: String,
    pub reference: String,
    pub label: String,
    pub access: &'static str,
    pub kind: &'static str,
    pub watch: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppFileEntry {
    pub reference: String,
    pub label: String,
    pub media_type: String,
    pub version: String,
    pub size: u64,
    pub is_directory: bool,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppFileWriteResult {
    pub written: bool,
    pub version: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppFileContents {
    pub reference: String,
    pub label: String,
    pub media_type: String,
    pub version: String,
    pub size: u64,
    pub modified_at: Option<String>,
    pub bytes_base64: String,
}

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
                    requested_capabilities: capability_requests(&manifest, &[], None),
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
        let capabilities = capability_requests(
            &snapshot.manifest,
            &snapshot.operations,
            snapshot.agent_context.as_ref(),
        );
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
    async fn authorize_kv(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
    ) -> Result<bool> {
        let Some(instance) = self
            .repository
            .get_instance_for_work_directory(instance_id, &work_directory.id)
            .await?
        else {
            return Ok(false);
        };
        if instance.status != STATUS_ACTIVE {
            return Ok(false);
        }
        let Some(grant) = self.repository.get_capability_grant(instance_id).await? else {
            return Ok(false);
        };
        if grant.decision != "allowed"
            || grant.revoked_at.is_some()
            || grant.decided_by_account_id != account_id
            || grant.definition_id != instance.definition_id
            || grant.content_digest != instance.content_digest
        {
            return Ok(false);
        }
        let capabilities = stored_capabilities(&grant.request_json)?;
        Ok(capabilities
            .iter()
            .any(|capability| matches!(capability, AppCapabilityRequest::Kv)))
    }

    pub async fn kv_get(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
        key: &str,
    ) -> Result<Option<Option<serde_json::Value>>> {
        validate_kv_key(key)?;
        if !self
            .authorize_kv(account_id, work_directory, instance_id)
            .await?
        {
            return Ok(None);
        }
        Ok(Some(
            self.repository
                .get_instance_kv(instance_id, account_id, key)
                .await?,
        ))
    }

    pub async fn kv_set(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<bool> {
        validate_kv_key(key)?;
        validate_kv_value(value)?;
        if !self
            .authorize_kv(account_id, work_directory, instance_id)
            .await?
        {
            return Ok(false);
        }
        self.repository
            .set_instance_kv(instance_id, account_id, key, value)
            .await?;
        Ok(true)
    }

    pub async fn kv_delete(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
        key: &str,
    ) -> Result<bool> {
        validate_kv_key(key)?;
        if !self
            .authorize_kv(account_id, work_directory, instance_id)
            .await?
        {
            return Ok(false);
        }
        self.repository
            .delete_instance_kv(instance_id, account_id, key)
            .await?;
        Ok(true)
    }

    async fn authorized_file_resources(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
    ) -> Result<Option<Vec<AppFileResourceRequest>>> {
        let Some(instance) = self
            .repository
            .get_instance_for_work_directory(instance_id, &work_directory.id)
            .await?
        else {
            return Ok(None);
        };
        if instance.status != STATUS_ACTIVE {
            return Ok(None);
        }
        let Some(grant) = self.repository.get_capability_grant(instance_id).await? else {
            return Ok(None);
        };
        if grant.decision != "allowed"
            || grant.revoked_at.is_some()
            || grant.decided_by_account_id != account_id
            || grant.definition_id != instance.definition_id
            || grant.content_digest != instance.content_digest
        {
            return Ok(None);
        }
        let resources = stored_capabilities(&grant.request_json)?
            .into_iter()
            .find_map(|capability| match capability {
                AppCapabilityRequest::Files { resources } => Some(resources),
                _ => None,
            });
        Ok(resources)
    }

    pub async fn file_resources(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
    ) -> Result<Option<Vec<AppGrantedFileResource>>> {
        Ok(self
            .authorized_file_resources(account_id, work_directory, instance_id)
            .await?
            .map(|resources| {
                resources
                    .into_iter()
                    .map(|resource| AppGrantedFileResource {
                        reference: resource.role.clone(),
                        label: resource.role.clone(),
                        role: resource.role,
                        kind: if Path::new(&resource.path).extension().is_some() {
                            "document"
                        } else {
                            "collection"
                        },
                        access: match resource.access {
                            AppFileAccess::Read => "read",
                            AppFileAccess::ReadWrite => "readwrite",
                        },
                        watch: resource.watch,
                    })
                    .collect()
            }))
    }

    async fn resolve_file_reference(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
        reference: &str,
        require_write: bool,
    ) -> Result<Option<(PathBuf, PathBuf)>> {
        let mut segments = reference.split('/');
        let role = segments.next().unwrap_or_default();
        let relative = segments.collect::<PathBuf>();
        if role.is_empty()
            || reference.len() > 2048
            || relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Ok(None);
        }
        let Some(resource) = self
            .authorized_file_resources(account_id, work_directory, instance_id)
            .await?
            .and_then(|resources| resources.into_iter().find(|resource| resource.role == role))
        else {
            return Ok(None);
        };
        if require_write && resource.access != AppFileAccess::ReadWrite {
            return Ok(None);
        }
        let resource_root = work_directory.root.join(resource.path);
        Ok(Some((resource_root.join(relative), resource_root)))
    }

    pub async fn file_list(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
        reference: &str,
    ) -> Result<Option<Vec<AppFileEntry>>> {
        let Some((path, resource_root)) = self
            .resolve_file_reference(account_id, work_directory, instance_id, reference, false)
            .await?
        else {
            return Ok(None);
        };
        let canonical_resource = match resource_root.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Some(Vec::new()));
            }
            Err(error) => return Err(error).context("canonicalizing App resource root"),
        };
        let canonical = match path.canonicalize() {
            Ok(path) if path.starts_with(&canonical_resource) => path,
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Some(Vec::new()));
            }
            Err(error) => return Err(error).context("canonicalizing App file collection"),
        };
        let entries = work_directory
            .plane
            .list_directory(&canonical, false)
            .await?;
        let mut result = Vec::new();
        for entry in entries.into_iter().take(256) {
            if entry.is_symlink
                || entry.name.contains('/')
                || entry.name == "."
                || entry.name == ".."
            {
                continue;
            }
            let child_ref = format!("{reference}/{}", entry.name);
            let child_path = canonical.join(&entry.name);
            let version = file_version(&work_directory.plane, &child_path, entry.is_dir).await?;
            result.push(AppFileEntry {
                reference: child_ref,
                label: entry.name.clone(),
                media_type: if entry.is_dir {
                    "inode/directory".to_owned()
                } else {
                    media_type(&entry.name)
                },
                version,
                size: entry.size,
                is_directory: entry.is_dir,
                modified_at: Some(entry.modified_at.to_string()),
            });
        }
        Ok(Some(result))
    }

    pub async fn file_read(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
        reference: &str,
    ) -> Result<Option<AppFileContents>> {
        use base64::Engine;
        let Some((path, resource_root)) = self
            .resolve_file_reference(account_id, work_directory, instance_id, reference, false)
            .await?
        else {
            return Ok(None);
        };
        let canonical_resource = resource_root
            .canonicalize()
            .context("canonicalizing App resource root")?;
        let canonical = path.canonicalize().context("canonicalizing App file")?;
        if !canonical.starts_with(canonical_resource) {
            return Ok(None);
        }
        let content = work_directory
            .plane
            .read_file(&canonical, None, Some(8 * 1024 * 1024))
            .await?;
        if content.truncated {
            anyhow::bail!("App file exceeds the 8 MiB read limit");
        }
        let stat = work_directory.plane.stat(&canonical).await?;
        let label = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(reference)
            .to_owned();
        let version = format!("sha256:{}", hex::encode(Sha256::digest(&content.content)));
        Ok(Some(AppFileContents {
            reference: reference.to_owned(),
            media_type: media_type(&label),
            label,
            version,
            size: content.size,
            modified_at: Some(stat.modified_at.to_string()),
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(content.content),
        }))
    }

    pub async fn file_write(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
        reference: &str,
        expected_version: &str,
        bytes: &[u8],
    ) -> Result<Option<AppFileWriteResult>> {
        if bytes.len() > 8 * 1024 * 1024 {
            anyhow::bail!("App file exceeds the 8 MiB write limit");
        }
        let Some((path, resource_root)) = self
            .resolve_file_reference(account_id, work_directory, instance_id, reference, true)
            .await?
        else {
            return Ok(None);
        };
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("App file has no parent"))?;
        work_directory.plane.create_directory(parent, true).await?;
        let canonical_parent = parent
            .canonicalize()
            .context("canonicalizing App file parent")?;
        let canonical_resource = resource_root
            .canonicalize()
            .context("canonicalizing App resource root")?;
        if !canonical_parent.starts_with(canonical_resource) {
            return Ok(None);
        }
        let current = match work_directory.plane.stat(&path).await {
            Ok(stat) if stat.exists && stat.is_file => {
                file_version(&work_directory.plane, &path, false).await?
            }
            _ => "missing".to_owned(),
        };
        if current != expected_version {
            return Ok(Some(AppFileWriteResult {
                written: false,
                version: current,
            }));
        }
        work_directory.plane.write_file(&path, bytes, false).await?;
        let version = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
        Ok(Some(AppFileWriteResult {
            written: true,
            version,
        }))
    }

    pub async fn invoke_operation(
        &self,
        account_id: &str,
        work_directory: &AuthorizedWorkDirectory,
        instance_id: &str,
        operation_id: &str,
        input: &serde_json::Value,
    ) -> Result<Option<AppOperationResult>> {
        let Some(instance) = self
            .repository
            .get_instance_for_work_directory(instance_id, &work_directory.id)
            .await?
        else {
            return Ok(None);
        };
        if instance.status != STATUS_ACTIVE {
            return Ok(None);
        }
        let Some(grant) = self.repository.get_capability_grant(instance_id).await? else {
            return Ok(None);
        };
        if grant.decision != "allowed"
            || grant.revoked_at.is_some()
            || grant.decided_by_account_id != account_id
            || grant.definition_id != instance.definition_id
            || grant.content_digest != instance.content_digest
        {
            return Ok(None);
        }
        let capabilities = stored_capabilities(&grant.request_json)?;
        if !capabilities.iter().any(|capability| {
            matches!(
                capability,
                AppCapabilityRequest::Operations { operations }
                    if operations.iter().any(|operation| operation.id == operation_id)
            )
        }) {
            return Ok(None);
        }

        let definition = self
            .artifacts
            .definition_dir(&instance.content_digest)
            .ok_or_else(|| anyhow!("invalid pinned App Definition digest"))?;
        let manifest_bytes = tokio::fs::read(definition.join("oqto-app.toml"))
            .await
            .context("reading pinned App manifest")?;
        let manifest = parse_manifest(
            &format!("{}.oqtoapp", instance.app_id),
            &manifest_bytes,
            self.limits.max_manifest_bytes,
        )
        .map_err(|error| anyhow!(error))?;
        let request = manifest
            .operations_request()
            .ok_or_else(|| anyhow!("granted App Definition has no operations request"))?;
        let table_bytes = tokio::fs::read(definition.join(&request.table))
            .await
            .context("reading pinned App operations table")?;
        let table =
            parse_operations_table(&table_bytes, self.limits.max_file_bytes, &request.table)
                .map_err(|error| anyhow!(error))?;
        let operation = table
            .resolve(&[operation_id.to_owned()])
            .map_err(|error| anyhow!(error))?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("granted operation is absent from its pinned table"))?;
        let validated_input = operation
            .validate_input(input)
            .map_err(|error| anyhow!(error))?;
        let result = work_directory
            .plane
            .run_app_operation(AppOperationExecution {
                content_digest: instance.content_digest,
                app_id: instance.app_id,
                operation_id: operation.id,
                work_directory: work_directory.root.clone(),
                input: validated_input,
            })
            .await
            .context("executing pinned App operation")?;
        Ok(Some(AppOperationResult {
            ok: result.success,
            code: result.code,
            message: result.message,
            output: result.output,
        }))
    }

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
    agent_context: Option<&oqto_apps::AgentContextCatalog>,
) -> Vec<AppCapabilityRequest> {
    manifest
        .capabilities
        .iter()
        .filter_map(|capability| match capability {
            oqto_apps::AppCapabilityRequest::Files(files) => Some(AppCapabilityRequest::Files {
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
            }),
            oqto_apps::AppCapabilityRequest::Operations(request) => {
                Some(AppCapabilityRequest::Operations {
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
                })
            }
            // Theme and Presentation Context are baseline Host presentation
            // environment, never grant-bearing authority.
            oqto_apps::AppCapabilityRequest::Theme => None,
            oqto_apps::AppCapabilityRequest::Kv => Some(AppCapabilityRequest::Kv),
            oqto_apps::AppCapabilityRequest::AgentContext(_) => {
                Some(AppCapabilityRequest::AgentContext {
                    topics: agent_context
                        .map(|catalog| catalog.topics.as_slice())
                        .unwrap_or_default()
                        .iter()
                        .map(|topic| AppContextTopicRequest {
                            id: topic.id.clone(),
                            title: topic.title.clone(),
                            disclosure: format!("{:?}", topic.disclosure).to_lowercase(),
                        })
                        .collect(),
                    actions: agent_context
                        .map(|catalog| catalog.actions.as_slice())
                        .unwrap_or_default()
                        .iter()
                        .map(|action| AppContextActionRequest {
                            id: action.id.clone(),
                            title: action.title.clone(),
                            requires_user_activation: action.requires_user_activation,
                        })
                        .collect(),
                })
            }
        })
        .collect()
}

async fn file_version(plane: &Arc<dyn UserPlane>, path: &Path, directory: bool) -> Result<String> {
    if directory {
        let stat = plane.stat(path).await?;
        return Ok(format!("dir:{}:{}", stat.modified_at, stat.size));
    }
    let content = plane.read_file(path, None, Some(8 * 1024 * 1024)).await?;
    if content.truncated {
        return Ok(format!("large:{}", content.size));
    }
    Ok(format!(
        "sha256:{}",
        hex::encode(Sha256::digest(&content.content))
    ))
}

fn media_type(name: &str) -> String {
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "json" => "application/json",
        "jsonl" => "application/x-ndjson",
        "txt" | "log" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_owned()
}

fn validate_kv_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.len() > 256
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
    {
        return Err(anyhow!(
            "App KV key must contain 1-256 ASCII letters, digits, '.', '_', '-', or '/'"
        ));
    }
    Ok(())
}

fn validate_kv_value(value: &serde_json::Value) -> Result<()> {
    const MAX_KV_BYTES: usize = 65_536;
    const MAX_DEPTH: usize = 32;
    fn depth(value: &serde_json::Value, current: usize) -> bool {
        if current > MAX_DEPTH {
            return false;
        }
        match value {
            serde_json::Value::Array(values) => {
                values.iter().all(|value| depth(value, current + 1))
            }
            serde_json::Value::Object(values) => {
                values.values().all(|value| depth(value, current + 1))
            }
            _ => true,
        }
    }
    if !depth(value, 0) {
        return Err(anyhow!("App KV value exceeds maximum nesting depth"));
    }
    if serde_json::to_vec(value)?.len() > MAX_KV_BYTES {
        return Err(anyhow!("App KV value exceeds {MAX_KV_BYTES} bytes"));
    }
    Ok(())
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

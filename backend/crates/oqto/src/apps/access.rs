use std::path::PathBuf;
use std::sync::Arc;

use oqto_protocol::apps::AppOwnerKind;

use crate::api::handlers::trx::validate_workspace_path;
use crate::api::{ApiError, AppState};
use crate::runner::router::{
    ExecutionTarget, resolve_runner_for_workspace_path, resolve_target_for_workspace_path,
};
use crate::user_plane::{MeteredUserPlane, RunnerUserPlane, UserPlane, UserPlanePath};

use super::{AppRepository, AuthorizedWorkDirectory};

/// Resolve one authenticated work-directory request into an opaque public id
/// plus a runner-mediated filesystem plane. The caller never selects a
/// Principal or Workspace owner independently from the path authorization.
pub async fn authorize_work_directory(
    state: &AppState,
    repository: &AppRepository,
    account_id: &str,
    workspace_path: &str,
) -> Result<AuthorizedWorkDirectory, ApiError> {
    let canonical = validate_workspace_path(state, account_id, workspace_path).await?;
    let target = resolve_target_for_workspace_path(state, account_id, workspace_path)
        .await
        .map_err(|error| ApiError::forbidden(format!("Work-directory access denied: {error:#}")))?;
    let runner = resolve_runner_for_workspace_path(state, account_id, workspace_path)
        .await
        .map_err(|error| {
            ApiError::internal(format!(
                "Failed to resolve work-directory runner: {error:#}"
            ))
        })?
        .ok_or_else(|| ApiError::service_unavailable("Work-directory runner unavailable"))?;

    let (owner_kind, owner_id) = match target {
        ExecutionTarget::Personal => (AppOwnerKind::Account, account_id.to_owned()),
        ExecutionTarget::SharedWorkspace { workspace_id } => {
            (AppOwnerKind::Workspace, workspace_id)
        }
    };
    let owner_kind_db = match owner_kind {
        AppOwnerKind::Account => "account",
        AppOwnerKind::Workspace => "workspace",
        AppOwnerKind::WorkDirectory | AppOwnerKind::Deployment => {
            return Err(ApiError::internal(
                "invalid resolved owner kind for a work directory",
            ));
        }
    };
    let canonical_string = canonical.to_string_lossy().into_owned();
    let work_directory_id = repository
        .upsert_work_directory(owner_kind_db, &owner_id, &canonical_string)
        .await
        .map_err(|error| {
            ApiError::internal(format!(
                "Failed to register work-directory identity: {error:#}"
            ))
        })?;

    let base: Arc<dyn UserPlane> = Arc::new(RunnerUserPlane::new(runner));
    let plane: Arc<dyn UserPlane> = Arc::new(MeteredUserPlane::new(
        base,
        UserPlanePath::Runner,
        state.user_plane_metrics.clone(),
    ));

    Ok(AuthorizedWorkDirectory {
        id: work_directory_id,
        owner_kind,
        owner_id,
        // Runner file operations use the authenticated request's namespace;
        // `canonical` is a private identity binding fact and may be host-side.
        root: PathBuf::from(workspace_path),
        plane,
    })
}

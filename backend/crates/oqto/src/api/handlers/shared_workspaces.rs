//! Shared workspace API handlers.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use tracing::info;

use crate::auth::CurrentUser;
use crate::shared_workspace::{
    AddMemberRequest, AdminSharedWorkspaceInfo, ConvertToSharedRequest,
    CreateSharedWorkspaceRequest, CreateSharedWorkspaceWorkdirRequest, SharedWorkspaceInfo,
    SharedWorkspaceMemberInfo, TransferOwnershipRequest, UpdateMemberRequest,
    UpdateSharedWorkspaceRequest,
};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

// ============================================================================
// Shared workspace CRUD
// ============================================================================

/// Select principal provisioning from the placement decision.
fn provisioning_for(state: &AppState) -> crate::shared_workspace::WorkspaceProvisioning {
    match state.placement_manager.as_ref() {
        Some(manager) => crate::shared_workspace::WorkspaceProvisioning::ContainerVolume {
            state_root: manager.state_root().to_path_buf(),
        },
        None if state.linux_users.is_some() => {
            crate::shared_workspace::WorkspaceProvisioning::HostUser
        }
        None => crate::shared_workspace::WorkspaceProvisioning::HostDirectory,
    }
}

/// Container placement + EAVS provisioning for a freshly created workspace.
/// Fail-closed: if the container cannot be provisioned the workspace is
/// rolled back and an error returned.
async fn finish_container_placement(
    state: &AppState,
    service: &crate::shared_workspace::SharedWorkspaceService,
    workspace: &crate::shared_workspace::SharedWorkspace,
    user_id: &str,
) -> Result<(), ApiError> {
    let Some(manager) = state.placement_manager.as_ref() else {
        return Ok(());
    };

    // EAVS into the workspace volume BEFORE the container starts: with
    // userns=auto the volume is chowned into the container's subuid range at
    // start, after which the backend user can no longer write into it.
    // Best-effort: log warning if it fails.
    if let Some(eavs_client) = state.eavs_client.as_ref() {
        let sw_user_id = format!("shared-{}", workspace.id);
        let home = std::path::Path::new(&workspace.path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from(&workspace.path));
        match super::admin::provision_eavs_into_home(
            eavs_client,
            &home,
            &sw_user_id,
            Some(&state.auto_rename_config),
        )
        .await
        {
            Ok(_) => {
                tracing::info!(
                    workspace_id = %workspace.id,
                    home = %home.display(),
                    "provisioned EAVS key and models.json into workspace volume"
                );
            }
            Err(e) => {
                tracing::warn!(
                    workspace_id = %workspace.id,
                    error = %e,
                    "failed to provision EAVS into workspace volume (sessions won't have LLM access)"
                );
            }
        }
    }

    if let Err(e) = manager
        .provision(
            &workspace.id,
            user_id,
            std::path::PathBuf::from(&workspace.path),
        )
        .await
    {
        tracing::error!(
            workspace_id = %workspace.id,
            error = format!("{e:#}"),
            "failed to provision container placement; rolling back workspace"
        );
        if let Err(rollback) = service.delete(&workspace.id, user_id, false).await {
            tracing::error!(
                workspace_id = %workspace.id,
                error = %rollback,
                "rollback of unplaced workspace failed"
            );
        }
        return Err(ApiError::internal(format!(
            "failed to provision workspace container: {e:#}"
        )));
    }
    Ok(())
}

/// Host Linux users are removed only for host-placed workspaces in
/// multi-user mode; container placements never created one.
async fn should_remove_linux_user(state: &AppState, workspace_id: &str) -> bool {
    if state.linux_users.is_none() {
        return false;
    }
    match state.placement_store.as_ref() {
        Some(store) => match store.find_workspace(workspace_id).await {
            Ok(record) => record.is_none(),
            Err(e) => {
                tracing::warn!(
                    workspace_id,
                    error = %e,
                    "placement lookup failed; not removing Linux user"
                );
                false
            }
        },
        None => true,
    }
}

/// Create a new shared workspace.
pub async fn create_shared_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<CreateSharedWorkspaceRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let multi_user = state.linux_users.is_some();
    let container_placed = state.placement_manager.is_some();
    let workspace = service
        .create(&request, user.id(), provisioning_for(&state))
        .await
        .map_err(|e| ApiError::bad_request(format!("failed to create shared workspace: {}", e)))?;

    // Container placement (provision + rollback + EAVS into the volume) is
    // handled by finish_container_placement; host placement provisions EAVS
    // into the Linux user home below.
    finish_container_placement(&state, service, &workspace, user.id()).await?;

    if !container_placed
        && multi_user
        && let Some(eavs_client) = state.eavs_client.as_ref()
        && let Some(linux_users) = state.linux_users.as_ref()
    {
        let sw_user_id = format!("shared-{}", workspace.id);
        match super::admin::provision_eavs_for_user(
            eavs_client,
            linux_users,
            &workspace.linux_user,
            &sw_user_id,
            Some(&state.auto_rename_config),
        )
        .await
        {
            Ok(_) => {
                tracing::info!(
                    workspace_id = %workspace.id,
                    linux_user = %workspace.linux_user,
                    "provisioned EAVS key and models.json for shared workspace"
                );
            }
            Err(e) => {
                tracing::warn!(
                    workspace_id = %workspace.id,
                    error = %e,
                    "failed to provision EAVS for shared workspace (sessions won't have LLM access)"
                );
            }
        }
    }

    info!(
        workspace_id = %workspace.id,
        name = %workspace.name,
        user = %user.id(),
        "created shared workspace"
    );

    Ok(Json(serde_json::json!({
        "id": workspace.id,
        "name": workspace.name,
        "slug": workspace.slug,
        "path": workspace.path,
        "icon": workspace.icon,
        "color": workspace.color,
        "created_at": workspace.created_at,
    })))
}

/// List shared workspaces the current user has access to.
pub async fn list_shared_workspaces(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<SharedWorkspaceInfo>>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let workspaces = service
        .list_for_user(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("failed to list shared workspaces: {}", e)))?;

    Ok(Json(workspaces))
}

/// Get a shared workspace by ID.
pub async fn get_shared_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(workspace_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let (ws, role) = service
        .get(&workspace_id, user.id())
        .await
        .map_err(|e| ApiError::internal(format!("failed to get shared workspace: {}", e)))?
        .ok_or_else(|| ApiError::not_found("shared workspace not found or access denied"))?;

    Ok(Json(serde_json::json!({
        "id": ws.id,
        "name": ws.name,
        "slug": ws.slug,
        "path": ws.path,
        "owner_id": ws.owner_id,
        "description": ws.description,
        "icon": ws.icon,
        "color": ws.color,
        "created_at": ws.created_at,
        "updated_at": ws.updated_at,
        "my_role": role.to_string(),
    })))
}

/// Update a shared workspace.
pub async fn update_shared_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(workspace_id): Path<String>,
    Json(request): Json<UpdateSharedWorkspaceRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let ws = service
        .update(&workspace_id, user.id(), &request)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({
        "id": ws.id,
        "name": ws.name,
        "updated_at": ws.updated_at,
    })))
}

/// Delete a shared workspace.
pub async fn delete_shared_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(workspace_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let remove_linux_user = should_remove_linux_user(&state, &workspace_id).await;
    service
        .delete(&workspace_id, user.id(), remove_linux_user)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    if let Some(manager) = state.placement_manager.as_ref()
        && let Err(e) = manager.remove(&workspace_id).await
    {
        tracing::warn!(
            workspace_id = %workspace_id,
            error = %e,
            "failed to remove container placement for deleted workspace"
        );
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ============================================================================
// Member management
// ============================================================================

/// Migrate a host-placed shared workspace into container placement:
/// copy its durable state into a volume, repoint the path, provision the
/// container. The legacy Linux user home is left untouched for operator
/// cleanup after verification.
pub async fn admin_migrate_shared_workspace_placement(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;
    let manager = state.placement_manager.as_ref().ok_or_else(|| {
        ApiError::bad_request("container placement is not enabled ([placement] mode)")
    })?;
    if let Some(store) = state.placement_store.as_ref()
        && store
            .find_workspace(&workspace_id)
            .await
            .map_err(|e| ApiError::internal(format!("placement lookup failed: {e}")))?
            .is_some()
    {
        return Err(ApiError::bad_request(
            "workspace is already container-placed",
        ));
    }

    let legacy = service
        .admin_get(&workspace_id)
        .await
        .map_err(|e| ApiError::internal(format!("{e}")))?
        .ok_or_else(|| ApiError::not_found("workspace not found"))?;
    let legacy_path = legacy.path.clone();

    let migrated = service
        .migrate_to_container_volume(&workspace_id, manager.state_root())
        .await
        .map_err(|e| ApiError::bad_request(format!("migration failed: {e}")))?;

    if let Err(e) = manager
        .provision(
            &migrated.id,
            &migrated.owner_id,
            std::path::PathBuf::from(&migrated.path),
        )
        .await
    {
        tracing::error!(
            workspace_id = %migrated.id,
            error = %e,
            "container provisioning failed after migration copy; reverting"
        );
        let dest_home = manager.state_root().join(&migrated.id);
        if let Err(revert) = service
            .revert_container_migration(&migrated.id, &legacy_path, &dest_home)
            .await
        {
            tracing::error!(
                workspace_id = %migrated.id,
                error = %revert,
                "migration revert failed; workspace may need manual repair"
            );
        }
        return Err(ApiError::internal(format!(
            "failed to provision migrated workspace container: {e}"
        )));
    }

    info!(
        workspace_id = %migrated.id,
        path = %migrated.path,
        "migrated shared workspace to container placement"
    );
    Ok(Json(serde_json::json!({
        "migrated": true,
        "id": migrated.id,
        "path": migrated.path,
    })))
}

/// List members of a shared workspace.
pub async fn list_shared_workspace_members(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(workspace_id): Path<String>,
) -> ApiResult<Json<Vec<SharedWorkspaceMemberInfo>>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let members = service
        .list_members(&workspace_id, user.id())
        .await
        .map_err(|e| ApiError::internal(format!("failed to list members: {}", e)))?;

    Ok(Json(members))
}

/// Add a member to a shared workspace.
pub async fn add_shared_workspace_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(workspace_id): Path<String>,
    Json(request): Json<AddMemberRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    service
        .add_member(&workspace_id, user.id(), &request)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({ "added": true })))
}

/// Update a member's role in a shared workspace.
pub async fn update_shared_workspace_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((workspace_id, target_user_id)): Path<(String, String)>,
    Json(request): Json<UpdateMemberRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    service
        .update_member_role(&workspace_id, user.id(), &target_user_id, request.role)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({ "updated": true })))
}

/// Remove a member from a shared workspace.
pub async fn remove_shared_workspace_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((workspace_id, target_user_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    service
        .remove_member(&workspace_id, user.id(), &target_user_id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({ "removed": true })))
}

// ============================================================================
// Workdir management
// ============================================================================

/// List workdirs (project directories) in a shared workspace.
pub async fn list_shared_workspace_workdirs(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(workspace_id): Path<String>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    // Verify user has access
    let (ws, _role) = service
        .get(&workspace_id, user.id())
        .await
        .map_err(|e| ApiError::internal(format!("failed to get shared workspace: {}", e)))?
        .ok_or_else(|| ApiError::not_found("shared workspace not found or access denied"))?;

    // Scan the workspace directory for non-hidden subdirectories
    let ws_path = std::path::Path::new(&ws.path);
    let mut workdirs = Vec::new();

    if ws_path.is_dir()
        && let Ok(mut entries) = tokio::fs::read_dir(ws_path).await
    {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if let Ok(ft) = entry.file_type().await
                && !ft.is_dir()
            {
                continue;
            }
            let path = entry.path().to_string_lossy().to_string();
            workdirs.push(serde_json::json!({
                "name": name,
                "path": path,
            }));
        }
    }

    workdirs.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });

    Ok(Json(workdirs))
}

/// Add a workdir to a shared workspace by copying a source path.
pub async fn add_shared_workspace_workdir(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(workspace_id): Path<String>,
    Json(mut request): Json<CreateSharedWorkspaceWorkdirRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let validated = state
        .sessions
        .for_user(user.id())
        .validate_workspace_path(&request.source_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("invalid source path: {}", e)))?;
    request.source_path = validated.to_string_lossy().to_string();

    let multi_user = state.linux_users.is_some();
    let workdir_path = service
        .add_workdir_from_source(&workspace_id, user.id(), &request, multi_user)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({
        "workspace_id": workspace_id,
        "workdir_path": workdir_path,
    })))
}

#[derive(Debug, Deserialize)]
pub struct DeleteSharedWorkspaceWorkdirQuery {
    pub path: String,
}

/// Delete a workdir from a shared workspace.
pub async fn delete_shared_workspace_workdir(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(workspace_id): Path<String>,
    Query(query): Query<DeleteSharedWorkspaceWorkdirQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    service
        .remove_workdir(&workspace_id, user.id(), &query.path)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ============================================================================
// Convert and transfer
// ============================================================================

/// Convert a personal project to a shared workspace.
pub async fn convert_to_shared_workspace(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(mut request): Json<ConvertToSharedRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let validated = state
        .sessions
        .for_user(user.id())
        .validate_workspace_path(&request.source_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("invalid source path: {}", e)))?;
    request.source_path = validated.to_string_lossy().to_string();

    let workspace = service
        .convert_to_shared(&request, user.id(), provisioning_for(&state))
        .await
        .map_err(|e| ApiError::bad_request(format!("failed to convert: {}", e)))?;

    finish_container_placement(&state, service, &workspace, user.id()).await?;

    info!(
        workspace_id = %workspace.id,
        source = %request.source_path,
        user = %user.id(),
        "converted personal project to shared workspace"
    );

    Ok(Json(serde_json::json!({
        "id": workspace.id,
        "name": workspace.name,
        "slug": workspace.slug,
        "path": workspace.path,
        "icon": workspace.icon,
        "color": workspace.color,
        "created_at": workspace.created_at,
    })))
}

/// Transfer ownership of a shared workspace to another member.
pub async fn transfer_shared_workspace_ownership(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(workspace_id): Path<String>,
    Json(request): Json<TransferOwnershipRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    service
        .transfer_ownership(&workspace_id, user.id(), &request)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({ "transferred": true })))
}

// ============================================================================
// Admin handlers (require admin role -- enforced by route layer)
// ============================================================================

/// Admin: list all shared workspaces.
pub async fn admin_list_shared_workspaces(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<AdminSharedWorkspaceInfo>>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let workspaces = service
        .admin_list_all()
        .await
        .map_err(|e| ApiError::internal(format!("failed to list workspaces: {}", e)))?;

    Ok(Json(workspaces))
}

/// Admin: get shared workspace details including all members.
pub async fn admin_get_shared_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let ws = service
        .repo()
        .get_by_id(&workspace_id)
        .await
        .map_err(|e| ApiError::internal(format!("{}", e)))?
        .ok_or_else(|| ApiError::not_found("workspace not found"))?;

    let members = service
        .repo()
        .list_members(&workspace_id)
        .await
        .map_err(|e| ApiError::internal(format!("{}", e)))?;

    Ok(Json(serde_json::json!({
        "id": ws.id,
        "name": ws.name,
        "slug": ws.slug,
        "linux_user": ws.linux_user,
        "path": ws.path,
        "owner_id": ws.owner_id,
        "description": ws.description,
        "icon": ws.icon,
        "color": ws.color,
        "created_at": ws.created_at,
        "updated_at": ws.updated_at,
        "members": members,
    })))
}

/// Admin: force-delete a shared workspace.
pub async fn admin_delete_shared_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    let remove_linux_user = should_remove_linux_user(&state, &workspace_id).await;
    service
        .admin_force_delete(&workspace_id, remove_linux_user)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    if let Some(manager) = state.placement_manager.as_ref()
        && let Err(e) = manager.remove(&workspace_id).await
    {
        tracing::warn!(
            workspace_id = %workspace_id,
            error = %e,
            "failed to remove container placement for deleted workspace"
        );
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// Admin: force-transfer ownership.
pub async fn admin_transfer_shared_workspace_ownership(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Json(request): Json<TransferOwnershipRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    service
        .admin_force_transfer_ownership(&workspace_id, &request.new_owner_id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({ "transferred": true })))
}

/// Admin: force-remove a member.
pub async fn admin_remove_shared_workspace_member(
    State(state): State<AppState>,
    Path((workspace_id, target_user_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| ApiError::internal("shared workspaces not configured"))?;

    service
        .admin_force_remove_member(&workspace_id, &target_user_id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({ "removed": true })))
}

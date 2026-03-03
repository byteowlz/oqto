//! Shared workspace API handlers.

use axum::{
    Json,
    extract::{Path, State},
};
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
    let workspace = service
        .create(&request, user.id(), multi_user)
        .await
        .map_err(|e| ApiError::bad_request(format!("failed to create shared workspace: {}", e)))?;

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

    service
        .delete(&workspace_id, user.id())
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ============================================================================
// Member management
// ============================================================================

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

    if ws_path.is_dir() {
        if let Ok(mut entries) = tokio::fs::read_dir(ws_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if let Ok(ft) = entry.file_type().await {
                    if !ft.is_dir() {
                        continue;
                    }
                }
                let path = entry.path().to_string_lossy().to_string();
                workdirs.push(serde_json::json!({
                    "name": name,
                    "path": path,
                }));
            }
        }
    }

    workdirs.sort_by(|a, b| {
        a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or(""))
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

    let multi_user = state.linux_users.is_some();
    let workspace = service
        .convert_to_shared(&request, user.id(), multi_user)
        .await
        .map_err(|e| ApiError::bad_request(format!("failed to convert: {}", e)))?;

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

    service
        .admin_force_delete(&workspace_id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{}", e)))?;

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

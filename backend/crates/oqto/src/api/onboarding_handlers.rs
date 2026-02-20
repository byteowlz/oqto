//! Onboarding API handlers.
//!
//! Endpoints for managing user onboarding state:
//! - GET /onboarding - Get current onboarding state
//! - PUT /onboarding - Update onboarding state
//! - POST /onboarding/advance - Advance to next stage
//! - POST /onboarding/unlock/{component} - Unlock a UI component
//! - POST /onboarding/godmode - Activate godmode (skip onboarding)
//! - POST /onboarding/complete - Mark onboarding as complete
//! - POST /onboarding/reset - Reset onboarding state

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use std::path::Path as FsPath;
use tracing::instrument;

use super::error::{ApiError, ApiResult};
use super::state::AppState;
use crate::auth::CurrentUser;
use crate::onboarding::{OnboardingResponse, UnlockComponentRequest, UpdateOnboardingRequest};
use crate::templates::UserTemplateOverrides;
use crate::workspace::meta::{WorkspaceMeta, write_workspace_meta};

/// Get the current onboarding state.
#[instrument(skip(state, user))]
pub async fn get_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .get(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Update the onboarding state.
#[instrument(skip(state, user, request))]
pub async fn update_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<UpdateOnboardingRequest>,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .update(user.id(), request)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Advance to the next onboarding stage.
#[instrument(skip(state, user))]
pub async fn advance_stage(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .advance_stage(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Unlock a UI component.
#[instrument(skip(state, user))]
pub async fn unlock_component(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(component): Path<String>,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .unlock_component(user.id(), UnlockComponentRequest { component })
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Activate godmode (skip onboarding).
#[instrument(skip(state, user))]
pub async fn godmode(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .godmode(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Mark onboarding as complete.
#[instrument(skip(state, user))]
pub async fn complete_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .complete(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Reset onboarding state.
#[instrument(skip(state, user))]
pub async fn reset_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .reset(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

#[derive(Debug, Deserialize)]
pub struct BootstrapOnboardingRequest {
    pub display_name: String,
    pub language: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BootstrapOnboardingResponse {
    pub workspace_path: String,
}

/// Bootstrap the default workspace and seed the first Pi session.
#[instrument(skip(state, user, request))]
pub async fn bootstrap_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<BootstrapOnboardingRequest>,
) -> ApiResult<Json<BootstrapOnboardingResponse>> {
    let display_name = request.display_name.trim();
    if display_name.is_empty() {
        return Err(ApiError::BadRequest("display_name is required".into()));
    }

    let language = request
        .language
        .as_ref()
        .map(|lang| lang.trim().to_string())
        .filter(|lang| !lang.is_empty());

    let workspace_root = state.sessions.for_user(user.id()).workspace_root();
    let workspace_path = workspace_root.join("main");

    // Check if workspace is actually initialized by looking for the metadata file,
    // not just whether the directory has any entries. A partially-created workspace
    // (e.g., empty main/ dir from a failed attempt) should be re-initialized.
    let workspace_already_initialized =
        workspace_path.join(".oqto").join("workspace.toml").exists();

    let templates_service = state.onboarding_templates.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Onboarding templates not configured".into())
    })?;

    let overrides = UserTemplateOverrides {
        language: language.clone(),
        ..Default::default()
    };

    let templates = templates_service
        .resolve(Some(&overrides))
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to resolve templates: {e}")))?;

    let meta = WorkspaceMeta {
        display_name: Some(display_name.to_string()),
        language: language.clone(),
        pinned: Some(true),
        bootstrap_pending: Some(true),
    };

    let meta_toml = toml::to_string_pretty(&meta)
        .map_err(|e| ApiError::Internal(format!("Failed to serialize workspace meta: {e}")))?;

    // In multi-user mode, create workspace via usermgr (runs as root, sets ownership).
    // In single-user mode, write directly.
    let is_multi_user = state.linux_users.as_ref().is_some_and(|lu| lu.enabled);

    // Only create workspace files if not already initialized.
    // This makes bootstrap idempotent: if workspace exists (e.g., from a previous
    // attempt that failed partway through), we skip to session creation.
    if !workspace_already_initialized {
        if is_multi_user {
            let linux_username = state
                .linux_users
                .as_ref()
                .unwrap()
                .linux_username(user.id());
            let ws_str = workspace_path
                .to_str()
                .ok_or_else(|| ApiError::Internal("invalid workspace path".into()))?;

            // Copy the entire template directory (includes .pi/skills/ etc.),
            // then overlay the workspace metadata and any resolved template overrides.
            let template_src = templates_service
                .templates_dir()
                .join(&templates_service.subdirectory());
            let mut files = serde_json::Map::new();
            files.insert(
                ".oqto/workspace.toml".into(),
                serde_json::Value::String(meta_toml.clone()),
            );
            // Only include resolved templates if they differ from what's in the template dir
            // (i.e., language overrides or preset overrides were applied)
            files.insert(
                "BOOTSTRAP.md".into(),
                serde_json::Value::String(templates.onboard.clone()),
            );
            files.insert(
                "PERSONALITY.md".into(),
                serde_json::Value::String(templates.personality.clone()),
            );
            files.insert(
                "USER.md".into(),
                serde_json::Value::String(templates.user.clone()),
            );
            files.insert(
                "AGENTS.md".into(),
                serde_json::Value::String(templates.agents.clone()),
            );

            crate::local::linux_users::usermgr_request(
                "create-workspace",
                serde_json::json!({
                    "username": linux_username,
                    "path": ws_str,
                    "template_src": template_src.to_string_lossy(),
                    "files": files,
                }),
            )
            .map_err(|e| ApiError::Internal(format!("Failed to create workspace: {e}")))?;
        } else {
            // Single-user: copy template dir then overlay files
            let template_src = templates_service
                .templates_dir()
                .join(&templates_service.subdirectory());
            if template_src.is_dir() {
                copy_dir_all(&template_src, &workspace_path)
                    .map_err(|e| ApiError::Internal(format!("Failed to copy template: {e}")))?;
            } else {
                std::fs::create_dir_all(&workspace_path)
                    .map_err(|e| ApiError::Internal(format!("Failed to create workspace: {e}")))?;
            }

            write_workspace_meta(&workspace_path, &meta).map_err(|e| {
                ApiError::Internal(format!("Failed to write workspace metadata: {e}"))
            })?;

            // Write resolved templates (may have language/preset overrides)
            std::fs::write(workspace_path.join("BOOTSTRAP.md"), &templates.onboard)
                .map_err(|e| ApiError::Internal(format!("write BOOTSTRAP.md: {e}")))?;
            std::fs::write(workspace_path.join("PERSONALITY.md"), &templates.personality)
                .map_err(|e| ApiError::Internal(format!("write PERSONALITY.md: {e}")))?;
            std::fs::write(workspace_path.join("USER.md"), &templates.user)
                .map_err(|e| ApiError::Internal(format!("write USER.md: {e}")))?;
            std::fs::write(workspace_path.join("AGENTS.md"), &templates.agents)
                .map_err(|e| ApiError::Internal(format!("write AGENTS.md: {e}")))?;
        }
    } else {
        tracing::info!(
            "Workspace already initialized at {}, skipping file creation",
            workspace_root.display()
        );
    }

    let workspace_path_str = workspace_path.to_string_lossy().to_string();

    Ok(Json(BootstrapOnboardingResponse {
        workspace_path: workspace_path_str,
    }))
}

/// Recursively copy a directory tree, creating destination dirs as needed.
fn copy_dir_all(src: &FsPath, dst: &FsPath) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

/// Check if user needs onboarding (lightweight endpoint).
#[instrument(skip(state, user))]
pub async fn needs_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    let Some(ref service) = state.onboarding else {
        // If onboarding service is not configured, user doesn't need onboarding
        return Ok(Json(serde_json::json!({
            "needs_onboarding": false,
            "reason": "service_disabled"
        })));
    };

    let onboarding_state = service
        .get(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "needs_onboarding": onboarding_state.needs_onboarding(),
        "stage": onboarding_state.stage,
        "completed": onboarding_state.completed,
        "godmode": onboarding_state.godmode
    })))
}

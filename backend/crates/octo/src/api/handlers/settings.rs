//! Settings handlers.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use tracing::{info, instrument};

use crate::auth::{CurrentUser, RequireAdmin};
use crate::settings::{ConfigUpdate, SettingsScope, SettingsService, SettingsValue};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

use super::trx::validate_workspace_path;

/// Query parameters for settings endpoints.
#[derive(Debug, Deserialize)]
pub struct SettingsQuery {
    /// App to get settings for (e.g., "octo", "mmry")
    pub app: String,
    /// Optional workspace path for project-scoped settings.
    #[serde(default)]
    pub workspace_path: Option<String>,
}

/// Get the settings schema for an app, filtered by user permissions.
#[instrument(skip(state, user))]
pub async fn get_settings_schema(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SettingsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let service =
        resolve_settings_service(&state, &user, &query.app, query.workspace_path.as_deref())
            .await?;
    let scope = user_to_scope(&user);

    let schema = service.get_schema(scope);

    info!(user_id = %user.id(), app = %query.app, scope = ?scope, "Retrieved settings schema");
    Ok(Json(schema))
}

/// Get current settings values for an app.
#[instrument(skip(state, user))]
pub async fn get_settings_values(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SettingsQuery>,
) -> ApiResult<Json<HashMap<String, SettingsValue>>> {
    let service =
        resolve_settings_service(&state, &user, &query.app, query.workspace_path.as_deref())
            .await?;
    let scope = user_to_scope(&user);

    let values = service.get_values(scope).await;

    info!(user_id = %user.id(), app = %query.app, count = values.len(), "Retrieved settings values");
    Ok(Json(values))
}

/// Update settings values for an app.
#[instrument(skip(state, user))]
pub async fn update_settings_values(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SettingsQuery>,
    Json(updates): Json<ConfigUpdate>,
) -> ApiResult<Json<HashMap<String, SettingsValue>>> {
    let service =
        resolve_settings_service(&state, &user, &query.app, query.workspace_path.as_deref())
            .await?;
    let scope = user_to_scope(&user);

    service
        .update_values(updates, scope)
        .await
        .map_err(|e| ApiError::bad_request(format!("Failed to update settings: {}", e)))?;

    // Return updated values
    let values = service.get_values(scope).await;

    info!(user_id = %user.id(), app = %query.app, "Updated settings");
    Ok(Json(values))
}

/// Reload settings from disk (admin only).
#[instrument(skip(state, admin))]
pub async fn reload_settings(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Query(query): Query<SettingsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = resolve_settings_service(
        &state,
        &admin.0,
        &query.app,
        query.workspace_path.as_deref(),
    )
    .await?;

    service
        .reload()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to reload settings: {}", e)))?;

    info!(app = %query.app, "Settings reloaded");
    Ok(Json(serde_json::json!({ "status": "reloaded" })))
}

/// Convert user role to settings scope.
fn user_to_scope(user: &CurrentUser) -> SettingsScope {
    if user.is_admin() {
        SettingsScope::Admin
    } else {
        SettingsScope::User
    }
}

async fn resolve_settings_service(
    state: &AppState,
    user: &CurrentUser,
    app: &str,
    workspace_path: Option<&str>,
) -> ApiResult<Arc<SettingsService>> {
    let service = get_settings_service(state, app)?;

    let Some(workspace_path) = workspace_path else {
        return Ok(service);
    };

    match app {
        "pi-agent" | "pi-models" => {}
        _ => {
            return Err(ApiError::bad_request(
                "workspace_path is only supported for pi-agent and pi-models",
            ));
        }
    }

    let validated = validate_workspace_path(state, user.id(), workspace_path).await?;
    let config_dir = validated.join(".pi");
    let scoped = service
        .with_config_dir(config_dir)
        .map_err(|e| ApiError::internal(format!("Failed to load workspace settings: {}", e)))?;
    Ok(Arc::new(scoped))
}

/// Get the settings service for an app.
fn get_settings_service(state: &AppState, app: &str) -> ApiResult<Arc<SettingsService>> {
    match app {
        "octo" => state.settings_octo.as_ref().map(Arc::clone),
        "mmry" => state.settings_mmry.as_ref().map(Arc::clone),
        "pi-agent" => state.settings_pi_agent.as_ref().map(Arc::clone),
        "pi-models" => state.settings_pi_models.as_ref().map(Arc::clone),
        _ => None,
    }
    .ok_or_else(|| ApiError::not_found(format!("Settings for app '{}' not found", app)))
}



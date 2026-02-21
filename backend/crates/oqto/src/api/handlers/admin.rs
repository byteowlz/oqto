//! Admin-only handlers.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_stream::{StreamExt, wrappers::IntervalStream};
use tracing::{error, info, instrument, warn};

use crate::auth::RequireAdmin;
use crate::observability::{CpuTimes, HostMetrics, read_host_metrics};
use crate::session::{Session, SessionContainerStats};
use crate::user::{
    CreateUserRequest, UpdateUserRequest, UserInfo as DbUserInfo, UserListQuery, UserStats,
};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

/// Admin stats response for status bar.
#[derive(Debug, Serialize)]
pub struct AdminStatsResponse {
    pub total_users: i64,
    pub active_users: i64,
    pub total_sessions: i64,
    pub running_sessions: i64,
}

/// Get admin stats for the status bar (admin only).
#[instrument(skip(state, _user))]
pub async fn get_admin_stats(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<AdminStatsResponse>> {
    // Get user stats
    let user_stats = state.users.get_stats().await?;

    // Get session counts
    let sessions = state.sessions.list_sessions().await?;
    let total_sessions = sessions.len() as i64;
    let running_sessions = sessions
        .iter()
        .filter(|s| s.status == crate::session::SessionStatus::Running)
        .count() as i64;

    // Count active users (users with running sessions)
    let active_user_ids: std::collections::HashSet<_> = sessions
        .iter()
        .filter(|s| s.status == crate::session::SessionStatus::Running)
        .map(|s| s.user_id.as_str())
        .collect();
    let active_users = active_user_ids.len() as i64;

    Ok(Json(AdminStatsResponse {
        total_users: user_stats.total,
        active_users,
        total_sessions,
        running_sessions,
    }))
}

#[derive(Debug, Serialize)]
pub struct AdminMetricsSnapshot {
    pub timestamp: String,
    pub host: Option<HostMetrics>,
    pub containers: Vec<SessionContainerStats>,
    pub error: Option<String>,
}

/// List all sessions (admin only).
#[instrument(skip(state, _user))]
pub async fn admin_list_sessions(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<Vec<Session>>> {
    let sessions = state.sessions.list_sessions().await?;
    info!(count = sessions.len(), "Admin listed all sessions");
    Ok(Json(sessions))
}

/// Force stop a session (admin only).
#[instrument(skip(state, _user))]
pub async fn admin_force_stop_session(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    // Uses centralized From<anyhow::Error> conversion
    state.sessions.stop_session(&session_id).await?;

    info!(session_id = %session_id, "Admin force stopped session");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub struct LocalCleanupResponse {
    pub cleared: usize,
}

/// Clean up orphan local session processes (admin only).
#[instrument(skip(state, _user))]
pub async fn admin_cleanup_local_sessions(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<LocalCleanupResponse>> {
    let cleared = state.sessions.cleanup_local_orphans().await?;
    info!(cleared, "Admin cleaned up local sessions");
    Ok(Json(LocalCleanupResponse { cleared }))
}

/// SSE metrics stream (admin only).
#[instrument(skip(state, _user))]
pub async fn admin_metrics_stream(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>> {
    let state = state.clone();
    let cpu_state: Arc<Mutex<Option<CpuTimes>>> = Arc::new(Mutex::new(None));
    let interval = tokio::time::interval(Duration::from_secs(2));

    let stream = IntervalStream::new(interval).then(move |_| {
        let state = state.clone();
        let cpu_state = cpu_state.clone();
        async move {
            let mut guard = cpu_state.lock().await;
            let snapshot = build_admin_metrics_snapshot(&state, &mut guard).await;
            let data = match serde_json::to_string(&snapshot) {
                Ok(data) => data,
                Err(err) => {
                    warn!("Failed to serialize metrics snapshot: {:?}", err);
                    "{\"error\":\"metrics_serialization_failed\"}".to_string()
                }
            };
            Ok(Event::default().data(data))
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

async fn build_admin_metrics_snapshot(
    state: &AppState,
    prev_cpu: &mut Option<CpuTimes>,
) -> AdminMetricsSnapshot {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut errors = Vec::new();

    let previous_cpu = prev_cpu.clone();
    let host = match read_host_metrics(previous_cpu.clone()).await {
        Ok((metrics, cpu)) => {
            *prev_cpu = Some(cpu);
            Some(metrics)
        }
        Err(err) => {
            *prev_cpu = previous_cpu;
            errors.push(format!("host_metrics: {}", err));
            None
        }
    };

    let containers = match state.sessions.collect_container_stats().await {
        Ok(report) => {
            if !report.errors.is_empty() {
                errors.extend(report.errors);
            }
            report.stats
        }
        Err(err) => {
            errors.push(format!("container_stats: {}", err));
            Vec::new()
        }
    };

    let error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    AdminMetricsSnapshot {
        timestamp,
        host,
        containers,
        error,
    }
}

// ============================================================================
// User Management Handlers
// ============================================================================

/// List all users (admin only).
#[instrument(skip(state, _user))]
pub async fn list_users(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Query(query): Query<UserListQuery>,
) -> ApiResult<Json<Vec<DbUserInfo>>> {
    // Uses centralized From<anyhow::Error> conversion
    let users = state.users.list_users(query).await?;

    let user_infos: Vec<DbUserInfo> = users.into_iter().map(|u| u.into()).collect();
    info!(count = user_infos.len(), "Listed users");
    Ok(Json(user_infos))
}

#[derive(Debug, Deserialize)]
pub struct SyncUserConfigsRequest {
    pub user_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncUserConfigResult {
    pub user_id: String,
    pub linux_username: Option<String>,
    pub runner_configured: bool,
    pub shell_configured: bool,
    pub mmry_configured: bool,
    pub eavs_configured: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncUserConfigsResponse {
    pub results: Vec<SyncUserConfigResult>,
}

/// Sync per-user config files and runner services (admin only).
#[instrument(skip(state, _user))]
pub async fn sync_user_configs(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Json(request): Json<SyncUserConfigsRequest>,
) -> ApiResult<Json<SyncUserConfigsResponse>> {
    let linux_users = state
        .linux_users
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("Linux user isolation is not enabled."))?;

    let users = if let Some(ref user_id) = request.user_id {
        let user = state
            .users
            .get_user(user_id)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("User {} not found", user_id)))?;
        vec![user]
    } else {
        state.users.list_users(UserListQuery::default()).await?
    };

    let mut results = Vec::with_capacity(users.len());

    for user in users {
        let mut result = SyncUserConfigResult {
            user_id: user.id.clone(),
            linux_username: user.linux_username.clone(),
            runner_configured: false,
            shell_configured: false,
            mmry_configured: false,
            eavs_configured: false,
            error: None,
        };

        let ensure_result = if let (Some(linux_username), Some(linux_uid)) =
            (user.linux_username.as_ref(), user.linux_uid)
        {
            linux_users.ensure_user_with_verification(
                &user.id,
                Some(linux_username),
                Some(linux_uid as u32),
            )
        } else {
            linux_users.ensure_user(&user.id)
        };

        match ensure_result {
            Ok((uid, linux_username)) => {
                result.runner_configured = true;
                result.linux_username = Some(linux_username.clone());

                if (user.linux_username.as_deref() != Some(linux_username.as_str())
                    || user.linux_uid != Some(uid as i64))
                    && let Err(e) = state
                        .users
                        .update_user(
                            &user.id,
                            crate::user::UpdateUserRequest {
                                linux_username: Some(linux_username.clone()),
                                linux_uid: Some(uid as i64),
                                ..Default::default()
                            },
                        )
                        .await
                {
                    warn!(
                        user_id = %user.id,
                        error = %e,
                        "Failed to store linux_username/uid in database"
                    );
                }

                // Provision shell dotfiles (zsh + starship)
                match linux_users.setup_user_shell(&linux_username) {
                    Ok(()) => {
                        result.shell_configured = true;
                    }
                    Err(e) => {
                        let msg = format!("shell setup failed: {e}");
                        if let Some(ref mut existing) = result.error {
                            existing.push_str("; ");
                            existing.push_str(&msg);
                        } else {
                            result.error = Some(msg);
                        }
                    }
                }

                if state.mmry.enabled && !state.mmry.single_user {
                    let mmry_port = state
                        .users
                        .ensure_mmry_port(&user.id, state.mmry.user_base_port, state.mmry.user_port_range)
                        .await
                        .ok()
                        .map(|p| p as u16);
                    match linux_users.ensure_mmry_config_for_user(
                        &linux_username,
                        uid,
                        &state.mmry.host_service_url,
                        state.mmry.host_api_key.as_deref(),
                        &state.mmry.default_model,
                        state.mmry.dimension,
                        mmry_port,
                    ) {
                        Ok(()) => {
                            result.mmry_configured = true;
                        }
                        Err(err) => {
                            result.error = Some(format!("mmry config update failed: {err}"));
                        }
                    }
                }

                // Sync EAVS models.json (regenerates from current eavs catalog, no key rotation)
                if let Some(ref eavs_client) = state.eavs_client {
                    match sync_eavs_models_json(eavs_client, linux_users, &linux_username).await {
                        Ok(()) => {
                            result.eavs_configured = true;
                        }
                        Err(err) => {
                            let msg = format!("eavs models.json sync failed: {err}");
                            if let Some(ref mut existing) = result.error {
                                existing.push_str("; ");
                                existing.push_str(&msg);
                            } else {
                                result.error = Some(msg);
                            }
                        }
                    }
                }
            }
            Err(err) => {
                result.error = Some(format!("runner provisioning failed: {err}"));
            }
        }

        results.push(result);
    }

    Ok(Json(SyncUserConfigsResponse { results }))
}

/// Get a specific user (admin only).
#[instrument(skip(state, _user))]
pub async fn get_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
) -> ApiResult<Json<DbUserInfo>> {
    // Uses centralized From<anyhow::Error> conversion
    state
        .users
        .get_user(&user_id)
        .await?
        .map(|u| Json(u.into()))
        .ok_or_else(|| ApiError::not_found(format!("User {} not found", user_id)))
}

/// Create a new user (admin only).
#[instrument(skip(state, _user, request), fields(username = ?request.username))]
pub async fn create_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Json(request): Json<CreateUserRequest>,
) -> ApiResult<(StatusCode, Json<DbUserInfo>)> {
    // SECURITY: In multi-user mode, generate a user_id that won't collide with existing
    // Linux users BEFORE creating the DB user.
    let user_id = if let Some(ref linux_users) = state.linux_users {
        Some(linux_users.generate_unique_user_id(&request.username)?)
    } else {
        None
    };

    // Create the database user (with pre-generated ID if in multi-user mode)
    let user = if let Some(id) = &user_id {
        state.users.create_user_with_id(id, request).await?
    } else {
        state.users.create_user(request).await?
    };

    // SECURITY: In multi-user mode, we MUST create the Linux user or fail.
    // Since we pre-generated a unique ID, this should succeed unless there's a system error.
    if let Some(ref linux_users) = state.linux_users {
        match linux_users.ensure_user(&user.id) {
            Ok((uid, actual_linux_username)) => {
                // Store both linux_username and linux_uid for verification
                // UID is immutable by non-root, unlike GECOS which users can change via chfn
                if let Err(e) = state
                    .users
                    .update_user(
                        &user.id,
                        crate::user::UpdateUserRequest {
                            linux_username: Some(actual_linux_username.clone()),
                            linux_uid: Some(uid as i64),
                            ..Default::default()
                        },
                    )
                    .await
                {
                    warn!(
                        user_id = %user.id,
                        error = %e,
                        "Failed to store linux_username/uid in database"
                    );
                }

                if state.mmry.enabled && !state.mmry.single_user {
                    let mmry_port = state
                        .users
                        .ensure_mmry_port(&user.id, state.mmry.user_base_port, state.mmry.user_port_range)
                        .await
                        .ok()
                        .map(|p| p as u16);
                    if let Err(e) = linux_users.ensure_mmry_config_for_user(
                        &actual_linux_username,
                        uid,
                        &state.mmry.host_service_url,
                        state.mmry.host_api_key.as_deref(),
                        &state.mmry.default_model,
                        state.mmry.dimension,
                        mmry_port,
                    ) {
                        warn!(
                            user_id = %user.id,
                            error = %e,
                            "Failed to update mmry config for user"
                        );
                    }
                }

                // Provision shell dotfiles (zsh + starship)
                if let Err(e) = linux_users.setup_user_shell(&actual_linux_username) {
                    warn!(
                        user_id = %user.id,
                        error = ?e,
                        "Failed to provision shell dotfiles (non-fatal)"
                    );
                }

                info!(
                    user_id = %user.id,
                    linux_user = %actual_linux_username,
                    linux_uid = uid,
                    "Created Linux user for platform user"
                );
            }
            Err(e) => {
                // This shouldn't happen since we pre-checked, but handle it safely.
                // Use {:?} to log the full anyhow error chain (context + root cause).
                error!(
                    user_id = %user.id,
                    error = ?e,
                    "Failed to create Linux user - rolling back user creation"
                );

                // Delete the user from the database
                if let Err(delete_err) = state.users.delete_user(&user.id).await {
                    error!(
                        user_id = %user.id,
                        error = ?delete_err,
                        "Failed to delete user after Linux user creation failure"
                    );
                }

                return Err(ApiError::internal(format!(
                    "Failed to create Linux user for isolation: {:?}",
                    e
                )));
            }
        }
    }

    // Allocate a stable per-user mmry port in local multi-user mode.
    if state.mmry.enabled
        && !state.mmry.single_user
        && let Err(e) = state
            .users
            .ensure_mmry_port(
                &user.id,
                state.mmry.user_base_port,
                state.mmry.user_port_range,
            )
            .await
    {
        warn!(user_id = %user.id, error = %e, "Failed to allocate user mmry port");
    }

    // Provision EAVS virtual key and write Pi models.json if eavs client is available
    if let (Some(eavs_client), Some(linux_users)) = (&state.eavs_client, &state.linux_users) {
        let linux_username = user.linux_username.as_deref().unwrap_or(&user.id);

        match provision_eavs_for_user(eavs_client, linux_users, linux_username, &user.id).await {
            Ok(key_id) => {
                info!(
                    user_id = %user.id,
                    eavs_key_id = %key_id,
                    "Provisioned EAVS key and models.json"
                );
            }
            Err(e) => {
                warn!(
                    user_id = %user.id,
                    error = ?e,
                    "Failed to provision EAVS (non-fatal)"
                );
            }
        }
    }

    info!(user_id = %user.id, "Created new user");
    Ok((StatusCode::CREATED, Json(user.into())))
}

/// Update a user (admin only).
#[instrument(skip(state, _user, request))]
pub async fn update_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> ApiResult<Json<DbUserInfo>> {
    // Uses centralized From<anyhow::Error> conversion
    let user = state.users.update_user(&user_id, request).await?;

    info!(user_id = %user.id, "Updated user");
    Ok(Json(user.into()))
}

/// Delete a user (admin only).
#[instrument(skip(state, _user))]
pub async fn delete_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
) -> ApiResult<StatusCode> {
    // Uses centralized From<anyhow::Error> conversion
    state.users.delete_user(&user_id).await?;

    info!(user_id = %user_id, "Deleted user");
    Ok(StatusCode::NO_CONTENT)
}

/// Deactivate a user (admin only).
#[instrument(skip(state, _user))]
pub async fn deactivate_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
) -> ApiResult<Json<DbUserInfo>> {
    // Uses centralized From<anyhow::Error> conversion
    let user = state.users.deactivate_user(&user_id).await?;

    info!(user_id = %user.id, "Deactivated user");
    Ok(Json(user.into()))
}

/// Activate a user (admin only).
#[instrument(skip(state, _user))]
pub async fn activate_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
) -> ApiResult<Json<DbUserInfo>> {
    // Uses centralized From<anyhow::Error> conversion
    let user = state.users.activate_user(&user_id).await?;

    info!(user_id = %user.id, "Activated user");
    Ok(Json(user.into()))
}

/// Get user statistics (admin only).
#[instrument(skip(state, _user))]
pub async fn get_user_stats(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<UserStats>> {
    // Uses centralized From<anyhow::Error> conversion
    let stats = state.users.get_stats().await?;

    Ok(Json(stats))
}

/// Provision an EAVS virtual key and Pi models.json for a new user.
///
/// Creates a virtual key for the user (no oauth_user binding -- that would
/// route all requests through OAuth, which only works for providers that
/// support it like Anthropic/OpenAI Codex). The key is a plain proxy key
/// that uses the provider's master API key.
pub(crate) async fn provision_eavs_for_user(
    eavs_client: &crate::eavs::EavsClient,
    linux_users: &crate::local::LinuxUsersConfig,
    linux_username: &str,
    oqto_user_id: &str,
) -> anyhow::Result<String> {
    use crate::eavs::CreateKeyRequest;

    // 1. Create virtual key (no oauth_user -- uses provider's master API key)
    let key_req = CreateKeyRequest::new(format!("oqto-user-{}", oqto_user_id));

    let key_resp = eavs_client
        .create_key(key_req)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create eavs key: {}", e))?;

    // 2. Write eavs.env with the new key
    let home = linux_users.get_user_home(linux_username)?;
    let eavs_base = eavs_client.base_url();
    let env_content = format!("EAVS_API_KEY={}\nEAVS_URL={}\n", key_resp.key, eavs_base);
    let env_dir = format!("{}/.config/oqto", home);
    linux_users.write_file_as_user(linux_username, &env_dir, "eavs.env", &env_content)?;
    // 640 so the oqto service user (in the shared group) can read it for env injection.
    // Non-fatal: file still works with default perms, just slightly more permissive.
    if let Err(e) =
        linux_users.chmod_file(linux_username, &format!("{}/eavs.env", env_dir), "640")
    {
        tracing::warn!("chmod eavs.env failed (non-fatal): {e}");
    }

    // 3. Regenerate models.json from current catalog (embed the key directly)
    sync_eavs_models_json_with_key(eavs_client, linux_users, linux_username, &key_resp.key)
        .await?;

    Ok(key_resp.key_id)
}

/// Regenerate Pi models.json from the current eavs model catalog.
///
/// This is safe to call repeatedly -- it only regenerates models.json,
/// it does NOT create or rotate eavs keys. Reads the user's eavs virtual
/// key from `~/.config/oqto/eavs.env` so the key is embedded directly in
/// models.json (Pi uses it as a literal API key).
pub(crate) async fn sync_eavs_models_json(
    eavs_client: &crate::eavs::EavsClient,
    linux_users: &crate::local::LinuxUsersConfig,
    linux_username: &str,
) -> anyhow::Result<()> {
    // Read eavs key from the user's eavs.env file
    let home = linux_users.get_user_home(linux_username)?;
    let eavs_env_path = format!("{}/.config/oqto/eavs.env", home);
    let api_key = read_eavs_key_from_env(&eavs_env_path);

    sync_eavs_models_json_inner(eavs_client, linux_users, linux_username, api_key.as_deref())
        .await
}

/// Same as `sync_eavs_models_json` but with the key already in hand (avoids re-reading eavs.env).
async fn sync_eavs_models_json_with_key(
    eavs_client: &crate::eavs::EavsClient,
    linux_users: &crate::local::LinuxUsersConfig,
    linux_username: &str,
    api_key: &str,
) -> anyhow::Result<()> {
    sync_eavs_models_json_inner(eavs_client, linux_users, linux_username, Some(api_key)).await
}

async fn sync_eavs_models_json_inner(
    eavs_client: &crate::eavs::EavsClient,
    linux_users: &crate::local::LinuxUsersConfig,
    linux_username: &str,
    api_key: Option<&str>,
) -> anyhow::Result<()> {
    use crate::eavs::generate_pi_models_json;

    let providers = eavs_client
        .providers_detail()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query eavs providers: {}", e))?;

    let eavs_base = eavs_client.base_url();
    let models_json = generate_pi_models_json(&providers, eavs_base, api_key);
    let models_content = serde_json::to_string_pretty(&models_json)?;

    let home = linux_users.get_user_home(linux_username)?;
    let pi_dir = format!("{}/.pi/agent", home);
    linux_users.write_file_as_user(linux_username, &pi_dir, "models.json", &models_content)?;

    // Generate auto-rename.json with the first available model so the
    // auto-rename extension can generate LLM-based session titles.
    if let Some(first_model) = extract_first_model(&models_json) {
        let auto_rename = serde_json::json!({
            "enabled": true,
            "model": {
                "provider": first_model.0,
                "id": first_model.1,
            },
            "prefixCommand": "basename $(git rev-parse --show-toplevel 2>/dev/null || pwd)",
            "maxNameLength": 60
        });
        let auto_rename_content = serde_json::to_string_pretty(&auto_rename)
            .unwrap_or_default();
        if !auto_rename_content.is_empty() {
            // Best-effort: don't fail provisioning if this doesn't work
            let _ = linux_users.write_file_as_user(
                linux_username,
                &pi_dir,
                "auto-rename.json",
                &auto_rename_content,
            );
        }
    }

    Ok(())
}

/// Extract the first (provider, model_id) from a Pi models.json value.
/// Used to configure auto-rename with an available model.
fn extract_first_model(models_json: &serde_json::Value) -> Option<(String, String)> {
    let providers = models_json.get("providers")?.as_object()?;
    for (provider_name, provider_val) in providers {
        if let Some(models) = provider_val.get("models").and_then(|m| m.as_array()) {
            if let Some(first) = models.first() {
                if let Some(id) = first.get("id").and_then(|v| v.as_str()) {
                    return Some((provider_name.clone(), id.to_string()));
                }
            }
        }
    }
    None
}

/// Read the EAVS_API_KEY value from an eavs.env file.
/// Returns None if the file doesn't exist or the key isn't found.
fn read_eavs_key_from_env(path: &str) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("EAVS_API_KEY=") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

// ============================================================================
// EAVS / Model Provider Management
// ============================================================================

/// List configured eavs providers with their models.
#[instrument(skip(state, _user))]
pub async fn list_eavs_providers(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<EavsProvidersResponse>> {
    let eavs_client = state
        .eavs_client
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("EAVS is not configured.".into()))?;

    let providers = eavs_client
        .providers_detail()
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to query eavs providers: {e}")))?;

    let provider_summaries: Vec<EavsProviderSummary> = providers
        .iter()
        .map(|p| EavsProviderSummary {
            name: p.name.clone(),
            type_: p.type_.clone(),
            pi_api: p.pi_api.clone(),
            has_api_key: p.has_api_key,
            model_count: p.models.len(),
            models: p
                .models
                .iter()
                .map(|m| EavsModelSummary {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    reasoning: m.reasoning,
                })
                .collect(),
        })
        .collect();

    Ok(Json(EavsProvidersResponse {
        providers: provider_summaries,
        eavs_url: eavs_client.base_url().to_string(),
    }))
}

#[derive(Debug, Serialize)]
pub struct EavsProvidersResponse {
    pub providers: Vec<EavsProviderSummary>,
    pub eavs_url: String,
}

#[derive(Debug, Serialize)]
pub struct EavsProviderSummary {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub pi_api: Option<String>,
    pub has_api_key: bool,
    pub model_count: usize,
    pub models: Vec<EavsModelSummary>,
}

#[derive(Debug, Serialize)]
pub struct EavsModelSummary {
    pub id: String,
    pub name: String,
    pub reasoning: bool,
}

// ============================================================================
// EAVS Provider Management (Admin)
// ============================================================================

/// Request to add or update an eavs provider.
#[derive(Debug, Deserialize)]
pub struct UpsertEavsProviderRequest {
    /// Provider name (used as key in config, e.g. "anthropic", "openai").
    pub name: String,
    /// Provider type (e.g. "openai", "anthropic", "google", "groq").
    #[serde(rename = "type")]
    pub type_: String,
    /// API key value (stored in env file, referenced as env: in config).
    pub api_key: Option<String>,
    /// Custom base URL (if not using provider default).
    pub base_url: Option<String>,
    /// API version (primarily for Azure).
    pub api_version: Option<String>,
    /// Azure deployment name.
    pub deployment: Option<String>,
    /// Curated model shortlist for this provider.
    #[serde(default)]
    pub models: Vec<UpsertModelEntry>,
}

/// A model entry in the provider shortlist for upsert.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpsertModelEntry {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub context_window: u64,
    #[serde(default)]
    pub max_tokens: u64,
    #[serde(default)]
    pub cost_input: f64,
    #[serde(default)]
    pub cost_output: f64,
    #[serde(default)]
    pub cost_cache_read: f64,
    #[serde(default)]
    pub compat: std::collections::HashMap<String, serde_json::Value>,
}

/// Add or update a provider in the eavs config.
///
/// Writes the provider section to eavs config.toml and the API key to the env file,
/// then restarts the eavs service so changes take effect.
#[instrument(skip(state, _user, request))]
pub async fn upsert_eavs_provider(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Json(request): Json<UpsertEavsProviderRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let eavs_paths = state
        .eavs_config
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("EAVS config paths not configured.".into()))?;

    let config_path = &eavs_paths.config_path;
    let env_path = &eavs_paths.env_path;

    // Validate provider name
    if request.name.is_empty()
        || !request
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ApiError::bad_request(
            "Provider name must be alphanumeric with - or _",
        ));
    }

    // Read existing config
    let config_content = tokio::fs::read_to_string(config_path)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to read eavs config: {e}")))?;

    // Build the provider section
    let env_key_name = format!(
        "{}_API_KEY",
        request.name.to_uppercase().replace('-', "_")
    );
    let mut provider_toml = format!(
        "\n[providers.{}]\ntype = \"{}\"\n",
        request.name, request.type_
    );
    if let Some(ref api_key) = request.api_key {
        provider_toml.push_str(&format!("api_key = \"env:{}\"\n", env_key_name));

        // Write API key to env file
        let mut env_content = tokio::fs::read_to_string(env_path)
            .await
            .unwrap_or_default();
        // Remove existing key if present
        env_content = env_content
            .lines()
            .filter(|l| !l.starts_with(&format!("{}=", env_key_name)))
            .collect::<Vec<_>>()
            .join("\n");
        if !env_content.ends_with('\n') && !env_content.is_empty() {
            env_content.push('\n');
        }
        env_content.push_str(&format!("{}={}\n", env_key_name, api_key));
        tokio::fs::write(env_path, &env_content)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to write eavs env: {e}")))?;
    }
    if let Some(ref base_url) = request.base_url {
        provider_toml.push_str(&format!("base_url = \"{}\"\n", base_url));
    }
    if let Some(ref api_version) = request.api_version {
        provider_toml.push_str(&format!("api_version = \"{}\"\n", api_version));
    }
    if let Some(ref deployment) = request.deployment {
        provider_toml.push_str(&format!("deployment = \"{}\"\n", deployment));
    }

    // Write model shortlist entries
    for model in &request.models {
        provider_toml.push_str(&format!(
            "\n[[providers.{}.models]]\nid = \"{}\"\nname = \"{}\"\nreasoning = {}\n",
            request.name, model.id, model.name, model.reasoning,
        ));
        // Input modalities
        if !model.input.is_empty() {
            let input_str = model
                .input
                .iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(", ");
            provider_toml.push_str(&format!("input = [{}]\n", input_str));
        }
        // Context window / max tokens
        if model.context_window > 0 {
            provider_toml.push_str(&format!("context_window = {}\n", model.context_window));
        }
        if model.max_tokens > 0 {
            provider_toml.push_str(&format!("max_tokens = {}\n", model.max_tokens));
        }
        // Cost (inline table)
        if model.cost_input > 0.0 || model.cost_output > 0.0 || model.cost_cache_read > 0.0 {
            provider_toml.push_str(&format!(
                "cost = {{ input = {}, output = {}, cache_read = {} }}\n",
                model.cost_input, model.cost_output, model.cost_cache_read,
            ));
        }
        // Compat flags (inline table)
        if !model.compat.is_empty() {
            let compat_entries: Vec<String> = model
                .compat
                .iter()
                .map(|(k, v)| format!("{} = {}", k, v))
                .collect();
            provider_toml.push_str(&format!(
                "compat = {{ {} }}\n",
                compat_entries.join(", ")
            ));
        }
    }

    // Remove existing provider section if it exists, then append new one
    // Also remove any [[providers.NAME.models]] array entries
    let mut new_config =
        remove_toml_section(&config_content, &format!("providers.{}", request.name));
    // Remove leftover [[providers.NAME.models]] entries that survive the section removal
    new_config = remove_toml_array_entries(&new_config, &format!("providers.{}.models", request.name));
    let new_config = format!("{}\n{}", new_config.trim_end(), provider_toml);

    tokio::fs::write(config_path, &new_config)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to write eavs config: {e}")))?;

    // Restart eavs service
    restart_eavs_service().await?;

    Ok(Json(
        serde_json::json!({"ok": true, "provider": request.name}),
    ))
}

/// Delete a provider from the eavs config.
#[instrument(skip(state, _user))]
pub async fn delete_eavs_provider(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let eavs_paths = state
        .eavs_config
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("EAVS config paths not configured.".into()))?;

    let config_path = &eavs_paths.config_path;
    let env_path = &eavs_paths.env_path;

    // Read and modify config
    let config_content = tokio::fs::read_to_string(config_path)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to read eavs config: {e}")))?;

    let new_config = remove_toml_section(&config_content, &format!("providers.{}", name));

    tokio::fs::write(config_path, &new_config)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to write eavs config: {e}")))?;

    // Also remove API key from env file
    let env_key_name = format!("{}_API_KEY", name.to_uppercase().replace('-', "_"));
    if let Ok(env_content) = tokio::fs::read_to_string(env_path).await {
        let new_env: String = env_content
            .lines()
            .filter(|l| !l.starts_with(&format!("{}=", env_key_name)))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = tokio::fs::write(env_path, format!("{}\n", new_env.trim_end())).await;
    }

    // Restart eavs service
    restart_eavs_service().await?;

    Ok(Json(serde_json::json!({"ok": true, "deleted": name})))
}

/// Sync (regenerate) models.json for all users.
#[instrument(skip(state, _user))]
pub async fn sync_all_models(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<serde_json::Value>> {
    let eavs_client = state
        .eavs_client
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("EAVS is not configured.".into()))?;

    let linux_users = state
        .linux_users
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("Linux user isolation is not enabled."))?;

    let users = state
        .users
        .list_users(crate::user::UserListQuery::default())
        .await?;

    let mut synced = 0;
    let mut errors = Vec::new();

    for user in &users {
        if let Some(ref linux_username) = user.linux_username {
            // Skip users without a valid oqto_ prefix (e.g. legacy admin/dev entries)
            if !linux_username.starts_with("oqto_") {
                continue;
            }
            match sync_eavs_models_json(
                eavs_client.as_ref(),
                linux_users,
                linux_username,
            )
            .await
            {
                Ok(()) => synced += 1,
                Err(e) => errors.push(format!("{}: {}", user.id, e)),
            }
        }
    }

    Ok(Json(serde_json::json!({
        "ok": errors.is_empty(),
        "synced": synced,
        "total": users.len(),
        "errors": errors,
    })))
}

/// Remove a [section.name] block from a TOML string.
/// Removes from the section header until the next section header or end of file.
fn remove_toml_section(content: &str, section: &str) -> String {
    let header = format!("[{}]", section);
    let array_prefix = format!("[[{}.", section);
    let mut result = String::new();
    let mut skipping = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == header || trimmed.starts_with(&array_prefix) {
            skipping = true;
            continue;
        }
        // A new section header ends the skip (but not array entries of the same section)
        if skipping && trimmed.starts_with('[') && !trimmed.starts_with(&array_prefix) {
            skipping = false;
        }
        if !skipping {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

/// Remove leftover TOML array-of-table entries ([[section]]) that might
/// survive a section removal (e.g., [[providers.NAME.models]]).
fn remove_toml_array_entries(content: &str, array_name: &str) -> String {
    let header = format!("[[{}]]", array_name);
    let mut result = String::new();
    let mut skipping = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == header {
            skipping = true;
            continue;
        }
        // A new section or array header ends the skip
        if skipping && trimmed.starts_with('[') {
            skipping = false;
        }
        if !skipping {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

/// Restart the eavs systemd service via oqto-usermgr (which runs as root).
async fn restart_eavs_service() -> Result<(), ApiError> {
    // Use usermgr daemon to restart the service (it runs as root)
    tokio::task::spawn_blocking(|| {
        crate::local::linux_users::usermgr_request(
            "restart-service",
            serde_json::json!({"service": "eavs"}),
        )
    })
    .await
    .map_err(|e| ApiError::Internal(format!("Task join error: {e}")))?
    .map_err(|e| ApiError::Internal(format!("Failed to restart eavs: {e}")))?;

    // Wait a moment for eavs to start
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Catalog lookup -- proxy to eavs /catalog/lookup
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CatalogLookupQuery {
    pub model_id: String,
    pub provider: Option<String>,
}

/// `GET /api/admin/eavs/catalog-lookup?model_id=...`
///
/// Proxies to eavs's `/catalog/lookup` endpoint to look up model metadata
/// from models.dev. Used by the admin UI to auto-fill cost, context window,
/// etc. when adding models to a provider.
pub async fn catalog_lookup(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Query(query): Query<CatalogLookupQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let eavs = state
        .eavs_client
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("EAVS is not configured.".into()))?;

    let mut url = format!("{}/catalog/lookup?model_id={}", eavs.base_url(), urlencoding::encode(&query.model_id));
    if let Some(ref provider) = query.provider {
        url.push_str(&format!("&provider={}", urlencoding::encode(provider)));
    }

    let client = reqwest::Client::new();
    let mut req = client.get(&url);
    let key = eavs.master_key();
    if !key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to query eavs catalog: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!(
            "Eavs catalog lookup failed ({}): {}",
            status, body
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to parse eavs catalog response: {e}")))?;

    Ok(Json(data))
}

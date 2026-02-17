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
            mmry_configured: false,
            eavs_configured: false,
            error: None,
        };

        let ensure_result = if let (Some(ref linux_username), Some(linux_uid)) =
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

                if user.linux_username.as_deref() != Some(linux_username.as_str())
                    || user.linux_uid != Some(uid as i64)
                {
                    if let Err(e) = state
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
                }

                if state.mmry.enabled && !state.mmry.single_user {
                    match linux_users.ensure_mmry_config_for_user(
                        &linux_username,
                        uid,
                        &state.mmry.host_service_url,
                        state.mmry.host_api_key.as_deref(),
                        &state.mmry.default_model,
                        state.mmry.dimension,
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
                    match sync_eavs_models_json(
                        eavs_client,
                        linux_users,
                        &linux_username,
                    )
                    .await
                    {
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
                    if let Err(e) = linux_users.ensure_mmry_config_for_user(
                        &actual_linux_username,
                        uid,
                        &state.mmry.host_service_url,
                        state.mmry.host_api_key.as_deref(),
                        &state.mmry.default_model,
                        state.mmry.dimension,
                    ) {
                        warn!(
                            user_id = %user.id,
                            error = %e,
                            "Failed to update mmry config for user"
                        );
                    }
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
    if let (Some(eavs_client), Some(linux_users)) =
        (&state.eavs_client, &state.linux_users)
    {
        let linux_username = user
            .linux_username
            .as_deref()
            .unwrap_or(&user.id);

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
/// Creates a virtual key bound to the user via oauth_user, queries provider
/// details for model catalog, generates models.json, and writes eavs.env.
async fn provision_eavs_for_user(
    eavs_client: &crate::eavs::EavsClient,
    linux_users: &crate::local::LinuxUsersConfig,
    linux_username: &str,
    octo_user_id: &str,
) -> anyhow::Result<String> {
    use crate::eavs::CreateKeyRequest;

    // 1. Create virtual key with oauth_user binding
    let key_req =
        CreateKeyRequest::new(format!("octo-user-{}", octo_user_id)).oauth_user(octo_user_id);

    let key_resp = eavs_client
        .create_key(key_req)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create eavs key: {}", e))?;

    // 2. Write eavs.env with the new key
    let home = linux_users.get_user_home(linux_username)?;
    let eavs_base = eavs_client.base_url();
    let env_content = format!("EAVS_API_KEY={}\nEAVS_URL={}\n", key_resp.key, eavs_base);
    let env_dir = format!("{}/.config/octo", home);
    linux_users.write_file_as_user(linux_username, &env_dir, "eavs.env", &env_content)?;
    // 640 so the octo service user (in the shared group) can read it for env injection
    linux_users.chmod_file(linux_username, &format!("{}/eavs.env", env_dir), "640")?;

    // 3. Regenerate models.json from current catalog
    sync_eavs_models_json(eavs_client, linux_users, linux_username).await?;

    Ok(key_resp.key_id)
}

/// Regenerate Pi models.json from the current eavs model catalog.
///
/// This is safe to call repeatedly -- it only regenerates models.json,
/// it does NOT create or rotate eavs keys.
async fn sync_eavs_models_json(
    eavs_client: &crate::eavs::EavsClient,
    linux_users: &crate::local::LinuxUsersConfig,
    linux_username: &str,
) -> anyhow::Result<()> {
    use crate::eavs::generate_pi_models_json;

    let providers = eavs_client
        .providers_detail()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query eavs providers: {}", e))?;

    let eavs_base = eavs_client.base_url();
    let models_json = generate_pi_models_json(&providers, eavs_base);
    let models_content = serde_json::to_string_pretty(&models_json)?;

    let home = linux_users.get_user_home(linux_username)?;
    let pi_dir = format!("{}/.pi/agent", home);
    linux_users.write_file_as_user(linux_username, &pi_dir, "models.json", &models_content)?;

    Ok(())
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

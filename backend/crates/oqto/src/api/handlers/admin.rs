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

/// Get event bus statistics (admin only).
pub async fn get_bus_stats(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<crate::bus::engine::BusStats>> {
    Ok(Json(state.bus.stats()))
}

#[derive(Debug, Deserialize)]
pub struct PublishBusEventRequest {
    pub scope: crate::bus::BusScope,
    pub scope_id: String,
    pub topic: String,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub version: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct PublishBusEventResponse {
    pub event_id: String,
}

/// Publish a bus event as admin (admin only).
pub async fn publish_bus_event(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Json(req): Json<PublishBusEventRequest>,
) -> ApiResult<Json<PublishBusEventResponse>> {
    use crate::bus::{BusEvent, EventSource};

    let mut event = BusEvent::new(
        req.scope,
        req.scope_id,
        req.topic,
        req.payload,
        EventSource::Admin {
            user_id: user.id().to_string(),
        },
    );
    if let Some(v) = req.version {
        event.v = v;
    }

    let event_id = event.event_id.clone();
    state
        .bus
        .publish_internal(event)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(PublishBusEventResponse { event_id }))
}

#[derive(Debug, Serialize)]
pub struct AdminMetricsSnapshot {
    pub timestamp: String,
    pub host: Option<HostMetrics>,
    pub containers: Vec<SessionContainerStats>,
    pub user_plane: Vec<crate::user_plane::UserPlaneMetricRow>,
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
        user_plane: state.user_plane_metrics.snapshot(),
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
        } else if state.strict_identity_enabled() {
            result.error = Some(
                "strict identity mode enabled and user is missing linux_username/linux_uid"
                    .to_string(),
            );
            results.push(result);
            continue;
        } else {
            warn!(
                user_id = %user.id,
                "using legacy identity fallback during sync-configs (ensure_user by user_id)"
            );
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

                let _ = uid;

                // Sync EAVS: provision virtual key if missing, then regenerate models.json.
                if let Some(ref eavs_client) = state.eavs_client {
                    let home = linux_users
                        .get_user_home(&linux_username)
                        .unwrap_or_default();
                    let eavs_env_path = format!("{}/.config/oqto/eavs.env", home);
                    let has_eavs_key = std::path::Path::new(&eavs_env_path).exists();

                    if has_eavs_key {
                        // Key exists, just sync models.json (no key rotation)
                        match sync_eavs_models_json(
                            eavs_client,
                            linux_users,
                            &linux_username,
                            Some(&state.auto_rename_config),
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
                    } else {
                        // No eavs.env -- provision a new virtual key + write eavs.env + models.json
                        match provision_eavs_for_user(
                            eavs_client,
                            linux_users,
                            &linux_username,
                            &user.id,
                            Some(&state.auto_rename_config),
                        )
                        .await
                        {
                            Ok(_key_id) => {
                                result.eavs_configured = true;
                                info!(
                                    user_id = %user.id,
                                    "Provisioned missing EAVS key during sync-configs"
                                );
                            }
                            Err(err) => {
                                let msg = format!("eavs provisioning failed: {err}");
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

    // Provision EAVS virtual key and write Pi models.json if eavs client is available
    if let (Some(eavs_client), Some(linux_users)) = (&state.eavs_client, &state.linux_users) {
        let linux_username = user.linux_username.as_deref().unwrap_or(&user.id);

        match provision_eavs_for_user(
            eavs_client,
            linux_users,
            linux_username,
            &user.id,
            Some(&state.auto_rename_config),
        )
        .await
        {
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
///
/// In multi-user mode, also deletes the Linux user via oqto-usermgr.
/// This stops user services (runner), disables linger,
/// removes the home directory, and cleans up the runner socket.
#[instrument(skip(state, _user))]
pub async fn delete_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
) -> ApiResult<StatusCode> {
    // Look up the user first to get linux_username (needed for OS cleanup)
    let user = state
        .users
        .get_user(&user_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("User not found: {user_id}")))?;
    let linux_username = user.linux_username.clone();

    // Delete from oqto DB first
    state.users.delete_user(&user_id).await?;

    // In multi-user mode, clean up the Linux user + services
    if let Some(ref linux_user) = linux_username {
        let linux_user = linux_user.clone();
        if let Err(e) = tokio::task::spawn_blocking(move || {
            crate::local::linux_users::usermgr_request(
                "delete-user",
                serde_json::json!({"username": linux_user}),
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {e}"))?
        {
            // Log but don't fail -- the DB record is already gone
            warn!(
                user_id = %user_id,
                linux_username = ?linux_username,
                error = %e,
                "Failed to delete Linux user (DB record already removed)"
            );
        }
    }

    info!(user_id = %user_id, linux_username = ?linux_username, "Deleted user");
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
    auto_rename_config: Option<&serde_json::Value>,
) -> anyhow::Result<String> {
    use crate::eavs::CreateKeyRequest;

    // 1. Create virtual key (no oauth_user -- uses provider's master API key)
    let key_req = CreateKeyRequest::new(format!("oqto-user-{}", oqto_user_id));

    let key_resp = eavs_client
        .create_key(key_req)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create eavs key: {}", e))?;

    // 2. Write models.json with the virtual key embedded directly.
    // The key is written as a literal value in the apiKey field so Pi uses it
    // as a Bearer token when calling eavs. No eavs.env indirection needed.
    sync_eavs_models_json_with_key(
        eavs_client,
        linux_users,
        linux_username,
        &key_resp.key,
        auto_rename_config,
    )
    .await?;

    Ok(key_resp.key_id)
}

/// Regenerate Pi models.json from the current eavs model catalog.
///
/// This is safe to call repeatedly -- it only regenerates models.json,
/// it does NOT create or rotate eavs keys. Reads the user's existing
/// eavs virtual key from the current models.json so it can be preserved
/// across regenerations.
pub(crate) async fn sync_eavs_models_json(
    eavs_client: &crate::eavs::EavsClient,
    linux_users: &crate::local::LinuxUsersConfig,
    linux_username: &str,
    auto_rename_config: Option<&serde_json::Value>,
) -> anyhow::Result<()> {
    // Read existing eavs key from models.json (embedded in apiKey field).
    // Fall back to legacy eavs.env for migration from older installs.
    let home = linux_users.get_user_home(linux_username)?;
    let models_path = format!("{}/.pi/agent/models.json", home);
    let api_key = read_eavs_key_from_models_json(&models_path).or_else(|| {
        let eavs_env_path = format!("{}/.config/oqto/eavs.env", home);
        read_eavs_key_from_env(&eavs_env_path)
    });

    sync_eavs_models_json_inner(
        eavs_client,
        linux_users,
        linux_username,
        api_key.as_deref(),
        auto_rename_config,
    )
    .await
}

/// Same as `sync_eavs_models_json` but with the key already in hand (avoids re-reading eavs.env).
pub(crate) async fn sync_eavs_models_json_with_key(
    eavs_client: &crate::eavs::EavsClient,
    linux_users: &crate::local::LinuxUsersConfig,
    linux_username: &str,
    api_key: &str,
    auto_rename_config: Option<&serde_json::Value>,
) -> anyhow::Result<()> {
    sync_eavs_models_json_inner(
        eavs_client,
        linux_users,
        linux_username,
        Some(api_key),
        auto_rename_config,
    )
    .await
}

async fn sync_eavs_models_json_inner(
    eavs_client: &crate::eavs::EavsClient,
    linux_users: &crate::local::LinuxUsersConfig,
    linux_username: &str,
    api_key: Option<&str>,
    auto_rename_config: Option<&serde_json::Value>,
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

    // Write auto-rename.json from config (configurable via [pi.auto_rename] in config.toml).
    if let Some(auto_rename) = auto_rename_config
        && !auto_rename.is_null()
        && auto_rename.is_object()
        && let Ok(content) = serde_json::to_string_pretty(auto_rename)
    {
        let _ =
            linux_users.write_file_as_user(linux_username, &pi_dir, "auto-rename.json", &content);
    }

    Ok(())
}

/// Read the EAVS_API_KEY value from an eavs.env file.
/// Returns None if the file doesn't exist or the key isn't found.
/// Read the eavs virtual key from an existing models.json.
/// Looks for the first provider whose apiKey is a non-empty literal value
/// (not "EAVS_API_KEY" or "env:..." references).
fn read_eavs_key_from_models_json(path: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&content).ok()?;
    let providers = config.get("providers")?.as_object()?;
    for (_name, provider) in providers {
        if let Some(key) = provider.get("apiKey").and_then(|k| k.as_str()) {
            let key = key.trim();
            // Skip env var references and placeholder values
            if !key.is_empty()
                && key != "EAVS_API_KEY"
                && !key.starts_with("env:")
                && key != "not-needed"
            {
                return Some(key.to_string());
            }
        }
    }
    None
}

/// Read the eavs virtual key from a legacy eavs.env file.
/// Used as fallback for migration from older installs.
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

    let configured_by_name = if let Some(eavs_paths) = state.eavs_config.as_ref() {
        match tokio::fs::read_to_string(&eavs_paths.config_path).await {
            Ok(content) => parse_existing_providers_config(&content),
            Err(err) => {
                warn!(
                    path = %eavs_paths.config_path.display(),
                    error = %err,
                    "Failed to read eavs config for provider edit metadata"
                );
                std::collections::HashMap::new()
            }
        }
    } else {
        std::collections::HashMap::new()
    };

    let provider_summaries: Vec<EavsProviderSummary> = providers
        .iter()
        .map(|p| {
            let configured = configured_by_name.get(&p.name);
            EavsProviderSummary {
                name: p.name.clone(),
                type_: p.type_.clone(),
                pi_api: p.pi_api.clone(),
                has_api_key: p.has_api_key,
                base_url: configured.and_then(|cfg| cfg.base_url.clone()),
                api_version: configured
                    .and_then(|cfg| cfg.api_version.clone())
                    .or_else(|| p.api_version.clone()),
                deployment: configured.and_then(|cfg| cfg.deployment.clone()),
                supports_developer_role: configured.and_then(|cfg| cfg.supports_developer_role),
                model_count: p.models.len(),
                models: p
                    .models
                    .iter()
                    .map(|m| {
                        let configured_model = configured.and_then(|cfg| cfg.models.get(&m.id));
                        EavsModelSummary {
                            id: m.id.clone(),
                            name: configured_model
                                .map(|cm| cm.name.clone())
                                .filter(|name| !name.is_empty())
                                .unwrap_or_else(|| m.name.clone()),
                            reasoning: configured_model
                                .map(|cm| cm.reasoning)
                                .unwrap_or(m.reasoning),
                            input: configured_model
                                .map(|cm| cm.input.clone())
                                .filter(|input| !input.is_empty())
                                .unwrap_or_else(|| m.input.clone()),
                            context_window: configured_model
                                .map(|cm| cm.context_window)
                                .filter(|v| *v > 0)
                                .unwrap_or(m.context_window),
                            max_tokens: configured_model
                                .map(|cm| cm.max_tokens)
                                .filter(|v| *v > 0)
                                .unwrap_or(m.max_tokens),
                            cost_input: configured_model
                                .map(|cm| cm.cost_input)
                                .filter(|v| *v > 0.0)
                                .unwrap_or(m.cost.input),
                            cost_output: configured_model
                                .map(|cm| cm.cost_output)
                                .filter(|v| *v > 0.0)
                                .unwrap_or(m.cost.output),
                            cost_cache_read: configured_model
                                .map(|cm| cm.cost_cache_read)
                                .filter(|v| *v > 0.0)
                                .unwrap_or(m.cost.cache_read),
                            compat: configured_model
                                .map(|cm| cm.compat.clone())
                                .filter(|compat| !compat.is_empty())
                                .unwrap_or_else(|| m.compat.clone()),
                        }
                    })
                    .collect(),
            }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_developer_role: Option<bool>,
    pub model_count: usize,
    pub models: Vec<EavsModelSummary>,
}

#[derive(Debug, Serialize)]
pub struct EavsModelSummary {
    pub id: String,
    pub name: String,
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
    /// Whether this endpoint accepts OpenAI `developer` messages.
    pub supports_developer_role: Option<bool>,
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

/// Probe an unsaved provider draft through EAVS without changing live config.
#[instrument(skip(state, _user, request))]
pub async fn probe_eavs_provider(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Json(request): Json<UpsertEavsProviderRequest>,
) -> ApiResult<Json<oqto_eavs::ProviderProbeResponse>> {
    let eavs_client = state
        .eavs_client
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("EAVS is not configured.".into()))?;
    let model = request
        .models
        .first()
        .map(|model| model.id.trim().to_string())
        .filter(|model| !model.is_empty())
        .ok_or_else(|| ApiError::bad_request("Add at least one model before testing."))?;

    let provider_name = request.name;
    let probe = oqto_eavs::ProviderProbeRequest {
        provider_name: Some(provider_name),
        config: oqto_eavs::ProviderProbeConfig {
            type_: request.type_,
            // Empty on edit means "reuse the saved provider credential". EAVS
            // resolves it internally; Oqto must not read or shuttle stored keys.
            api_key: request.api_key.unwrap_or_default(),
            base_url: request.base_url.filter(|value| !value.trim().is_empty()),
            api_version: request.api_version.filter(|value| !value.trim().is_empty()),
            deployment: request.deployment.filter(|value| !value.trim().is_empty()),
            compat: request
                .supports_developer_role
                .map(|value| oqto_eavs::ProviderProbeCompat {
                    supports_developer_role: Some(value),
                }),
        },
        model,
    };

    let result = eavs_client
        .probe_provider(probe)
        .await
        .map_err(|error| ApiError::bad_request(format!("Provider test failed: {error}")))?;
    Ok(Json(result))
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

    let existing_providers = parse_existing_providers_config(&config_content);
    let existing_provider = existing_providers.get(&request.name);

    // Resolve the API key reference and (if a new key was provided) stage the
    // env-file update. The provider section itself is built structurally below.
    let env_key_name = format!("{}_API_KEY", request.name.to_uppercase().replace('-', "_"));
    let mut env_write: Option<String> = None;
    let api_key_ref: Option<String> = if let Some(ref api_key) = request.api_key {
        let mut env_content = tokio::fs::read_to_string(env_path)
            .await
            .unwrap_or_default();
        env_content = env_content
            .lines()
            .filter(|l| !l.starts_with(&format!("{}=", env_key_name)))
            .collect::<Vec<_>>()
            .join("\n");
        if !env_content.ends_with('\n') && !env_content.is_empty() {
            env_content.push('\n');
        }
        env_content.push_str(&format!("{}={}\n", env_key_name, api_key));
        env_write = Some(env_content);
        Some(format!("env:{env_key_name}"))
    } else {
        // Preserve existing api_key reference when the edit UI leaves it blank.
        existing_provider.and_then(|provider| provider.api_key_ref.clone())
    };

    let base_url = request
        .base_url
        .clone()
        .or_else(|| existing_provider.and_then(|provider| provider.base_url.clone()));
    let api_version = request
        .api_version
        .clone()
        .or_else(|| existing_provider.and_then(|provider| provider.api_version.clone()));
    let deployment = request
        .deployment
        .clone()
        .or_else(|| existing_provider.and_then(|provider| provider.deployment.clone()));
    let supports_developer_role = request
        .supports_developer_role
        .or_else(|| existing_provider.and_then(|provider| provider.supports_developer_role));

    let merged_models: Vec<UpsertModelEntry> = request
        .models
        .iter()
        .map(|model| {
            merge_model_with_existing(
                model,
                existing_provider.and_then(|provider| provider.models.get(&model.id)),
            )
        })
        .collect();

    // Build the provider table as a structured TOML value, then splice it into
    // the parsed document. Structured editing makes duplicate/orphaned
    // sub-tables impossible, and the whole document is round-trip validated
    // before it is atomically written -- a malformed config can never land on
    // disk and crash EAVS. See ADR/incident: duplicate `[providers.*.compat]`.
    let mut prov = toml_edit::Table::new();
    prov["type"] = toml_edit::value(request.type_.clone());
    if let Some(api_key_ref) = api_key_ref {
        prov["api_key"] = toml_edit::value(api_key_ref);
    }
    if let Some(base_url) = base_url {
        prov["base_url"] = toml_edit::value(base_url);
    }
    if let Some(api_version) = api_version {
        prov["api_version"] = toml_edit::value(api_version);
    }
    if let Some(deployment) = deployment {
        prov["deployment"] = toml_edit::value(deployment);
    }
    if let Some(supports_developer_role) = supports_developer_role {
        let mut compat = toml_edit::Table::new();
        compat["supports_developer_role"] = toml_edit::value(supports_developer_role);
        prov["compat"] = toml_edit::Item::Table(compat);
    }
    if !merged_models.is_empty() {
        let mut models = toml_edit::ArrayOfTables::new();
        for model in &merged_models {
            models.push(build_model_table(model));
        }
        prov["models"] = toml_edit::Item::ArrayOfTables(models);
    }

    let new_config = upsert_provider_in_config(&config_content, &request.name, prov)?;

    // Persist env first (only if a new key was provided), then the validated
    // config. Both writes are atomic (temp + rename).
    if let Some(env_content) = env_write {
        write_file_atomic(env_path, env_content.as_bytes())
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to write eavs env: {e}")))?;
    }
    write_toml_config_atomic(config_path, &new_config).await?;

    // Restart eavs service
    restart_eavs_service(state.single_user).await?;

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

    // Structurally remove the entire providers.<name> subtree, then validate +
    // atomically write so a malformed config can never reach EAVS.
    let new_config = remove_provider_in_config(&config_content, &name)?;
    write_toml_config_atomic(config_path, &new_config).await?;

    // Also remove API key from env file (atomic).
    let env_key_name = format!("{}_API_KEY", name.to_uppercase().replace('-', "_"));
    if let Ok(env_content) = tokio::fs::read_to_string(env_path).await {
        let new_env: String = env_content
            .lines()
            .filter(|l| !l.starts_with(&format!("{}=", env_key_name)))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = write_file_atomic(env_path, format!("{}\n", new_env.trim_end()).as_bytes()).await;
    }

    // Restart eavs service
    restart_eavs_service(state.single_user).await?;

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
                Some(&state.auto_rename_config),
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

#[derive(Debug, Clone, Default)]
struct ExistingProviderConfig {
    api_key_ref: Option<String>,
    base_url: Option<String>,
    api_version: Option<String>,
    deployment: Option<String>,
    supports_developer_role: Option<bool>,
    models: std::collections::HashMap<String, UpsertModelEntry>,
}

fn merge_model_with_existing(
    model: &UpsertModelEntry,
    existing: Option<&UpsertModelEntry>,
) -> UpsertModelEntry {
    let mut merged = model.clone();

    if let Some(existing) = existing {
        if merged.name.is_empty() {
            merged.name = existing.name.clone();
        }
        if merged.input.is_empty() {
            merged.input = existing.input.clone();
        }
        if merged.context_window == 0 {
            merged.context_window = existing.context_window;
        }
        if merged.max_tokens == 0 {
            merged.max_tokens = existing.max_tokens;
        }
        if merged.cost_input == 0.0 {
            merged.cost_input = existing.cost_input;
        }
        if merged.cost_output == 0.0 {
            merged.cost_output = existing.cost_output;
        }
        if merged.cost_cache_read == 0.0 {
            merged.cost_cache_read = existing.cost_cache_read;
        }
        if merged.compat.is_empty() {
            merged.compat = existing.compat.clone();
        }
    }

    merged
}

fn parse_existing_providers_config(
    content: &str,
) -> std::collections::HashMap<String, ExistingProviderConfig> {
    let parsed: toml::Value = match content.parse() {
        Ok(value) => value,
        Err(err) => {
            warn!(error = %err, "Failed to parse eavs config TOML");
            return std::collections::HashMap::new();
        }
    };

    let providers = match parsed.get("providers").and_then(|value| value.as_table()) {
        Some(table) => table,
        None => return std::collections::HashMap::new(),
    };

    providers
        .iter()
        .filter_map(|(provider_name, provider_value)| {
            let provider_table = provider_value.as_table()?;

            let mut provider = ExistingProviderConfig {
                api_key_ref: provider_table
                    .get("api_key")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                base_url: provider_table
                    .get("base_url")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                api_version: provider_table
                    .get("api_version")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                deployment: provider_table
                    .get("deployment")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                supports_developer_role: provider_table
                    .get("compat")
                    .and_then(|value| value.as_table())
                    .and_then(|compat| compat.get("supports_developer_role"))
                    .and_then(|value| value.as_bool()),
                ..ExistingProviderConfig::default()
            };

            if let Some(models) = provider_table
                .get("models")
                .and_then(|value| value.as_array())
            {
                for model in models {
                    let Some(model_table) = model.as_table() else {
                        continue;
                    };

                    let Some(id) = model_table
                        .get("id")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                    else {
                        continue;
                    };

                    let name = model_table
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let reasoning = model_table
                        .get("reasoning")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                    let input = model_table
                        .get("input")
                        .and_then(|value| value.as_array())
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(|value| value.as_str().map(str::to_string))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let context_window = model_table
                        .get("context_window")
                        .and_then(|value| value.as_integer())
                        .and_then(|value| u64::try_from(value).ok())
                        .unwrap_or(0);
                    let max_tokens = model_table
                        .get("max_tokens")
                        .and_then(|value| value.as_integer())
                        .and_then(|value| u64::try_from(value).ok())
                        .unwrap_or(0);

                    let (cost_input, cost_output, cost_cache_read) = model_table
                        .get("cost")
                        .and_then(|value| value.as_table())
                        .map(|cost| {
                            (
                                cost.get("input")
                                    .and_then(toml_value_to_f64)
                                    .unwrap_or_default(),
                                cost.get("output")
                                    .and_then(toml_value_to_f64)
                                    .unwrap_or_default(),
                                cost.get("cache_read")
                                    .and_then(toml_value_to_f64)
                                    .unwrap_or_default(),
                            )
                        })
                        .unwrap_or((0.0, 0.0, 0.0));

                    let compat = model_table
                        .get("compat")
                        .and_then(|value| value.as_table())
                        .map(|compat_table| {
                            compat_table
                                .iter()
                                .filter_map(|(k, v)| {
                                    toml_to_json_value(v).map(|json| (k.clone(), json))
                                })
                                .collect::<std::collections::HashMap<_, _>>()
                        })
                        .unwrap_or_default();

                    provider.models.insert(
                        id.clone(),
                        UpsertModelEntry {
                            id,
                            name,
                            reasoning,
                            input,
                            context_window,
                            max_tokens,
                            cost_input,
                            cost_output,
                            cost_cache_read,
                            compat,
                        },
                    );
                }
            }

            Some((provider_name.to_string(), provider))
        })
        .collect()
}

fn toml_value_to_f64(value: &toml::Value) -> Option<f64> {
    if let Some(float_value) = value.as_float() {
        return Some(float_value);
    }
    value.as_integer().map(|int_value| int_value as f64)
}

fn toml_to_json_value(value: &toml::Value) -> Option<serde_json::Value> {
    match value {
        toml::Value::String(s) => Some(serde_json::Value::String(s.clone())),
        toml::Value::Integer(i) => Some(serde_json::Value::Number((*i).into())),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f).map(serde_json::Value::Number),
        toml::Value::Boolean(b) => Some(serde_json::Value::Bool(*b)),
        toml::Value::Datetime(dt) => Some(serde_json::Value::String(dt.to_string())),
        toml::Value::Array(arr) => Some(serde_json::Value::Array(
            arr.iter().filter_map(toml_to_json_value).collect(),
        )),
        toml::Value::Table(table) => Some(serde_json::Value::Object(
            table
                .iter()
                .filter_map(|(k, v)| toml_to_json_value(v).map(|json| (k.clone(), json)))
                .collect(),
        )),
    }
}

/// Build a structured `[[providers.NAME.models]]` table entry from a model.
fn build_model_table(model: &UpsertModelEntry) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table["id"] = toml_edit::value(model.id.clone());
    table["name"] = toml_edit::value(model.name.clone());
    table["reasoning"] = toml_edit::value(model.reasoning);
    if !model.input.is_empty() {
        let mut arr = toml_edit::Array::new();
        for modality in &model.input {
            arr.push(modality.as_str());
        }
        table["input"] = toml_edit::value(arr);
    }
    if model.context_window > 0 {
        table["context_window"] = toml_edit::value(model.context_window as i64);
    }
    if model.max_tokens > 0 {
        table["max_tokens"] = toml_edit::value(model.max_tokens as i64);
    }
    if model.cost_input > 0.0 || model.cost_output > 0.0 || model.cost_cache_read > 0.0 {
        let mut cost = toml_edit::InlineTable::new();
        cost.insert("input", model.cost_input.into());
        cost.insert("output", model.cost_output.into());
        cost.insert("cache_read", model.cost_cache_read.into());
        table["cost"] = toml_edit::value(cost);
    }
    if !model.compat.is_empty() {
        let mut compat = toml_edit::InlineTable::new();
        // Deterministic key order keeps diffs stable across edits.
        let mut keys: Vec<&String> = model.compat.keys().collect();
        keys.sort();
        for key in keys {
            if let Some(val) = json_to_toml_edit(&model.compat[key]) {
                compat.insert(key, val);
            }
        }
        table["compat"] = toml_edit::value(compat);
    }
    table
}

/// Convert a JSON scalar/array into a toml_edit value (best-effort; skips nulls).
fn json_to_toml_edit(value: &serde_json::Value) -> Option<toml_edit::Value> {
    match value {
        serde_json::Value::String(s) => Some(s.as_str().into()),
        serde_json::Value::Bool(b) => Some((*b).into()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i.into())
            } else {
                n.as_f64().map(Into::into)
            }
        }
        serde_json::Value::Array(arr) => {
            let mut out = toml_edit::Array::new();
            for item in arr {
                out.push(json_to_toml_edit(item)?);
            }
            Some(toml_edit::Value::Array(out))
        }
        serde_json::Value::Null | serde_json::Value::Object(_) => None,
    }
}

/// Write bytes atomically: temp file in the same directory, fsync, then rename
/// over the target. A reader (e.g. EAVS) never observes a partial file.
async fn write_file_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config"),
        std::process::id()
    ));
    tokio::fs::write(&tmp, bytes).await?;
    // Best-effort durability before the rename.
    if let Ok(file) = tokio::fs::File::open(&tmp).await {
        let _ = file.sync_all().await;
    }
    match tokio::fs::rename(&tmp, path).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

/// Validate that `content` parses as TOML, then write it atomically. Refuses to
/// persist an unparseable config -- the guarantee that a broken config file can
/// never reach EAVS and crash-loop it.
async fn write_toml_config_atomic(path: &std::path::Path, content: &str) -> Result<(), ApiError> {
    content.parse::<toml_edit::DocumentMut>().map_err(|e| {
        ApiError::Internal(format!(
            "refusing to write invalid TOML to {}: {e}",
            path.display()
        ))
    })?;
    write_file_atomic(path, content.as_bytes())
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to write config {}: {e}", path.display())))
}

/// Replace (or insert) the `providers.<name>` table in an EAVS config document,
/// returning the serialized, revalidated TOML. Structural removal takes the
/// whole subtree (compat + models) with it, so duplicate/orphaned sub-tables
/// are impossible; the result is round-trip parsed before it is returned.
fn upsert_provider_in_config(
    config_content: &str,
    name: &str,
    provider: toml_edit::Table,
) -> Result<String, ApiError> {
    let mut doc = config_content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| ApiError::Internal(format!("Existing eavs config is not valid TOML: {e}")))?;
    let providers = doc["providers"].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let providers = providers
        .as_table_mut()
        .ok_or_else(|| ApiError::Internal("eavs config `providers` is not a table".to_string()))?;
    providers.remove(name);
    providers.insert(name, toml_edit::Item::Table(provider));
    let out = doc.to_string();
    out.parse::<toml_edit::DocumentMut>()
        .map_err(|e| ApiError::Internal(format!("produced invalid eavs config TOML: {e}")))?;
    Ok(out)
}

/// Structurally remove the `providers.<name>` subtree, returning revalidated TOML.
fn remove_provider_in_config(config_content: &str, name: &str) -> Result<String, ApiError> {
    let mut doc = config_content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| ApiError::Internal(format!("Existing eavs config is not valid TOML: {e}")))?;
    if let Some(providers) = doc
        .get_mut("providers")
        .and_then(toml_edit::Item::as_table_mut)
    {
        providers.remove(name);
    }
    let out = doc.to_string();
    out.parse::<toml_edit::DocumentMut>()
        .map_err(|e| ApiError::Internal(format!("produced invalid eavs config TOML: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod eavs_config_tests {
    use super::*;

    // Config that reproduces the octo-azure outage shape: a provider with an
    // existing [providers.X.compat] sub-table, surrounded by other providers.
    const CONFIG: &str = r#"[providers.foundry]
type = "openai-compatible"
api_key = "env:FOUNDRY_API_KEY"

[providers.fhgenie]
type = "openai-compatible"
api_key = "env:FHGENIE_API_KEY"
base_url = "https://fhgenie.example/v1/"

[providers.fhgenie.compat]
supports_developer_role = false

[[providers.fhgenie.models]]
id = "m1"
name = "model one"
reasoning = false

[providers.other]
type = "openai-compatible"
"#;

    fn provider(dev_role: bool) -> toml_edit::Table {
        let mut prov = toml_edit::Table::new();
        prov["type"] = toml_edit::value("openai-compatible");
        prov["api_key"] = toml_edit::value("env:FHGENIE_API_KEY");
        prov["base_url"] = toml_edit::value("https://fhgenie.example/v1/");
        let mut compat = toml_edit::Table::new();
        compat["supports_developer_role"] = toml_edit::value(dev_role);
        prov["compat"] = toml_edit::Item::Table(compat);
        prov
    }

    #[test]
    fn upsert_over_existing_compat_never_duplicates() {
        // The exact regression: replacing a provider that already has a compat
        // sub-table must not leave an orphaned/duplicate [providers.X.compat].
        let out = upsert_provider_in_config(CONFIG, "fhgenie", provider(true)).expect("upsert");
        assert_eq!(
            out.matches("[providers.fhgenie.compat]").count(),
            1,
            "exactly one compat table expected, got:\n{out}"
        );
        // Result parses, and other providers are untouched.
        let doc: toml::Value = toml::from_str(&out).expect("valid TOML");
        let providers = doc["providers"].as_table().unwrap();
        assert!(providers.contains_key("foundry"));
        assert!(providers.contains_key("other"));
        assert_eq!(
            providers["fhgenie"]["compat"]["supports_developer_role"]
                .as_bool()
                .unwrap(),
            true
        );
        // Stale models from the previous definition are gone (subtree replaced).
        assert!(providers["fhgenie"].get("models").is_none());
    }

    #[test]
    fn upsert_repeated_edits_stay_valid_and_single() {
        let mut cur = CONFIG.to_string();
        for i in 0..5 {
            cur = upsert_provider_in_config(&cur, "fhgenie", provider(i % 2 == 0)).expect("upsert");
            toml::from_str::<toml::Value>(&cur).expect("valid TOML each iteration");
            assert_eq!(cur.matches("[providers.fhgenie.compat]").count(), 1);
        }
    }

    #[test]
    fn remove_takes_whole_subtree() {
        let out = remove_provider_in_config(CONFIG, "fhgenie").expect("remove");
        assert!(
            !out.contains("providers.fhgenie"),
            "subtree fully removed:\n{out}"
        );
        let doc: toml::Value = toml::from_str(&out).expect("valid TOML");
        let providers = doc["providers"].as_table().unwrap();
        assert!(!providers.contains_key("fhgenie"));
        assert!(providers.contains_key("foundry") && providers.contains_key("other"));
    }

    #[tokio::test]
    async fn atomic_write_refuses_invalid_and_preserves_prior() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_toml_config_atomic(&path, "[providers.a]\ntype = \"x\"\n")
            .await
            .expect("valid write");
        // A duplicate-key document (the crash shape) must be rejected...
        let bad = "[providers.a.compat]\nx = 1\n[providers.a]\n[providers.a.compat]\ny = 2\n";
        assert!(write_toml_config_atomic(&path, bad).await.is_err());
        // ...and the previously good file must be untouched (no partial/temp).
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, "[providers.a]\ntype = \"x\"\n");
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "no temp files should remain");
    }
}

/// Restart the eavs systemd service via oqto-usermgr (which runs as root).
async fn restart_eavs_service(single_user: bool) -> Result<(), ApiError> {
    if single_user {
        // Single-user mode: restart the user systemd service directly
        let output = tokio::process::Command::new("systemctl")
            .args(["--user", "restart", "eavs"])
            .output()
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to run systemctl: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("eavs restart via systemctl --user failed: {}", stderr);
            // Not fatal — eavs may be running in a tmux pane or manually
        }
    } else {
        // Multi-user mode: use usermgr daemon to restart (it runs as root)
        tokio::task::spawn_blocking(|| {
            crate::local::linux_users::usermgr_request(
                "restart-service",
                serde_json::json!({"service": "eavs"}),
            )
        })
        .await
        .map_err(|e| ApiError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApiError::Internal(format!("Failed to restart eavs: {e}")))?;
    }

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

    let mut url = format!(
        "{}/catalog/lookup?model_id={}",
        eavs.base_url(),
        urlencoding::encode(&query.model_id)
    );
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

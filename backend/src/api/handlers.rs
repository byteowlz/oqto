//! API request handlers.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header::SET_COOKIE},
    response::sse::{Event, KeepAlive, Sse},
    response::{AppendHeaders, IntoResponse},
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_stream::{StreamExt, wrappers::IntervalStream};
use tracing::{info, instrument, warn};

use crate::auth::{AuthError, CurrentUser, RequireAdmin};
use crate::observability::{CpuTimes, HostMetrics, read_host_metrics};
use crate::session::{CreateSessionRequest, Session, SessionContainerStats};
use crate::user::{
    CreateUserRequest, UpdateUserRequest, UserInfo as DbUserInfo, UserListQuery, UserStats,
};

use super::error::{ApiError, ApiResult};
use super::state::AppState;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
struct AdminMetricsSnapshot {
    pub timestamp: String,
    pub host: Option<HostMetrics>,
    pub containers: Vec<SessionContainerStats>,
    pub error: Option<String>,
}

/// Health check endpoint.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Feature flags exposed to the frontend.
#[derive(Debug, Serialize)]
pub struct FeaturesResponse {
    /// Whether mmry (memories) integration is enabled.
    pub mmry_enabled: bool,
}

/// Get enabled features/capabilities.
pub async fn features(State(state): State<AppState>) -> Json<FeaturesResponse> {
    Json(FeaturesResponse {
        mmry_enabled: state.mmry.enabled,
    })
}

/// Session response with URLs.
#[derive(Debug, Serialize)]
pub struct SessionWithUrls {
    #[serde(flatten)]
    pub session: Session,
    pub urls: SessionUrls,
}

/// URLs for accessing session services.
#[derive(Debug, Serialize)]
pub struct SessionUrls {
    pub opencode: String,
    pub fileserver: String,
    pub terminal: String,
}

impl SessionWithUrls {
    pub fn from_session(session: Session, host: &str) -> Self {
        let _ = host;
        let urls = SessionUrls {
            opencode: format!("/session/{}/code", session.id),
            fileserver: format!("/session/{}/files", session.id),
            terminal: format!("/session/{}/term", session.id),
        };
        Self { session, urls }
    }
}

/// List all sessions.
#[instrument(skip(state))]
pub async fn list_sessions(State(state): State<AppState>) -> ApiResult<Json<Vec<Session>>> {
    let sessions = state.sessions.list_sessions().await?;
    info!(count = sessions.len(), "Listed sessions");
    Ok(Json(sessions))
}

/// Get a specific session.
#[instrument(skip(state))]
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<Session>> {
    state
        .sessions
        .get_session(&session_id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("Session {} not found", session_id)))
}

/// Create a new session.
#[instrument(skip(state, request), fields(workspace_path = ?request.workspace_path))]
pub async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> ApiResult<(StatusCode, Json<SessionWithUrls>)> {
    let session = state.sessions.create_session(request).await?;
    info!(session_id = %session.id, "Created new session");

    // TODO: Get actual host from request headers
    let response = SessionWithUrls::from_session(session, "localhost");
    Ok((StatusCode::CREATED, Json(response)))
}

/// Stop a session.
#[instrument(skip(state))]
pub async fn stop_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    // First check if session exists
    let session = state.sessions.get_session(&session_id).await?;

    if session.is_none() {
        return Err(ApiError::not_found(format!(
            "Session {} not found",
            session_id
        )));
    }

    state.sessions.stop_session(&session_id).await?;
    info!(session_id = %session_id, "Stopped session");

    Ok(StatusCode::NO_CONTENT)
}

/// Delete a session.
#[instrument(skip(state))]
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    // Uses centralized From<anyhow::Error> conversion
    state.sessions.delete_session(&session_id).await?;

    info!(session_id = %session_id, "Deleted session");
    Ok(StatusCode::NO_CONTENT)
}

/// Resume a stopped session.
///
/// This restarts a stopped container, which is faster than creating a new session
/// because the container already exists with all its state preserved.
#[instrument(skip(state))]
pub async fn resume_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state.sessions.resume_session(&session_id).await?;
    info!(session_id = %session_id, "Resumed session");

    let response = SessionWithUrls::from_session(session, "localhost");
    Ok(Json(response))
}

/// Get or create a session.
///
/// This is the preferred way to get a session. It will:
/// 1. Resume an existing stopped session if one exists (fast)
/// 2. Create a new session if no resumable session exists
#[instrument(skip(state, request), fields(workspace_path = ?request.workspace_path))]
pub async fn get_or_create_session(
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state.sessions.get_or_create_session(request).await?;
    info!(session_id = %session.id, status = ?session.status, "Got or created session");

    let response = SessionWithUrls::from_session(session, "localhost");
    Ok(Json(response))
}

/// Request for getting or creating a session for a specific workspace.
#[derive(Debug, Deserialize)]
pub struct GetOrCreateForWorkspaceRequest {
    /// Path to the workspace directory.
    pub workspace_path: String,
}

/// Get or create a session for a specific workspace path.
///
/// This is the preferred way to resume a session from chat history.
/// It will:
/// 1. Find an existing running session for the workspace (if any)
/// 2. Enforce LRU cap by stopping oldest idle session if needed
/// 3. Create a new session for that workspace (if none running)
#[instrument(skip(state, request), fields(workspace_path = %request.workspace_path))]
pub async fn get_or_create_session_for_workspace(
    State(state): State<AppState>,
    Json(request): Json<GetOrCreateForWorkspaceRequest>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state
        .sessions
        .get_or_create_session_for_workspace(&request.workspace_path)
        .await?;
    info!(
        session_id = %session.id,
        workspace_path = %request.workspace_path,
        status = ?session.status,
        "Got or created session for workspace"
    );

    let response = SessionWithUrls::from_session(session, "localhost");
    Ok(Json(response))
}

/// Touch session activity (update last_activity_at).
///
/// This should be called when the user interacts with the session
/// (e.g., sends a message, runs a command).
#[instrument(skip(state))]
pub async fn touch_session_activity(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    state.sessions.touch_session_activity(&session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Check if a session has an available image update.
#[instrument(skip(state))]
pub async fn check_session_update(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionUpdateStatus>> {
    let update_available = state.sessions.check_for_image_update(&session_id).await?;

    Ok(Json(SessionUpdateStatus {
        session_id,
        update_available: update_available.is_some(),
        new_digest: update_available,
    }))
}

/// Upgrade a session to the latest image version.
#[instrument(skip(state))]
pub async fn upgrade_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state.sessions.upgrade_session(&session_id).await?;
    info!(session_id = %session_id, "Upgraded session");

    let response = SessionWithUrls::from_session(session, "localhost");
    Ok(Json(response))
}

/// Response for session update check.
#[derive(Debug, Serialize)]
pub struct SessionUpdateStatus {
    pub session_id: String,
    pub update_available: bool,
    pub new_digest: Option<String>,
}

/// Check all sessions for available updates.
#[instrument(skip(state))]
pub async fn check_all_updates(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<SessionUpdateStatus>>> {
    let updates = state.sessions.check_all_for_updates().await?;

    let statuses: Vec<SessionUpdateStatus> = updates
        .into_iter()
        .map(|(session_id, new_digest)| SessionUpdateStatus {
            session_id,
            update_available: true,
            new_digest: Some(new_digest),
        })
        .collect();

    info!(count = statuses.len(), "Checked all sessions for updates");
    Ok(Json(statuses))
}

// ============================================================================
// Project/Workspace Handlers
// ============================================================================

/// Query for listing workspace directories.
#[derive(Debug, Deserialize)]
pub struct WorkspaceDirQuery {
    pub path: Option<String>,
}

/// Workspace directory entry.
#[derive(Debug, Serialize)]
pub struct WorkspaceDirEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub entry_type: String,
}

/// List directories under the workspace root (projects view).
#[instrument(skip(state))]
pub async fn list_workspace_dirs(
    State(state): State<AppState>,
    Query(query): Query<WorkspaceDirQuery>,
) -> ApiResult<Json<Vec<WorkspaceDirEntry>>> {
    let root = state.sessions.workspace_root();
    let relative = query.path.unwrap_or_else(|| ".".to_string());
    let rel_path = std::path::PathBuf::from(&relative);

    if rel_path.is_absolute() || rel_path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(ApiError::bad_request("invalid path"));
    }

    let target = root.join(&rel_path);
    let entries = std::fs::read_dir(&target)
        .with_context(|| format!("reading workspace directory {:?}", target))
        .map_err(|e| ApiError::internal(format!("Failed to list workspace directories: {}", e)))?;

    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| ApiError::internal(format!("Failed to read directory entry: {}", e)))?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry
                .file_name()
                .to_str()
                .unwrap_or_default()
                .to_string();
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            dirs.push(WorkspaceDirEntry {
                name,
                path: if rel.is_empty() { ".".to_string() } else { rel },
                entry_type: "directory".to_string(),
            });
        }
    }

    dirs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(dirs))
}

// ============================================================================
// Authentication Handlers
// ============================================================================

/// Login request for dev mode.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

/// User info in login response.
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub email: String,
    pub role: String,
}

/// Dev mode login endpoint.
pub async fn dev_login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, AuthError> {
    // Only works in dev mode
    if !state.auth.is_dev_mode() {
        return Err(AuthError::InvalidCredentials);
    }

    // Validate credentials
    let user = state
        .auth
        .validate_dev_credentials(&request.username, &request.password)
        .ok_or(AuthError::InvalidCredentials)?;

    // Generate token
    let token = state.auth.generate_dev_token(user)?;

    // Build cookie with security flags
    // In dev mode, omit Secure flag to allow http://localhost
    // In production, always include Secure flag
    let secure_flag = if state.auth.is_dev_mode() {
        ""
    } else {
        " Secure;"
    };
    let cookie = format!(
        "auth_token={}; Path=/; HttpOnly; SameSite=Lax;{} Max-Age={}",
        token,
        secure_flag,
        60 * 60 * 24
    );

    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(LoginResponse {
            token,
            user: UserInfo {
                id: user.id.clone(),
                name: user.name.clone(),
                email: user.email.clone(),
                role: user.role.to_string(),
            },
        }),
    ))
}

/// Get current user info.
#[allow(dead_code)]
pub async fn get_current_user(user: CurrentUser) -> Json<UserInfo> {
    Json(UserInfo {
        id: user.id().to_string(),
        name: user.display_name().to_string(),
        email: user.claims.email.clone().unwrap_or_default(),
        role: user.role().to_string(),
    })
}

/// Registration request.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub invite_code: String,
    pub display_name: Option<String>,
}

/// Registration response.
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub token: String,
    pub user: UserInfo,
}

/// Register a new user with invite code.
///
/// This operation is designed to be safe against race conditions:
/// 1. Atomically consume the invite code (prevents double-use)
/// 2. Create the user
/// 3. If user creation fails, restore the invite code use
#[instrument(skip(state, request), fields(username = %request.username))]
pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> ApiResult<impl IntoResponse> {
    // Atomically consume the invite code first.
    // This prevents TOCTOU race conditions where two requests could both
    // validate and then both try to use the same single-use code.
    let _invite_code_id = state
        .invites
        .try_consume_atomic(&request.invite_code, "pending") // Use "pending" as placeholder
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    // Create the user
    let user = match state
        .users
        .create_user(CreateUserRequest {
            username: request.username.clone(),
            email: request.email.clone(),
            password: Some(request.password),
            display_name: request.display_name,
            role: None, // Default to user role
            external_id: None,
        })
        .await
    {
        Ok(user) => user,
        Err(e) => {
            // User creation failed - restore the invite code use
            // This is best-effort; if it fails, we log but don't change the error
            if let Err(restore_err) = state.invites.restore_use(&request.invite_code).await {
                warn!(
                    "Failed to restore invite code use after user creation failure: {:?}",
                    restore_err
                );
            }
            return Err(e.into());
        }
    };

    // Update the invite code to record the actual user ID
    // This is informational and not critical for correctness
    if let Err(e) = sqlx::query("UPDATE invite_codes SET used_by = ? WHERE code = ?")
        .bind(&user.id)
        .bind(&request.invite_code)
        .execute(state.invites.pool())
        .await
    {
        warn!("Failed to update invite code used_by: {:?}", e);
    }

    // Generate JWT token for the new user
    let token = state.auth.generate_token(
        &user.id,
        &user.email,
        &user.display_name,
        &user.role.to_string(),
    )?;

    // Build cookie
    let secure_flag = if state.auth.is_dev_mode() {
        ""
    } else {
        " Secure;"
    };
    let cookie = format!(
        "auth_token={}; Path=/; HttpOnly; SameSite=Lax;{} Max-Age={}",
        token,
        secure_flag,
        60 * 60 * 24 // 24 hours
    );

    info!(user_id = %user.id, username = %user.username, "User registered successfully");

    Ok((
        StatusCode::CREATED,
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(RegisterResponse {
            token,
            user: UserInfo {
                id: user.id,
                name: user.display_name,
                email: user.email,
                role: user.role.to_string(),
            },
        }),
    ))
}

/// Login endpoint (works with database users).
#[instrument(skip(state, request), fields(username = %request.username))]
pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    // Try to verify against database users first
    let user = state
        .users
        .verify_credentials(&request.username, &request.password)
        .await?;

    let (token, user_info) = match user {
        Some(db_user) => {
            // Database user found and verified
            let token = state.auth.generate_token(
                &db_user.id,
                &db_user.email,
                &db_user.display_name,
                &db_user.role.to_string(),
            )?;
            let user_info = UserInfo {
                id: db_user.id,
                name: db_user.display_name,
                email: db_user.email,
                role: db_user.role.to_string(),
            };
            (token, user_info)
        }
        None => {
            // Fall back to dev mode credentials if enabled
            if state.auth.is_dev_mode() {
                let dev_user = state
                    .auth
                    .validate_dev_credentials(&request.username, &request.password)
                    .ok_or_else(|| ApiError::unauthorized("Invalid username or password"))?;

                let token = state.auth.generate_dev_token(dev_user)?;
                let user_info = UserInfo {
                    id: dev_user.id.clone(),
                    name: dev_user.name.clone(),
                    email: dev_user.email.clone(),
                    role: dev_user.role.to_string(),
                };
                (token, user_info)
            } else {
                return Err(ApiError::unauthorized("Invalid username or password"));
            }
        }
    };

    // Build cookie
    let secure_flag = if state.auth.is_dev_mode() {
        ""
    } else {
        " Secure;"
    };
    let cookie = format!(
        "auth_token={}; Path=/; HttpOnly; SameSite=Lax;{} Max-Age={}",
        token,
        secure_flag,
        60 * 60 * 24 // 24 hours
    );

    info!(user_id = %user_info.id, "User logged in successfully");

    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(LoginResponse {
            token,
            user: user_info,
        }),
    ))
}

/// Logout endpoint (clears auth cookie).
pub async fn logout() -> impl IntoResponse {
    // Clear the auth cookie by setting it to empty with immediate expiry
    let cookie = "auth_token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";

    (
        AppendHeaders([(SET_COOKIE, cookie.to_string())]),
        StatusCode::NO_CONTENT,
    )
}

// ============================================================================
// Admin Handlers
// ============================================================================

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
    // Uses centralized From<anyhow::Error> conversion
    let user = state.users.create_user(request).await?;

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

/// Get current user profile.
#[instrument(skip(state, user))]
pub async fn get_me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<DbUserInfo>> {
    // Try to get user from database (uses centralized From<anyhow::Error> conversion)
    if let Some(db_user) = state.users.get_user(user.id()).await? {
        return Ok(Json(db_user.into()));
    }

    // Fallback to creating UserInfo from JWT claims
    Ok(Json(DbUserInfo {
        id: user.id().to_string(),
        username: user
            .claims
            .preferred_username
            .clone()
            .unwrap_or_else(|| user.id().to_string()),
        email: user.claims.email.clone().unwrap_or_default(),
        display_name: user.display_name().to_string(),
        avatar_url: None,
        role: user.role().into(),
        is_active: true,
        created_at: String::new(),
        last_login_at: None,
    }))
}

/// Update current user profile.
#[instrument(skip(state, user, request))]
pub async fn update_me(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<UpdateMeRequest>,
) -> ApiResult<Json<DbUserInfo>> {
    // Only allow updating display_name, avatar_url, and settings
    let update = UpdateUserRequest {
        display_name: request.display_name,
        avatar_url: request.avatar_url,
        settings: request.settings,
        ..Default::default()
    };

    // Uses centralized From<anyhow::Error> conversion
    let updated = state.users.update_user(user.id(), update).await?;

    Ok(Json(updated.into()))
}

/// Request body for updating own profile.
#[derive(Debug, Deserialize)]
pub struct UpdateMeRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub settings: Option<String>,
}

// Helper to convert auth Role to user Role
impl From<crate::auth::Role> for crate::user::UserRole {
    fn from(role: crate::auth::Role) -> Self {
        match role {
            crate::auth::Role::Admin => crate::user::UserRole::Admin,
            crate::auth::Role::User => crate::user::UserRole::User,
        }
    }
}

// ============================================================================
// Invite Code Management Handlers (Admin)
// ============================================================================

use crate::invite::{
    BatchCreateInviteCodesRequest, CreateInviteCodeRequest, InviteCodeListQuery, InviteCodeSummary,
};

/// List all invite codes (admin only).
#[instrument(skip(state, user))]
pub async fn list_invite_codes(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Query(query): Query<InviteCodeListQuery>,
) -> ApiResult<Json<Vec<InviteCodeSummary>>> {
    let _ = user;
    let codes = state.invites.list(query).await?;
    let summaries: Vec<InviteCodeSummary> = codes.into_iter().map(|c| c.into()).collect();
    info!(count = summaries.len(), "Listed invite codes");
    Ok(Json(summaries))
}

/// Create a single invite code (admin only).
#[instrument(skip(state, user, request))]
pub async fn create_invite_code(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Json(request): Json<CreateInviteCodeRequest>,
) -> ApiResult<(StatusCode, Json<InviteCodeSummary>)> {
    let code = state.invites.create(request, user.id()).await?;
    info!(code_id = %code.id, "Created invite code");
    Ok((StatusCode::CREATED, Json(code.into())))
}

/// Create multiple invite codes at once (admin only).
#[instrument(skip(state, user, request))]
pub async fn create_invite_codes_batch(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Json(request): Json<BatchCreateInviteCodesRequest>,
) -> ApiResult<(StatusCode, Json<Vec<InviteCodeSummary>>)> {
    let codes = state
        .invites
        .create_batch(
            request.count,
            request.uses_per_code,
            request.expires_in_secs,
            request.prefix.as_deref(),
            request.note.as_deref(),
            user.id(),
        )
        .await?;

    let summaries: Vec<InviteCodeSummary> = codes.into_iter().map(|c| c.into()).collect();
    info!(count = summaries.len(), "Created batch of invite codes");
    Ok((StatusCode::CREATED, Json(summaries)))
}

/// Get a specific invite code (admin only).
#[instrument(skip(state, user))]
pub async fn get_invite_code(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Path(code_id): Path<String>,
) -> ApiResult<Json<InviteCodeSummary>> {
    let _ = user;
    state
        .invites
        .get(&code_id)
        .await?
        .map(|c| Json(c.into()))
        .ok_or_else(|| ApiError::not_found(format!("Invite code {} not found", code_id)))
}

/// Revoke an invite code (admin only).
#[instrument(skip(state, user))]
pub async fn revoke_invite_code(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Path(code_id): Path<String>,
) -> ApiResult<StatusCode> {
    let _ = user;
    state.invites.revoke(&code_id).await?;
    info!(code_id = %code_id, "Revoked invite code");
    Ok(StatusCode::NO_CONTENT)
}

/// Delete an invite code (admin only).
#[instrument(skip(state, user))]
pub async fn delete_invite_code(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Path(code_id): Path<String>,
) -> ApiResult<StatusCode> {
    let _ = user;
    state.invites.delete(&code_id).await?;
    info!(code_id = %code_id, "Deleted invite code");
    Ok(StatusCode::NO_CONTENT)
}

/// Get invite code statistics (admin only).
#[derive(Debug, Serialize)]
pub struct InviteCodeStats {
    pub total: i64,
    pub valid: i64,
}

#[instrument(skip(state, user))]
pub async fn get_invite_code_stats(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
) -> ApiResult<Json<InviteCodeStats>> {
    let _ = user;
    let total = state.invites.count().await?;
    let valid = state.invites.count_valid().await?;
    Ok(Json(InviteCodeStats { total, valid }))
}

// ============================================================================
// Agent Management Handlers
// ============================================================================

use super::super::agent::{
    AgentExecRequest, AgentExecResponse, AgentInfo, CreateAgentRequest, CreateAgentResponse,
    StartAgentRequest, StartAgentResponse, StopAgentResponse,
};

#[derive(Debug, Deserialize)]
pub struct AgentListQuery {
    #[serde(default)]
    pub include_context: bool,
}

/// List all agents for a session (running + available directories).
#[instrument(skip(state))]
pub async fn list_agents(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<AgentListQuery>,
) -> ApiResult<Json<Vec<AgentInfo>>> {
    let agents = state
        .agents
        .list_agents(&session_id, query.include_context)
        .await?;
    info!(session_id = %session_id, count = agents.len(), "Listed agents");
    Ok(Json(agents))
}

/// Get a specific agent.
#[instrument(skip(state))]
pub async fn get_agent(
    State(state): State<AppState>,
    Path((session_id, agent_id)): Path<(String, String)>,
    Query(query): Query<AgentListQuery>,
) -> ApiResult<Json<AgentInfo>> {
    state
        .agents
        .get_agent(&session_id, &agent_id, query.include_context)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("Agent {} not found", agent_id)))
}

/// Start an agent in a subdirectory.
#[instrument(skip(state, request), fields(directory = ?request.directory))]
pub async fn start_agent(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<StartAgentRequest>,
) -> ApiResult<(StatusCode, Json<StartAgentResponse>)> {
    let response = state
        .agents
        .start_agent(&session_id, &request.directory)
        .await?;
    info!(
        session_id = %session_id,
        agent_id = %response.id,
        port = response.port,
        "Started agent"
    );
    Ok((StatusCode::CREATED, Json(response)))
}

/// Stop an agent.
#[instrument(skip(state))]
pub async fn stop_agent(
    State(state): State<AppState>,
    Path((session_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<StopAgentResponse>> {
    let response = state.agents.stop_agent(&session_id, &agent_id).await?;
    info!(session_id = %session_id, agent_id = %agent_id, stopped = response.stopped, "Stopped agent");
    Ok(Json(response))
}

/// Rediscover agents after control plane restart.
#[instrument(skip(state))]
pub async fn rediscover_agents(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    state.agents.rediscover_agents(&session_id).await?;
    info!(session_id = %session_id, "Rediscovered agents");
    Ok(StatusCode::NO_CONTENT)
}

/// Create a new agent directory with AGENTS.md file.
#[instrument(skip(state, request), fields(name = ?request.name))]
pub async fn create_agent(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<CreateAgentRequest>,
) -> ApiResult<(StatusCode, Json<CreateAgentResponse>)> {
    let response = state
        .agents
        .create_agent(
            &session_id,
            &request.name,
            &request.description,
            request.scaffold.as_ref(),
        )
        .await?;
    info!(
        session_id = %session_id,
        agent_id = %response.id,
        directory = %response.directory,
        "Created agent"
    );
    Ok((StatusCode::CREATED, Json(response)))
}

/// Execute a command in a session workspace.
#[instrument(skip(state, request), fields(command = %request.command))]
pub async fn exec_agent_command(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<AgentExecRequest>,
) -> ApiResult<Json<AgentExecResponse>> {
    let response = state.agents.exec_command(&session_id, request).await?;
    Ok(Json(response))
}

// ============================================================================
// Chat History Handlers
// ============================================================================

use crate::history::ChatSession;

/// Query parameters for listing chat history.
#[derive(Debug, Deserialize)]
pub struct ChatHistoryQuery {
    /// Filter by workspace path.
    pub workspace: Option<String>,
    /// Include child sessions (default: false).
    #[serde(default)]
    pub include_children: bool,
    /// Maximum number of sessions to return.
    pub limit: Option<usize>,
}

/// List all chat sessions from OpenCode history.
///
/// This reads sessions directly from disk without requiring a running OpenCode instance.
#[instrument(skip(_state))]
pub async fn list_chat_history(
    State(_state): State<AppState>,
    Query(query): Query<ChatHistoryQuery>,
) -> ApiResult<Json<Vec<ChatSession>>> {
    let sessions = crate::history::list_sessions()
        .map_err(|e| ApiError::internal(format!("Failed to list chat history: {}", e)))?;

    let mut filtered: Vec<ChatSession> = sessions
        .into_iter()
        .filter(|s| {
            // Filter by workspace if specified
            if let Some(ref ws) = query.workspace {
                if s.workspace_path != *ws {
                    return false;
                }
            }
            // Filter out child sessions unless explicitly included
            if !query.include_children && s.is_child {
                return false;
            }
            true
        })
        .collect();

    // Apply limit if specified
    if let Some(limit) = query.limit {
        filtered.truncate(limit);
    }

    info!(count = filtered.len(), "Listed chat history");
    Ok(Json(filtered))
}

/// Get a specific chat session by ID.
#[instrument]
pub async fn get_chat_session(
    Path(session_id): Path<String>,
) -> ApiResult<Json<ChatSession>> {
    crate::history::get_session(&session_id)
        .map_err(|e| ApiError::internal(format!("Failed to get chat session: {}", e)))?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("Chat session {} not found", session_id)))
}

/// Response for grouped chat history.
#[derive(Debug, Serialize)]
pub struct GroupedChatHistory {
    pub workspace_path: String,
    pub project_name: String,
    pub sessions: Vec<ChatSession>,
}

/// List chat sessions grouped by workspace/project.
#[instrument(skip(_state))]
pub async fn list_chat_history_grouped(
    State(_state): State<AppState>,
    Query(query): Query<ChatHistoryQuery>,
) -> ApiResult<Json<Vec<GroupedChatHistory>>> {
    let grouped = crate::history::list_sessions_grouped()
        .map_err(|e| ApiError::internal(format!("Failed to list chat history: {}", e)))?;

    let mut result: Vec<GroupedChatHistory> = grouped
        .into_iter()
        .map(|(workspace_path, mut sessions)| {
            // Filter out child sessions unless explicitly included
            if !query.include_children {
                sessions.retain(|s| !s.is_child);
            }
            
            // Apply limit per workspace
            if let Some(limit) = query.limit {
                sessions.truncate(limit);
            }

            let project_name = sessions
                .first()
                .map(|s| s.project_name.clone())
                .unwrap_or_else(|| crate::history::project_name_from_path(&workspace_path));

            GroupedChatHistory {
                workspace_path,
                project_name,
                sessions,
            }
        })
        .filter(|g| !g.sessions.is_empty())
        .collect();

    // Sort by most recently updated session in each group
    result.sort_by(|a, b| {
        let a_updated = a.sessions.first().map(|s| s.updated_at).unwrap_or(0);
        let b_updated = b.sessions.first().map(|s| s.updated_at).unwrap_or(0);
        b_updated.cmp(&a_updated)
    });

    info!(count = result.len(), "Listed grouped chat history");
    Ok(Json(result))
}

use crate::history::ChatMessage;

/// Query parameters for chat messages endpoint.
#[derive(Debug, Deserialize)]
pub struct ChatMessagesQuery {
    /// If true, include pre-rendered HTML for text parts (slower but saves client CPU)
    #[serde(default)]
    pub render: bool,
}

/// Get all messages for a chat session.
///
/// This reads messages and their parts directly from OpenCode's storage on disk.
/// Uses async I/O with caching for better performance on large sessions.
///
/// Query params:
/// - `render=true`: Include pre-rendered markdown HTML in `text_html` field
#[instrument]
pub async fn get_chat_messages(
    Path(session_id): Path<String>,
    Query(query): Query<ChatMessagesQuery>,
) -> ApiResult<Json<Vec<ChatMessage>>> {
    let messages = if query.render {
        crate::history::get_session_messages_rendered(&session_id).await
    } else {
        crate::history::get_session_messages_async(&session_id).await
    }
    .map_err(|e| ApiError::internal(format!("Failed to get chat messages: {}", e)))?;

    info!(session_id = %session_id, count = messages.len(), render = query.render, "Listed chat messages");
    Ok(Json(messages))
}

// ============================================================================
// AgentRPC Handlers (new unified backend API)
// ============================================================================

use crate::agent_rpc::{self, Conversation as RpcConversation, Message as RpcMessage, HealthStatus as RpcHealthStatus, SessionHandle, SendMessagePart};

/// Request to start a new agent session.
#[derive(Debug, Deserialize)]
pub struct StartAgentSessionRequest {
    /// Working directory for the session
    pub workdir: String,
    /// Model to use (optional)
    pub model: Option<String>,
    /// Agent/mode to use (optional, passed to opencode via --agent flag)
    pub agent: Option<String>,
    /// Session ID to resume (optional)
    pub resume_session_id: Option<String>,
    /// Project ID for shared project sessions (optional)
    pub project_id: Option<String>,
}

/// Request to send a message to an agent session.
#[derive(Debug, Deserialize)]
pub struct SendAgentMessageRequest {
    /// Message text
    pub text: String,
    /// Model override (optional)
    pub model: Option<agent_rpc::MessageModel>,
}

/// List conversations via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_list_conversations(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<RpcConversation>>> {
    let backend = state.agent_backend.as_ref().ok_or_else(|| {
        ApiError::internal("AgentRPC backend not enabled")
    })?;

    let conversations = backend.list_conversations(user.id()).await
        .map_err(|e| ApiError::internal(format!("Failed to list conversations: {}", e)))?;

    info!(user_id = %user.id(), count = conversations.len(), "Listed agent conversations");
    Ok(Json(conversations))
}

/// Get a specific conversation via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_get_conversation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(conversation_id): Path<String>,
) -> ApiResult<Json<RpcConversation>> {
    let backend = state.agent_backend.as_ref().ok_or_else(|| {
        ApiError::internal("AgentRPC backend not enabled")
    })?;

    let conversation = backend.get_conversation(user.id(), &conversation_id).await
        .map_err(|e| ApiError::internal(format!("Failed to get conversation: {}", e)))?
        .ok_or_else(|| ApiError::not_found(format!("Conversation {} not found", conversation_id)))?;

    Ok(Json(conversation))
}

/// Get messages for a conversation via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_get_messages(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(conversation_id): Path<String>,
) -> ApiResult<Json<Vec<RpcMessage>>> {
    let backend = state.agent_backend.as_ref().ok_or_else(|| {
        ApiError::internal("AgentRPC backend not enabled")
    })?;

    let messages = backend.get_messages(user.id(), &conversation_id).await
        .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

    info!(user_id = %user.id(), conversation_id = %conversation_id, count = messages.len(), "Listed agent messages");
    Ok(Json(messages))
}

/// Start a new agent session via AgentBackend.
#[instrument(skip(state, user, request))]
pub async fn agent_start_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<StartAgentSessionRequest>,
) -> ApiResult<(StatusCode, Json<SessionHandle>)> {
    let backend = state.agent_backend.as_ref().ok_or_else(|| {
        ApiError::internal("AgentRPC backend not enabled")
    })?;

    let opts = agent_rpc::StartSessionOpts {
        model: request.model,
        agent: request.agent,
        resume_session_id: request.resume_session_id,
        project_id: request.project_id,
        env: std::collections::HashMap::new(),
    };

    let workdir = std::path::Path::new(&request.workdir);
    let handle = backend.start_session(user.id(), workdir, opts).await
        .map_err(|e| ApiError::internal(format!("Failed to start session: {}", e)))?;

    info!(user_id = %user.id(), session_id = %handle.session_id, "Started agent session");
    Ok((StatusCode::CREATED, Json(handle)))
}

/// Send a message to an agent session via AgentBackend.
#[instrument(skip(state, user, request))]
pub async fn agent_send_message(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Json(request): Json<SendAgentMessageRequest>,
) -> ApiResult<StatusCode> {
    let backend = state.agent_backend.as_ref().ok_or_else(|| {
        ApiError::internal("AgentRPC backend not enabled")
    })?;

    let send_request = agent_rpc::SendMessageRequest {
        parts: vec![SendMessagePart::Text { text: request.text }],
        model: request.model,
    };

    backend.send_message(user.id(), &session_id, send_request).await
        .map_err(|e| ApiError::internal(format!("Failed to send message: {}", e)))?;

    info!(user_id = %user.id(), session_id = %session_id, "Sent message to agent session");
    Ok(StatusCode::ACCEPTED)
}

/// Stop an agent session via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_stop_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    let backend = state.agent_backend.as_ref().ok_or_else(|| {
        ApiError::internal("AgentRPC backend not enabled")
    })?;

    backend.stop_session(user.id(), &session_id).await
        .map_err(|e| ApiError::internal(format!("Failed to stop session: {}", e)))?;

    info!(user_id = %user.id(), session_id = %session_id, "Stopped agent session");
    Ok(StatusCode::NO_CONTENT)
}

/// Get the session URL for an agent session.
#[instrument(skip(state, user))]
pub async fn agent_get_session_url(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionUrlResponse>> {
    let backend = state.agent_backend.as_ref().ok_or_else(|| {
        ApiError::internal("AgentRPC backend not enabled")
    })?;

    let url = backend.get_session_url(user.id(), &session_id).await
        .map_err(|e| ApiError::internal(format!("Failed to get session URL: {}", e)))?;

    Ok(Json(SessionUrlResponse { session_id, url }))
}

/// Response for session URL query.
#[derive(Debug, Serialize)]
pub struct SessionUrlResponse {
    pub session_id: String,
    pub url: Option<String>,
}

/// Health check for the AgentRPC backend.
#[instrument(skip(state))]
pub async fn agent_health(
    State(state): State<AppState>,
) -> ApiResult<Json<RpcHealthStatus>> {
    let backend = state.agent_backend.as_ref().ok_or_else(|| {
        ApiError::internal("AgentRPC backend not enabled")
    })?;

    let health = backend.health().await
        .map_err(|e| ApiError::internal(format!("Health check failed: {}", e)))?;

    Ok(Json(health))
}

/// Attach to a session's event stream via AgentBackend.
///
/// Returns an SSE stream of agent events (messages, tool calls, etc.).
#[instrument(skip(state, user))]
pub async fn agent_attach(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let backend = state.agent_backend.as_ref().ok_or_else(|| {
        ApiError::internal("AgentRPC backend not enabled")
    })?;

    let event_stream = backend.attach(user.id(), &session_id).await
        .map_err(|e| ApiError::internal(format!("Failed to attach to session: {}", e)))?;

    // Convert AgentEvent stream to SSE Event stream
    let sse_stream = tokio_stream::StreamExt::map(event_stream, |result| {
        match result {
            Ok(event) => {
                // Serialize the event to JSON
                match serde_json::to_string(&event) {
                    Ok(json) => Ok(Event::default().data(json)),
                    Err(e) => {
                        warn!("Failed to serialize agent event: {}", e);
                        Ok(Event::default().data(format!(r#"{{"error":"{}"}}"#, e)))
                    }
                }
            }
            Err(e) => {
                warn!("Error in agent event stream: {}", e);
                Ok(Event::default().data(format!(r#"{{"error":"{}"}}"#, e)))
            }
        }
    });

    info!(user_id = %user.id(), session_id = %session_id, "Attached to agent session event stream");
    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

//! Authentication handlers.

use axum::{
    Json,
    extract::State,
    http::{StatusCode, header::SET_COOKIE},
    response::{AppendHeaders, IntoResponse},
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, instrument, warn};

use crate::auth::{AuthError, CurrentUser};
use crate::user::{CreateUserRequest, UpdateUserRequest, UserInfo as DbUserInfo};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

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

    if let Some(ref linux_users) = state.linux_users {
        let (uid, linux_username) = linux_users
            .ensure_user(&user.id)
            .map_err(|e| AuthError::Internal(format!("Failed to initialize user runtime: {e:#}")))?;

        if state.mmry.enabled && !state.mmry.single_user {
            if let Err(e) = linux_users.ensure_mmry_config_for_user(
                &linux_username,
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
    }

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
    // This prevents TOQTOU race conditions where two requests could both
    // validate and then both try to use the same single-use code.
    let _invite_code_id = state
        .invites
        .try_consume_atomic(&request.invite_code, "pending") // Use "pending" as placeholder
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    // SECURITY: In multi-user mode, generate a user_id that won't collide with existing
    // Linux users BEFORE creating the DB user. This avoids the need for rollback.
    let user_id = if let Some(ref linux_users) = state.linux_users {
        match linux_users.generate_unique_user_id(&request.username) {
            Ok(id) => Some(id),
            Err(e) => {
                // Restore the invite code
                if let Err(restore_err) = state.invites.restore_use(&request.invite_code).await {
                    warn!(
                        "Failed to restore invite code after ID generation failure: {:?}",
                        restore_err
                    );
                }
                return Err(ApiError::internal(format!(
                    "Failed to generate user ID: {}",
                    e
                )));
            }
        }
    } else {
        None
    };

    // Create the database user (with pre-generated ID if in multi-user mode)
    let user = match if let Some(id) = &user_id {
        state
            .users
            .create_user_with_id(
                id,
                CreateUserRequest {
                    username: request.username.clone(),
                    email: request.email.clone(),
                    password: Some(request.password),
                    display_name: request.display_name,
                    role: None,
                    external_id: None,
                },
            )
            .await
    } else {
        state
            .users
            .create_user(CreateUserRequest {
                username: request.username.clone(),
                email: request.email.clone(),
                password: Some(request.password),
                display_name: request.display_name,
                role: None,
                external_id: None,
            })
            .await
    } {
        Ok(user) => user,
        Err(e) => {
            // User creation failed - restore the invite code use
            if let Err(restore_err) = state.invites.restore_use(&request.invite_code).await {
                warn!(
                    "Failed to restore invite code use after user creation failure: {:?}",
                    restore_err
                );
            }
            return Err(e.into());
        }
    };

    // SECURITY: Create Linux user if multi-user isolation is enabled.
    // Since we pre-generated a unique ID, this should succeed unless there's a system error.
    if let Some(ref linux_users) = state.linux_users {
        match linux_users.ensure_user_with_verification(
            &user.id,
            user.linux_username.as_deref(),
            user.linux_uid.map(|u| u as u32),
        ) {
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
                    "Created Linux user for registered user"
                );
            }
            Err(e) => {
                // This shouldn't happen since we pre-checked, but handle it safely.
                // Use {:?} to log the full anyhow error chain (context + root cause).
                error!(
                    user_id = %user.id,
                    error = ?e,
                    "Failed to create Linux user - deleting database user"
                );

                // Delete the database user to maintain isolation invariant
                if let Err(delete_err) = state.users.delete_user(&user.id).await {
                    error!(
                        user_id = %user.id,
                        error = ?delete_err,
                        "Failed to delete user after Linux user creation failure"
                    );
                }

                // Restore the invite code
                if let Err(restore_err) = state.invites.restore_use(&request.invite_code).await {
                    warn!(
                        "Failed to restore invite code after rollback: {:?}",
                        restore_err
                    );
                }

                return Err(ApiError::internal(format!(
                    "Failed to create user account: {:?}. Please contact an administrator.",
                    e
                )));
            }
        }
    }

    // Update the invite code to record the actual user ID
    if let Err(e) = sqlx::query("UPDATE invite_codes SET used_by = ? WHERE code = ?")
        .bind(&user.id)
        .bind(&request.invite_code)
        .execute(state.invites.pool())
        .await
    {
        warn!("Failed to update invite code used_by: {:?}", e);
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
            if let Some(ref linux_users) = state.linux_users {
                let ensure_result = if let (Some(ref linux_username), Some(linux_uid)) =
                    (db_user.linux_username.as_ref(), db_user.linux_uid)
                {
                    linux_users.ensure_user_with_verification(
                        &db_user.id,
                        Some(linux_username),
                        Some(linux_uid as u32),
                    )
                } else {
                    linux_users.ensure_user(&db_user.id)
                };

                match ensure_result {
                    Ok((uid, actual_linux_username)) => {
                        if db_user.linux_username.as_deref() != Some(actual_linux_username.as_str())
                            || db_user.linux_uid != Some(uid as i64)
                        {
                            if let Err(e) = state
                                .users
                                .update_user(
                                    &db_user.id,
                                    crate::user::UpdateUserRequest {
                                        linux_username: Some(actual_linux_username.clone()),
                                        linux_uid: Some(uid as i64),
                                        ..Default::default()
                                    },
                                )
                                .await
                            {
                                warn!(
                                    user_id = %db_user.id,
                                    error = %e,
                                    "Failed to store linux_username/uid in database"
                                );
                            }
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
                                    user_id = %db_user.id,
                                    error = %e,
                                    "Failed to update mmry config for user"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        return Err(ApiError::internal(format!(
                            "Failed to initialize user runtime: {e:#}"
                        )));
                    }
                }
            }

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

                if let Some(ref linux_users) = state.linux_users {
                    let (uid, linux_username) =
                        linux_users.ensure_user(&dev_user.id).map_err(|e| {
                            ApiError::internal(format!("Failed to initialize user runtime: {e}"))
                        })?;

                    if state.mmry.enabled && !state.mmry.single_user {
                        if let Err(e) = linux_users.ensure_mmry_config_for_user(
                            &linux_username,
                            uid,
                            &state.mmry.host_service_url,
                            state.mmry.host_api_key.as_deref(),
                            &state.mmry.default_model,
                            state.mmry.dimension,
                        ) {
                            warn!(
                                user_id = %dev_user.id,
                                error = %e,
                                "Failed to update mmry config for user"
                            );
                        }
                    }
                }

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

/// Request body for updating own profile.
#[derive(Debug, Deserialize)]
pub struct UpdateMeRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub settings: Option<String>,
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

/// Request body for changing own password.
#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Change current user's password (self-service).
#[instrument(skip(state, user, request))]
pub async fn change_password(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<ChangePasswordRequest>,
) -> ApiResult<StatusCode> {
    // Look up the user to get their username for credential verification.
    let db_user = state
        .users
        .get_user(user.id())
        .await?
        .ok_or_else(|| ApiError::not_found("User not found"))?;

    // Verify the current password.
    let verified = state
        .users
        .verify_credentials(&db_user.username, &request.current_password)
        .await?;

    if verified.is_none() {
        return Err(ApiError::unauthorized("Current password is incorrect"));
    }

    // Update to new password (service layer handles validation + hashing).
    let update = UpdateUserRequest {
        password: Some(request.new_password),
        ..Default::default()
    };

    state.users.update_user(user.id(), update).await?;
    info!(user_id = %user.id(), "User changed their password");

    Ok(StatusCode::NO_CONTENT)
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

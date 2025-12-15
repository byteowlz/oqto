//! API request handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthError, CurrentUser, RequireAdmin};
use crate::session::{CreateSessionRequest, Session};

use super::state::AppState;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Health check endpoint.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            error: message.into(),
        }
    }
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
        let urls = SessionUrls {
            opencode: format!("http://{}:{}", host, session.opencode_port),
            fileserver: format!("http://{}:{}", host, session.fileserver_port),
            terminal: format!("ws://{}:{}", host, session.ttyd_port),
        };
        Self { session, urls }
    }
}

/// List all sessions.
pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<Session>>, (StatusCode, Json<ErrorResponse>)> {
    state
        .sessions
        .list_sessions()
        .await
        .map(Json)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })
}

/// Get a specific session.
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Session>, (StatusCode, Json<ErrorResponse>)> {
    state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?
        .map(Json)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("session not found")),
            )
        })
}

/// Create a new session.
pub async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<SessionWithUrls>), (StatusCode, Json<ErrorResponse>)> {
    let session = state
        .sessions
        .create_session(request)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    // TODO: Get actual host from request headers
    let response = SessionWithUrls::from_session(session, "localhost");
    Ok((StatusCode::CREATED, Json(response)))
}

/// Stop a session.
pub async fn stop_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // First check if session exists
    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    if session.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("session not found")),
        ));
    }

    state
        .sessions
        .stop_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Delete a session.
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state
        .sessions
        .delete_session(&session_id)
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else if e.to_string().contains("active") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ErrorResponse::new(e.to_string())))
        })?;

    Ok(StatusCode::NO_CONTENT)
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
) -> Result<Json<LoginResponse>, AuthError> {
    // Only works in dev mode
    if !state.auth.is_dev_mode() {
        return Err(AuthError::InvalidCredentials);
    }
    
    // Validate credentials
    let user = state.auth
        .validate_dev_credentials(&request.username, &request.password)
        .ok_or(AuthError::InvalidCredentials)?;
    
    // Generate token
    let token = state.auth.generate_dev_token(user)?;
    
    Ok(Json(LoginResponse {
        token,
        user: UserInfo {
            id: user.id.clone(),
            name: user.name.clone(),
            email: user.email.clone(),
            role: user.role.to_string(),
        },
    }))
}

/// Get current user info.
#[allow(dead_code)]
pub async fn get_current_user(
    user: CurrentUser,
) -> Json<UserInfo> {
    Json(UserInfo {
        id: user.id().to_string(),
        name: user.display_name().to_string(),
        email: user.claims.email.clone().unwrap_or_default(),
        role: user.role().to_string(),
    })
}

// ============================================================================
// Admin Handlers
// ============================================================================

/// List all sessions (admin only).
pub async fn admin_list_sessions(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> Result<Json<Vec<Session>>, (StatusCode, Json<ErrorResponse>)> {
    state
        .sessions
        .list_sessions()
        .await
        .map(Json)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })
}

/// Force stop a session (admin only).
pub async fn admin_force_stop_session(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state
        .sessions
        .stop_session(&session_id)
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ErrorResponse::new(e.to_string())))
        })?;

    Ok(StatusCode::NO_CONTENT)
}

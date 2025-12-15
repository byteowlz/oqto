//! API request handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

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

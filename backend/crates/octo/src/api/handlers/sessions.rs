//! Session CRUD handlers.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

use crate::auth::CurrentUser;
use crate::session::{CreateSessionRequest, Session};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

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

/// List all sessions for the authenticated user.
#[instrument(skip(state, user))]
pub async fn list_sessions(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Session>>> {
    let sessions = state.sessions.for_user(user.id()).list_sessions().await?;
    info!(count = sessions.len(), "Listed sessions");
    Ok(Json(sessions))
}

/// Get a specific session by ID or readable alias.
///
/// The session_id parameter can be either:
/// - A full session UUID (e.g., "6a03da55-2757-4d71-b421-af929bc4aef5")
/// - A readable alias (e.g., "foxy-geek")
#[instrument(skip(state))]
pub async fn get_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<Session>> {
    if let Some(session) = state
        .sessions
        .for_user(user.id())
        .get_session(&session_id)
        .await?
    {
        return Ok(Json(session));
    }

    Err(ApiError::not_found(format!(
        "Session {} not found",
        session_id
    )))
}

/// Create a new session.
#[instrument(skip(state, request), fields(workspace_path = ?request.workspace_path))]
pub async fn create_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<CreateSessionRequest>,
) -> ApiResult<(StatusCode, Json<SessionWithUrls>)> {
    let session = state
        .sessions
        .for_user(user.id())
        .create_session(request)
        .await?;
    info!(session_id = %session.id, "Created new session");

    // TODO: Get actual host from request headers
    let response = SessionWithUrls::from_session(session, "localhost");
    Ok((StatusCode::CREATED, Json(response)))
}

/// Stop a session.
#[instrument(skip(state))]
pub async fn stop_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    // First check if session exists
    let session = state
        .sessions
        .for_user(user.id())
        .get_session(&session_id)
        .await?;

    if session.is_none() {
        return Err(ApiError::not_found(format!(
            "Session {} not found",
            session_id
        )));
    }

    state
        .sessions
        .for_user(user.id())
        .stop_session(&session_id)
        .await?;
    info!(session_id = %session_id, "Stopped session");

    Ok(StatusCode::NO_CONTENT)
}

/// Delete a session.
#[instrument(skip(state))]
pub async fn delete_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    // Uses centralized From<anyhow::Error> conversion
    state
        .sessions
        .for_user(user.id())
        .delete_session(&session_id)
        .await?;

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
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state
        .sessions
        .for_user(user.id())
        .resume_session(&session_id)
        .await?;
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
    user: CurrentUser,
    Json(request): Json<CreateSessionRequest>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state
        .sessions
        .for_user(user.id())
        .get_or_create_session(request)
        .await?;
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
    user: CurrentUser,
    Json(request): Json<GetOrCreateForWorkspaceRequest>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state
        .sessions
        .for_user(user.id())
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
#[instrument(skip(state, user))]
pub async fn touch_session_activity(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    state
        .sessions
        .for_user(user.id())
        .touch_session_activity(&session_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Check if a session has an available image update.
#[instrument(skip(state, user))]
pub async fn check_session_update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionUpdateStatus>> {
    let update_available = state
        .sessions
        .for_user(user.id())
        .check_for_image_update(&session_id)
        .await?;

    Ok(Json(SessionUpdateStatus {
        session_id,
        update_available: update_available.is_some(),
        new_digest: update_available,
    }))
}

/// Upgrade a session to the latest image version.
#[instrument(skip(state, user))]
pub async fn upgrade_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state
        .sessions
        .for_user(user.id())
        .upgrade_session(&session_id)
        .await?;
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

/// Request body for browser navigation.
#[derive(Debug, Deserialize)]
pub struct BrowserNavigateRequest {
    pub url: String,
    pub workspace_path: String,
    /// The Pi/chat session ID to use as the agent-browser session name.
    /// This is the session ID the agent knows, so it can control the same browser.
    pub session_id: String,
    /// Optional viewport width in pixels.
    pub viewport_width: Option<u32>,
    /// Optional viewport height in pixels.
    pub viewport_height: Option<u32>,
}

/// Response for browser start.
#[derive(Debug, Serialize)]
pub struct BrowserStartResponse {
    /// The agent-browser session name (short, derived from Pi/chat session ID).
    /// Use this for the browser stream WebSocket.
    pub session_id: String,
}

/// Request body for browser actions.
#[derive(Debug, Deserialize)]
pub struct BrowserActionRequest {
    /// Pi/chat session ID.
    pub session_id: String,
    /// Action name: back, forward, reload.
    pub action: String,
}

/// Start or navigate the agent-browser for a chat session.
///
/// Uses the Pi/chat session ID to derive a short agent-browser session name.
/// The browser stream WebSocket also uses this session ID.
#[instrument(skip(state))]
pub async fn start_browser(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(request): Json<BrowserNavigateRequest>,
) -> ApiResult<Json<BrowserStartResponse>> {
    if !state.sessions.agent_browser_enabled() {
        return Err(ApiError::service_unavailable(
            "Agent browser integration is not enabled",
        ));
    }

    let browser_session_id = crate::agent_browser::browser_session_name(&request.session_id);

    state
        .sessions
        .navigate_browser(
            &browser_session_id,
            &request.url,
            request.viewport_width,
            request.viewport_height,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start browser: {}", e)))?;

    info!(
        session_id = %request.session_id,
        browser_session_id = %browser_session_id,
        workspace = %request.workspace_path,
        url = %request.url,
        "Started/navigated browser"
    );
    Ok(Json(BrowserStartResponse {
        session_id: browser_session_id,
    }))
}

/// Trigger a browser action (back, forward, reload) for a chat session.
#[instrument(skip(state))]
pub async fn browser_action(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(request): Json<BrowserActionRequest>,
) -> ApiResult<StatusCode> {
    if !state.sessions.agent_browser_enabled() {
        return Err(ApiError::service_unavailable(
            "Agent browser integration is not enabled",
        ));
    }

    let action = crate::session::BrowserAction::parse(&request.action).ok_or_else(|| {
        ApiError::bad_request("Invalid browser action (expected back, forward, reload)")
    })?;
    let browser_session_id = crate::agent_browser::browser_session_name(&request.session_id);

    state
        .sessions
        .browser_action(&browser_session_id, action)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to run browser action: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Check all sessions for available updates.
#[instrument(skip(state, user))]
pub async fn check_all_updates(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<SessionUpdateStatus>>> {
    let updates: Vec<(String, String)> = state
        .sessions
        .for_user(user.id())
        .check_all_for_updates()
        .await?;

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

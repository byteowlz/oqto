//! Workspace Pi session API handlers.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use axum::extract::ws::WebSocketUpgrade;
use serde::Deserialize;

use crate::api::handlers::validate_workspace_path;
use crate::api::main_chat_pi::{pi_state_to_response, PiStateResponse};
use crate::api::{ApiError, ApiResult, AppState};

#[derive(Debug, Deserialize)]
pub struct WorkspaceQuery {
    pub workspace_path: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceSessionQuery {
    pub workspace_path: String,
    pub session_id: Option<String>,
}

fn get_workspace_pi_service(state: &AppState) -> ApiResult<&crate::pi_workspace::WorkspacePiService> {
    state
        .workspace_pi
        .as_ref()
        .map(|svc| svc.as_ref())
        .ok_or_else(|| ApiError::internal("Workspace Pi service not enabled"))
}

/// Start a new Pi session for a workspace.
///
/// POST /api/pi/workspace/sessions
pub async fn new_workspace_session(
    State(state): State<AppState>,
    user: crate::auth::CurrentUser,
    Json(req): Json<WorkspaceQuery>,
) -> ApiResult<Json<PiStateResponse>> {
    let svc = get_workspace_pi_service(&state)?;
    let work_dir = validate_workspace_path(&state, user.id(), &req.workspace_path)?;
    let (_session_id, session) = svc
        .start_new_session(user.id(), &work_dir)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start workspace Pi session: {e}")))?;
    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {e}")))?;
    Ok(Json(pi_state_to_response(pi_state)))
}

/// Resume a Pi session for a workspace.
///
/// POST /api/pi/workspace/sessions/{session_id}/resume?workspace_path=...
pub async fn resume_workspace_session(
    State(state): State<AppState>,
    user: crate::auth::CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<WorkspaceQuery>,
) -> ApiResult<Json<PiStateResponse>> {
    let svc = get_workspace_pi_service(&state)?;
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path)?;
    let session = svc
        .resume_session(user.id(), &work_dir, &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resume Pi session: {e}")))?;
    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {e}")))?;
    Ok(Json(pi_state_to_response(pi_state)))
}

/// Get messages from a workspace Pi session file.
///
/// GET /api/pi/workspace/sessions/{session_id}/messages?workspace_path=...
pub async fn get_workspace_session_messages(
    State(state): State<AppState>,
    user: crate::auth::CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<WorkspaceQuery>,
) -> ApiResult<Json<Vec<crate::pi_workspace::PiSessionMessage>>> {
    let svc = get_workspace_pi_service(&state)?;
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path)?;
    let messages = svc
        .get_session_messages(&work_dir, &session_id)
        .map_err(|e| ApiError::internal(format!("Failed to load session messages: {e}")))?;
    Ok(Json(messages))
}

/// Abort a workspace Pi session.
///
/// POST /api/pi/workspace/sessions/{session_id}/abort?workspace_path=...
pub async fn abort_workspace_session(
    State(state): State<AppState>,
    user: crate::auth::CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<WorkspaceQuery>,
) -> ApiResult<StatusCode> {
    let svc = get_workspace_pi_service(&state)?;
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path)?;
    let session = svc
        .get_session(user.id(), &work_dir, &session_id)
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;
    session
        .abort()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to abort: {e}")))?;
    Ok(StatusCode::OK)
}

/// Get current Pi state for a workspace session.
///
/// GET /api/pi/workspace/state?workspace_path=...&session_id=...
pub async fn get_workspace_state(
    State(state): State<AppState>,
    user: crate::auth::CurrentUser,
    Query(query): Query<WorkspaceSessionQuery>,
) -> ApiResult<Json<PiStateResponse>> {
    let session_id = query
        .session_id
        .clone()
        .ok_or_else(|| ApiError::bad_request("session_id is required"))?;
    let svc = get_workspace_pi_service(&state)?;
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path)?;
    let session = svc
        .get_session(user.id(), &work_dir, &session_id)
        .await
        .ok_or_else(|| ApiError::not_found("Pi session not active"))?;
    let pi_state = session
        .get_state()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get Pi state: {e}")))?;
    Ok(Json(pi_state_to_response(pi_state)))
}

/// WebSocket endpoint for streaming Pi events for workspace sessions.
///
/// GET /api/pi/workspace/ws?workspace_path=...&session_id=...
pub async fn ws_handler(
    State(state): State<AppState>,
    user: crate::auth::CurrentUser,
    Query(query): Query<WorkspaceSessionQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let session_id = query
        .session_id
        .clone()
        .ok_or_else(|| ApiError::bad_request("session_id is required"))?;
    let svc = get_workspace_pi_service(&state)?;
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path)?;
    let session = svc
        .resume_session(user.id(), &work_dir, &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resume Pi session: {e}")))?;

    let user_id = user.id().to_string();
    let mmry_state = state.mmry.clone();

    Ok(ws.on_upgrade(move |socket| {
        crate::api::main_chat_pi::handle_ws(socket, session, user_id, None, mmry_state)
    }))
}

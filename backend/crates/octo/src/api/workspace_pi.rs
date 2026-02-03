//! Workspace Pi session API handlers.

use axum::Json;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use sqlx::Row;
use std::future::Future;

use crate::api::handlers::validate_workspace_path;
use crate::api::main_chat_pi::{
    PiModelInfo, PiModelsResponse, PiStateResponse, pi_state_to_response,
};
use crate::api::{ApiError, ApiResult, AppState};
use crate::main_chat::UserPiSession;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct WorkspaceQuery {
    pub workspace_path: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceSessionQuery {
    pub workspace_path: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetPiModelRequest {
    pub provider: String,
    #[serde(rename = "model_id")]
    pub model_id: String,
}

fn get_workspace_pi_service(
    state: &AppState,
) -> ApiResult<&crate::pi_workspace::WorkspacePiService> {
    state
        .workspace_pi
        .as_ref()
        .map(|svc| svc.as_ref())
        .ok_or_else(|| ApiError::internal("Workspace Pi service not enabled"))
}

fn is_runner_writer_error(err: &str) -> bool {
    err.contains("runner pi writer") || err.contains("response channel closed")
}

async fn with_workspace_session_retry<T, F, Fut>(
    svc: &crate::pi_workspace::WorkspacePiService,
    user_id: &str,
    work_dir: &std::path::Path,
    session_id: &str,
    op: F,
) -> ApiResult<T>
where
    F: Fn(Arc<UserPiSession>) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let session = get_or_resume_session(svc, user_id, work_dir, session_id).await?;
    match op(Arc::clone(&session)).await {
        Ok(value) => Ok(value),
        Err(err) if is_runner_writer_error(&err.to_string()) => {
            let _ = svc.remove_session(user_id, work_dir, session_id).await;
            let session = get_or_resume_session(svc, user_id, work_dir, session_id).await?;
            op(session)
                .await
                .map_err(|e| ApiError::internal(format!("Pi session error after restart: {e}")))
        }
        Err(err) => Err(ApiError::internal(format!("Pi session error: {err}"))),
    }
}

async fn get_or_resume_session(
    svc: &crate::pi_workspace::WorkspacePiService,
    user_id: &str,
    work_dir: &std::path::Path,
    session_id: &str,
) -> ApiResult<Arc<UserPiSession>> {
    if let Some(active) = svc.get_session(user_id, work_dir, session_id).await {
        return Ok(active);
    }
    svc.resume_session(user_id, work_dir, session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resume Pi session: {e}")))
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
    let work_dir = validate_workspace_path(&state, user.id(), &req.workspace_path).await?;
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
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path).await?;
    let pi_state = with_workspace_session_retry(
        svc,
        user.id(),
        &work_dir,
        &session_id,
        |session| async move { session.get_state().await },
    )
    .await?;
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
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path).await?;

    let multi_user = state.linux_users.is_some();
    if multi_user {
        let Some(runner) = crate::api::handlers::get_runner_for_user(&state, user.id()) else {
            return Err(ApiError::internal(
                "Chat history service not configured for this user.",
            ));
        };
        let resp = runner
            .get_workspace_chat_messages(query.workspace_path.clone(), session_id.clone(), None)
            .await
            .map_err(|e| {
                ApiError::internal(format!("Runner get_workspace_chat_messages failed: {e}"))
            })?;

        let messages = resp
            .messages
            .into_iter()
            .map(|msg| crate::pi_workspace::PiSessionMessage {
                id: msg.id,
                role: msg.role,
                content: msg.content,
                tool_call_id: None,
                tool_name: None,
                is_error: None,
                timestamp: msg.timestamp,
                usage: None,
            })
            .collect();

        return Ok(Json(messages));
    }

    if let Some(db_path) = crate::history::hstry_db_path() {
        let pool = crate::history::repository::open_hstry_pool(&db_path)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to open hstry DB: {e}")))?;

        let conv_row = sqlx::query(
            r#"
            SELECT id
            FROM conversations
            WHERE source_id = 'pi'
              AND (external_id = ? OR readable_id = ? OR id = ?)
              AND workspace = ?
            LIMIT 1
            "#,
        )
        .bind(&session_id)
        .bind(&session_id)
        .bind(&session_id)
        .bind(&query.workspace_path)
        .fetch_optional(&pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resolve hstry conversation: {e}")))?;

        let conv_row = if conv_row.is_some() {
            conv_row
        } else {
            sqlx::query(
                r#"
                SELECT id
                FROM conversations
                WHERE source_id = 'pi' AND (external_id = ? OR readable_id = ? OR id = ?)
                LIMIT 1
                "#,
            )
            .bind(&session_id)
            .bind(&session_id)
            .bind(&session_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to resolve hstry conversation: {e}")))?
        };

        let Some(conv_row) = conv_row else {
            return Ok(Json(Vec::new()));
        };

        let conversation_id: String = conv_row
            .try_get("id")
            .map_err(|e| ApiError::internal(format!("Failed to read conversation id: {e}")))?;

        let rows = sqlx::query(
            r#"
            SELECT id, role, content, created_at, parts_json
            FROM messages
            WHERE conversation_id = ?
            ORDER BY idx
            "#,
        )
        .bind(&conversation_id)
        .fetch_all(&pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load hstry messages: {e}")))?;

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.try_get("id").unwrap_or_default();
            let role_raw: String = row
                .try_get("role")
                .unwrap_or_else(|_| "assistant".to_string());
            let content_raw: String = row.try_get("content").unwrap_or_default();
            let created_at: Option<i64> = row.try_get("created_at").ok();
            let parts_json: Option<String> = row.try_get("parts_json").ok();

            let role = match role_raw.as_str() {
                "user" => "user",
                "assistant" => "assistant",
                "system" => "system",
                "tool" | "toolResult" => "assistant",
                _ => "assistant",
            }
            .to_string();

            let content = if let Some(parts_json) = parts_json.as_deref()
                && let Ok(v) = serde_json::from_str::<Value>(parts_json)
                && v.is_array()
            {
                v
            } else {
                serde_json::json!([{ "type": "text", "text": content_raw }])
            };

            let timestamp = created_at
                .map(|ts| ts * 1000)
                .unwrap_or_else(|| Utc::now().timestamp_millis());

            messages.push(crate::pi_workspace::PiSessionMessage {
                id,
                role,
                content,
                tool_call_id: None,
                tool_name: None,
                is_error: None,
                timestamp,
                usage: None,
            });
        }

        return Ok(Json(messages));
    }

    let messages = match svc.get_session_messages(user.id(), &work_dir, &session_id) {
        Ok(messages) => messages,
        Err(err) => {
            let msg = err.to_string();
            if msg.contains("Session not found") || msg.contains("Sessions directory not found") {
                Vec::new()
            } else {
                return Err(ApiError::internal(format!(
                    "Failed to load session messages: {err}"
                )));
            }
        }
    };

    Ok(Json(messages))
}

/// Delete a workspace Pi session (soft delete).
///
/// DELETE /api/pi/workspace/sessions/{session_id}?workspace_path=...
pub async fn delete_workspace_session(
    State(state): State<AppState>,
    user: crate::auth::CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<WorkspaceQuery>,
) -> ApiResult<StatusCode> {
    let svc = get_workspace_pi_service(&state)?;
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path).await?;

    let _ = svc.remove_session(user.id(), &work_dir, &session_id).await;
    match svc
        .mark_session_deleted(user.id(), &work_dir, &session_id)
        .await
    {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(err) if err.to_string().contains("Session not found") => {
            Err(ApiError::not_found(format!(
                "Session not found: {}",
                session_id
            )))
        }
        Err(err) => Err(ApiError::internal(format!(
            "Failed to delete workspace Pi session: {}",
            err
        ))),
    }
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
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path).await?;
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
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path).await?;
    let pi_state = with_workspace_session_retry(
        svc,
        user.id(),
        &work_dir,
        &session_id,
        |session| async move { session.get_state().await },
    )
    .await?;
    Ok(Json(pi_state_to_response(pi_state)))
}

/// Get available models for a workspace Pi session.
///
/// GET /api/pi/workspace/models?workspace_path=...&session_id=...
pub async fn get_workspace_models(
    State(state): State<AppState>,
    user: crate::auth::CurrentUser,
    Query(query): Query<WorkspaceSessionQuery>,
) -> ApiResult<Json<PiModelsResponse>> {
    let session_id = query
        .session_id
        .clone()
        .ok_or_else(|| ApiError::bad_request("session_id is required"))?;
    let svc = get_workspace_pi_service(&state)?;
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path).await?;
    let models = with_workspace_session_retry(
        svc,
        user.id(),
        &work_dir,
        &session_id,
        |session: Arc<UserPiSession>| async move { session.get_available_models().await },
    )
    .await?;
    let mapped = models
        .into_iter()
        .map(|model| PiModelInfo {
            id: model.id,
            provider: model.provider,
            name: model.name,
            context_window: model.context_window,
            max_tokens: model.max_tokens,
        })
        .collect();
    Ok(Json(PiModelsResponse { models: mapped }))
}

/// Set the model for a workspace Pi session.
///
/// POST /api/pi/workspace/model?workspace_path=...&session_id=...
pub async fn set_workspace_model(
    State(state): State<AppState>,
    user: crate::auth::CurrentUser,
    Query(query): Query<WorkspaceSessionQuery>,
    Json(req): Json<SetPiModelRequest>,
) -> ApiResult<Json<PiStateResponse>> {
    let session_id = query
        .session_id
        .clone()
        .ok_or_else(|| ApiError::bad_request("session_id is required"))?;
    let svc = get_workspace_pi_service(&state)?;
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path).await?;
    let provider = req.provider.clone();
    let model_id = req.model_id.clone();
    let pi_state = with_workspace_session_retry(
        svc,
        user.id(),
        &work_dir,
        &session_id,
        |session: Arc<UserPiSession>| {
            let provider = provider.clone();
            let model_id = model_id.clone();
            async move {
                session.set_model(&provider, &model_id).await?;
                session.get_state().await
            }
        },
    )
    .await?;
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
    let work_dir = validate_workspace_path(&state, user.id(), &query.workspace_path).await?;
    let session = get_or_resume_session(svc, user.id(), &work_dir, &session_id).await?;

    let user_id = user.id().to_string();
    let hstry_client = state.hstry.clone();

    Ok(ws.on_upgrade(move |socket| {
        crate::api::main_chat_pi::handle_ws(socket, session, user_id, None, None, hstry_client)
    }))
}

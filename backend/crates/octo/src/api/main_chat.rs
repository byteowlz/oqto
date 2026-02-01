//! Main Chat API handlers.
//!
//! Each user has ONE Main Chat (no longer multiple named assistants).
//! The Main Chat stores history, sessions, and configuration.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use tracing::warn;

use crate::auth::CurrentUser;
use crate::main_chat::{
    AssistantInfo, CreateHistoryEntry, CreateSession, HistoryEntry, HistoryEntryType,
    MainChatService, MainChatSession, MainChatTemplates,
};

use super::error::{ApiError, ApiResult};
use super::state::AppState;

// ========== Request/Response Types ==========

/// Request to initialize Main Chat.
#[derive(Debug, Deserialize)]
pub struct InitializeMainChatRequest {
    /// Optional name for the assistant (default: "main")
    pub name: Option<String>,
}

/// Request to update Main Chat metadata.
#[derive(Debug, Deserialize)]
pub struct UpdateMainChatRequest {
    /// New name for the assistant
    pub name: String,
}

/// Request to add a history entry.
#[derive(Debug, Deserialize)]
pub struct AddHistoryRequest {
    /// Entry type
    #[serde(rename = "type")]
    pub entry_type: String,
    /// Content
    pub content: String,
    /// Optional session ID
    pub session_id: Option<String>,
    /// Optional metadata
    pub meta: Option<serde_json::Value>,
}

/// Request to register a session.
#[derive(Debug, Deserialize)]
pub struct RegisterSessionRequest {
    /// OpenCode session ID
    pub session_id: String,
    /// Optional title
    pub title: Option<String>,
}

/// Query params for listing history.
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Maximum entries to return (default 20)
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

/// Response for Main Chat status.
#[derive(Debug, Serialize)]
pub struct MainChatStatusResponse {
    pub exists: bool,
    pub info: Option<AssistantInfo>,
}

/// Response for export.
#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub jsonl: String,
}

// ========== Handlers ==========

/// Get Main Chat status and info.
///
/// GET /api/main
pub async fn get_main_chat(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<MainChatStatusResponse>> {
    let service = get_main_chat_service(&state)?;

    let exists = service.main_chat_exists(user.id());
    let info = if exists {
        Some(
            service
                .get_main_chat_info(user.id())
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get main chat info: {}", e)))?,
        )
    } else {
        None
    };

    Ok(Json(MainChatStatusResponse { exists, info }))
}

/// Initialize Main Chat for the current user.
///
/// POST /api/main
pub async fn initialize_main_chat(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<InitializeMainChatRequest>,
) -> ApiResult<(StatusCode, Json<AssistantInfo>)> {
    let service = get_main_chat_service(&state)?;

    // Validate name if provided
    if let Some(ref name) = req.name {
        if name.is_empty() {
            return Err(ApiError::bad_request("Name cannot be empty"));
        }
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ApiError::bad_request(
                "Name can only contain alphanumeric characters, hyphens, and underscores",
            ));
        }
    }

    if service.main_chat_exists(user.id()) {
        return Err(ApiError::conflict("Main Chat already exists"));
    }

    // Resolve templates from the templates service (if available)
    let templates = if let Some(ref templates_service) = state.onboarding_templates {
        match templates_service.resolve(None).await {
            Ok(resolved) => Some(MainChatTemplates {
                agents: Some(resolved.agents),
                personality: Some(resolved.personality),
                onboard: Some(resolved.onboard),
                bootstrap: None,
                user: Some(resolved.user),
            }),
            Err(e) => {
                warn!(
                    "Failed to resolve onboarding templates, using embedded: {}",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    let info = service
        .initialize_main_chat(user.id(), req.name.as_deref(), templates)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to initialize main chat: {}", e)))?;

    Ok((StatusCode::CREATED, Json(info)))
}

/// Update Main Chat for the current user.
///
/// PATCH /api/main
pub async fn update_main_chat(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<UpdateMainChatRequest>,
) -> ApiResult<Json<AssistantInfo>> {
    let service = get_main_chat_service(&state)?;

    if req.name.is_empty() {
        return Err(ApiError::bad_request("Name cannot be empty"));
    }
    if !req
        .name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::bad_request(
            "Name can only contain alphanumeric characters, hyphens, and underscores",
        ));
    }

    if !service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found("Main Chat not found"));
    }

    let info = service
        .update_main_chat_name(user.id(), &req.name)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update main chat: {}", e)))?;

    Ok(Json(info))
}

/// Delete Main Chat for the current user.
///
/// DELETE /api/main
pub async fn delete_main_chat(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<StatusCode> {
    let service = get_main_chat_service(&state)?;

    if !service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found("Main Chat not found"));
    }

    service
        .delete_main_chat(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete main chat: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get recent history.
///
/// GET /api/main/history
pub async fn get_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<Vec<HistoryEntry>>> {
    let service = get_main_chat_service(&state)?;

    if !service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found("Main Chat not found"));
    }

    let entries = service
        .get_recent_history(user.id(), query.limit)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get history: {}", e)))?;

    Ok(Json(entries))
}

/// Add a history entry.
///
/// POST /api/main/history
pub async fn add_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<AddHistoryRequest>,
) -> ApiResult<(StatusCode, Json<HistoryEntry>)> {
    let service = get_main_chat_service(&state)?;

    if !service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found("Main Chat not found"));
    }

    // Parse entry type
    let entry_type: HistoryEntryType = req
        .entry_type
        .parse()
        .map_err(|e: String| ApiError::bad_request(e))?;

    let entry = service
        .add_history(
            user.id(),
            CreateHistoryEntry {
                entry_type,
                content: req.content,
                session_id: req.session_id,
                meta: req.meta,
            },
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to add history: {}", e)))?;

    Ok((StatusCode::CREATED, Json(entry)))
}

/// Export history as JSONL.
///
/// GET /api/main/export
pub async fn export_history(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<ExportResponse>> {
    let service = get_main_chat_service(&state)?;

    if !service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found("Main Chat not found"));
    }

    let jsonl = service
        .export_history_jsonl(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to export history: {}", e)))?;

    Ok(Json(ExportResponse { jsonl }))
}

/// List sessions.
///
/// GET /api/main/sessions
pub async fn list_sessions(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<MainChatSession>>> {
    let service = get_main_chat_service(&state)?;

    if !service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found("Main Chat not found"));
    }

    let sessions = service
        .list_sessions(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list sessions: {}", e)))?;

    Ok(Json(sessions))
}

/// Register a new session.
///
/// POST /api/main/sessions
pub async fn register_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<RegisterSessionRequest>,
) -> ApiResult<(StatusCode, Json<MainChatSession>)> {
    let service = get_main_chat_service(&state)?;

    if !service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found("Main Chat not found"));
    }

    let session = service
        .add_session(
            user.id(),
            CreateSession {
                session_id: req.session_id,
                title: req.title,
            },
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to register session: {}", e)))?;

    Ok((StatusCode::CREATED, Json(session)))
}

/// Get the latest session.
///
/// GET /api/main/sessions/latest
pub async fn get_latest_session(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Option<MainChatSession>>> {
    let service = get_main_chat_service(&state)?;

    if !service.main_chat_exists(user.id()) {
        return Err(ApiError::not_found("Main Chat not found"));
    }

    let session = service
        .get_latest_session(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get latest session: {}", e)))?;

    Ok(Json(session))
}

// ========== Helper Functions ==========

/// Get the MainChatService from AppState.
fn get_main_chat_service(state: &AppState) -> ApiResult<&MainChatService> {
    state
        .main_chat
        .as_ref()
        .map(|arc| arc.as_ref())
        .ok_or_else(|| ApiError::internal("Main Chat service not configured"))
}

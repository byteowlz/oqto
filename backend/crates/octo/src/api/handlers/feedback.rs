use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;

use crate::auth::CurrentUser;
use crate::feedback::{FeedbackEntry, new_feedback_id, now_rfc3339, write_feedback_entry};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateFeedbackRequest {
    pub title: String,
    pub body: String,
    pub workspace_path: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

pub async fn create_feedback(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<CreateFeedbackRequest>,
) -> ApiResult<(StatusCode, Json<FeedbackEntry>)> {
    if req.title.trim().is_empty() {
        return Err(ApiError::bad_request("title is required"));
    }
    if req.body.trim().is_empty() {
        return Err(ApiError::bad_request("body is required"));
    }

    let entry = FeedbackEntry {
        id: new_feedback_id(),
        title: req.title.trim().to_string(),
        body: req.body.trim().to_string(),
        created_at: now_rfc3339(),
        user_id: user.id().to_string(),
        user_name: Some(user.display_name().to_string()),
        workspace_path: req.workspace_path,
        tags: req.tags,
    };

    write_feedback_entry(&state.feedback, &entry)
        .await
        .map_err(|e| ApiError::internal(format!("failed to write feedback: {}", e)))?;

    Ok((StatusCode::CREATED, Json(entry)))
}

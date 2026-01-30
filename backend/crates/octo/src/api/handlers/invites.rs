//! Invite code management handlers (admin only).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Serialize;
use tracing::{info, instrument};

use crate::auth::RequireAdmin;
use crate::invite::{
    BatchCreateInviteCodesRequest, CreateInviteCodeRequest, InviteCodeListQuery, InviteCodeSummary,
};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

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

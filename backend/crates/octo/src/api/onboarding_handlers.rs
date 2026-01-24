//! Onboarding API handlers.
//!
//! Endpoints for managing user onboarding state:
//! - GET /onboarding - Get current onboarding state
//! - PUT /onboarding - Update onboarding state
//! - POST /onboarding/advance - Advance to next stage
//! - POST /onboarding/unlock/{component} - Unlock a UI component
//! - POST /onboarding/godmode - Activate godmode (skip onboarding)
//! - POST /onboarding/complete - Mark onboarding as complete
//! - POST /onboarding/reset - Reset onboarding state

use axum::{
    Json,
    extract::{Path, State},
};
use tracing::instrument;

use super::error::{ApiError, ApiResult};
use super::state::AppState;
use crate::auth::CurrentUser;
use crate::onboarding::{OnboardingResponse, UnlockComponentRequest, UpdateOnboardingRequest};

/// Get the current onboarding state.
#[instrument(skip(state, user))]
pub async fn get_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .get(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Update the onboarding state.
#[instrument(skip(state, user, request))]
pub async fn update_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<UpdateOnboardingRequest>,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .update(user.id(), request)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Advance to the next onboarding stage.
#[instrument(skip(state, user))]
pub async fn advance_stage(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .advance_stage(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Unlock a UI component.
#[instrument(skip(state, user))]
pub async fn unlock_component(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(component): Path<String>,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .unlock_component(user.id(), UnlockComponentRequest { component })
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Activate godmode (skip onboarding).
#[instrument(skip(state, user))]
pub async fn godmode(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .godmode(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Mark onboarding as complete.
#[instrument(skip(state, user))]
pub async fn complete_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .complete(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Reset onboarding state.
#[instrument(skip(state, user))]
pub async fn reset_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .reset(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Check if user needs onboarding (lightweight endpoint).
#[instrument(skip(state, user))]
pub async fn needs_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    let Some(ref service) = state.onboarding else {
        // If onboarding service is not configured, user doesn't need onboarding
        return Ok(Json(serde_json::json!({
            "needs_onboarding": false,
            "reason": "service_disabled"
        })));
    };

    let onboarding_state = service
        .get(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "needs_onboarding": onboarding_state.needs_onboarding(),
        "stage": onboarding_state.stage,
        "completed": onboarding_state.completed,
        "godmode": onboarding_state.godmode
    })))
}

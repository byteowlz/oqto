use super::{ApiError, ApiResult, AppState};
use crate::{auth::CurrentUser, runner::targets::RunnerTargetStatus};
use axum::{Json, extract::State};

/// Read-only, Account-scoped inventory for any authenticated client. Raw runner
/// endpoints, credentials and errors never cross this boundary.
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<RunnerTargetStatus>>> {
    let targets = state
        .runner_targets
        .list_for_account(user.id())
        .await
        .map_err(|_| ApiError::internal("runner target status is unavailable"))?;
    Ok(Json(targets))
}

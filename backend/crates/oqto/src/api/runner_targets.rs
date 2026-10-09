use super::{ApiError, ApiResult, AppState};
use crate::{auth::CurrentUser, runner::targets::RunnerTargetStatus};
use axum::{
    Json,
    extract::{Path, State},
    http::header,
    response::IntoResponse,
};
use oqto_runner::provider_login::{ProviderLoginOperation, ProviderLoginRequest};

/// Read-only owning-machine history, independent of execution admission.
pub async fn history_read(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(target): Path<String>,
    payload: Result<
        Json<oqto_runner::history_read::HistoryReadOperation>,
        axum::extract::rejection::JsonRejection,
    >,
) -> ApiResult<impl IntoResponse> {
    let Json(operation) = payload.map_err(|_| ApiError::bad_request("invalid history request"))?;
    let client = state
        .runner_targets
        .history_read_client(user.id(), &target)
        .map_err(|_| ApiError::forbidden("history access denied"))?;
    let data = client
        .history_read(oqto_runner::history_read::HistoryReadRequest {
            account_id: user.id().to_owned(),
            operation,
        })
        .await
        .map_err(|_| ApiError::service_unavailable("machine history unavailable"))?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(data)))
}

/// Private machine-scoped auth. No browser-supplied owner, paths, or commit permits.
pub async fn provider_login(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(target): Path<String>,
    payload: Result<Json<ProviderLoginOperation>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<impl IntoResponse> {
    let Json(operation) =
        payload.map_err(|_| ApiError::bad_request("invalid provider login request"))?;
    if matches!(operation, ProviderLoginOperation::Commit { .. }) {
        return Err(ApiError::forbidden(
            "credential commit is control-plane owned",
        ));
    }
    let client = state
        .runner_targets
        .provider_login_client(user.id(), &target)
        .map_err(|_| ApiError::forbidden("provider login denied"))?;
    let is_status = matches!(operation, ProviderLoginOperation::Status { .. });
    let call = |operation| ProviderLoginRequest {
        account_id: user.id().to_owned(),
        operation,
    };
    let mut result = client
        .provider_login(call(operation))
        .await
        .map_err(|_| ApiError::internal("provider login unavailable or rejected"))?;
    if is_status && result["state"].as_str() == Some("awaiting_commit") {
        // Re-resolve current authority AFTER the asynchronous provider flow.
        let client = state
            .runner_targets
            .provider_login_client(user.id(), &target)
            .map_err(|_| ApiError::forbidden("provider login denied"))?;
        let attempt = result["id"]
            .as_str()
            .ok_or_else(|| ApiError::internal("invalid login attempt"))?
            .to_owned();
        let nonce = result["commitNonce"]
            .as_str()
            .ok_or_else(|| ApiError::internal("invalid login permit"))?
            .to_owned();
        result = client
            .provider_login(call(ProviderLoginOperation::Commit { attempt, nonce }))
            .await
            .map_err(|_| ApiError::internal("credential commit unavailable"))?;
    }
    if let Some(object) = result.as_object_mut() {
        object.remove("commitNonce");
    }
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(result),
    ))
}

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

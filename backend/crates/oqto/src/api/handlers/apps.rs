use axum::extract::{Path, Query, State};
use axum::response::Json;
use oqto_protocol::apps::{
    AppCandidateList, AppInstanceList, AppKvDeleteRequest, AppKvGetResponse, AppKvSetRequest,
    AppOperationInvokeRequest, AppOperationResult, AppPermissionDecisionRequest,
    AppPermissionStatus, AppPresentationDocument, AppPublishRequest, AppPublishResult,
};
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, ApiResult, AppState};
use crate::apps::authorize_work_directory;
use crate::apps::{AppPermissionOutcome, AppRuntimeService, AuthorizedWorkDirectory};
use crate::auth::CurrentUser;
use crate::ws::WsEvent;

#[derive(Debug, Deserialize)]
pub struct AppWorkDirectoryQuery {
    pub workspace_path: String,
}

/// HTTP compatibility envelope. Canonical runner-side `oqto.apps.publish`
/// derives the work directory from session authority and uses
/// [`AppPublishRequest`] directly.
#[derive(Debug, Deserialize)]
pub struct AppPublishHttpRequest {
    pub workspace_path: String,
    #[serde(flatten)]
    pub publish: AppPublishRequest,
}

pub async fn list_app_candidates(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<AppWorkDirectoryQuery>,
) -> ApiResult<Json<AppCandidateList>> {
    let apps = state
        .apps
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Oqto App runtime is disabled"))?;
    let work_directory =
        authorize_work_directory(&state, apps.repository(), user.id(), &query.workspace_path)
            .await?;
    let result = apps
        .list_candidates(&work_directory)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to discover Oqto Apps: {error:#}")))?;
    Ok(Json(result))
}

pub async fn publish_app(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<AppPublishHttpRequest>,
) -> ApiResult<Json<AppPublishResult>> {
    let apps = state
        .apps
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Oqto App runtime is disabled"))?;
    let work_directory = authorize_work_directory(
        &state,
        apps.repository(),
        user.id(),
        &request.workspace_path,
    )
    .await?;
    let result = apps
        .publish(user.id(), &work_directory, &request.publish.app_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to publish Oqto App: {error:#}")))?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct AppPresentationHttpRequest {
    pub workspace_path: String,
}

#[derive(Debug, Deserialize)]
pub struct AppFileHttpRequest {
    pub workspace_path: String,
    pub reference: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AppFileResourcesResponse<T> {
    pub resources: Vec<T>,
}

#[derive(Debug, Serialize)]
pub struct AppFileListResponse<T> {
    pub entries: Vec<T>,
}

pub async fn get_app_file_resources(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppFileHttpRequest>,
) -> ApiResult<Json<AppFileResourcesResponse<crate::apps::AppGrantedFileResource>>> {
    let (apps, work_directory) =
        authorized_apps(&state, user.id(), &request.workspace_path).await?;
    apps.file_resources(user.id(), &work_directory, &instance_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to load App files: {error:#}")))?
        .map(|resources| Json(AppFileResourcesResponse { resources }))
        .ok_or_else(|| ApiError::forbidden("App files are not granted"))
}

pub async fn list_app_files(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppFileHttpRequest>,
) -> ApiResult<Json<AppFileListResponse<crate::apps::AppFileEntry>>> {
    let reference = request
        .reference
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("file reference is required"))?;
    let (apps, work_directory) =
        authorized_apps(&state, user.id(), &request.workspace_path).await?;
    apps.file_list(user.id(), &work_directory, &instance_id, reference)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to list App files: {error:#}")))?
        .map(|entries| Json(AppFileListResponse { entries }))
        .ok_or_else(|| ApiError::forbidden("App file reference is not granted"))
}

pub async fn read_app_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppFileHttpRequest>,
) -> ApiResult<Json<crate::apps::AppFileContents>> {
    let reference = request
        .reference
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("file reference is required"))?;
    let (apps, work_directory) =
        authorized_apps(&state, user.id(), &request.workspace_path).await?;
    apps.file_read(user.id(), &work_directory, &instance_id, reference)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to read App file: {error:#}")))?
        .map(Json)
        .ok_or_else(|| ApiError::forbidden("App file reference is not granted"))
}

#[derive(Debug, Deserialize)]
pub struct AppFileWriteHttpRequest {
    pub workspace_path: String,
    pub reference: String,
    pub expected_version: String,
    pub bytes_base64: String,
}

pub async fn write_app_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppFileWriteHttpRequest>,
) -> ApiResult<Json<crate::apps::AppFileWriteResult>> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.bytes_base64)
        .map_err(|_| ApiError::bad_request("file bytes are not valid base64"))?;
    let (apps, work_directory) =
        authorized_apps(&state, user.id(), &request.workspace_path).await?;
    apps.file_write(
        user.id(),
        &work_directory,
        &instance_id,
        &request.reference,
        &request.expected_version,
        &bytes,
    )
    .await
    .map_err(|error| ApiError::internal(format!("Failed to write App file: {error:#}")))?
    .map(Json)
    .ok_or_else(|| ApiError::forbidden("App file reference is not writable"))
}

pub async fn invoke_app_operation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppOperationInvokeRequest>,
) -> ApiResult<Json<AppOperationResult>> {
    let (apps, work_directory) =
        authorized_apps(&state, user.id(), &request.workspace_path).await?;
    apps.invoke_operation(
        user.id(),
        &work_directory,
        &instance_id,
        &request.operation_id,
        &request.input,
    )
    .await
    .map_err(|error| ApiError::internal(format!("App operation failed: {error:#}")))?
    .map(Json)
    .ok_or_else(|| ApiError::forbidden("App operation is not granted"))
}

pub async fn get_app_presentation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppPresentationHttpRequest>,
) -> ApiResult<Json<AppPresentationDocument>> {
    let apps = state
        .apps
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Oqto App runtime is disabled"))?;
    let work_directory = authorize_work_directory(
        &state,
        apps.repository(),
        user.id(),
        &request.workspace_path,
    )
    .await?;
    let instance = apps
        .repository()
        .get_instance_for_work_directory(&instance_id, &work_directory.id)
        .await
        .map_err(|error| {
            ApiError::internal(format!("Failed to load Oqto App Instance: {error:#}"))
        })?
        .ok_or_else(|| ApiError::not_found("App Instance unavailable"))?;
    let html = apps
        .presentation_document(&work_directory, &instance_id)
        .await
        .map_err(|error| {
            ApiError::internal(format!("Failed to render Oqto App presentation: {error:#}"))
        })?
        .ok_or_else(|| ApiError::not_found("App Instance unavailable"))?;
    Ok(Json(AppPresentationDocument {
        instance_id: instance.instance_id,
        presentation_id: "main".to_owned(),
        definition_id: instance.definition_id,
        content_digest: instance.content_digest,
        html,
    }))
}

/// Resolve the App runtime plus a freshly authorized work directory.
///
/// Every permission route re-authorizes both, so a decision can never ride on
/// a path or Account the caller merely referenced earlier.
async fn authorized_apps<'a>(
    state: &'a AppState,
    account_id: &str,
    workspace_path: &str,
) -> Result<(&'a AppRuntimeService, AuthorizedWorkDirectory), ApiError> {
    let apps = state
        .apps
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Oqto App runtime is disabled"))?;
    let work_directory =
        authorize_work_directory(state, apps.repository(), account_id, workspace_path).await?;
    Ok((apps, work_directory))
}

fn permission_outcome(outcome: AppPermissionOutcome) -> ApiResult<Json<AppPermissionStatus>> {
    match outcome {
        AppPermissionOutcome::Applied(status) | AppPermissionOutcome::NotRequired(status) => {
            Ok(Json(*status))
        }
        AppPermissionOutcome::Stale {
            pinned_content_digest,
        } => Err(ApiError::conflict(format!(
            "This App changed since it was reviewed. Review it again before deciding \
             (current version {pinned_content_digest})."
        ))),
        AppPermissionOutcome::InvalidState { instance_status } => Err(ApiError::conflict(format!(
            "This App is {instance_status:?} and has nothing to decide right now."
        ))),
    }
}

pub async fn get_app_permissions(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Query(query): Query<AppWorkDirectoryQuery>,
) -> ApiResult<Json<AppPermissionStatus>> {
    let (apps, work_directory) = authorized_apps(&state, user.id(), &query.workspace_path).await?;
    apps.permission_status(&work_directory, &instance_id)
        .await
        .map_err(|error| ApiError::internal(format!("Failed to load App permissions: {error:#}")))?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("App Instance unavailable"))
}

#[derive(Debug, Deserialize)]
pub struct AppPermissionDecisionHttpRequest {
    pub workspace_path: String,
    #[serde(flatten)]
    pub decision: AppPermissionDecisionRequest,
}

pub async fn decide_app_permissions(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppPermissionDecisionHttpRequest>,
) -> ApiResult<Json<AppPermissionStatus>> {
    let (apps, work_directory) =
        authorized_apps(&state, user.id(), &request.workspace_path).await?;
    let outcome = apps
        .decide_permissions(
            user.id(),
            &work_directory,
            &instance_id,
            request.decision.decision,
            &request.decision.reviewed_content_digest,
        )
        .await
        .map_err(|error| {
            ApiError::internal(format!(
                "Failed to record App permission decision: {error:#}"
            ))
        })?
        .ok_or_else(|| ApiError::not_found("App Instance unavailable"))?;
    permission_outcome(outcome)
}

#[derive(Debug, Deserialize)]
pub struct AppPermissionRevokeHttpRequest {
    pub workspace_path: String,
}

pub async fn revoke_app_permissions(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppPermissionRevokeHttpRequest>,
) -> ApiResult<Json<AppPermissionStatus>> {
    let (apps, work_directory) =
        authorized_apps(&state, user.id(), &request.workspace_path).await?;
    let outcome = apps
        .revoke_permissions(user.id(), &work_directory, &instance_id)
        .await
        .map_err(|error| {
            ApiError::internal(format!("Failed to revoke App permissions: {error:#}"))
        })?
        .ok_or_else(|| ApiError::not_found("App Instance unavailable"))?;
    let response = permission_outcome(outcome)?;
    state
        .ws_hub
        .send_to_user(
            user.id(),
            WsEvent::AppLifecycle {
                instance_id,
                state: "revoked".to_owned(),
            },
        )
        .await;
    Ok(response)
}

#[derive(Debug, Deserialize)]
pub struct AppKvGetQuery {
    pub workspace_path: String,
    pub key: String,
}

pub async fn get_app_kv(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Query(query): Query<AppKvGetQuery>,
) -> ApiResult<Json<AppKvGetResponse>> {
    let (apps, work_directory) = authorized_apps(&state, user.id(), &query.workspace_path).await?;
    let value = apps
        .kv_get(user.id(), &work_directory, &instance_id, &query.key)
        .await
        .map_err(|error| ApiError::bad_request(format!("Invalid App KV request: {error:#}")))?
        .ok_or_else(|| ApiError::forbidden("App KV capability is not active"))?;
    Ok(Json(AppKvGetResponse { value }))
}

pub async fn set_app_kv(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppKvSetRequest>,
) -> ApiResult<Json<AppKvGetResponse>> {
    let (apps, work_directory) =
        authorized_apps(&state, user.id(), &request.workspace_path).await?;
    if !apps
        .kv_set(
            user.id(),
            &work_directory,
            &instance_id,
            &request.key,
            &request.value,
        )
        .await
        .map_err(|error| ApiError::bad_request(format!("Invalid App KV request: {error:#}")))?
    {
        return Err(ApiError::forbidden("App KV capability is not active"));
    }
    Ok(Json(AppKvGetResponse {
        value: Some(request.value),
    }))
}

pub async fn delete_app_kv(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance_id): Path<String>,
    Json(request): Json<AppKvDeleteRequest>,
) -> ApiResult<Json<AppKvGetResponse>> {
    let (apps, work_directory) =
        authorized_apps(&state, user.id(), &request.workspace_path).await?;
    if !apps
        .kv_delete(user.id(), &work_directory, &instance_id, &request.key)
        .await
        .map_err(|error| ApiError::bad_request(format!("Invalid App KV request: {error:#}")))?
    {
        return Err(ApiError::forbidden("App KV capability is not active"));
    }
    Ok(Json(AppKvGetResponse { value: None }))
}

pub async fn list_app_instances(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<AppWorkDirectoryQuery>,
) -> ApiResult<Json<AppInstanceList>> {
    let apps = state
        .apps
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Oqto App runtime is disabled"))?;
    let work_directory =
        authorize_work_directory(&state, apps.repository(), user.id(), &query.workspace_path)
            .await?;
    let result = apps
        .list_instances(&work_directory)
        .await
        .map_err(|error| {
            ApiError::internal(format!("Failed to list Oqto App Instances: {error:#}"))
        })?;
    Ok(Json(result))
}

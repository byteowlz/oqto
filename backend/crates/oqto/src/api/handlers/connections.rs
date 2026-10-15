//! One host gate for App, CLI and native-MCP adapters. Workloads never choose
//! a credential, runner, Account, destination, header, or authorization receipt.
use crate::{
    api::{ApiError, ApiResult, AppState},
    apps::{AppRuntimeService, AuthorizedWorkDirectory},
    auth::CurrentUser,
    credentials::{
        ConnectionRepository, ConnectionService, CredentialReference, RunnerConnectionAudit,
    },
    runner::router::{
        ExecutionTarget, resolve_runner_for_workspace_path, resolve_target_for_workspace_path,
    },
};
use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
};
use oqto_protocol::credential_connections::*;
use oqto_runner::client::RunnerClient;
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionRequest {
    pub workspace_path: String,
    pub connection: ConnectionCreateRequest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindRequest {
    pub workspace_path: String,
    pub binding: AppConnectionBindRequest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeRequest {
    pub workspace_path: String,
    pub expected_revision: u32,
}

struct Host {
    service: ConnectionService,
    runner: RunnerClient,
    apps: Arc<AppRuntimeService>,
    work: AuthorizedWorkDirectory,
}
async fn host(state: &AppState, account: &str, path: &str) -> ApiResult<Host> {
    super::credentials::require_real_auth(state)?;
    let target = resolve_target_for_workspace_path(state, account, path)
        .await
        .map_err(|_| ApiError::forbidden("Connection placement denied"))?;
    // No credential forwarding to shared/unconfined/generated processes. Other
    // placements require their own reviewed provider/placement implementation.
    if !matches!(target, ExecutionTarget::Personal) {
        return Err(ApiError::forbidden(
            "Connections currently require a personal runner",
        ));
    }
    let apps = state
        .apps
        .as_ref()
        .cloned()
        .ok_or_else(|| ApiError::service_unavailable("Apps unavailable"))?;
    let work =
        crate::apps::authorize_work_directory(state, apps.repository(), account, path).await?;
    let runner = resolve_runner_for_workspace_path(state, account, path)
        .await
        .map_err(|_| ApiError::service_unavailable("Connection runner unavailable"))?
        .ok_or_else(|| ApiError::service_unavailable("Connection runner unavailable"))?;
    let service = ConnectionService::new(
        ConnectionRepository::new(apps.repository().pool().clone()),
        Arc::new(RunnerConnectionAudit::new(
            runner.clone(),
            account.to_owned(),
            work.id.clone(),
        )),
    );
    Ok(Host {
        service,
        runner,
        apps,
        work,
    })
}
fn reference(value: CredentialDescriptor) -> CredentialReference {
    CredentialReference {
        provider_id: value.provider_id,
        credential_id: value.credential_id,
        revision: value.revision,
    }
}
fn denied() -> ApiError {
    ApiError::forbidden("Connection use denied or unavailable")
}
fn protected(state: &AppState, peer: SocketAddr, headers: &HeaderMap) -> ApiResult<()> {
    super::credentials::require_real_auth(state)?;
    super::credentials::validate_submission_channel(
        Some(peer),
        headers,
        state.auth.allowed_origins(),
    )
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(credential_id): Path<String>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<ProvisionRequest>,
) -> ApiResult<Json<ConnectionMetadata>> {
    protected(&state, peer, &headers)?;
    let host = host(&state, user.id(), &request.workspace_path).await?;
    let credential = host
        .runner
        .describe_credential(user.id().to_owned(), credential_id)
        .await
        .map_err(|_| denied())?;
    host.service
        .create(user.id(), &request.connection, &reference(credential))
        .await
        .map(Json)
        .map_err(|_| denied())
}
pub async fn bind(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((instance, slot)): Path<(String, String)>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<BindRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    protected(&state, peer, &headers)?;
    let host = host(&state, user.id(), &request.workspace_path).await?;
    let bound = host
        .service
        .bind(
            user.id(),
            &host.apps,
            &host.work,
            &instance,
            &slot,
            &request.binding,
        )
        .await
        .map_err(|_| denied())?;
    if !bound {
        return Err(denied());
    }
    Ok(Json(serde_json::json!({"bound": true})))
}
pub async fn revoke(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<RevokeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    protected(&state, peer, &headers)?;
    let host = host(&state, user.id(), &request.workspace_path).await?;
    if !host
        .service
        .revoke(user.id(), &id, request.expected_revision)
        .await
        .map_err(|_| denied())?
    {
        return Err(denied());
    }
    Ok(Json(serde_json::json!({"revoked": true})))
}

pub async fn invoke(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(instance): Path<String>,
    Json(request): Json<AppConnectionInvokeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let host = host(&state, user.id(), &request.workspace_path).await?;
    // Check the live binding before asking the runner for metadata. A stale
    // credential revision is not silently refreshed/rebound on the user's behalf.
    let bound = host
        .service
        .bound_credential_reference(user.id(), &host.apps, &host.work, &instance, &request.slot)
        .await
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    let current = host
        .runner
        .describe_credential(user.id().to_owned(), bound.credential_id.clone())
        .await
        .map_err(|_| denied())?;
    let invocation = host
        .service
        .authorize_invocation(
            user.id(),
            &host.apps,
            &host.work,
            &instance,
            &request.slot,
            &request.path,
            &reference(current.clone()),
        )
        .await
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    // Authorization linearizes at the service's post-audit live-state check.
    // Revocation denies subsequent dispatches; already-dispatched bounded HTTP
    // may complete (<=10s); it is not claimed to retract delivered credentials.
    let dispatch = CredentialHttpDispatch {
        account_id: user.id().to_owned(),
        workspace_id: host.work.id.clone(),
        credential: current,
        connection_id: invocation.connection_id().to_owned(),
        connection_revision: invocation.connection_revision(),
        instance_id: instance,
        content_digest: invocation.content_digest().to_owned(),
        slot: request.slot,
        audit_event_id: invocation.audit_event_id().to_owned(),
        path: request.path,
        policy: invocation.policy().clone(),
    };
    host.runner
        .credential_http(dispatch)
        .await
        .map(Json)
        .map_err(|_| denied())
}

pub fn routes() -> axum::Router<AppState> {
    use axum::extract::DefaultBodyLimit;
    use axum::routing::post;
    axum::Router::new()
        .route("/credentials/{credential_id}/connections", post(create))
        .route(
            "/apps/instances/{instance}/connections/{slot}/bind",
            post(bind),
        )
        .route("/connections/{id}/revoke", post(revoke))
        .route(
            "/apps/instances/{instance}/connections/invoke",
            post(invoke),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        ))
}

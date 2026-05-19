//! TRX (Issue Tracking) handlers.
//!
//! list/create/update/close go through the per-user oqto-runner, which
//! holds a `trx_core::UnifiedStore` open in process. `sync` still shells
//! out to the `trx` CLI because it is a git operation, runs out-of-band,
//! and is not on the chat-loading hot path.

use std::path::PathBuf;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{info, instrument};

use crate::auth::CurrentUser;

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;
use crate::runner::router::{
    ExecutionTarget, resolve_runner_for_workspace_path, resolve_target_for_workspace_path,
};
use oqto_runner::client::RunnerClient;
use oqto_runner::protocol::TrxIssueData;

/// TRX issue as returned by the API. Mirrors `TrxIssueData` from the
/// runner protocol; we keep a separate public type so the API surface
/// stays under our control independent of the runner wire format.
#[derive(Debug, Serialize, Deserialize)]
pub struct TrxIssue {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: String,
    pub priority: i32,
    pub issue_type: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub closed_at: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

impl From<TrxIssueData> for TrxIssue {
    fn from(d: TrxIssueData) -> Self {
        TrxIssue {
            id: d.id,
            title: d.title,
            description: d.description,
            status: d.status,
            priority: d.priority,
            issue_type: d.issue_type,
            created_at: d.created_at,
            updated_at: d.updated_at,
            closed_at: d.closed_at,
            parent_id: d.parent_id,
            labels: Vec::new(),
            blocked_by: d.blocked_by,
        }
    }
}

/// Request body for creating a TRX issue.
#[derive(Debug, Deserialize)]
pub struct CreateTrxIssueRequest {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_issue_type")]
    pub issue_type: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub parent_id: Option<String>,
}

fn default_issue_type() -> String {
    "task".to_string()
}

fn default_priority() -> i32 {
    2
}

/// Request body for updating a TRX issue.
#[derive(Debug, Deserialize)]
pub struct UpdateTrxIssueRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
}

/// Request body for closing a TRX issue.
#[derive(Debug, Deserialize)]
pub struct CloseTrxIssueRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

/// Query parameters for workspace-based TRX routes.
#[derive(Debug, Deserialize)]
pub struct TrxWorkspaceQuery {
    pub workspace_path: String,
}

/// Validate and resolve a workspace path, ensuring it's within the allowed workspace root
/// or is a valid Main Chat workspace path.
pub async fn validate_workspace_path(
    state: &AppState,
    user_id: &str,
    workspace_path: &str,
) -> Result<PathBuf, ApiError> {
    let session_owner = match resolve_target_for_workspace_path(state, user_id, workspace_path)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resolve workspace target: {e}")))?
    {
        ExecutionTarget::Personal => user_id.to_string(),
        ExecutionTarget::SharedWorkspace { workspace_id } => {
            let sw = state
                .shared_workspaces
                .as_ref()
                .ok_or_else(|| ApiError::internal("Shared workspace service not configured"))?;
            sw.linux_user_for_id(&workspace_id)
                .await
                .map_err(|e| {
                    ApiError::internal(format!(
                        "Failed to resolve shared workspace linux user: {e}"
                    ))
                })?
                .ok_or_else(|| {
                    ApiError::internal("Missing linux user mapping for shared workspace")
                })?
        }
    };

    let canonical = state
        .sessions
        .for_user(&session_owner)
        .validate_workspace_path(workspace_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?;

    Ok(canonical)
}

/// Validate the workspace path and resolve the runner that owns it.
async fn validated_runner(
    state: &AppState,
    user_id: &str,
    workspace_path: &str,
) -> Result<(PathBuf, RunnerClient), ApiError> {
    let canonical = validate_workspace_path(state, user_id, workspace_path).await?;
    let runner = resolve_runner_for_workspace_path(state, user_id, workspace_path)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to resolve runner: {e}")))?
        .ok_or_else(|| ApiError::internal("No runner available for workspace"))?;
    Ok((canonical, runner))
}

/// List TRX issues for a workspace.
#[instrument(skip(state, user))]
pub async fn list_trx_issues(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<TrxWorkspaceQuery>,
) -> ApiResult<Json<Vec<TrxIssue>>> {
    let (canonical, runner) = validated_runner(&state, user.id(), &query.workspace_path).await?;
    info!(
        user_id = %user.id(),
        workspace_path = %query.workspace_path,
        canonical_workspace = %canonical.display(),
        "trx list request resolved"
    );

    let resp = runner
        .trx_list(canonical.clone())
        .await
        .map_err(|e| ApiError::internal(format!("trx list failed: {e}")))?;

    info!(
        user_id = %user.id(),
        canonical_workspace = %canonical.display(),
        issue_count = resp.issues.len(),
        "trx list response"
    );

    Ok(Json(resp.issues.into_iter().map(TrxIssue::from).collect()))
}

/// Create a new TRX issue.
#[instrument(skip(state, user, request))]
pub async fn create_trx_issue(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<TrxWorkspaceQuery>,
    Json(request): Json<CreateTrxIssueRequest>,
) -> ApiResult<Json<TrxIssue>> {
    let (canonical, runner) = validated_runner(&state, user.id(), &query.workspace_path).await?;

    let resp = runner
        .trx_create(
            canonical,
            request.title,
            request.description,
            request.issue_type,
            request.priority,
            request.parent_id,
        )
        .await
        .map_err(|e| ApiError::internal(format!("trx create failed: {e}")))?;

    let issue = TrxIssue::from(resp.issue);
    info!(issue_id = %issue.id, "Created TRX issue");
    Ok(Json(issue))
}

/// Update a TRX issue.
#[instrument(skip(state, user, request))]
pub async fn update_trx_issue(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(issue_id): Path<String>,
    Query(query): Query<TrxWorkspaceQuery>,
    Json(request): Json<UpdateTrxIssueRequest>,
) -> ApiResult<Json<TrxIssue>> {
    let (canonical, runner) = validated_runner(&state, user.id(), &query.workspace_path).await?;

    let resp = runner
        .trx_update(
            canonical,
            issue_id,
            request.title,
            request.description,
            request.status,
            request.priority,
        )
        .await
        .map_err(|e| ApiError::internal(format!("trx update failed: {e}")))?;

    let issue = TrxIssue::from(resp.issue);
    info!(issue_id = %issue.id, "Updated TRX issue");
    Ok(Json(issue))
}

/// Close a TRX issue.
#[instrument(skip(state, user, request))]
pub async fn close_trx_issue(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(issue_id): Path<String>,
    Query(query): Query<TrxWorkspaceQuery>,
    Json(request): Json<CloseTrxIssueRequest>,
) -> ApiResult<Json<TrxIssue>> {
    let (canonical, runner) = validated_runner(&state, user.id(), &query.workspace_path).await?;

    let resp = runner
        .trx_close(canonical, issue_id, request.reason)
        .await
        .map_err(|e| ApiError::internal(format!("trx close failed: {e}")))?;

    let issue = TrxIssue::from(resp.issue);
    info!(issue_id = %issue.id, "Closed TRX issue");
    Ok(Json(issue))
}

/// Sync TRX changes (git add and commit `.trx/`).
///
/// Still shells out to the `trx` CLI: this is a git operation, runs
/// out-of-band on user demand, and is not on the chat-loading hot path.
/// Moving sync into the runner is a follow-up.
#[instrument(skip(state))]
pub async fn sync_trx(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<TrxWorkspaceQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let validated_path = validate_workspace_path(&state, user.id(), &query.workspace_path).await?;

    if let Some(linux_users) = state.linux_users.as_ref().filter(|cfg| cfg.enabled) {
        let linux_username = linux_users.linux_username(user.id());
        let home_dir = linux_users
            .get_home_dir(user.id())
            .map_err(|e| ApiError::internal(format!("Failed to resolve linux user home: {e}")))?
            .unwrap_or_else(|| PathBuf::from(format!("/home/{}", linux_username)));
        let xdg_config = home_dir.join(".config");
        let xdg_data = home_dir.join(".local/share");
        let cwd = validated_path.to_string_lossy().to_string();

        let env = serde_json::json!({
            "XDG_CONFIG_HOME": xdg_config.to_string_lossy(),
            "XDG_DATA_HOME": xdg_data.to_string_lossy(),
        });

        tokio::task::spawn_blocking(move || {
            crate::local::linux_users::usermgr_request(
                "run-as-user",
                serde_json::json!({
                    "username": linux_username,
                    "binary": "trx",
                    "args": ["sync"],
                    "env": env,
                    "cwd": cwd,
                }),
            )
        })
        .await
        .map_err(|e| ApiError::internal(format!("Task join error: {e}")))?
        .map_err(|e| ApiError::internal(format!("trx sync failed: {e}")))?;
    } else {
        let output = Command::new("trx")
            .args(["sync"])
            .current_dir(&validated_path)
            .output()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to execute trx sync: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApiError::internal(format!("trx sync failed: {}", stderr)));
        }
    }

    info!("TRX synced");
    Ok(Json(serde_json::json!({ "synced": true })))
}

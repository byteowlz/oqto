//! TRX (Issue Tracking) handlers.

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

/// Dependency as returned by trx CLI.
#[derive(Debug, Deserialize)]
struct TrxDependency {
    #[allow(dead_code)]
    issue_id: String,
    depends_on_id: String,
    #[serde(rename = "type")]
    dep_type: String,
    #[allow(dead_code)]
    created_at: String,
}

/// Raw TRX issue as returned by `trx list --json`.
#[derive(Debug, Deserialize)]
struct TrxIssueRaw {
    id: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    status: String,
    priority: i32,
    issue_type: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    closed_at: Option<String>,
    #[serde(default)]
    dependencies: Vec<TrxDependency>,
}

/// TRX issue as returned by API (transformed from raw).
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

impl From<TrxIssueRaw> for TrxIssue {
    fn from(raw: TrxIssueRaw) -> Self {
        // Extract parent_id from dependencies with type "parent_child"
        // Also check "blocks" type where depends_on_id is a prefix of issue_id (hierarchical IDs like oqto-k8z1.1 -> oqto-k8z1)
        let parent_id = raw
            .dependencies
            .iter()
            .find(|d| {
                d.dep_type == "parent_child"
                    || (d.dep_type == "blocks"
                        && raw.id.starts_with(&format!("{}.", d.depends_on_id)))
            })
            .map(|d| d.depends_on_id.clone());

        // Extract blocked_by from dependencies with type "blocks"
        let blocked_by: Vec<String> = raw
            .dependencies
            .iter()
            .filter(|d| d.dep_type == "blocks")
            .map(|d| d.depends_on_id.clone())
            .collect();

        TrxIssue {
            id: raw.id,
            title: raw.title,
            description: raw.description,
            status: raw.status,
            priority: raw.priority,
            issue_type: raw.issue_type,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
            closed_at: raw.closed_at,
            parent_id,
            labels: Vec::new(),
            blocked_by,
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
    let canonical = state
        .sessions
        .for_user(user_id)
        .validate_workspace_path(workspace_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?;

    Ok(canonical)
}

/// Execute trx command in a validated workspace directory.
async fn exec_trx_command(
    state: &AppState,
    user_id: &str,
    workspace_path: &str,
    args: &[&str],
) -> Result<String, ApiError> {
    // Validate workspace path before executing command
    let validated_path = validate_workspace_path(state, user_id, workspace_path).await?;

    let mut full_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    full_args.push("--json".to_string());

    if let Some(linux_users) = state.linux_users.as_ref().filter(|cfg| cfg.enabled) {
        // Multi-user mode: delegate to usermgr (oqto runs with NoNewPrivileges)
        let linux_username = linux_users.linux_username(user_id);
        let home_dir = linux_users
            .get_home_dir(user_id)
            .map_err(|e| ApiError::internal(format!("Failed to resolve linux user home: {e}")))?
            .unwrap_or_else(|| PathBuf::from(format!("/home/{}", linux_username)));
        let xdg_config = home_dir.join(".config");
        let xdg_data = home_dir.join(".local/share");

        let cwd = validated_path.to_string_lossy().to_string();

        let env = serde_json::json!({
            "XDG_CONFIG_HOME": xdg_config.to_string_lossy(),
            "XDG_DATA_HOME": xdg_data.to_string_lossy(),
        });

        let result = tokio::task::spawn_blocking(move || {
            crate::local::linux_users::usermgr_request_with_data(
                "run-as-user",
                serde_json::json!({
                    "username": linux_username,
                    "binary": "trx",
                    "args": full_args,
                    "env": env,
                    "cwd": cwd,
                }),
            )
        })
        .await
        .map_err(|e| ApiError::internal(format!("Task join error: {e}")))?
        .map_err(|e| ApiError::internal(format!("trx command failed: {e}")))?;

        let stdout = result
            .as_ref()
            .and_then(|d| d.get("stdout"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(stdout)
    } else {
        // Single-user mode: run directly
        let output = Command::new("trx")
            .args(full_args.iter().map(String::as_str))
            .current_dir(&validated_path)
            .output()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to execute trx: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApiError::internal(format!(
                "trx command failed: {}",
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// List TRX issues for a workspace.
#[instrument(skip(state, user))]
pub async fn list_trx_issues(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<TrxWorkspaceQuery>,
) -> ApiResult<Json<Vec<TrxIssue>>> {
    let output =
        exec_trx_command(&state, user.id(), &query.workspace_path, &["list", "--all"]).await?;

    // Parse the raw JSON output and transform to API format
    let raw_issues: Vec<TrxIssueRaw> = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    let issues: Vec<TrxIssue> = raw_issues.into_iter().map(TrxIssue::from).collect();

    Ok(Json(issues))
}

/// Get a specific TRX issue.
#[instrument(skip(state, user))]
pub async fn get_trx_issue(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(issue_id): Path<String>,
    Query(query): Query<TrxWorkspaceQuery>,
) -> ApiResult<Json<TrxIssue>> {
    let output = exec_trx_command(
        &state,
        user.id(),
        &query.workspace_path,
        &["show", &issue_id],
    )
    .await?;

    let raw_issue: TrxIssueRaw = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    Ok(Json(TrxIssue::from(raw_issue)))
}

/// Create a new TRX issue.
#[instrument(skip(state, user, request))]
pub async fn create_trx_issue(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<TrxWorkspaceQuery>,
    Json(request): Json<CreateTrxIssueRequest>,
) -> ApiResult<Json<TrxIssue>> {
    let mut args = vec!["create", &request.title, "-t", &request.issue_type, "-p"];
    let priority_str = request.priority.to_string();
    args.push(&priority_str);

    if let Some(ref desc) = request.description {
        args.push("-d");
        args.push(desc);
    }

    if let Some(ref parent) = request.parent_id {
        args.push("--parent");
        args.push(parent);
    }

    let output = exec_trx_command(&state, user.id(), &query.workspace_path, &args).await?;

    // trx create --json returns the created issue
    let raw_issue: TrxIssueRaw = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    let issue = TrxIssue::from(raw_issue);
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
    let mut args = vec!["update", &issue_id];

    // Build args based on what's being updated
    let title_arg;
    if let Some(ref title) = request.title {
        args.push("--title");
        title_arg = title.clone();
        args.push(&title_arg);
    }

    let desc_arg;
    if let Some(ref desc) = request.description {
        args.push("--description");
        desc_arg = desc.clone();
        args.push(&desc_arg);
    }

    let status_arg;
    if let Some(ref status) = request.status {
        args.push("--status");
        status_arg = status.clone();
        args.push(&status_arg);
    }

    let priority_arg;
    if let Some(priority) = request.priority {
        args.push("-p");
        priority_arg = priority.to_string();
        args.push(&priority_arg);
    }

    let output = exec_trx_command(&state, user.id(), &query.workspace_path, &args).await?;

    // Parse the updated issue (trx update --json returns a single issue object)
    let raw_issue: TrxIssueRaw = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    let issue = TrxIssue::from(raw_issue);

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
    let mut args = vec!["close", &issue_id];

    let reason_arg;
    if let Some(ref reason) = request.reason {
        args.push("-r");
        reason_arg = reason.clone();
        args.push(&reason_arg);
    }

    let output = exec_trx_command(&state, user.id(), &query.workspace_path, &args).await?;

    // Parse the closed issue (trx close --json returns a single issue object)
    let raw_issue: TrxIssueRaw = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    let issue = TrxIssue::from(raw_issue);

    info!(issue_id = %issue.id, "Closed TRX issue");
    Ok(Json(issue))
}

/// Sync TRX changes (git add and commit .trx/).
#[instrument(skip(state))]
pub async fn sync_trx(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<TrxWorkspaceQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    // Note: trx sync doesn't have JSON output -- use exec_trx_command without --json
    // We pass "sync" and the handler appends --json, but trx sync ignores unknown flags
    // so this is safe. Alternatively, call it directly for single-user.
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

use anyhow::{Context, Result};

use crate::api::AppState;

use super::client::RunnerClient;

/// Canonical backend-resolved execution target.
///
/// Frontend and API layers should identify *what* to run against (target),
/// never *how* (socket path, linux user, runner id). The backend resolves the
/// concrete runner client from this target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionTarget {
    /// Personal runner for the authenticated user.
    Personal,
    /// Shared workspace runner (resolved via workspace -> linux_user mapping).
    SharedWorkspace { workspace_id: String },
}

impl ExecutionTarget {
    /// Stable target id for logging/diagnostics.
    pub fn id(&self, user_id: &str) -> String {
        match self {
            Self::Personal => format!("target:personal:{user_id}"),
            Self::SharedWorkspace { workspace_id } => {
                format!("target:shared:{workspace_id}")
            }
        }
    }
}

/// Resolve a concrete runner client from an execution target.
///
/// This is the single place where target -> runner mapping should live.
pub async fn resolve_runner_for_target(
    state: &AppState,
    user_id: &str,
    target: &ExecutionTarget,
) -> Result<Option<RunnerClient>> {
    match target {
        ExecutionTarget::Personal => Ok(resolve_personal_runner(state, user_id)),
        ExecutionTarget::SharedWorkspace { workspace_id } => {
            resolve_shared_workspace_runner(state, user_id, workspace_id).await
        }
    }
}

fn resolve_personal_runner(state: &AppState, user_id: &str) -> Option<RunnerClient> {
    let pattern = state.runner_socket_pattern.as_ref()?;

    let effective_user = if let Some(ref lu) = state.linux_users {
        lu.linux_username(user_id)
    } else {
        user_id.to_string()
    };

    RunnerClient::for_user_with_pattern(&effective_user, pattern).ok()
}

pub async fn resolve_target_for_workspace_path(
    state: &AppState,
    user_id: &str,
    workspace_path: &str,
) -> Result<ExecutionTarget> {
    if let Some(sw_service) = state.shared_workspaces.as_ref() {
        if let Some((ws, _role)) = sw_service
            .check_access_for_path(workspace_path, user_id)
            .await
            .with_context(|| format!("shared workspace access check for path {}", workspace_path))?
        {
            return Ok(ExecutionTarget::SharedWorkspace {
                workspace_id: ws.id,
            });
        }
    }

    Ok(ExecutionTarget::Personal)
}

pub async fn resolve_runner_for_workspace_path(
    state: &AppState,
    user_id: &str,
    workspace_path: &str,
) -> Result<Option<RunnerClient>> {
    let target = resolve_target_for_workspace_path(state, user_id, workspace_path).await?;
    resolve_runner_for_target(state, user_id, &target).await
}

async fn resolve_shared_workspace_runner(
    state: &AppState,
    user_id: &str,
    workspace_id: &str,
) -> Result<Option<RunnerClient>> {
    let sw_service = state
        .shared_workspaces
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("shared workspaces not configured"))?;

    let (_ws, _role) = sw_service
        .get(workspace_id, user_id)
        .await
        .with_context(|| format!("shared workspace lookup for {}", workspace_id))?
        .ok_or_else(|| anyhow::anyhow!("shared workspace not found or access denied"))?;

    let linux_user = sw_service
        .linux_user_for_id(workspace_id)
        .await
        .with_context(|| format!("shared workspace linux user for {}", workspace_id))?
        .ok_or_else(|| anyhow::anyhow!("shared workspace linux user not found"))?;

    let pattern = state
        .runner_socket_pattern
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("runner socket pattern not configured"))?;

    let client = RunnerClient::for_user_with_pattern(&linux_user, pattern)
        .with_context(|| format!("creating runner client for linux user {}", linux_user))?;

    Ok(Some(client))
}

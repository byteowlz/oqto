use std::collections::HashSet;

use anyhow::{Context, Result};
use log::{info, warn};

use crate::api;
use crate::runner::router::{ExecutionTarget, resolve_runner_for_target};
use crate::session_target::{SessionTargetRecord, SessionTargetScope};
use crate::user::UserListQuery;

async fn backfill_sessions_for_target(
    state: &api::AppState,
    user_id: &str,
    target: &ExecutionTarget,
    owner_user_id: Option<&str>,
) -> Result<usize> {
    let Some(runner) = resolve_runner_for_target(state, user_id, target).await? else {
        return Ok(0);
    };

    if let Err(err) = runner
        .repair_workspace_chat_history(Some(10_000), None)
        .await
    {
        warn!(
            "workspace chat history repair failed before session target backfill (user={} target={:?}): {}",
            user_id, target, err
        );
    }

    let response = runner
        .list_workspace_chat_sessions(None, true, None)
        .await
        .with_context(|| {
            format!(
                "list_workspace_chat_sessions failed for target {:?}",
                target
            )
        })?;

    let mut upserted = 0usize;
    for session in response.sessions {
        let record = match target {
            ExecutionTarget::Personal => SessionTargetRecord {
                session_id: session.id,
                owner_user_id: owner_user_id.map(ToOwned::to_owned),
                scope: SessionTargetScope::Personal,
                workspace_id: None,
                workspace_path: Some(session.workspace_path),
            },
            ExecutionTarget::SharedWorkspace { workspace_id } => SessionTargetRecord {
                session_id: session.id,
                owner_user_id: None,
                scope: SessionTargetScope::SharedWorkspace,
                workspace_id: Some(workspace_id.clone()),
                workspace_path: Some(session.workspace_path),
            },
        };

        state.session_targets.upsert(&record).await?;
        upserted += 1;
    }

    Ok(upserted)
}

pub async fn backfill_session_targets_once(state: &api::AppState) -> Result<()> {
    let users = state
        .users
        .list_users(UserListQuery {
            limit: Some(10_000),
            ..Default::default()
        })
        .await
        .context("listing users for session target backfill")?;

    let mut upserted_total = 0usize;
    let mut processed_shared_workspaces: HashSet<String> = HashSet::new();

    for user in &users {
        upserted_total += backfill_sessions_for_target(
            state,
            &user.id,
            &ExecutionTarget::Personal,
            Some(&user.id),
        )
        .await
        .with_context(|| format!("personal target backfill for user {}", user.id))?;

        let Some(sw_service) = state.shared_workspaces.as_ref() else {
            continue;
        };

        let workspaces = sw_service
            .list_for_user(&user.id)
            .await
            .with_context(|| format!("listing shared workspaces for user {}", user.id))?;

        for workspace in workspaces {
            if !processed_shared_workspaces.insert(workspace.id.clone()) {
                continue;
            }

            let target = ExecutionTarget::SharedWorkspace {
                workspace_id: workspace.id.clone(),
            };

            upserted_total += backfill_sessions_for_target(state, &user.id, &target, None)
                .await
                .with_context(|| {
                    format!("shared target backfill for workspace {}", workspace.id)
                })?;
        }
    }

    info!(
        "session target backfill completed: users={}, shared_workspaces={}, upserted={}",
        users.len(),
        processed_shared_workspaces.len(),
        upserted_total
    );

    Ok(())
}

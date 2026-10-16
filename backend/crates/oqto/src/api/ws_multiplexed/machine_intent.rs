//! A machine hint constrains intent; it never grants execution or selects transport.
use anyhow::{Context, Result, ensure};
use oqto_protocol::commands::{Command, CommandPayload};

use crate::{api::AppState, runner::router::ExecutionTarget, session_target::SessionTargetScope};

/// Check before resolving/dialing any runner, including the personal default.
/// Existing durable identity wins over paths and connection-local metadata.
pub(super) async fn validate(state: &AppState, account: &str, command: &Command) -> Result<()> {
    let Some(machine) = command.runner_id.as_deref().filter(|id| *id != "local") else {
        return Ok(());
    };
    ensure!(
        command.session_id != "_system"
            && !command.session_id.starts_with("pending-")
            && !command.session_id.starts_with("tmp:"),
        "not a public machine Session identity"
    );
    let grant = state.runner_targets.execution_grant(account, machine)?;
    let bound = state.session_targets.get(&command.session_id).await?;
    if let Some(bound) = bound {
        ensure!(
            bound.scope == SessionTargetScope::RemoteMachine
                && bound.owner_user_id.as_deref() == Some(account)
                && bound.workspace_id.as_deref() == Some(machine),
            "machine binding mismatch"
        );
        let path = bound
            .workspace_path
            .context("missing machine work directory")?;
        ensure!(
            grant.permits(std::path::Path::new(&path)),
            "machine root access denied"
        );
        if let CommandPayload::SessionCreate { config } = &command.payload
            && let Some(requested) = &config.cwd
        {
            ensure!(
                std::path::Path::new(requested) == std::path::Path::new(&path),
                "machine work directory changed"
            );
        }
        return Ok(());
    }
    let CommandPayload::SessionCreate { config } = &command.payload else {
        anyhow::bail!("machine Session has no durable binding");
    };
    let path = config
        .cwd
        .as_deref()
        .context("machine work directory required")?;
    ensure!(
        grant.permits(std::path::Path::new(path)),
        "machine root access denied"
    );
    let target =
        crate::runner::router::resolve_target_for_workspace_path(state, account, path).await?;
    ensure!(
        matches!(target, ExecutionTarget::RemoteMachine { machine_id, .. } if machine_id == machine),
        "machine target mismatch"
    );
    Ok(())
}

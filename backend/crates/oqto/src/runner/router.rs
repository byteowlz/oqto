use anyhow::{Context, Result};

use crate::api::AppState;

use oqto_runner::client::RunnerClient;

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

/// Backend-dialable address of a workspace service (fileserver, ttyd,
/// previews). Resolved through the placement layer: host placements share
/// loopback, container placements expose a Unix socket via the runner's
/// reverse bridge. Call sites never branch on placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceTarget {
    Tcp { host: String, port: u16 },
    Unix { path: std::path::PathBuf },
}

/// Resolve how the backend reaches a service listening on `port` inside the
/// placement of `target`'s workspace.
///
/// No placement record means a host placement: loopback is shared, dial it
/// directly. With a record, ask the runner to expose the port (idempotent)
/// and translate the returned container socket path (`/run/oqto/...`) to its
/// host-side location: exposed sockets live in the same bind-mounted
/// directory as the runner socket.
pub async fn resolve_service_target(
    state: &AppState,
    user_id: &str,
    target: &ExecutionTarget,
    port: u16,
) -> Result<ServiceTarget> {
    let localhost = ServiceTarget::Tcp {
        host: "localhost".to_string(),
        port,
    };
    let Some(store) = &state.placement_store else {
        return Ok(localhost);
    };
    let workspace_id = match target {
        ExecutionTarget::Personal => user_id,
        ExecutionTarget::SharedWorkspace { workspace_id } => workspace_id.as_str(),
    };
    let Some(record) = store.find_workspace(workspace_id).await? else {
        return Ok(localhost);
    };
    let client = RunnerClient::from_endpoint(&record.runner_endpoint)
        .with_context(|| format!("building runner endpoint for workspace {workspace_id}"))?;
    exposed_service_target(&client, &record, port).await
}

/// Ask a placement's runner to expose `port` (idempotent) and translate the
/// returned endpoint into a host-side dialable target.
pub async fn exposed_service_target(
    client: &RunnerClient,
    record: &oqto_placement::PlacementRecord,
    port: u16,
) -> Result<ServiceTarget> {
    let workspace_id = &record.workspace_id;
    let endpoint = client
        .expose_port(port)
        .await
        .with_context(|| format!("exposing port {port} for workspace {workspace_id}"))?;
    match endpoint {
        oqto_runner::protocol::ExposedEndpoint::Tcp { host, port } => {
            Ok(ServiceTarget::Tcp { host, port })
        }
        oqto_runner::protocol::ExposedEndpoint::Unix { path } => {
            let oqto_runner::transport::RunnerEndpointConfig::Unix {
                path: runner_socket,
            } = &record.runner_endpoint
            else {
                anyhow::bail!(
                    "workspace {workspace_id} exposed a unix socket over a non-unix runner \
                     endpoint; remote placements need a transport mapping"
                );
            };
            Ok(ServiceTarget::Unix {
                path: translate_exposed_socket(runner_socket, &path)?,
            })
        }
    }
}

/// Map a container-side exposed socket path to its host-side location.
/// Exposed sockets share the bind-mounted directory of the runner socket, so
/// the host path is the runner socket's directory plus the socket file name.
fn translate_exposed_socket(
    host_runner_socket: &std::path::Path,
    exposed: &std::path::Path,
) -> Result<std::path::PathBuf> {
    let dir = host_runner_socket
        .parent()
        .context("runner socket path has no parent directory")?;
    let name = exposed
        .file_name()
        .context("exposed socket path has no file name")?;
    Ok(dir.join(name))
}

/// Resolve a concrete runner client from an execution target.
///
/// This is the single place where target -> runner mapping should live.
pub async fn resolve_runner_for_target(
    state: &AppState,
    user_id: &str,
    target: &ExecutionTarget,
) -> Result<Option<RunnerClient>> {
    if let Some(store) = &state.placement_store {
        let workspace_id = match target {
            ExecutionTarget::Personal => user_id,
            ExecutionTarget::SharedWorkspace { workspace_id } => workspace_id,
        };
        if let Some(endpoint) = store.resolve_workspace(workspace_id).await? {
            let client = RunnerClient::from_endpoint(&endpoint).with_context(|| {
                format!("building runner endpoint for workspace {workspace_id}")
            })?;
            client
                .ensure_ready_with_recovery()
                .await
                .with_context(|| format!("runner not ready for workspace {workspace_id}"))?;
            return Ok(Some(client));
        }

        // Container mode: personal Workspaces are provisioned lazily on first
        // use. Shared Workspaces provision explicitly at creation; a missing
        // shared record falls through to host resolution (mixed placement).
        if matches!(target, ExecutionTarget::Personal)
            && let Some(manager) = &state.placement_manager
        {
            let record = manager
                .provision_personal(user_id)
                .await
                .with_context(|| format!("provisioning personal placement for {user_id}"))?;
            let client = RunnerClient::from_endpoint(&record.runner_endpoint)
                .with_context(|| format!("building runner endpoint for {user_id}"))?;
            wait_for_runner_ready(&client, user_id).await?;

            if let Some(eavs_client) = &state.eavs_client
                && let Err(error) = crate::api::handlers::admin::sync_eavs_models_json_via_runner(
                    eavs_client,
                    &client,
                    std::path::Path::new(oqto_placement::CONTAINER_HOME),
                    user_id,
                    Some(&state.auto_rename_config),
                )
                .await
            {
                tracing::warn!(
                        user_id,
                        %error,
                    "EAVS model config provisioning failed for new personal placement"
                );
            }
            return Ok(Some(client));
        }
    }

    match target {
        ExecutionTarget::Personal => resolve_personal_runner(state, user_id).await,
        ExecutionTarget::SharedWorkspace { workspace_id } => {
            resolve_shared_workspace_runner(state, user_id, workspace_id).await
        }
    }
}

/// Container starts take a few seconds; poll readiness with a bounded budget.
async fn wait_for_runner_ready(client: &RunnerClient, workspace_id: &str) -> Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if client.ensure_ready_with_recovery().await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("runner for {workspace_id} did not become ready within 30s"))
}

async fn ensure_runner_healthy(
    state: &AppState,
    linux_user: &str,
    client: RunnerClient,
) -> Result<RunnerClient> {
    // Health must be O(1): a data-listing probe here scanned every workspace
    // database on each request and dominated chat-open latency.
    if client.ensure_ready_with_recovery().await.is_ok() {
        return Ok(client);
    }

    if state.linux_users.is_none() {
        return Ok(client);
    }

    let uid = resolve_linux_uid(linux_user)
        .with_context(|| format!("resolving uid for linux user {}", linux_user))?;

    crate::local::linux_users::usermgr_request(
        "setup-user-runner",
        serde_json::json!({
            "username": linux_user,
            "uid": uid,
        }),
    )
    .with_context(|| format!("healing runner for linux user {}", linux_user))?;

    let pattern = state
        .runner_socket_pattern
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("runner socket pattern not configured"))?;

    let healed = RunnerClient::for_user_with_pattern(linux_user, pattern).with_context(|| {
        format!(
            "creating healed runner client for linux user {}",
            linux_user
        )
    })?;

    Ok(healed)
}

fn resolve_linux_uid(linux_user: &str) -> Result<u32> {
    use std::process::Command;

    let output = Command::new("id")
        .arg("-u")
        .arg(linux_user)
        .output()
        .with_context(|| format!("running id -u {}", linux_user))?;

    if !output.status.success() {
        anyhow::bail!(
            "id -u {} failed: {}",
            linux_user,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let uid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .with_context(|| format!("parsing uid for linux user {}", linux_user))?;

    Ok(uid)
}

async fn resolve_personal_runner(state: &AppState, user_id: &str) -> Result<Option<RunnerClient>> {
    let effective_user = state.effective_linux_username(user_id);

    let client = if state.linux_users.is_none() {
        RunnerClient::for_user(&effective_user).with_context(|| {
            format!(
                "creating local runner client for linux user {}",
                effective_user
            )
        })?
    } else {
        let pattern = state
            .runner_socket_pattern
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("runner socket pattern not configured"))?;
        RunnerClient::for_user_with_pattern(&effective_user, pattern)
            .with_context(|| format!("creating runner client for linux user {}", effective_user))?
    };

    Ok(Some(
        ensure_runner_healthy(state, &effective_user, client).await?,
    ))
}

pub async fn resolve_target_for_workspace_path(
    state: &AppState,
    user_id: &str,
    workspace_path: &str,
) -> Result<ExecutionTarget> {
    if let Some(sw_service) = state.shared_workspaces.as_ref()
        && let Some((ws, _role)) = sw_service
            .check_access_for_path(workspace_path, user_id)
            .await
            .with_context(|| format!("shared workspace access check for path {}", workspace_path))?
    {
        return Ok(ExecutionTarget::SharedWorkspace {
            workspace_id: ws.id,
        });
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

    Ok(Some(
        ensure_runner_healthy(state, &linux_user, client).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn exposed_socket_translates_to_runner_socket_directory() {
        let host = translate_exposed_socket(
            Path::new("/var/lib/oqto/runtime/ws-1/rsock/runner.sock"),
            Path::new("/run/oqto/port-4101.sock"),
        )
        .unwrap();
        assert_eq!(
            host,
            Path::new("/var/lib/oqto/runtime/ws-1/rsock/port-4101.sock")
        );
    }

    #[test]
    fn exposed_socket_translation_rejects_bare_paths() {
        assert!(translate_exposed_socket(Path::new("/"), Path::new("/run/oqto/x.sock")).is_err());
        assert!(
            translate_exposed_socket(Path::new("/run/oqto/runner.sock"), Path::new("/")).is_err()
        );
    }
}

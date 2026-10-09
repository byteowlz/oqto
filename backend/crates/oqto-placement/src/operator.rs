use crate::{PlacementKind, PlacementRecord};
use anyhow::{Context, Result};
use async_trait::async_trait;
use oqto_runner::{client::RunnerClient, protocol::DirectoryListingResponse};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, time::Duration};

#[derive(Debug, Clone, Serialize)]
pub struct PlacementRuntimeStatus {
    pub backend: &'static str,
    pub ready: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlacementRuntimeInspection {
    pub backend: &'static str,
    pub runtime: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceExecResult {
    pub exit_code: i32,
    pub output: String,
}

/// Backend-specific runtime diagnostics. Workspace operations deliberately do
/// not live here: they use the runner endpoint on PlacementRecord and are
/// therefore identical for local, container, and remote placements.
#[async_trait]
pub trait PlacementOperator: Send + Sync {
    async fn status(&self, record: &PlacementRecord) -> Result<PlacementRuntimeStatus>;
    async fn logs(&self, record: &PlacementRecord, tail: &str) -> Result<String>;
    async fn inspect(&self, record: &PlacementRecord) -> Result<PlacementRuntimeInspection>;
}

pub struct LocalProcessOperator;

#[async_trait]
impl PlacementOperator for LocalProcessOperator {
    async fn status(&self, record: &PlacementRecord) -> Result<PlacementRuntimeStatus> {
        let client = RunnerClient::from_endpoint(&record.runner_endpoint)?;
        match client.ensure_ready_with_recovery().await {
            Ok(()) => Ok(PlacementRuntimeStatus {
                backend: "local_process",
                ready: true,
                detail: "runner ready".to_string(),
            }),
            Err(error) => Ok(PlacementRuntimeStatus {
                backend: "local_process",
                ready: false,
                detail: error.to_string(),
            }),
        }
    }

    async fn logs(&self, _record: &PlacementRecord, _tail: &str) -> Result<String> {
        anyhow::bail!(
            "local-process logs have no durable per-placement journal identity; use the backend journal until local runner log routing is implemented"
        )
    }

    async fn inspect(&self, record: &PlacementRecord) -> Result<PlacementRuntimeInspection> {
        let status = self.status(record).await?;
        Ok(PlacementRuntimeInspection {
            backend: "local_process",
            runtime: serde_json::json!({
                "runtime_name": record.runtime_name,
                "endpoint": record.runner_endpoint,
                "status": status,
            }),
        })
    }
}

pub struct PodmanOperator;

impl PodmanOperator {
    async fn checked_output(record: &PlacementRecord, args: &[&str]) -> Result<Vec<u8>> {
        verify_podman_labels(record).await?;
        let output = tokio::process::Command::new("podman")
            .args(args)
            .arg(&record.runtime_name)
            .output()
            .await
            .context("executing Podman placement operation")?;
        anyhow::ensure!(
            output.status.success(),
            "Podman operation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(output.stdout)
    }
}

#[async_trait]
impl PlacementOperator for PodmanOperator {
    async fn status(&self, record: &PlacementRecord) -> Result<PlacementRuntimeStatus> {
        let output =
            match Self::checked_output(record, &["inspect", "--format", "{{.State.Status}}"]).await
            {
                Ok(output) => output,
                Err(error) => {
                    return Ok(PlacementRuntimeStatus {
                        backend: "rootless_podman",
                        ready: false,
                        detail: error.to_string(),
                    });
                }
            };
        let state = String::from_utf8(output)?.trim().to_string();
        let runner_ready = RunnerClient::from_endpoint(&record.runner_endpoint)?
            .ensure_ready_with_recovery()
            .await
            .is_ok();
        Ok(PlacementRuntimeStatus {
            backend: "rootless_podman",
            ready: state == "running" && runner_ready,
            detail: format!("container={state}, runner_ready={runner_ready}"),
        })
    }

    async fn logs(&self, record: &PlacementRecord, tail: &str) -> Result<String> {
        verify_podman_labels(record).await?;
        let output = tokio::process::Command::new("podman")
            .args(["logs", "--tail", tail, &record.runtime_name])
            .output()
            .await
            .context("reading Podman placement logs")?;
        anyhow::ensure!(
            output.status.success(),
            "Podman logs failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        let mut bytes = output.stdout;
        bytes.extend(output.stderr);
        String::from_utf8(bytes).context("Podman logs were not UTF-8")
    }

    async fn inspect(&self, record: &PlacementRecord) -> Result<PlacementRuntimeInspection> {
        let output = Self::checked_output(record, &["inspect"]).await?;
        Ok(PlacementRuntimeInspection {
            backend: "rootless_podman",
            runtime: serde_json::from_slice(&output).context("parsing Podman inspection")?,
        })
    }
}

pub fn operator_for(record: &PlacementRecord) -> Box<dyn PlacementOperator> {
    match record.kind {
        PlacementKind::LocalProcess => Box::new(LocalProcessOperator),
        PlacementKind::RootlessPodman => Box::new(PodmanOperator),
    }
}

/// Execute a command through the canonical runner protocol. This never calls
/// a placement backend (`podman exec`, ssh, etc.).
pub async fn workspace_exec(
    record: &PlacementRecord,
    command: &[String],
) -> Result<WorkspaceExecResult> {
    let (binary, args) = command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("command must not be empty"))?;
    let cwd = record
        .spec
        .as_ref()
        .map(|spec| spec.workspace_dir.clone())
        .unwrap_or_else(|| PathBuf::from("/workspace"));
    let process_id = format!("oqtoctl-{}", uuid::Uuid::new_v4());
    let client = RunnerClient::from_endpoint(&record.runner_endpoint)?;
    // LocalProcess has no outer isolation boundary and must use oqto-sandbox.
    // RootlessPodman already executes inside the Workspace container; its
    // in-container sandbox profile is tracked separately (oqto-nppq.7/.11).
    let sandboxed = matches!(record.kind, PlacementKind::LocalProcess);
    client
        .spawn_rpc_process(
            &process_id,
            binary,
            args.to_vec(),
            cwd,
            HashMap::new(),
            sandboxed,
        )
        .await?;

    let mut output = String::new();
    loop {
        let chunk = client.read_stdout(&process_id, 500).await?;
        output.push_str(&chunk.data);
        let status = client.get_status(&process_id).await?;
        if !status.running && !chunk.has_more {
            return Ok(WorkspaceExecResult {
                exit_code: status.exit_code.unwrap_or(1),
                output,
            });
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

pub async fn workspace_list_directory(
    record: &PlacementRecord,
    path: impl Into<PathBuf>,
) -> Result<DirectoryListingResponse> {
    RunnerClient::from_endpoint(&record.runner_endpoint)?
        .list_directory(path, true)
        .await
}

async fn verify_podman_labels(record: &PlacementRecord) -> Result<()> {
    let output = tokio::process::Command::new("podman")
        .args([
            "inspect",
            "--format",
            "{{json .Config.Labels}}",
            &record.runtime_name,
        ])
        .output()
        .await
        .context("inspecting placement labels")?;
    anyhow::ensure!(
        output.status.success(),
        "placement {} is not inspectable: {}",
        record.runtime_name,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let labels: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&output.stdout).context("parsing placement labels")?;
    for (key, value) in [
        ("oqto.workspace", record.workspace_id.as_str()),
        ("oqto.account", record.account_id.as_str()),
        ("oqto.placement", "rootless-podman"),
    ] {
        anyhow::ensure!(
            labels.get(key).is_some_and(|actual| actual == value),
            "runtime {} has missing or mismatched label {key}={value}",
            record.runtime_name
        );
    }
    Ok(())
}

use crate::{
    PlacementHealth, PlacementId, PlacementKind, PlacementRecord, PlacementSpec,
    PlacementSupervisor, runtime_name,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use oqto_runner::client::RunnerClient;
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Single-tenant development adapter. It intentionally supports only Unix
/// endpoints; network/container details belong to their own adapters.
pub struct LocalProcessSupervisor {
    runner_binary: PathBuf,
    children: Mutex<HashMap<String, Child>>,
}

impl LocalProcessSupervisor {
    pub fn new(runner_binary: impl Into<PathBuf>) -> Self {
        Self {
            runner_binary: runner_binary.into(),
            children: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl PlacementSupervisor for LocalProcessSupervisor {
    async fn start(&self, spec: &PlacementSpec) -> Result<PlacementRecord> {
        spec.validate()?;
        let RunnerEndpointConfig::Unix { path } = &spec.runner_endpoint else {
            anyhow::bail!("local-process placement requires a Unix runner endpoint");
        };
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("creating runner directory {}", parent.display()))?;
        }
        let runtime_name = runtime_name(&spec.workspace_id);
        let mut command = Command::new(&self.runner_binary);
        command
            .arg("--socket")
            .arg(path)
            .env("HOME", &spec.state_dir)
            .env("OQTO_WORKSPACE_DIR", &spec.workspace_dir)
            .kill_on_drop(true);
        for (key, value) in &spec.environment {
            command.env(key, value);
        }
        let child = command
            .spawn()
            .with_context(|| format!("starting local runner {}", self.runner_binary.display()))?;
        self.children
            .lock()
            .await
            .insert(runtime_name.clone(), child);
        Ok(PlacementRecord {
            id: PlacementId(uuid::Uuid::new_v4().to_string()),
            workspace_id: spec.workspace_id.clone(),
            account_id: spec.account_id.clone(),
            kind: PlacementKind::LocalProcess,
            runner_endpoint: spec.runner_endpoint.clone(),
            runtime_name,
            spec: Some(spec.clone()),
        })
    }

    async fn stop(&self, placement: &PlacementRecord) -> Result<()> {
        if let Some(mut child) = self.children.lock().await.remove(&placement.runtime_name) {
            child.kill().await.context("stopping local runner")?;
        }
        Ok(())
    }

    async fn health(&self, placement: &PlacementRecord) -> Result<PlacementHealth> {
        let client = RunnerClient::from_endpoint(&placement.runner_endpoint)?;
        match client.ensure_ready_with_recovery().await {
            Ok(()) => Ok(PlacementHealth::Ready),
            Err(error) => Ok(PlacementHealth::Unhealthy {
                detail: error.to_string(),
            }),
        }
    }
}

//! Placement supervision and runner reachability.
//!
//! Product code asks this module where a Workspace runs. Concrete adapters own
//! process/container lifecycle and return a typed runner endpoint.

mod local;
mod podman;
mod store;

use anyhow::Result;
use async_trait::async_trait;
use oqto_runner::transport::RunnerEndpointConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub use local::LocalProcessSupervisor;
pub use podman::{CommandOutput, CommandRunner, PodmanSupervisor, TokioCommandRunner};
pub use store::{JsonPlacementStore, PlacementStore};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlacementId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlacementSpec {
    pub workspace_id: String,
    pub account_id: String,
    pub image: String,
    pub workspace_dir: PathBuf,
    pub state_dir: PathBuf,
    pub runner_endpoint: RunnerEndpointConfig,
    /// Server-side TLS material injected only into network runner placements.
    /// Client credentials remain exclusively in `runner_endpoint`.
    #[serde(default)]
    pub server_tls: Option<RunnerServerTlsConfig>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub cpu_limit: Option<String>,
    #[serde(default)]
    pub memory_limit: Option<String>,
}

impl PlacementSpec {
    pub fn validate(&self) -> Result<()> {
        if self.workspace_id.trim().is_empty() {
            anyhow::bail!("workspace_id must not be empty");
        }
        if self.account_id.trim().is_empty() {
            anyhow::bail!("account_id must not be empty");
        }
        if self.image.trim().is_empty() {
            anyhow::bail!("container image must not be empty");
        }
        if !self.workspace_dir.is_absolute() || !self.state_dir.is_absolute() {
            anyhow::bail!("workspace and state directories must be absolute");
        }
        match (&self.runner_endpoint, &self.server_tls) {
            (RunnerEndpointConfig::Unix { .. }, None)
            | (RunnerEndpointConfig::TcpTls { .. }, Some(_)) => {}
            (RunnerEndpointConfig::Unix { .. }, Some(_)) => {
                anyhow::bail!("Unix runner placement must not include server TLS material");
            }
            (RunnerEndpointConfig::TcpTls { .. }, None) => {
                anyhow::bail!("TCP/TLS runner placement requires server TLS material");
            }
        }
        for key in self.environment.keys() {
            let upper = key.to_ascii_uppercase();
            if ["TOKEN", "KEY", "PASSWORD", "SECRET"]
                .iter()
                .any(|sensitive| upper.contains(sensitive))
            {
                anyhow::bail!(
                    "secret-like environment variable {key} must use the placement secret provider"
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RunnerServerTlsConfig {
    pub client_ca: PathBuf,
    pub certificate: PathBuf,
    pub key: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PlacementKind {
    LocalProcess,
    RootlessPodman,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlacementRecord {
    pub id: PlacementId,
    pub workspace_id: String,
    pub account_id: String,
    pub kind: PlacementKind,
    pub runner_endpoint: RunnerEndpointConfig,
    pub runtime_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlacementHealth {
    Starting,
    Ready,
    Stopped,
    Unhealthy { detail: String },
}

#[async_trait]
pub trait PlacementSupervisor: Send + Sync {
    async fn start(&self, spec: &PlacementSpec) -> Result<PlacementRecord>;
    async fn stop(&self, placement: &PlacementRecord) -> Result<()>;
    async fn health(&self, placement: &PlacementRecord) -> Result<PlacementHealth>;
}

pub fn runtime_name(workspace_id: &str) -> String {
    let safe: String = workspace_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("oqto-ws-{}", safe.trim_matches('-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_spec_rejects_inline_secrets() {
        let spec = PlacementSpec {
            workspace_id: "workspace".to_string(),
            account_id: "account".to_string(),
            image: "image".to_string(),
            workspace_dir: PathBuf::from("/workspace"),
            state_dir: PathBuf::from("/state"),
            runner_endpoint: RunnerEndpointConfig::Unix {
                path: PathBuf::from("/run/oqto/runner.sock"),
            },
            server_tls: None,
            environment: BTreeMap::from([(
                "EAVS_API_KEY".to_string(),
                "must-not-be-in-inspect".to_string(),
            )]),
            cpu_limit: None,
            memory_limit: None,
        };
        assert!(spec.validate().is_err());
    }
}

//! Placement supervision and runner reachability.
//!
//! Product code asks this module where a Workspace runs. Concrete adapters own
//! process/container lifecycle and return a typed runner endpoint.

mod host_bridge;
mod local;
mod operator;
mod podman;
mod store;

use anyhow::Result;
use async_trait::async_trait;
use oqto_runner::transport::RunnerEndpointConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub use host_bridge::HostEndpointBridge;
pub use local::LocalProcessSupervisor;
pub use operator::{
    LocalProcessOperator, PlacementOperator, PlacementRuntimeInspection, PlacementRuntimeStatus,
    PodmanOperator, WorkspaceExecResult, operator_for, workspace_exec, workspace_list_directory,
};
pub use podman::{CommandOutput, CommandRunner, PodmanSupervisor, TokioCommandRunner};
pub use store::{JsonPlacementStore, PlacementStore};

/// Home directory inside every Workspace container: the state volume mount.
pub const CONTAINER_HOME: &str = "/home/oqto";

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
    #[serde(default)]
    pub network: PlacementNetwork,
    #[serde(default)]
    pub userns: PlacementUserns,
}

/// User-namespace strategy for container placements.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlacementUserns {
    /// Development mapping: container user == host user. No cross-tenant
    /// host-filesystem separation.
    #[default]
    KeepId,
    /// Podman-managed disjoint subuid range per container. Volumes are
    /// chowned into the range; host-side workspace files become unreadable
    /// to other tenants and the backend user.
    Auto {
        #[serde(default)]
        size: Option<u32>,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlacementNetworkMode {
    /// No network at all; granted endpoints are the only reachable services.
    #[default]
    Isolated,
    /// Ordinary rootless container networking (open egress).
    Open,
}

/// A named service the workspace may reach. Inside the container the runner
/// bridges 127.0.0.1:port to the bind-mounted socket /run/oqto/endpoints/<name>.sock.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlacementEndpoint {
    pub name: String,
    pub port: u16,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlacementNetwork {
    #[serde(default)]
    pub mode: PlacementNetworkMode,
    #[serde(default)]
    pub endpoints: Vec<PlacementEndpoint>,
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
        if self.network.mode == PlacementNetworkMode::Isolated
            && matches!(self.runner_endpoint, RunnerEndpointConfig::TcpTls { .. })
        {
            anyhow::bail!(
                "isolated network placement cannot expose a TCP runner endpoint; use open mode"
            );
        }
        let mut seen_names = std::collections::BTreeSet::new();
        let mut seen_ports = std::collections::BTreeSet::new();
        for endpoint in &self.network.endpoints {
            if endpoint.name.is_empty()
                || !endpoint
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                anyhow::bail!("endpoint name must be alphanumeric/dash/underscore");
            }
            if endpoint.port == 0 {
                anyhow::bail!("endpoint port must be non-zero");
            }
            if !seen_names.insert(&endpoint.name) || !seen_ports.insert(endpoint.port) {
                anyhow::bail!("duplicate endpoint name or port: {}", endpoint.name);
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
    /// The spec this placement was started from. Reconciliation needs it to
    /// restart a stopped placement; records written before this field existed
    /// cannot be auto-restarted.
    #[serde(default)]
    pub spec: Option<PlacementSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlacementHealth {
    Starting,
    Ready,
    Stopped,
    Unhealthy { detail: String },
}

/// OCI compatibility label the backend keys workspace-image attestation off of.
/// Distinct from `org.opencontainers.image.version`, which base images pollute
/// (e.g. Ubuntu stamps it with `24.04`).
pub const IMAGE_VERSION_LABEL: &str = "io.oqto.version";
pub const IMAGE_REVISION_LABEL: &str = "org.opencontainers.image.revision";

/// Provenance resolved from a workspace container image via `podman inspect`.
/// Product code compares `labels[IMAGE_VERSION_LABEL]` against the backend
/// release version and `digest` against a configured pin to detect drift.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageAttestation {
    pub labels: BTreeMap<String, String>,
    /// Canonical content digest (`sha256:...`) of the resolved image.
    pub digest: String,
}

#[async_trait]
pub trait PlacementSupervisor: Send + Sync {
    async fn start(&self, spec: &PlacementSpec) -> Result<PlacementRecord>;
    async fn stop(&self, placement: &PlacementRecord) -> Result<()>;
    async fn health(&self, placement: &PlacementRecord) -> Result<PlacementHealth>;
    /// Resolve labels + digest for a workspace image reference. Pulls the image
    /// if it is not present locally. Used for compatibility attestation.
    async fn resolve_image_attestation(&self, _image: &str) -> Result<ImageAttestation> {
        anyhow::bail!("this placement supervisor does not resolve container image attestation")
    }
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
            network: Default::default(),
            userns: Default::default(),
        };
        assert!(spec.validate().is_err());
    }
}

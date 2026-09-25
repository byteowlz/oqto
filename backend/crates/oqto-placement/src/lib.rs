//! Placement supervision and runner reachability.
//!
//! Product code asks this module where a Workspace runs. Concrete adapters own
//! process/container lifecycle and return a typed runner endpoint.

mod capability;
mod local;
mod podman;
mod store;

use anyhow::Result;
use async_trait::async_trait;
use oqto_runner::transport::RunnerEndpointConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

pub use capability::{PlacementAvailability, PlacementCapabilityReport, ProbeEvidence};
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
    /// Reserved for compatibility with serialized placement requests; no
    /// inline environment values are accepted until a typed, non-inspectable
    /// secret-provider contract exists. An empty map is the only valid value.
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
        // The image is passed as the first positional argument of `podman
        // run`. A leading '-' would be interpreted as another engine option,
        // potentially disabling the sandbox before the image is even chosen.
        if self.image.trim().is_empty()
            || self.image.starts_with('-')
            || self.image.chars().any(char::is_whitespace)
            || self.image.contains('\0')
        {
            anyhow::bail!("container image must be a non-option reference without whitespace");
        }
        validate_bind_source("workspace directory", &self.workspace_dir)?;
        validate_bind_source("state directory", &self.state_dir)?;
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
        match &self.runner_endpoint {
            RunnerEndpointConfig::Unix { path } => {
                let socket_dir = path.parent().ok_or_else(|| {
                    anyhow::anyhow!("Unix runner endpoint must have a parent directory")
                })?;
                validate_bind_source("runner socket directory", socket_dir)?;
            }
            RunnerEndpointConfig::TcpTls { .. } => {
                if let Some(tls) = &self.server_tls {
                    for (name, path) in [
                        ("client CA", &tls.client_ca),
                        ("server certificate", &tls.certificate),
                        ("server key", &tls.key),
                    ] {
                        validate_bind_source(name, path)?;
                    }
                }
            }
        }
        if !self.environment.is_empty() {
            anyhow::bail!(
                "inline placement environment is not supported; use a typed secret provider"
            );
        }
        Ok(())
    }
}

/// `podman --volume` uses colons as field separators. Validate every host
/// path before host directories or pods are created so a path cannot silently
/// become a different mount or an option (including through lossy display).
fn validate_bind_source(name: &str, path: &Path) -> Result<()> {
    let text = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("{name} must be valid UTF-8"))?;
    if !path.is_absolute()
        || path.components().any(|part| part == Component::ParentDir)
        || text
            .bytes()
            .any(|byte| matches!(byte, b':' | b'\0' | b'\r' | b'\n'))
    {
        anyhow::bail!("{name} must be absolute without traversal or Podman volume delimiters");
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RunnerServerTlsConfig {
    pub client_ca: PathBuf,
    pub certificate: PathBuf,
    pub key: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
    fn placement_spec_rejects_podman_option_and_bind_path_ambiguity() {
        let mut spec = PlacementSpec {
            workspace_id: "workspace".into(),
            account_id: "account".into(),
            image: "localhost/oqto:test".into(),
            workspace_dir: PathBuf::from("/workspace"),
            state_dir: PathBuf::from("/state"),
            runner_endpoint: RunnerEndpointConfig::Unix {
                path: PathBuf::from("/run/oqto/runner.sock"),
            },
            server_tls: None,
            environment: BTreeMap::new(),
            cpu_limit: None,
            memory_limit: None,
        };
        assert!(spec.validate().is_ok());
        spec.image = "--privileged".into();
        assert!(spec.validate().is_err(), "image must not become an option");
        spec.image = "localhost/oqto:test".into();
        spec.workspace_dir = PathBuf::from("/workspace:ro");
        assert!(
            spec.validate().is_err(),
            "colon must not change --volume parsing"
        );
        spec.workspace_dir = PathBuf::from("/workspace");
        spec.state_dir = PathBuf::from("/state/../wrong");
        assert!(spec.validate().is_err(), "bind source must not traverse");
        spec.state_dir = PathBuf::from("/state");
        spec.runner_endpoint = RunnerEndpointConfig::Unix {
            path: PathBuf::from("/run/oqto:rw/runner.sock"),
        };
        assert!(
            spec.validate().is_err(),
            "socket bind source must not reinterpret path"
        );
        spec.runner_endpoint = RunnerEndpointConfig::TcpTls {
            address: "127.0.0.1:7443".parse().expect("fixed loopback address"),
            server_name: "localhost".into(),
            ca: PathBuf::from("/cert/client-ca.pem"),
            certificate: PathBuf::from("/cert/client.pem"),
            key: PathBuf::from("/cert/client-key.pem"),
        };
        spec.server_tls = Some(RunnerServerTlsConfig {
            client_ca: PathBuf::from("/cert/server-ca.pem:ro"),
            certificate: PathBuf::from("/cert/server.pem"),
            key: PathBuf::from("/cert/server-key.pem"),
        });
        assert!(
            spec.validate().is_err(),
            "TLS bind source must not reinterpret path"
        );
    }

    #[test]
    fn placement_spec_rejects_inline_secrets_even_with_unremarkable_key() {
        let spec = PlacementSpec {
            workspace_id: "workspace".into(),
            account_id: "account".into(),
            image: "image".into(),
            workspace_dir: PathBuf::from("/workspace"),
            state_dir: PathBuf::from("/state"),
            runner_endpoint: RunnerEndpointConfig::Unix {
                path: PathBuf::from("/run/oqto/runner.sock"),
            },
            server_tls: None,
            environment: BTreeMap::from([("AUTH_HEADER".into(), "private-value-123".into())]),
            cpu_limit: None,
            memory_limit: None,
        };
        let error = spec.validate().expect_err("inline values must be refused");
        assert!(!error.to_string().contains("private-value-123"));
    }

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

//! Account-authorized runner inventory. Connectivity is NOT a Workspace route.
//! Registering a target never rebinds Sessions or grants remote execution.
use anyhow::{Result, ensure};
use oqto_runner::{
    client::RunnerClient, protocol::RUNNER_WIRE_VERSION, transport::RunnerEndpointConfig,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

/// Explicit permission to run work on a machine Oqto does not own.
///
/// The roots are ceilings, not defaults: a remote Workspace must resolve inside
/// one of them, so reachability plus history access still cannot execute.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RemoteExecutionGrant {
    /// Actual OS principal on the remote machine; never a provisioned Linux alias.
    pub principal: String,
    pub roots: Vec<std::path::PathBuf>,
}

impl RemoteExecutionGrant {
    /// Canonical, symlink-free containment check against the configured ceilings.
    pub fn permits(&self, path: &std::path::Path) -> bool {
        path.is_absolute()
            && !path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            && self
                .roots
                .iter()
                .any(|root| path == root || path.starts_with(root))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerTargetConfig {
    pub id: String,
    pub label: String,
    /// Account IDs, not usernames, roles or SSH principals. No implicit admin bypass.
    pub account_ids: Vec<String>,
    pub endpoint: RunnerEndpointConfig,
    /// Explicit credential-management grant; inventory alone is insufficient.
    #[serde(default)]
    pub provider_login: bool,
    #[serde(default)]
    pub history_read: bool,
    /// Absent means this machine may never execute Sessions, whatever else it offers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<RemoteExecutionGrant>,
}

impl RunnerTargetConfig {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty()
                && self.id.len() <= 64
                && self
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "invalid runner target ID"
        );
        ensure!(
            !self.label.trim().is_empty()
                && self.label.len() <= 80
                && !self.label.chars().any(char::is_control),
            "invalid runner target label"
        );
        ensure!(
            !self.account_ids.is_empty() && self.account_ids.iter().all(|id| !id.trim().is_empty()),
            "runner target requires explicit Account grants"
        );
        ensure!(
            !self.provider_login || self.account_ids.len() == 1,
            "provider login requires exactly one owning Account"
        );
        ensure!(
            !self.history_read || self.account_ids.len() == 1,
            "history reading requires exactly one owning Account"
        );
        if let Some(grant) = &self.execution {
            ensure!(
                self.account_ids.len() == 1,
                "remote execution requires exactly one owning Account"
            );
            ensure!(
                !grant.principal.is_empty()
                    && grant.principal.len() <= 64
                    && grant
                        .principal
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
                "invalid remote principal"
            );
            ensure!(
                !grant.roots.is_empty() && grant.roots.len() <= 16,
                "remote execution requires explicit workspace roots"
            );
            ensure!(
                grant.roots.iter().all(|root| root.is_absolute()
                    && root.parent().is_some()
                    && !root
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))),
                "explicit absolute workspace ceilings are required"
            );
        }
        match &self.endpoint {
            RunnerEndpointConfig::TcpTls {
                address,
                server_name,
                ca,
                certificate,
                key,
            } => {
                ensure!(
                    address.port() != 0 && !server_name.trim().is_empty(),
                    "runner target needs a TLS address and server identity"
                );
                ensure!(
                    [ca, certificate, key].iter().all(|p| p.is_absolute()),
                    "runner target credential paths must be absolute"
                );
            }
            RunnerEndpointConfig::Unix { .. } => anyhow::bail!(
                "remote runner targets require mutual TLS; Unix placement routing is unchanged"
            ),
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetConnection {
    Online,
    Unavailable,
    Incompatible,
}

/// Deliberately excludes endpoints, account grants, credentials and raw errors.
#[derive(Clone, Debug, Serialize)]
pub struct RunnerTargetStatus {
    pub id: String,
    pub label: String,
    pub connection: TargetConnection,
    pub checked_at: String,
    /// Inventory is not admission: placement/session routing is a separate step.
    pub session_creation: bool,
    pub provider_login: bool,
    pub history_read: bool,
}

struct Target {
    config: RunnerTargetConfig,
    cached: Mutex<Option<(Instant, RunnerTargetStatus)>>,
}

#[derive(Default)]
pub struct RunnerTargets {
    targets: Vec<Arc<Target>>,
}

impl RunnerTargets {
    pub fn new(configs: Vec<RunnerTargetConfig>) -> Result<Self> {
        ensure!(
            configs.len() <= 32,
            "at most 32 runner targets may be configured"
        );
        let mut ids = HashSet::new();
        for config in &configs {
            config.validate()?;
            ensure!(
                ids.insert(config.id.clone()),
                "duplicate runner target ID: {}",
                config.id
            );
        }
        // A workspace path must identify one machine, so overlapping execution
        // roots are a configuration error rather than a routing race.
        for (index, config) in configs.iter().enumerate() {
            let Some(grant) = &config.execution else {
                continue;
            };
            for other in configs.iter().skip(index + 1) {
                let Some(other_grant) = &other.execution else {
                    continue;
                };
                ensure!(
                    !grant.roots.iter().any(|root| other_grant
                        .roots
                        .iter()
                        .any(|peer| root.starts_with(peer) || peer.starts_with(root))),
                    "runner targets {} and {} claim overlapping execution roots",
                    config.id,
                    other.id
                );
            }
        }
        Ok(Self {
            targets: configs
                .into_iter()
                .map(|config| {
                    Arc::new(Target {
                        config,
                        cached: Mutex::new(None),
                    })
                })
                .collect(),
        })
    }

    pub fn history_read_client(&self, account_id: &str, target_id: &str) -> Result<RunnerClient> {
        let target = self
            .targets
            .iter()
            .find(|target| {
                target.config.id == target_id
                    && target.config.history_read
                    && target.config.account_ids.len() == 1
                    && target.config.account_ids[0] == account_id
            })
            .ok_or_else(|| anyhow::anyhow!("history access denied"))?;
        RunnerClient::from_endpoint(&target.config.endpoint)
    }

    /// Authorize execution on a machine, independent of reaching it.
    pub fn execution_grant(
        &self,
        account_id: &str,
        target_id: &str,
    ) -> Result<RemoteExecutionGrant> {
        let target = self
            .targets
            .iter()
            .find(|target| {
                target.config.id == target_id
                    && target.config.execution.is_some()
                    && target.config.account_ids.len() == 1
                    && target.config.account_ids[0] == account_id
            })
            .ok_or_else(|| anyhow::anyhow!("remote execution denied"))?;
        target
            .config
            .execution
            .clone()
            .ok_or_else(|| anyhow::anyhow!("remote execution denied"))
    }

    /// Which machine owns `path`, if any.
    ///
    /// Roots are globally unique (enforced at registration), so a workspace path
    /// identifies at most one machine and can never silently move between them.
    pub fn machine_for_path(
        &self,
        account_id: &str,
        path: &std::path::Path,
    ) -> Option<(String, RemoteExecutionGrant)> {
        self.targets.iter().find_map(|target| {
            let grant = target.config.execution.as_ref()?;
            (target.config.account_ids.len() == 1
                && target.config.account_ids[0] == account_id
                && grant.permits(path))
            .then(|| (target.config.id.clone(), grant.clone()))
        })
    }

    /// Transport for an already authorized execution target.
    pub fn execution_client(&self, account_id: &str, target_id: &str) -> Result<RunnerClient> {
        self.execution_grant(account_id, target_id)?;
        let target = self
            .targets
            .iter()
            .find(|target| target.config.id == target_id)
            .ok_or_else(|| anyhow::anyhow!("remote execution denied"))?;
        RunnerClient::from_endpoint(&target.config.endpoint)
    }

    pub fn provider_login_client(&self, account_id: &str, target_id: &str) -> Result<RunnerClient> {
        let target = self
            .targets
            .iter()
            .find(|target| {
                target.config.id == target_id
                    && target.config.provider_login
                    && target.config.account_ids.len() == 1
                    && target.config.account_ids[0] == account_id
            })
            .ok_or_else(|| anyhow::anyhow!("provider login denied"))?;
        RunnerClient::from_endpoint(&target.config.endpoint)
    }

    /// Authorization happens before probing, and concurrent readers share a
    /// bounded cache. An offline target cannot block healthy peers indefinitely.
    pub async fn list_for_account(&self, account_id: &str) -> Result<Vec<RunnerTargetStatus>> {
        let mut pending = tokio::task::JoinSet::new();
        for (index, target) in self.targets.iter().enumerate() {
            if !target.config.account_ids.iter().any(|id| id == account_id) {
                continue;
            }
            let target = Arc::clone(target);
            pending.spawn(async move { (index, target.status().await) });
        }
        let mut results = Vec::new();
        while let Some(result) = pending.join_next().await {
            results.push(result?);
        }
        results.sort_by_key(|(index, _)| *index);
        Ok(results.into_iter().map(|(_, status)| status).collect())
    }
}

impl Target {
    async fn status(&self) -> RunnerTargetStatus {
        let mut cached = self.cached.lock().await;
        if let Some((at, status)) = cached.as_ref()
            && at.elapsed() < Duration::from_secs(8)
        {
            return status.clone();
        }
        let connection = match RunnerClient::from_endpoint(&self.config.endpoint) {
            Ok(client) => probe(&client).await,
            Err(_) => TargetConnection::Unavailable,
        };
        let status = RunnerTargetStatus {
            id: self.config.id.clone(),
            label: self.config.label.clone(),
            connection,
            checked_at: chrono::Utc::now().to_rfc3339(),
            session_creation: self.config.execution.is_some(),
            provider_login: self.config.provider_login,
            history_read: self.config.history_read,
        };
        *cached = Some((Instant::now(), status.clone()));
        status
    }
}

async fn probe(client: &RunnerClient) -> TargetConnection {
    match tokio::time::timeout(Duration::from_secs(2), client.get_capabilities()).await {
        Ok(Ok(caps)) if caps.protocol_version == RUNNER_WIRE_VERSION => TargetConnection::Online,
        Ok(Ok(_)) => TargetConnection::Incompatible,
        _ => TargetConnection::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    fn config() -> RunnerTargetConfig {
        RunnerTargetConfig {
            provider_login: false,
            history_read: false,
            execution: None,
            id: "mac".into(),
            label: "Mac".into(),
            account_ids: vec!["alice".into()],
            endpoint: RunnerEndpointConfig::TcpTls {
                address: "127.0.0.1:9443".parse().unwrap(),
                server_name: "mac.runner".into(),
                ca: "/missing/ca.pem".into(),
                certificate: "/missing/client.pem".into(),
                key: "/missing/key.pem".into(),
            },
        }
    }

    #[test]
    fn history_requires_single_owner_and_explicit_grant_before_transport() {
        let targets = RunnerTargets::new(vec![config()]).unwrap();
        assert!(targets.history_read_client("alice", "mac").is_err());
        let mut cfg = config();
        cfg.history_read = true;
        cfg.account_ids.push("bob".into());
        assert!(RunnerTargets::new(vec![cfg.clone()]).is_err());
        cfg.account_ids = vec!["alice".into()];
        let targets = RunnerTargets::new(vec![cfg]).unwrap();
        assert_eq!(
            targets
                .history_read_client("bob", "mac")
                .err()
                .unwrap()
                .to_string(),
            "history access denied"
        );
        assert!(targets.history_read_client("alice", "missing").is_err());
    }

    #[test]
    fn provider_login_requires_single_owner_and_explicit_grant_before_transport() {
        let targets = RunnerTargets::new(vec![config()]).unwrap();
        assert!(targets.provider_login_client("alice", "mac").is_err());
        let mut cfg = config();
        cfg.provider_login = true;
        cfg.account_ids.push("bob".into());
        assert!(RunnerTargets::new(vec![cfg.clone()]).is_err());
        cfg.account_ids.pop();
        let targets = RunnerTargets::new(vec![cfg]).unwrap();
        let denied = targets.provider_login_client("bob", "mac").err().unwrap();
        assert_eq!(denied.to_string(), "provider login denied");
        assert!(targets.provider_login_client("alice", "unknown").is_err());
    }

    #[test]
    fn execution_requires_an_explicit_single_owner_grant_with_absolute_ceilings() {
        let mut cfg = config();
        assert!(
            RunnerTargets::new(vec![cfg.clone()])
                .unwrap()
                .execution_grant("alice", "mac")
                .is_err(),
            "inventory alone must not execute"
        );

        cfg.execution = Some(RemoteExecutionGrant {
            principal: "tommy".into(),
            roots: vec!["/Users/tommy/work".into()],
        });
        let targets = RunnerTargets::new(vec![cfg.clone()]).unwrap();
        let grant = targets.execution_grant("alice", "mac").unwrap();
        assert!(targets.execution_grant("bob", "mac").is_err());
        assert!(targets.execution_grant("alice", "other").is_err());

        assert!(grant.permits(std::path::Path::new("/Users/tommy/work")));
        assert!(grant.permits(std::path::Path::new("/Users/tommy/work/project")));
        assert!(!grant.permits(std::path::Path::new("/Users/tommy")));
        assert!(!grant.permits(std::path::Path::new("/Users/tommy/work-other")));
        assert!(!grant.permits(std::path::Path::new("/Users/tommy/work/../.ssh")));
        assert!(!grant.permits(std::path::Path::new("relative/path")));

        let mut shared = cfg.clone();
        shared.account_ids = vec!["alice".into(), "bob".into()];
        assert!(RunnerTargets::new(vec![shared]).is_err());
        for bad in [
            vec![],
            vec![std::path::PathBuf::from("relative")],
            vec![std::path::PathBuf::from("/")],
            vec![std::path::PathBuf::from("/Users/tommy/../root")],
        ] {
            let mut invalid = cfg.clone();
            invalid.execution = Some(RemoteExecutionGrant {
                principal: "tommy".into(),
                roots: bad,
            });
            assert!(RunnerTargets::new(vec![invalid]).is_err());
        }
        let mut peer = cfg.clone();
        peer.id = "mac2".into();
        peer.execution = Some(RemoteExecutionGrant {
            principal: "tommy".into(),
            roots: vec!["/Users/tommy/work/nested".into()],
        });
        assert!(
            RunnerTargets::new(vec![cfg.clone(), peer]).is_err(),
            "overlapping roots would make a path ambiguous"
        );

        assert_eq!(
            targets
                .machine_for_path("alice", std::path::Path::new("/Users/tommy/work/project"))
                .map(|(id, _)| id),
            Some("mac".to_string())
        );
        assert!(
            targets
                .machine_for_path("bob", std::path::Path::new("/Users/tommy/work/project"))
                .is_none()
        );
        assert!(
            targets
                .machine_for_path("alice", std::path::Path::new("/home/alice/project"))
                .is_none()
        );

        let mut bad_principal = cfg.clone();
        bad_principal.execution = Some(RemoteExecutionGrant {
            principal: "root; rm -rf".into(),
            roots: vec!["/Users/tommy/work".into()],
        });
        assert!(RunnerTargets::new(vec![bad_principal]).is_err());
    }

    #[test]
    fn invalid_and_duplicate_registration_is_rejected() {
        assert!(RunnerTargets::new(vec![config(), config()]).is_err());
        let mut cfg = config();
        cfg.account_ids.clear();
        assert!(RunnerTargets::new(vec![cfg]).is_err());
        let mut cfg = config();
        cfg.endpoint = RunnerEndpointConfig::Unix {
            path: "/tmp/runner.sock".into(),
        };
        assert!(RunnerTargets::new(vec![cfg]).is_err());
        assert!(RunnerTargets::new(vec![config(); 33]).is_err());
    }

    #[tokio::test]
    async fn grants_are_account_scoped_and_status_never_leaks_secrets() {
        let targets = RunnerTargets::new(vec![config()]).unwrap();
        assert!(targets.list_for_account("admin").await.unwrap().is_empty());
        assert!(targets.list_for_account("bob").await.unwrap().is_empty());
        let allowed = targets.list_for_account("alice").await.unwrap();
        assert_eq!(allowed.len(), 1);
        assert_eq!(allowed[0].connection, TargetConnection::Unavailable);
        assert!(!allowed[0].session_creation);
        let json = serde_json::to_string(&allowed).unwrap();
        for secret in ["alice", "127.0.0.1", "/missing", "endpoint", "key.pem"] {
            assert!(!json.contains(secret));
        }
    }

    #[tokio::test]
    async fn protocol_probe_distinguishes_live_and_incompatible_servers() {
        for (version, expected) in [
            (RUNNER_WIRE_VERSION, TargetConnection::Online),
            (u16::MAX, TargetConnection::Incompatible),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("runner.sock");
            let listener = tokio::net::UnixListener::bind(&path).unwrap();
            let task = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                let mut socket = BufReader::new(socket);
                let mut request = String::new();
                socket.read_line(&mut request).await.unwrap();
                assert!(request.contains("get_capabilities"));
                let caps = oqto_runner::protocol::RunnerResponse::RunnerCapabilities(
                    oqto_runner::protocol::RunnerCapabilitiesResponse {
                        protocol_version: version,
                        harnesses: vec!["pi".into()],
                        features: oqto_runner::protocol::RunnerFeatureFlags {
                            command_discovery: true,
                            model_discovery: true,
                            fork: true,
                            extension_ui: true,
                        },
                    },
                );
                socket
                    .get_mut()
                    .write_all(&oqto_runner::wire::encode_json_frame(&caps).unwrap())
                    .await
                    .unwrap();
            });
            assert_eq!(probe(&RunnerClient::new(path)).await, expected);
            task.await.unwrap();
        }
    }
}

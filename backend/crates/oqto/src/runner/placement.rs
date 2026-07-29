//! Container placement lifecycle: provision, remove, and reconcile
//! per-Workspace runner placements (ADR-0019/0020).

use anyhow::{Context, Result};
use oqto_placement::{
    HostEndpointBridge, PlacementHealth, PlacementNetwork, PlacementNetworkMode, PlacementRecord,
    PlacementSpec, PlacementStore, PlacementSupervisor, PlacementUserns,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementMode {
    /// Runners run on the host (legacy socket-per-user path).
    Local,
    /// Each Workspace runs as its own rootless container Pod.
    Container,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlacementConfig {
    pub mode: PlacementMode,
    /// Workspace container image.
    pub image: String,
    /// Root for per-workspace durable state (home) directories.
    pub state_root: Option<PathBuf>,
    /// Root for per-workspace runtime (socket) directories.
    pub runtime_root: Option<PathBuf>,
    pub cpu_limit: Option<String>,
    pub memory_limit: Option<String>,
    /// Workspace network containment. Isolated (default) means network=none;
    /// only listed endpoints are reachable.
    pub network: PlacementNetworkMode,
    /// User-namespace strategy: keep_id (dev) or auto (disjoint subuid
    /// range per workspace; production tenant separation).
    pub userns: PlacementUserns,
    /// Named services granted to every workspace container.
    pub endpoints: Vec<EndpointConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointConfig {
    /// Endpoint name; the container sees /run/oqto/endpoints/<name>.sock.
    pub name: String,
    /// Loopback port the runner exposes inside the container.
    pub port: u16,
    /// Host-side TCP target (host:port) the bridge forwards to.
    pub target: String,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self {
            mode: PlacementMode::Local,
            image: "localhost/oqto-workspace:dev".to_string(),
            state_root: None,
            runtime_root: None,
            cpu_limit: None,
            memory_limit: None,
            network: PlacementNetworkMode::Isolated,
            userns: PlacementUserns::KeepId,
            endpoints: Vec::new(),
        }
    }
}

/// Owns the supervisor + store pair and exposes workspace-level verbs.
pub struct PlacementManager {
    supervisor: Arc<dyn PlacementSupervisor>,
    store: Arc<dyn PlacementStore>,
    config: PlacementConfig,
    state_root: PathBuf,
    runtime_root: PathBuf,
    bridges: Mutex<HashMap<String, Vec<HostEndpointBridge>>>,
}

impl PlacementManager {
    pub fn new(
        supervisor: Arc<dyn PlacementSupervisor>,
        store: Arc<dyn PlacementStore>,
        config: PlacementConfig,
        default_state_root: PathBuf,
        default_runtime_root: PathBuf,
    ) -> Self {
        let state_root = config.state_root.clone().unwrap_or(default_state_root);
        let runtime_root = config.runtime_root.clone().unwrap_or(default_runtime_root);
        Self {
            supervisor,
            store,
            config,
            state_root,
            runtime_root,
            bridges: Mutex::new(HashMap::new()),
        }
    }

    /// Root directory holding per-workspace durable state volumes.
    pub fn state_root(&self) -> &std::path::Path {
        &self.state_root
    }

    /// Start (or restart) the host-side endpoint bridges for a workspace.
    async fn ensure_bridges(&self, workspace_id: &str) -> Result<()> {
        let endpoint_dir = self.runtime_root.join(workspace_id).join("endpoints");
        let mut bridges = self.bridges.lock().await;
        if bridges.contains_key(workspace_id) {
            return Ok(());
        }
        let mut spawned = Vec::new();
        for endpoint in &self.config.endpoints {
            let socket = endpoint_dir.join(format!("{}.sock", endpoint.name));
            spawned.push(
                HostEndpointBridge::spawn(socket, endpoint.target.clone())
                    .await
                    .with_context(|| {
                        format!(
                            "starting endpoint bridge {} for workspace {workspace_id}",
                            endpoint.name
                        )
                    })?,
            );
        }
        bridges.insert(workspace_id.to_string(), spawned);
        Ok(())
    }

    fn spec_for(
        &self,
        workspace_id: &str,
        account_id: &str,
        workspace_dir: PathBuf,
    ) -> PlacementSpec {
        PlacementSpec {
            workspace_id: workspace_id.to_string(),
            account_id: account_id.to_string(),
            image: self.config.image.clone(),
            workspace_dir,
            state_dir: self.state_root.join(workspace_id),
            // Auto userns chowns the runner socket dir into the container's
            // range; it gets its own subdirectory so endpoint sockets stay
            // backend-owned next to it.
            runner_endpoint: oqto_runner::transport::RunnerEndpointConfig::Unix {
                path: match self.config.userns {
                    PlacementUserns::KeepId => {
                        self.runtime_root.join(workspace_id).join("runner.sock")
                    }
                    PlacementUserns::Auto { .. } => self
                        .runtime_root
                        .join(workspace_id)
                        .join("rsock")
                        .join("runner.sock"),
                },
            },
            server_tls: None,
            environment: BTreeMap::new(),
            cpu_limit: self.config.cpu_limit.clone(),
            memory_limit: self.config.memory_limit.clone(),
            userns: self.config.userns.clone(),
            network: PlacementNetwork {
                mode: self.config.network.clone(),
                endpoints: self
                    .config
                    .endpoints
                    .iter()
                    .map(|endpoint| oqto_placement::PlacementEndpoint {
                        name: endpoint.name.clone(),
                        port: endpoint.port,
                    })
                    .collect(),
            },
        }
    }

    /// Provision (or replace) the container placement for a workspace.
    pub async fn provision(
        &self,
        workspace_id: &str,
        account_id: &str,
        workspace_dir: PathBuf,
    ) -> Result<PlacementRecord> {
        let spec = self.spec_for(workspace_id, account_id, workspace_dir);
        self.ensure_bridges(workspace_id).await?;
        let record = self
            .supervisor
            .start(&spec)
            .await
            .with_context(|| format!("starting placement for workspace {workspace_id}"))?;
        self.store
            .put(record.clone())
            .await
            .with_context(|| format!("recording placement for workspace {workspace_id}"))?;
        info!(
            workspace_id,
            runtime = %record.runtime_name,
            "provisioned container placement"
        );
        Ok(record)
    }

    /// Tear down the placement for a workspace. The durable state volume is
    /// retained; only the disposable compute is removed.
    pub async fn remove(&self, workspace_id: &str) -> Result<()> {
        let Some(record) = self.store.find_workspace(workspace_id).await? else {
            return Ok(());
        };
        self.supervisor
            .stop(&record)
            .await
            .with_context(|| format!("stopping placement for workspace {workspace_id}"))?;
        self.store.remove(&record.id).await?;
        self.bridges.lock().await.remove(workspace_id);
        info!(workspace_id, "removed container placement");
        Ok(())
    }

    /// Bring recorded placements back in line with reality at startup.
    /// Stopped placements with a recorded spec are restarted; records without
    /// a spec are reported but left alone.
    pub async fn reconcile(&self) -> Result<()> {
        for record in self.store.list().await? {
            if let Err(error) = self.ensure_bridges(&record.workspace_id).await {
                warn!(
                    workspace_id = %record.workspace_id,
                    %error,
                    "failed to start endpoint bridges during reconcile"
                );
            }
            let health = match self.supervisor.health(&record).await {
                Ok(health) => health,
                Err(error) => {
                    warn!(
                        workspace_id = %record.workspace_id,
                        %error,
                        "placement health check failed during reconcile"
                    );
                    continue;
                }
            };
            match health {
                PlacementHealth::Ready | PlacementHealth::Starting => {}
                PlacementHealth::Stopped | PlacementHealth::Unhealthy { .. } => {
                    let Some(spec) = record.spec.clone() else {
                        warn!(
                            workspace_id = %record.workspace_id,
                            "placement is down but has no recorded spec; manual repair needed"
                        );
                        continue;
                    };
                    info!(
                        workspace_id = %record.workspace_id,
                        "restarting stopped placement"
                    );
                    match self.supervisor.start(&spec).await {
                        Ok(new_record) => {
                            if let Err(error) = self.store.put(new_record).await {
                                warn!(
                                    workspace_id = %record.workspace_id,
                                    %error,
                                    "failed to record restarted placement"
                                );
                            }
                        }
                        Err(error) => {
                            warn!(
                                workspace_id = %record.workspace_id,
                                %error,
                                "failed to restart placement"
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oqto_placement::{PlacementId, PlacementKind};
    use std::sync::Mutex;

    struct FakeSupervisor {
        started: Mutex<Vec<String>>,
        stopped: Mutex<Vec<String>>,
        health: PlacementHealth,
    }

    impl FakeSupervisor {
        fn new(health: PlacementHealth) -> Self {
            Self {
                started: Mutex::new(Vec::new()),
                stopped: Mutex::new(Vec::new()),
                health,
            }
        }
    }

    #[async_trait]
    impl PlacementSupervisor for FakeSupervisor {
        async fn start(&self, spec: &PlacementSpec) -> Result<PlacementRecord> {
            self.started.lock().unwrap().push(spec.workspace_id.clone());
            Ok(PlacementRecord {
                id: PlacementId(format!("placement-{}", spec.workspace_id)),
                workspace_id: spec.workspace_id.clone(),
                account_id: spec.account_id.clone(),
                kind: PlacementKind::RootlessPodman,
                runner_endpoint: spec.runner_endpoint.clone(),
                runtime_name: format!("oqto-ws-{}", spec.workspace_id),
                spec: Some(spec.clone()),
            })
        }

        async fn stop(&self, placement: &PlacementRecord) -> Result<()> {
            self.stopped
                .lock()
                .unwrap()
                .push(placement.workspace_id.clone());
            Ok(())
        }

        async fn health(&self, _placement: &PlacementRecord) -> Result<PlacementHealth> {
            Ok(self.health.clone())
        }
    }

    async fn manager(
        supervisor: Arc<FakeSupervisor>,
        dir: &std::path::Path,
    ) -> Result<PlacementManager> {
        let store =
            Arc::new(oqto_placement::JsonPlacementStore::open(dir.join("placements.json")).await?);
        Ok(PlacementManager::new(
            supervisor,
            store,
            PlacementConfig {
                mode: PlacementMode::Container,
                ..Default::default()
            },
            dir.join("state"),
            dir.join("runtime"),
        ))
    }

    #[tokio::test]
    async fn provision_and_remove_round_trip() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let supervisor = Arc::new(FakeSupervisor::new(PlacementHealth::Ready));
        let manager = manager(supervisor.clone(), temp.path()).await?;

        let record = manager
            .provision("ws-1", "acct-1", temp.path().join("work"))
            .await?;
        assert_eq!(record.workspace_id, "ws-1");
        assert!(record.spec.is_some());

        manager.remove("ws-1").await?;
        assert_eq!(supervisor.stopped.lock().unwrap().as_slice(), ["ws-1"]);
        // Removing an unknown workspace is a no-op.
        manager.remove("ws-unknown").await?;
        Ok(())
    }

    #[tokio::test]
    async fn provision_spawns_endpoint_bridge_sockets() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let supervisor = Arc::new(FakeSupervisor::new(PlacementHealth::Ready));
        let store = Arc::new(
            oqto_placement::JsonPlacementStore::open(temp.path().join("placements.json")).await?,
        );
        let manager = PlacementManager::new(
            supervisor,
            store,
            PlacementConfig {
                mode: PlacementMode::Container,
                endpoints: vec![EndpointConfig {
                    name: "eavs".to_string(),
                    port: 3033,
                    target: "127.0.0.1:1".to_string(),
                }],
                ..Default::default()
            },
            temp.path().join("state"),
            temp.path().join("runtime"),
        );

        let record = manager
            .provision("ws-1", "acct-1", temp.path().join("work"))
            .await?;
        let spec = record.spec.expect("record carries spec");
        assert_eq!(
            spec.network.mode,
            oqto_placement::PlacementNetworkMode::Isolated
        );
        assert_eq!(spec.network.endpoints.len(), 1);
        assert!(
            temp.path()
                .join("runtime/ws-1/endpoints/eavs.sock")
                .exists()
        );

        manager.remove("ws-1").await?;
        assert!(
            !temp
                .path()
                .join("runtime/ws-1/endpoints/eavs.sock")
                .exists()
        );
        Ok(())
    }

    #[tokio::test]
    async fn reconcile_restarts_stopped_placements() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let supervisor = Arc::new(FakeSupervisor::new(PlacementHealth::Stopped));
        let manager = manager(supervisor.clone(), temp.path()).await?;

        manager
            .provision("ws-1", "acct-1", temp.path().join("work"))
            .await?;
        supervisor.started.lock().unwrap().clear();

        manager.reconcile().await?;
        assert_eq!(supervisor.started.lock().unwrap().as_slice(), ["ws-1"]);
        Ok(())
    }
}

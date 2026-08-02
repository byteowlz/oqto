//! Container placement lifecycle: provision, remove, and reconcile
//! per-Workspace runner placements (ADR-0019/0020).

use anyhow::{Context, Result};
use oqto_placement::{
    HostEndpointBridge, IMAGE_VERSION_LABEL, ImageAttestation, PlacementHealth, PlacementNetwork,
    PlacementNetworkMode, PlacementRecord, PlacementSpec, PlacementStore, PlacementSupervisor,
    PlacementUserns,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Backend release version this binary was built as. The workspace image's
/// `io.oqto.version` label must match this exactly for a strict placement.
const EXPECTED_IMAGE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementMode {
    /// Runners run on the host (legacy socket-per-user path).
    Local,
    /// Each Workspace runs as its own rootless container Pod.
    Container,
}

/// Workspace-image attestation policy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageVerification {
    /// Strict for registry refs, Dev for local refs (`localhost/`). Default.
    #[default]
    Auto,
    /// Require `io.oqto.version` label == backend version and (if set) digest
    /// pin. Applied to registry images under `Auto`.
    Strict,
    /// Skip attestation entirely. Only safe for local dev images.
    Dev,
}

impl ImageVerification {
    /// Resolve `Auto` against an image reference to a concrete policy.
    fn resolve(&self, image: &str) -> ResolvedPolicy {
        match self {
            ImageVerification::Strict => ResolvedPolicy::Strict,
            ImageVerification::Dev => ResolvedPolicy::Dev,
            ImageVerification::Auto => {
                if is_local_ref(image) {
                    ResolvedPolicy::Dev
                } else {
                    ResolvedPolicy::Strict
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedPolicy {
    Strict,
    Dev,
}

/// An image reference is "local" when it carries no registry host, i.e. it
/// resolves only from the local Podman store (`localhost/...`, `:dev`, bare
/// names without a `/`-separated host). Registry images (`ghcr.io/...`,
/// `docker.io/...`) are treated as remote and attested.
fn is_local_ref(image: &str) -> bool {
    let name = image.split(':').next().unwrap_or(image);
    if let Some(first_segment) = name.split('/').next() {
        // A first segment containing '.' or ':' or a port is a registry host.
        if first_segment.contains('.') || first_segment.contains(':') {
            return false;
        }
        // `localhost` (with or without a port) is the local registry daemon;
        // treat as local.
        if first_segment == "localhost" {
            return true;
        }
    }
    // Bare single-segment names (e.g. `oqto-workspace:dev`) resolve locally.
    !name.contains('/')
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlacementConfig {
    pub mode: PlacementMode,
    /// Workspace container image.
    pub image: String,
    /// Expected image digest (`sha256:...`) for production drift detection.
    /// When set and verification is not `Dev`, the resolved image digest must
    /// match exactly; mismatch fails closed.
    #[serde(default)]
    pub image_digest: Option<String>,
    /// Image attestation policy. `Auto` (default) enforces labels + digest for
    /// registry images and skips for clearly-local refs (`localhost/`).
    #[serde(default)]
    pub image_verification: ImageVerification,
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
            image_digest: None,
            image_verification: ImageVerification::default(),
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
    /// Cached image attestation result. Verified once per process before the
    /// first provision; reconcile of already-recorded placements bypasses it.
    image_attestation: Mutex<Option<Result<ImageAttestation, String>>>,
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
            image_attestation: Mutex::new(None),
        }
    }

    /// Verify the configured workspace image's compatibility attestation.
    ///
    /// Enforces, under a non-`Dev` policy:
    /// - the `io.oqto.version` label is present and equals the backend release
    ///   version (rejects unlabelled `:dev` and version drift like `X.Y.(Z-1)`);
    /// - when `image_digest` is configured, the resolved digest matches exactly
    ///   (rejects digest drift in production).
    ///
    /// The result is cached for the process lifetime; the first provision pays
    /// the `podman inspect` cost, subsequent provisions reuse the verdict.
    pub async fn verify_image(&self) -> Result<ImageAttestation> {
        let mut cache = self.image_attestation.lock().await;
        if let Some(cached) = cache.as_ref() {
            return match cached {
                Ok(attestation) => Ok(attestation.clone()),
                Err(message) => Err(anyhow::anyhow!(message.clone())),
            };
        }
        let outcome = self.verify_image_uncached().await;
        let stored = outcome
            .as_ref()
            .map(|a| a.clone())
            .map_err(|error| error.to_string());
        *cache = Some(stored);
        outcome
    }

    async fn verify_image_uncached(&self) -> Result<ImageAttestation> {
        let image = &self.config.image;
        let policy = self.config.image_verification.resolve(image);
        if policy == ResolvedPolicy::Dev {
            tracing::debug!(
                image,
                "image attestation skipped (dev policy); production deploys must pin a registry image"
            );
            return Ok(ImageAttestation::default());
        }
        let attestation = self
            .supervisor
            .resolve_image_attestation(image)
            .await
            .with_context(|| format!("resolving attestation for workspace image {image}"))?;
        match attestation.labels.get(IMAGE_VERSION_LABEL) {
            None => anyhow::bail!(
                "workspace image {image} is missing the {IMAGE_VERSION_LABEL} label; \
                 it is an unattested (likely :dev) image. Pull a version-matched release \
                 (ghcr.io/byteowlz/oqto-workspace:<version>) or set placement.image_verification = dev"
            ),
            Some(version) if version.is_empty() => anyhow::bail!(
                "workspace image {image} has an empty {IMAGE_VERSION_LABEL} label; \
                 build with deploy/workspace/build.sh --release so provenance is stamped"
            ),
            Some(version) if version != EXPECTED_IMAGE_VERSION => anyhow::bail!(
                "workspace image version mismatch: image reports {version}, backend expects \
                 {EXPECTED_IMAGE_VERSION}. Use ghcr.io/byteowlz/oqto-workspace:{EXPECTED_IMAGE_VERSION}"
            ),
            Some(_) => {}
        }
        if let Some(expected_digest) = &self.config.image_digest
            && !expected_digest.is_empty()
            && expected_digest != &attestation.digest
        {
            anyhow::bail!(
                "workspace image digest drift: configured {expected_digest} but {image} \
                 resolved to {}. Update placement.image_digest or repull the pinned image",
                attestation.digest
            );
        }
        tracing::info!(
            image,
            version = attestation.labels.get(IMAGE_VERSION_LABEL),
            digest = %attestation.digest,
            "workspace image attestation verified"
        );
        Ok(attestation)
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
        self.verify_image().await?;
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

    /// Provision the personal Workspace container for an Account. The work
    /// directory lives inside the durable state volume, so one volume carries
    /// both home state and files.
    pub async fn provision_personal(&self, user_id: &str) -> Result<PlacementRecord> {
        let workspace_dir = self.state_root.join(user_id).join("workspace");
        std::fs::create_dir_all(&workspace_dir)
            .with_context(|| format!("creating personal workspace dir for {user_id}"))?;
        self.provision(user_id, user_id, workspace_dir).await
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
        attestation: Mutex<Option<Result<ImageAttestation, String>>>,
    }

    impl FakeSupervisor {
        fn new(health: PlacementHealth) -> Self {
            Self {
                started: Mutex::new(Vec::new()),
                stopped: Mutex::new(Vec::new()),
                health,
                attestation: Mutex::new(None),
            }
        }

        fn with_attestation(self, outcome: Result<ImageAttestation, &str>) -> Self {
            *self.attestation.lock().unwrap() = Some(outcome.map_err(|e| e.to_string()));
            self
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

        async fn resolve_image_attestation(&self, _image: &str) -> Result<ImageAttestation> {
            match self.attestation.lock().unwrap().clone() {
                Some(Ok(a)) => Ok(a),
                Some(Err(msg)) => Err(anyhow::anyhow!(msg)),
                None => Err(anyhow::anyhow!("image not found locally (FakeSupervisor)")),
            }
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
    async fn personal_provisioning_keeps_workdir_inside_state_volume() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let supervisor = Arc::new(FakeSupervisor::new(PlacementHealth::Ready));
        let manager = manager(supervisor.clone(), temp.path()).await?;

        let record = manager.provision_personal("user-1").await?;
        assert_eq!(record.workspace_id, "user-1");
        assert_eq!(record.account_id, "user-1");
        let spec = record.spec.expect("record carries spec");
        assert_eq!(spec.state_dir, temp.path().join("state/user-1"));
        assert_eq!(
            spec.workspace_dir,
            temp.path().join("state/user-1/workspace")
        );
        assert!(spec.workspace_dir.is_dir());
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

    fn labelled(version: &str, digest: &str) -> ImageAttestation {
        let mut labels = BTreeMap::new();
        if !version.is_empty() {
            labels.insert(IMAGE_VERSION_LABEL.to_string(), version.to_string());
        }
        ImageAttestation {
            labels,
            digest: digest.to_string(),
        }
    }

    async fn strict_manager(
        supervisor: Arc<FakeSupervisor>,
        dir: &std::path::Path,
        image: &str,
        digest: Option<&str>,
    ) -> PlacementManager {
        let store = Arc::new(
            oqto_placement::JsonPlacementStore::open(dir.join("placements.json"))
                .await
                .unwrap(),
        );
        PlacementManager::new(
            supervisor,
            store,
            PlacementConfig {
                mode: PlacementMode::Container,
                image: image.to_string(),
                image_digest: digest.map(str::to_string),
                image_verification: ImageVerification::Strict,
                ..Default::default()
            },
            dir.join("state"),
            dir.join("runtime"),
        )
    }

    #[tokio::test]
    async fn verify_image_accepts_matching_release_labels() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let supervisor = Arc::new(
            FakeSupervisor::new(PlacementHealth::Ready)
                .with_attestation(Ok(labelled(EXPECTED_IMAGE_VERSION, "sha256:deadbeef"))),
        );
        let manager = strict_manager(
            supervisor,
            temp.path(),
            "ghcr.io/byteowlz/oqto-workspace:0.5.0",
            None,
        )
        .await;
        manager.verify_image().await?;
        Ok(())
    }

    #[tokio::test]
    async fn verify_image_rejects_version_drift() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let supervisor = Arc::new(
            FakeSupervisor::new(PlacementHealth::Ready).with_attestation(Ok(labelled("0.4.9", ""))),
        );
        let manager = strict_manager(
            supervisor,
            temp.path(),
            "ghcr.io/byteowlz/oqto-workspace:0.4.9",
            None,
        )
        .await;
        let error = manager.verify_image().await.unwrap_err().to_string();
        assert!(error.contains("version mismatch"), "{error}");
        assert!(error.contains("0.4.9"), "{error}");
        Ok(())
    }

    #[tokio::test]
    async fn verify_image_rejects_unlabelled_dev_image() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let supervisor = Arc::new(
            FakeSupervisor::new(PlacementHealth::Ready).with_attestation(Ok(labelled("", ""))),
        );
        let manager = strict_manager(
            supervisor,
            temp.path(),
            "ghcr.io/byteowlz/oqto-workspace:dev",
            None,
        )
        .await;
        let error = manager.verify_image().await.unwrap_err().to_string();
        assert!(
            error.contains("empty") || error.contains("missing"),
            "{error}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn verify_image_rejects_digest_drift() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let supervisor = Arc::new(
            FakeSupervisor::new(PlacementHealth::Ready)
                .with_attestation(Ok(labelled(EXPECTED_IMAGE_VERSION, "sha256:actual"))),
        );
        let manager = strict_manager(
            supervisor,
            temp.path(),
            "ghcr.io/byteowlz/oqto-workspace:0.5.0",
            Some("sha256:configured"),
        )
        .await;
        let error = manager.verify_image().await.unwrap_err().to_string();
        assert!(error.contains("digest drift"), "{error}");
        Ok(())
    }

    #[tokio::test]
    async fn verify_image_skipped_for_local_dev_ref_under_auto() -> Result<()> {
        let temp = tempfile::tempdir()?;
        // No attestation configured -> the default trait impl would bail, but
        // Auto policy must short-circuit before calling the supervisor.
        let supervisor = Arc::new(FakeSupervisor::new(PlacementHealth::Ready));
        let store = Arc::new(
            oqto_placement::JsonPlacementStore::open(temp.path().join("placements.json"))
                .await
                .unwrap(),
        );
        let manager = PlacementManager::new(
            supervisor,
            store,
            PlacementConfig {
                mode: PlacementMode::Container,
                image: "localhost/oqto-workspace:dev".to_string(),
                image_verification: ImageVerification::Auto,
                ..Default::default()
            },
            temp.path().join("state"),
            temp.path().join("runtime"),
        );
        manager.verify_image().await?;
        Ok(())
    }

    #[test]
    fn local_ref_detection() {
        assert!(is_local_ref("localhost/oqto-workspace:dev"));
        assert!(is_local_ref("oqto-workspace:dev"));
        assert!(!is_local_ref("ghcr.io/byteowlz/oqto-workspace:0.5.0"));
        assert!(!is_local_ref("docker.io/library/ubuntu:24.04"));
        assert!(!is_local_ref("registry.example.com:5000/oqto:1.0"));
    }
}

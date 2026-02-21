//! Session service - orchestrates container lifecycle.
//!
//! This service manages the lifecycle of user sessions, supporting both:
//! - Container mode (Docker/Podman)
//! - Local mode (native processes)
//!
//! The service manages session lifecycles and runtime orchestration.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use log::{debug, error, info, warn};
use serde::Serialize;
use uuid::Uuid;

use crate::agent_browser::{AgentBrowserConfig, AgentBrowserManager, browser_session_name};

#[derive(Debug, Clone)]
pub enum BrowserAction {
    Back,
    Forward,
    Reload,
    ColorScheme(String),
}

impl BrowserAction {
    pub fn parse(action: &str) -> Option<Self> {
        match action {
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            "reload" => Some(Self::Reload),
            _ => action
                .strip_prefix("color_scheme:")
                .map(|scheme| Self::ColorScheme(scheme.to_string())),
        }
    }
}
use crate::container::{ContainerConfig, ContainerRuntimeApi, ContainerStats};
use crate::eavs::{CreateKeyRequest, EavsApi, KeyPermissions};
use crate::local::{LocalRuntime, LocalRuntimeConfig, UserMmryManager};
use crate::runner::client::RunnerClient;
use crate::wordlist;

use super::models::{CreateSessionRequest, RuntimeMode, Session, SessionStatus};
use super::repository::SessionRepository;
use super::workspace_locations::WorkspaceLocationRepository;

/// Prefix used for container names managed by this orchestrator.
const CONTAINER_NAME_PREFIX: &str = "oqto-";

/// Default container image.
const DEFAULT_IMAGE: &str = "oqto-dev:latest";

/// Default base port.
const DEFAULT_BASE_PORT: i64 = 41820;

#[async_trait]
trait SessionReadiness: Send + Sync {
    async fn wait_for_session_services(&self, fileserver_port: u16, ttyd_port: u16) -> Result<()>;
}

#[derive(Debug, Default)]
struct HttpSessionReadiness;

#[async_trait]
impl SessionReadiness for HttpSessionReadiness {
    async fn wait_for_session_services(&self, fileserver_port: u16, ttyd_port: u16) -> Result<()> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("building readiness HTTP client")?;

        let fileserver_url = format!("http://localhost:{}/tree?path=.", fileserver_port);
        let ttyd_url = format!("http://localhost:{}/", ttyd_port);

        let start = tokio::time::Instant::now();
        let timeout = tokio::time::Duration::from_secs(60);
        let mut attempts: u32 = 0;

        loop {
            attempts += 1;

            let fileserver_ok = client
                .get(&fileserver_url)
                .send()
                .await
                .map(|res| res.status().is_success())
                .unwrap_or(false);

            let ttyd_ok = client
                .get(&ttyd_url)
                .send()
                .await
                .map(|res| res.status().is_success())
                .unwrap_or(false);

            if ttyd_ok && fileserver_ok {
                return Ok(());
            }

            if start.elapsed() >= timeout {
                anyhow::bail!(
                    "session services not ready after {} attempts over {:?} (fileserver_ok={}, ttyd_ok={})",
                    attempts,
                    timeout,
                    fileserver_ok,
                    ttyd_ok
                );
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }
    }
}

/// Session service configuration.
#[derive(Debug, Clone)]
pub struct SessionServiceConfig {
    /// Default container image to use (container mode only).
    pub default_image: String,
    /// Base port for allocating session ports.
    pub base_port: i64,
    /// Base directory for user home directories (container mode).
    /// Each user gets {base}/home/{user_id}/.
    pub user_data_path: String,
    /// Path to skeleton directory to copy into new user homes. If None, empty dirs are created.
    pub skel_path: Option<String>,
    /// Default budget limit per session in USD.
    pub default_session_budget_usd: Option<f64>,
    /// Default rate limit per session (requests per minute).
    pub default_session_rpm: Option<u32>,
    /// URL for containers to reach EAVS (e.g., http://host.docker.internal:41800).
    pub eavs_container_url: Option<String>,
    /// Runtime mode (container or local).
    pub runtime_mode: RuntimeMode,
    /// Local runtime configuration (used when runtime_mode is Local).
    pub local_config: Option<LocalRuntimeConfig>,
    /// Enable single-user mode. When true, the platform operates with a single user
    /// and uses simplified paths without user_id subdirectories.
    pub single_user: bool,
    /// Whether mmry integration is enabled.
    pub mmry_enabled: bool,
    /// URL for containers to reach the host mmry service.
    /// e.g., "http://host.docker.internal:8081" or "http://host.containers.internal:8081"
    pub mmry_container_url: Option<String>,
    /// Maximum concurrent running sessions per user.
    pub max_concurrent_sessions: i64,
    /// Idle timeout in minutes before stopping a session.
    pub idle_timeout_minutes: i64,
    /// Idle cleanup check interval in seconds.
    pub idle_check_interval_seconds: u64,
    /// Whether Pi (Main Chat AI) is enabled in container mode.
    /// When true, pi-bridge will be started inside containers.
    pub pi_bridge_enabled: bool,
    /// Default LLM provider for Pi (e.g., "anthropic").
    pub pi_provider: Option<String>,
    /// Default model for Pi (e.g., "claude-sonnet-4-20250514").
    pub pi_model: Option<String>,
    /// Agent-browser daemon configuration.
    pub agent_browser: AgentBrowserConfig,
    /// Runner socket pattern for multi-user mode.
    /// Supports `{user}` (Linux username) and `{uid}`.
    /// Example: `/run/oqto/runner-sockets/{user}/oqto-runner.sock`
    pub runner_socket_pattern: Option<String>,
    /// Linux user prefix for multi-user mode (e.g., "oqto_").
    /// Used to convert platform user_id to Linux username for runner socket paths.
    pub linux_user_prefix: Option<String>,
}

impl Default for SessionServiceConfig {
    fn default() -> Self {
        Self {
            default_image: DEFAULT_IMAGE.to_string(),
            base_port: DEFAULT_BASE_PORT,
            user_data_path: "./data".to_string(),
            skel_path: None,
            default_session_budget_usd: Some(10.0),
            default_session_rpm: Some(60),
            eavs_container_url: None,
            runtime_mode: RuntimeMode::Container,
            local_config: None,
            single_user: false,
            mmry_enabled: false,
            mmry_container_url: None,
            max_concurrent_sessions: SessionService::DEFAULT_MAX_CONCURRENT_SESSIONS,
            idle_timeout_minutes: SessionService::DEFAULT_IDLE_TIMEOUT_MINUTES,
            idle_check_interval_seconds: 5 * 60,
            pi_bridge_enabled: false,
            pi_provider: None,
            pi_model: None,
            agent_browser: AgentBrowserConfig::default(),
            runner_socket_pattern: None,
            linux_user_prefix: None,
        }
    }
}

/// A view of `SessionService` scoped to a single user.
#[derive(Clone, Copy)]
pub struct UserSessionService<'a> {
    svc: &'a SessionService,
    user_id: &'a str,
}

impl<'a> UserSessionService<'a> {
    pub fn workspace_root(&self) -> std::path::PathBuf {
        self.svc.workspace_root_for_user(self.user_id)
    }

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.svc.list_sessions_for_user(self.user_id).await
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<Session>> {
        self.svc
            .get_session_for_user(self.user_id, session_id)
            .await
    }

    pub async fn create_session(&self, request: CreateSessionRequest) -> Result<Session> {
        self.svc
            .create_session_for_user(self.user_id, request)
            .await
    }

    pub async fn get_or_create_session(&self, request: CreateSessionRequest) -> Result<Session> {
        self.svc
            .get_or_create_session_for_user(self.user_id, request)
            .await
    }

    pub async fn get_or_create_session_for_workspace(
        &self,
        workspace_path: &str,
    ) -> Result<Session> {
        self.svc
            .get_or_create_session_for_workspace_for_user(self.user_id, workspace_path)
            .await
    }

    pub async fn get_or_create_io_session_for_workspace(
        &self,
        workspace_path: &str,
    ) -> Result<Session> {
        self.svc
            .get_or_create_io_session_for_workspace_for_user(self.user_id, workspace_path)
            .await
    }

    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        self.svc
            .stop_session_for_user(self.user_id, session_id)
            .await
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        self.svc
            .delete_session_for_user(self.user_id, session_id)
            .await
    }

    pub async fn resume_session(&self, session_id: &str) -> Result<Session> {
        self.svc
            .resume_session_for_user(self.user_id, session_id)
            .await
    }

    pub async fn resume_session_for_io(&self, session_id: &str) -> Result<Session> {
        self.svc
            .resume_session_for_io_for_user(self.user_id, session_id)
            .await
    }

    pub async fn touch_session_activity(&self, session_id: &str) -> Result<()> {
        self.svc
            .touch_session_activity_for_user(self.user_id, session_id)
            .await
    }

    pub async fn check_for_image_update(&self, session_id: &str) -> Result<Option<String>> {
        self.svc
            .check_for_image_update_for_user(self.user_id, session_id)
            .await
    }

    pub async fn upgrade_session(&self, session_id: &str) -> Result<Session> {
        self.svc
            .upgrade_session_for_user(self.user_id, session_id)
            .await
    }

    pub async fn check_all_for_updates(&self) -> Result<Vec<(String, String)>> {
        self.svc.check_all_for_updates_for_user(self.user_id).await
    }

    pub async fn ensure_user_mmry_pinned(&self) -> Result<u16> {
        self.svc.ensure_user_mmry_pinned(self.user_id).await
    }

    pub async fn validate_workspace_path(&self, path: &str) -> Result<std::path::PathBuf> {
        self.svc.resolve_workspace_path(self.user_id, path).await
    }

    pub fn workspace_locations(&self) -> &WorkspaceLocationRepository {
        &self.svc.workspace_locations
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionContainerStats {
    pub session_id: String,
    pub container_id: String,
    pub container_name: String,
    pub stats: ContainerStats,
}

#[derive(Debug, Clone)]
pub struct ContainerStatsReport {
    pub stats: Vec<SessionContainerStats>,
    pub errors: Vec<String>,
}

/// Service for managing container sessions.
#[derive(Clone)]
pub struct SessionService {
    repo: SessionRepository,
    workspace_locations: WorkspaceLocationRepository,
    /// Container runtime (used when runtime_mode is Container).
    container_runtime: Option<Arc<dyn ContainerRuntimeApi>>,
    /// Runner client for local mode. All local process spawning goes through
    /// the runner daemon, which provides consistent process management and
    /// enables user-plane isolation in multi-user deployments.
    runner: Option<RunnerClient>,
    /// Local runtime config for port utilities only (check/clear ports).
    /// Process spawning goes through runner, not this.
    local_runtime: Option<Arc<LocalRuntime>>,
    eavs: Option<Arc<dyn EavsApi>>,
    readiness: Arc<dyn SessionReadiness>,
    agent_browser: AgentBrowserManager,
    config: SessionServiceConfig,
    user_mmry: Option<Arc<UserMmryManager>>,
}

impl SessionService {
    /// Create a view of this service scoped to a specific user.
    pub fn for_user<'a>(&'a self, user_id: &'a str) -> UserSessionService<'a> {
        UserSessionService { svc: self, user_id }
    }

    pub fn workspace_locations(&self) -> &WorkspaceLocationRepository {
        &self.workspace_locations
    }

    /// Create a new session service with container runtime.
    pub fn new(
        repo: SessionRepository,
        runtime: Arc<dyn ContainerRuntimeApi>,
        config: SessionServiceConfig,
    ) -> Self {
        let workspace_locations = WorkspaceLocationRepository::new(repo.pool().clone());
        Self {
            repo,
            workspace_locations,
            container_runtime: Some(runtime),
            runner: None,
            local_runtime: None,
            eavs: None,
            readiness: Arc::new(HttpSessionReadiness),
            agent_browser: AgentBrowserManager::new(config.agent_browser.clone()),
            config,
            user_mmry: None,
        }
    }

    /// Create a new session service with EAVS integration.
    pub fn with_eavs(
        repo: SessionRepository,
        runtime: Arc<dyn ContainerRuntimeApi>,
        eavs: Arc<dyn EavsApi>,
        config: SessionServiceConfig,
    ) -> Self {
        let workspace_locations = WorkspaceLocationRepository::new(repo.pool().clone());
        Self {
            repo,
            workspace_locations,
            container_runtime: Some(runtime),
            runner: None,
            local_runtime: None,
            eavs: Some(eavs),
            readiness: Arc::new(HttpSessionReadiness),
            agent_browser: AgentBrowserManager::new(config.agent_browser.clone()),
            config,
            user_mmry: None,
        }
    }

    /// Create a new session service with runner for local mode.
    ///
    /// All local process spawning goes through the runner daemon.
    /// LocalRuntime is used only for port utilities (check/clear).
    pub fn with_runner(
        repo: SessionRepository,
        runner: RunnerClient,
        local_runtime: LocalRuntime,
        config: SessionServiceConfig,
    ) -> Self {
        let workspace_locations = WorkspaceLocationRepository::new(repo.pool().clone());
        Self {
            repo,
            workspace_locations,
            container_runtime: None,
            runner: Some(runner),
            local_runtime: Some(Arc::new(local_runtime)),
            eavs: None,
            readiness: Arc::new(HttpSessionReadiness),
            agent_browser: AgentBrowserManager::new(config.agent_browser.clone()),
            config,
            user_mmry: None,
        }
    }

    /// Create a new session service with runner and EAVS integration.
    pub fn with_runner_and_eavs(
        repo: SessionRepository,
        runner: RunnerClient,
        local_runtime: LocalRuntime,
        eavs: Arc<dyn EavsApi>,
        config: SessionServiceConfig,
    ) -> Self {
        let workspace_locations = WorkspaceLocationRepository::new(repo.pool().clone());
        Self {
            repo,
            workspace_locations,
            container_runtime: None,
            runner: Some(runner),
            local_runtime: Some(Arc::new(local_runtime)),
            eavs: Some(eavs),
            readiness: Arc::new(HttpSessionReadiness),
            agent_browser: AgentBrowserManager::new(config.agent_browser.clone()),
            config,
            user_mmry: None,
        }
    }

    /// Enable per-user mmry instances (local multi-user mode).
    pub fn with_user_mmry(mut self, manager: UserMmryManager) -> Self {
        self.user_mmry = Some(Arc::new(manager));
        self
    }

    /// Get runner client for a user.
    ///
    /// In single-user mode, returns the shared runner.
    /// In multi-user mode, creates a client for the user's per-user runner socket.
    fn runner_for_user(&self, user_id: &str) -> Result<RunnerClient> {
        if self.config.single_user {
            // Single-user mode: use the shared runner
            self.runner
                .clone()
                .context("runner not configured for local mode")
        } else {
            // Multi-user mode: connect to user's runner socket.
            // Convert platform user_id (e.g., "hansgerd-u469") to Linux username
            // (e.g., "oqto_hansgerd-u469") using the configured prefix.
            let linux_user = if let Some(ref prefix) = self.config.linux_user_prefix {
                let sanitized = crate::local::linux_users::sanitize_username(user_id);
                if crate::local::linux_users::user_exists(&sanitized) {
                    sanitized
                } else {
                    format!("{prefix}{sanitized}")
                }
            } else {
                user_id.to_string()
            };

            if let Some(pattern) = &self.config.runner_socket_pattern {
                RunnerClient::for_user_with_pattern(&linux_user, pattern)
            } else {
                // Fallback to default pattern
                RunnerClient::for_user(&linux_user)
            }
        }
    }

    async fn ensure_user_mmry_pinned(&self, user_id: &str) -> Result<u16> {
        let Some(ref user_mmry) = self.user_mmry else {
            anyhow::bail!("UserMmryManager not configured");
        };
        user_mmry.pin_user_mmry(user_id).await
    }

    async fn stop_session_for_user(&self, user_id: &str, session_id: &str) -> Result<()> {
        if self
            .get_session_for_user(user_id, session_id)
            .await?
            .is_none()
        {
            anyhow::bail!("Session not found");
        }
        self.stop_session(session_id).await
    }

    async fn delete_session_for_user(&self, user_id: &str, session_id: &str) -> Result<()> {
        if self
            .get_session_for_user(user_id, session_id)
            .await?
            .is_none()
        {
            anyhow::bail!("Session not found");
        }
        self.delete_session(session_id).await
    }

    async fn resume_session_for_user(&self, user_id: &str, session_id: &str) -> Result<Session> {
        if self
            .get_session_for_user(user_id, session_id)
            .await?
            .is_none()
        {
            anyhow::bail!("Session not found");
        }
        self.resume_session(session_id).await
    }

    async fn resume_session_for_io_for_user(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Session> {
        if self
            .get_session_for_user(user_id, session_id)
            .await?
            .is_none()
        {
            anyhow::bail!("Session not found");
        }
        self.resume_session_for_io(session_id).await
    }

    async fn check_for_image_update_for_user(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Option<String>> {
        if self
            .get_session_for_user(user_id, session_id)
            .await?
            .is_none()
        {
            anyhow::bail!("Session not found");
        }
        self.check_for_image_update(session_id).await
    }

    async fn upgrade_session_for_user(&self, user_id: &str, session_id: &str) -> Result<Session> {
        if self
            .get_session_for_user(user_id, session_id)
            .await?
            .is_none()
        {
            anyhow::bail!("Session not found");
        }
        self.upgrade_session(session_id).await
    }

    async fn check_all_for_updates_for_user(&self, user_id: &str) -> Result<Vec<(String, String)>> {
        let sessions = self.repo.list_for_user(user_id).await?;
        let mut updates = Vec::new();
        for session in sessions {
            if let Ok(Some(new_digest)) = self.check_for_image_update(&session.id).await {
                updates.push((session.id, new_digest));
            }
        }
        Ok(updates)
    }

    /// Get the container runtime (if available).
    fn container_runtime(&self) -> Option<&Arc<dyn ContainerRuntimeApi>> {
        self.container_runtime.as_ref()
    }

    /// Get the local runtime (if available).
    fn local_runtime(&self) -> Option<&Arc<LocalRuntime>> {
        self.local_runtime.as_ref()
    }

    fn workspace_root_for_user(&self, user_id: &str) -> std::path::PathBuf {
        if self.config.runtime_mode == RuntimeMode::Local {
            if let Some(ref local_config) = self.config.local_config {
                if local_config.single_user {
                    return local_config.workspace_base();
                }
                return local_config.workspace_for_user(user_id);
            }
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            if self.config.single_user {
                return std::path::PathBuf::from(home).join("oqto");
            }
            return std::path::PathBuf::from(home).join("oqto").join(user_id);
        }
        // Container mode - use /workspace
        std::path::PathBuf::from("/workspace")
    }

    /// Check if agent-browser integration is enabled.
    pub fn agent_browser_enabled(&self) -> bool {
        self.config.agent_browser.enabled
    }

    /// Get the agent-browser stream port for a session.
    pub fn agent_browser_stream_port(&self, session_id: &str) -> Result<Option<u16>> {
        if !self.config.agent_browser.enabled {
            return Ok(None);
        }
        self.agent_browser
            .stream_port_for_session(session_id)
            .map(Some)
    }

    /// Navigate the agent-browser to a URL, launching the daemon if needed.
    /// Optionally sets the viewport size.
    pub async fn navigate_browser(
        &self,
        session_id: &str,
        url: &str,
        viewport_width: Option<u32>,
        viewport_height: Option<u32>,
    ) -> Result<()> {
        self.agent_browser.navigate_to(session_id, url).await?;
        if let (Some(w), Some(h)) = (viewport_width, viewport_height) {
            self.agent_browser.set_viewport(session_id, w, h).await?;
        }
        Ok(())
    }

    /// Run a browser action (back, forward, reload, color_scheme) for a session.
    pub async fn browser_action(&self, session_id: &str, action: BrowserAction) -> Result<()> {
        match action {
            BrowserAction::Back => self.agent_browser.go_back(session_id).await,
            BrowserAction::Forward => self.agent_browser.go_forward(session_id).await,
            BrowserAction::Reload => self.agent_browser.reload(session_id).await,
            BrowserAction::ColorScheme(ref scheme) => {
                self.agent_browser
                    .set_color_scheme(session_id, scheme)
                    .await
            }
        }
    }

    pub async fn list_sessions_for_user(&self, user_id: &str) -> Result<Vec<Session>> {
        let sessions = self.repo.list_for_user(user_id).await?;
        let mut reconciled = Vec::with_capacity(sessions.len());
        for session in sessions {
            reconciled.push(self.reconcile_session_container_state(session).await?);
        }
        Ok(reconciled)
    }

    pub async fn get_session_for_user(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Option<Session>> {
        let session = self.repo.get(session_id).await?;
        match session {
            Some(session) if session.user_id == user_id => {
                Ok(Some(self.reconcile_session_container_state(session).await?))
            }
            _ => Ok(None),
        }
    }

    fn allowed_workspace_roots(&self, user_id: &str) -> Vec<std::path::PathBuf> {
        let mut roots = Vec::new();

        let workspace_root = self.workspace_root_for_user(user_id);
        roots.push(workspace_root.canonicalize().unwrap_or(workspace_root));

        // Include this user's data directory (for per-user workspace data).
        //
        // IMPORTANT: do NOT allow the entire data directory; that would let users
        // reference other users' data by path. We only allow the per-user subtree.
        let users_data_root = std::path::PathBuf::from(&self.config.user_data_path).join("users");
        let user_data_root = if self.config.single_user {
            users_data_root.join("main")
        } else {
            users_data_root.join(user_id)
        };
        roots.push(user_data_root.canonicalize().unwrap_or(user_data_root));

        roots
    }

    async fn resolve_workspace_path(
        &self,
        user_id: &str,
        path: &str,
    ) -> Result<std::path::PathBuf> {
        let requested = std::path::PathBuf::from(path);
        let resolved = if requested.is_absolute() {
            requested
        } else {
            self.workspace_root_for_user(user_id).join(&requested)
        };

        if !resolved.exists() {
            if let Some(location) = self
                .workspace_locations
                .get_active_location(user_id, path)
                .await?
            {
                if location.kind != "local" {
                    anyhow::bail!(
                        "workspace location is remote; select a local location for {}",
                        path
                    );
                }
                let location_path = std::path::PathBuf::from(&location.path);
                if !location_path.exists() {
                    anyhow::bail!(
                        "workspace location path does not exist: {}",
                        location_path.display()
                    );
                }
                let canonical = location_path.canonicalize().with_context(|| {
                    format!("resolving workspace location {}", location_path.display())
                })?;
                return Ok(canonical);
            }

            if let Some(parent) = resolved.parent()
                && parent.exists()
            {
                let canonical_parent = parent
                    .canonicalize()
                    .with_context(|| format!("resolving workspace parent {}", parent.display()))?;
                let allowed_roots = self.allowed_workspace_roots(user_id);
                if !allowed_roots
                    .iter()
                    .any(|root| canonical_parent.starts_with(root))
                {
                    anyhow::bail!(
                        "workspace path {} is outside allowed roots",
                        resolved.display()
                    );
                }
                return Ok(resolved);
            }

            anyhow::bail!("workspace path does not exist: {}", resolved.display());
        }

        let canonical = resolved
            .canonicalize()
            .with_context(|| format!("resolving workspace path {}", resolved.display()))?;
        let allowed_roots = self.allowed_workspace_roots(user_id);
        if !allowed_roots.iter().any(|root| canonical.starts_with(root)) {
            anyhow::bail!(
                "workspace path {} is outside allowed roots",
                canonical.display()
            );
        }

        Ok(canonical)
    }

    /// Maximum number of retries for port allocation conflicts.
    const MAX_PORT_ALLOCATION_RETRIES: u32 = 5;

    /// Get or create a session for a user.
    ///
    /// This method:
    /// 1. Checks if there's a running session that needs upgrading (image changed)
    /// 2. Checks if there's a stopped session that can be resumed
    /// 3. Creates a new session if neither exists
    ///
    /// This provides the best user experience: auto-upgrades when image changes,
    /// fast restarts when possible, new sessions when needed.
    async fn get_or_create_session_for_user(
        &self,
        user_id: &str,
        request: CreateSessionRequest,
    ) -> Result<Session> {
        // Check for running sessions that need upgrading
        let running_sessions = self.repo.list_running_for_user(user_id).await?;
        if let Some(session) = running_sessions.into_iter().next() {
            if let Ok(Some(_new_digest)) = self.check_for_image_update(&session.id).await {
                info!(
                    "Running session {} has outdated image, auto-upgrading...",
                    session.id
                );
                return self.upgrade_session(&session.id).await;
            }
            // Session is running and up-to-date, return it
            return Ok(session);
        }

        // Check for a resumable stopped session
        if let Some(stopped_session) = self.repo.find_resumable_session(user_id).await? {
            // Verify the session can still be resumed
            let can_resume = match stopped_session.runtime_mode {
                RuntimeMode::Container => {
                    if let Some(ref container_id) = stopped_session.container_id {
                        if let Some(runtime) = self.container_runtime() {
                            match runtime.container_state_status(container_id).await {
                                Ok(Some(status)) if status == "exited" || status == "stopped" => {
                                    true
                                }
                                Ok(Some(status)) => {
                                    debug!(
                                        "Stopped session {} has container in unexpected state: {}",
                                        stopped_session.id, status
                                    );
                                    false
                                }
                                Ok(None) => {
                                    debug!(
                                        "Container {} for stopped session {} no longer exists",
                                        container_id, stopped_session.id
                                    );
                                    false
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to check container state for session {}: {:?}",
                                        stopped_session.id, e
                                    );
                                    false
                                }
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                RuntimeMode::Local => {
                    // Local mode sessions can always be resumed (processes are respawned)
                    true
                }
            };

            if can_resume {
                info!(
                    "Found resumable session {} for user {}, resuming...",
                    stopped_session.id, user_id
                );
                return self.resume_session(&stopped_session.id).await;
            }
        }

        // No resumable session found, create a new one
        self.create_session_for_user(user_id, request).await
    }

    /// Get or create the primary agent session for the user.
    /// Create and start a new session.
    ///
    /// This method handles port allocation with retry logic to handle race conditions
    /// when multiple users create sessions simultaneously. The database has partial unique
    /// indexes on ports for active sessions, so if two requests try to allocate the same
    /// ports concurrently, one will fail and retry with a different range.
    ///
    /// Security: the EAVS virtual key is never persisted to the database; it is passed
    /// directly into container env and then dropped.
    async fn create_session_for_user(
        &self,
        user_id: &str,
        request: CreateSessionRequest,
    ) -> Result<Session> {
        self.create_session_with_readiness(user_id, request).await
    }

    async fn create_session_with_readiness(
        &self,
        user_id: &str,
        request: CreateSessionRequest,
    ) -> Result<Session> {
        let image = request
            .image
            .unwrap_or_else(|| self.config.default_image.clone());

        // Get current image digest for tracking upgrades (best-effort, container mode only).
        let image_digest = if self.config.runtime_mode == RuntimeMode::Container {
            if let Some(runtime) = self.container_runtime() {
                match runtime.get_image_digest(&image).await {
                    Ok(digest) => digest,
                    Err(e) => {
                        warn!("Failed to get image digest for {}: {:?}", image, e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None // Local mode doesn't track image digests
        };

        // Determine user home path - either provided or create per-user home directory.
        let user_home_path = if let Some(path) = request.workspace_path {
            self.resolve_workspace_path(user_id, &path)
                .await?
                .to_string_lossy()
                .to_string()
        } else {
            // Determine workspace path based on runtime mode and single_user setting
            let user_home = if self.config.runtime_mode == RuntimeMode::Local {
                if let Some(ref local_config) = self.config.local_config {
                    if local_config.single_user {
                        // Single-user mode: use workspace_dir directly (no {user_id} substitution)
                        local_config.workspace_base()
                    } else {
                        // Multi-user mode: expand {user_id} placeholder
                        local_config.workspace_for_user(user_id)
                    }
                } else {
                    // Fallback to default local workspace pattern
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                    if self.config.single_user {
                        std::path::PathBuf::from(format!("{}/oqto", home))
                    } else {
                        std::path::PathBuf::from(format!("{}/oqto/{}", home, user_id))
                    }
                }
            } else {
                // Container mode: use user_data_path/home/{user_id}
                std::path::Path::new(&self.config.user_data_path)
                    .join("home")
                    .join(user_id)
            };

            if !user_home.exists() {
                // If skel_path is configured, copy it; otherwise create empty dirs
                if let Some(ref skel_path) = self.config.skel_path {
                    let skel = std::path::Path::new(skel_path);
                    if skel.exists() {
                        copy_dir_recursive(skel, &user_home).with_context(|| {
                            format!("copying skel from {:?} to {:?}", skel, user_home)
                        })?;
                        info!(
                            "Created home directory for user {} from skel: {:?}",
                            user_id, user_home
                        );
                    } else {
                        warn!("Skel path {:?} does not exist, creating empty dirs", skel);
                        create_empty_home_dirs(&user_home)?;
                    }
                } else {
                    create_empty_home_dirs(&user_home)?;
                    info!(
                        "Created home directory for user {}: {:?}",
                        user_id, user_home
                    );
                }
            }

            user_home.to_string_lossy().to_string()
        };

        // Use agent from request (LocalRuntime will apply default_agent if None)
        let agent = request.agent.clone();

        let mut last_error = None;
        for attempt in 0..Self::MAX_PORT_ALLOCATION_RETRIES {
            match self
                .try_create_session(
                    &user_home_path,
                    &image,
                    image_digest.as_deref(),
                    agent.as_deref(),
                    user_id,
                    attempt,
                )
                .await
            {
                Ok(session) => return Ok(session),
                Err(e) => {
                    if Self::is_retryable_unique_violation(&e) {
                        warn!(
                            "Port allocation conflict on attempt {}, retrying: {:?}",
                            attempt + 1,
                            e
                        );
                        last_error = Some(e);
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            50 * (attempt as u64 + 1),
                        ))
                        .await;
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!(
                "failed to allocate ports after {} attempts",
                Self::MAX_PORT_ALLOCATION_RETRIES
            )
        }))
    }

    /// Check if an error is a retryable unique constraint violation.
    fn is_retryable_unique_violation(error: &anyhow::Error) -> bool {
        for cause in error.chain() {
            if let Some(sqlx_error) = cause.downcast_ref::<sqlx::Error>()
                && let sqlx::Error::Database(db_err) = sqlx_error
                && db_err.is_unique_violation()
            {
                return true;
            }
        }

        let error_str = error.to_string().to_lowercase();
        error_str.contains("unique constraint")
            || error_str.contains("unique_violation")
            || error_str.contains("duplicate")
    }

    /// Maximum number of sub-agents per session.
    const DEFAULT_MAX_AGENTS: i64 = 10;

    /// Internal method to attempt session creation with a specific port range.
    async fn try_create_session(
        &self,
        user_home_path: &str,
        image: &str,
        image_digest: Option<&str>,
        agent: Option<&str>,
        user_id: &str,
        attempt: u32,
    ) -> Result<Session> {
        let session_id = Uuid::new_v4().to_string();
        let container_name = format!("{}{}", CONTAINER_NAME_PREFIX, &session_id[..8]);

        // Find available ports (agent, fileserver, ttyd, + sub-agent ports). On retry, offset the search window.
        // Container mode may also reserve a per-session mmry port when enabled.
        // Port layout:
        //   base+0: agent (reserved for future use)
        //   base+1: fileserver
        //   base+2: ttyd
        //   base+3+: sub-agents (local mode)
        //   base+3: mmry, base+4+: sub-agents (container mode, if mmry enabled)
        let max_agents = Self::DEFAULT_MAX_AGENTS;
        let include_mmry_port = self.config.runtime_mode == RuntimeMode::Container
            && self.config.mmry_enabled
            && !self.config.single_user;
        let ports_per_session = if include_mmry_port {
            4 + max_agents
        } else {
            3 + max_agents
        };
        let search_start = self.config.base_port + (attempt as i64 * ports_per_session);
        let base_port = self
            .repo
            .find_free_port_range_with_agents(search_start, max_agents)
            .await?;
        let agent_port = base_port;
        let fileserver_port = base_port + 1;
        let ttyd_port = base_port + 2;
        // mmry port is only allocated per-session for container mode.
        // For local multi-user mode, mmry_port is assigned at session start from the per-user manager.
        let (mmry_port, agent_base_port) = if include_mmry_port {
            (Some(base_port + 3), Some(base_port + 4))
        } else {
            (None, Some(base_port + 3))
        };

        let (eavs_key_id, eavs_key_hash, eavs_virtual_key) = if self.eavs.is_some() {
            match self.create_eavs_key(&session_id).await {
                Ok((key_id, key_hash, key_value)) => {
                    info!("Created EAVS key {} for session {}", key_id, session_id);
                    (Some(key_id), Some(key_hash), Some(key_value))
                }
                Err(e) => {
                    warn!(
                        "Failed to create EAVS key for session {}: {:?}",
                        session_id, e
                    );
                    (None, None, None)
                }
            }
        } else {
            (None, None, None)
        };

        let now = Utc::now().to_rfc3339();
        let session = Session {
            id: session_id.clone(),
            readable_id: Some(wordlist::readable_id_from_session_id(&session_id)),
            container_id: None,
            container_name: container_name.clone(),
            user_id: user_id.to_string(),
            workspace_path: user_home_path.to_string(),
            agent: agent.map(ToString::to_string),
            image: image.to_string(),
            image_digest: image_digest.map(ToString::to_string),
            agent_port,
            fileserver_port,
            ttyd_port,
            eavs_port: None,
            agent_base_port,
            max_agents: Some(max_agents),
            eavs_key_id,
            eavs_key_hash,
            eavs_virtual_key: None,
            mmry_port,
            status: SessionStatus::Pending,
            runtime_mode: self.config.runtime_mode,
            created_at: now.clone(),
            started_at: None,
            stopped_at: None,
            last_activity_at: Some(now), // Initialize with creation time
            error_message: None,
        };

        // Persist the session. This will fail with a unique constraint violation if another
        // session grabbed these ports/readable_id between our check and insert.
        self.repo.create(&session).await?;

        info!(
            "Created session {} with ports {}/{}/{}",
            session_id, agent_port, fileserver_port, ttyd_port
        );

        // Start the container synchronously so callers can reliably know whether startup succeeded.
        if let Err(e) = self
            .start_container(&session, eavs_virtual_key.as_deref())
            .await
        {
            error!(
                "Failed to start container for session {}: {:?}",
                session.id, e
            );
            let _ = self.repo.mark_failed(&session.id, &e.to_string()).await;

            // Best-effort cleanup: revoke EAVS key if we created one.
            if let (Some(eavs), Some(key_id)) = (&self.eavs, &session.eavs_key_id)
                && let Err(revoke_err) = eavs.revoke_key(key_id).await
            {
                warn!(
                    "Failed to revoke EAVS key {} after startup failure: {:?}",
                    key_id, revoke_err
                );
            }

            return Err(e);
        }

        Ok(self.repo.get(&session.id).await?.unwrap_or(session))
    }

    /// Create an EAVS virtual key for a session.
    async fn create_eavs_key(&self, session_id: &str) -> Result<(String, String, String)> {
        let eavs = self.eavs.as_ref().context("EAVS client not configured")?;

        // Build permissions based on config
        let mut permissions = KeyPermissions::default();
        if let Some(budget) = self.config.default_session_budget_usd {
            permissions.max_budget_usd = Some(budget);
        }
        if let Some(rpm) = self.config.default_session_rpm {
            permissions.rpm_limit = Some(rpm);
        }

        let request = CreateKeyRequest::new(format!("session-{}", &session_id[..8]))
            .permissions(permissions)
            .metadata(serde_json::json!({
                "session_id": session_id,
                "created_by": "oqto"
            }));

        let response = eavs.create_key(request).await?;

        Ok((response.key_id, response.key_hash, response.key))
    }

    /// Start services for the given session.
    ///
    /// For container mode: creates and starts a Docker/Podman container.
    /// For local mode: spawns native processes for fileserver, and ttyd.
    async fn start_container(
        &self,
        session: &Session,
        eavs_virtual_key: Option<&str>,
    ) -> Result<()> {
        debug!(
            "Starting session {} in {:?} mode",
            session.id, session.runtime_mode
        );

        let result = match session.runtime_mode {
            RuntimeMode::Container => self.start_container_mode(session, eavs_virtual_key).await,
            RuntimeMode::Local => self.start_local_mode(session, eavs_virtual_key).await,
        };

        if result.is_ok() {
            self.start_agent_browser_daemon(session).await;
        }

        result
    }

    async fn start_agent_browser_daemon(&self, session: &Session) {
        let browser_session_id = browser_session_name(&session.id);
        if let Err(err) = self.agent_browser.ensure_session(&browser_session_id).await {
            warn!(
                "Failed to start agent-browser daemon for session {} (browser session {}): {}",
                session.id, browser_session_id, err
            );
        }
    }

    async fn stop_agent_browser_daemon(&self, session_id: &str) {
        let browser_session_id = browser_session_name(session_id);
        if let Err(err) = self.agent_browser.stop_session(&browser_session_id).await {
            warn!(
                "Failed to stop agent-browser daemon for session {} (browser session {}): {}",
                session_id, browser_session_id, err
            );
        }
    }

    /// Internal port base for sub-agents inside the container.
    const INTERNAL_AGENT_BASE_PORT: u16 = 4001;

    /// Start a container for the given session (container mode).
    async fn start_container_mode(
        &self,
        session: &Session,
        eavs_virtual_key: Option<&str>,
    ) -> Result<()> {
        let runtime = self
            .container_runtime()
            .context("container runtime not available")?;

        // Build container config
        // Mount the full user home directory so dotfiles and tool state persist across restarts.
        let mut config = ContainerConfig::new(&session.image)
            .name(&session.container_name)
            .hostname(&session.container_name)
            .port(session.agent_port as u16, 41820)
            .port(session.fileserver_port as u16, 41821)
            .port(session.ttyd_port as u16, 41822)
            .volume(&session.workspace_path, "/home/dev")
            .env("OPENCODE_PORT", "41820")
            .env("FILESERVER_PORT", "41821")
            .env("TTYD_PORT", "41822");

        // Map sub-agent ports if configured
        // Each sub-agent gets a port: external (agent_base_port + i) -> internal (4001 + i)
        if let (Some(agent_base), Some(max_agents)) = (session.agent_base_port, session.max_agents)
        {
            for i in 0..max_agents {
                let external_port = (agent_base + i) as u16;
                let internal_port = Self::INTERNAL_AGENT_BASE_PORT + i as u16;
                config = config.port(external_port, internal_port);
            }
            // Pass agent port config to container via env vars
            config = config
                .env(
                    "AGENT_BASE_PORT",
                    Self::INTERNAL_AGENT_BASE_PORT.to_string(),
                )
                .env("MAX_AGENTS", max_agents.to_string());

            info!(
                "Mapped {} agent ports: external {}..{} -> internal {}..{}",
                max_agents,
                agent_base,
                agent_base + max_agents - 1,
                Self::INTERNAL_AGENT_BASE_PORT,
                Self::INTERNAL_AGENT_BASE_PORT + max_agents as u16 - 1
            );
        }

        // Pass EAVS URL and virtual key to container if available
        if let Some(ref eavs_url) = self.config.eavs_container_url {
            config = config.env("EAVS_URL", eavs_url);
        }
        if let Some(virtual_key) = eavs_virtual_key {
            config = config.env("EAVS_VIRTUAL_KEY", virtual_key);
        }

        // Pass mmry config to container if enabled
        if self.config.mmry_enabled {
            // Internal port is fixed at 41823 (set in Dockerfile)
            config = config.env("MMRY_PORT", "41823");
            if let Some(ref mmry_url) = self.config.mmry_container_url {
                config = config.env("MMRY_HOST_URL", mmry_url);
            }
            // Map mmry port if allocated (multi-user mode)
            if let Some(mmry_port) = session.mmry_port {
                config = config.port(mmry_port as u16, 41823);
                info!("Mapped mmry port: external {} -> internal 41823", mmry_port);
            }
        }

        // Pass pi-bridge config to container if enabled
        // pi-bridge runs inside the container and provides HTTP/WS access to Pi
        if self.config.pi_bridge_enabled {
            config = config
                .env("PI_BRIDGE_ENABLED", "true")
                .env("PI_BRIDGE_PORT", "41824");
            // Map pi-bridge port (internal 41824 -> same external port for simplicity)
            // The backend's ContainerPiRuntime will connect to localhost:41824 via the mapped port
            config = config.port(41824, 41824);
            if let Some(ref provider) = self.config.pi_provider {
                config = config.env("PI_PROVIDER", provider);
            }
            if let Some(ref model) = self.config.pi_model {
                config = config.env("PI_MODEL", model);
            }
            info!("Enabled pi-bridge for session {} on port 41824", session.id);
        }

        // Create and start the container
        let container_id = runtime
            .create_container(&config)
            .await
            .context("creating container")?;

        info!(
            "Started container {} for session {}",
            container_id, session.id
        );

        // Update session with container ID
        self.repo
            .set_container_id(&session.id, &container_id)
            .await?;

        // Wait for core services to become reachable before marking the session running.
        // This avoids clients receiving 502s due to fixed-delay startup races.
        if let Err(e) = self
            .readiness
            .wait_for_session_services(session.fileserver_port as u16, session.ttyd_port as u16)
            .await
        {
            // Best-effort cleanup: stop/remove the container, then surface the error.
            if let Err(stop_err) = runtime.stop_container(&container_id, Some(10)).await {
                warn!(
                    "Failed to stop container {} after readiness failure: {:?}",
                    container_id, stop_err
                );
            }
            if let Err(rm_err) = runtime.remove_container(&container_id, true).await {
                warn!(
                    "Failed to remove container {} after readiness failure: {:?}",
                    container_id, rm_err
                );
            }
            return Err(e);
        }

        // Mark as running
        self.repo.mark_running(&session.id).await?;

        Ok(())
    }

    /// Start local processes for the given session (local mode).
    ///
    /// All process spawning goes through the runner daemon.
    /// In multi-user mode, connects to the user's per-user runner socket.
    async fn start_local_mode(
        &self,
        session: &Session,
        eavs_virtual_key: Option<&str>,
    ) -> Result<()> {
        let runner = self.runner_for_user(&session.user_id)?;

        let agent_port = session.agent_port as u16;
        let fileserver_port = session.fileserver_port as u16;
        let ttyd_port = session.ttyd_port as u16;

        // Ensure per-user mmry is running (local multi-user mode).
        //
        // This is best-effort: if mmry fails to start, we still want fileserver/ttyd
        // to be usable. The mmry proxy will return 503 for memory endpoints until mmry is up.
        if self.config.mmry_enabled && !self.config.single_user {
            if let Some(ref user_mmry) = self.user_mmry {
                match user_mmry.ensure_user_mmry(&session.user_id).await {
                    Ok(mmry_port) => {
                        let _ = self
                            .repo
                            .set_mmry_port(&session.id, Some(mmry_port as i64))
                            .await;
                    }
                    Err(err) => {
                        warn!(
                            "Failed to ensure per-user mmry for user {}: {:?}",
                            session.user_id, err
                        );
                        let _ = self.repo.set_mmry_port(&session.id, None).await;
                    }
                }
            } else {
                warn!(
                    "mmry enabled in local multi-user mode but UserMmryManager is not configured"
                );
            }
        }

        // Build environment variables for the processes
        let mut env = std::collections::HashMap::new();
        if let Some(ref eavs_url) = self.config.eavs_container_url {
            env.insert("EAVS_URL".to_string(), eavs_url.clone());
        }
        if let Some(virtual_key) = eavs_virtual_key {
            env.insert("EAVS_VIRTUAL_KEY".to_string(), virtual_key.to_string());
            // Also set API keys for agent
            env.insert("ANTHROPIC_API_KEY".to_string(), virtual_key.to_string());
            env.insert("OPENAI_API_KEY".to_string(), virtual_key.to_string());
        } else {
            // Load per-user eavs.env if it exists (provisioned by admin API or oqtoctl --eavs).
            // This provides EAVS_API_KEY for Pi's models.json provider config.
            //
            // In single-user mode, read from $HOME. In multi-user mode, derive the
            // user's home from workspace_path (which is their linux home).
            let user_home = if self.config.single_user {
                std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
            } else {
                // workspace_path is typically /home/oqto_{user_id}
                session.workspace_path.clone()
            };
            let eavs_env_path = format!("{}/.config/oqto/eavs.env", user_home);
            if let Ok(contents) = std::fs::read_to_string(&eavs_env_path) {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((key, value)) = line.split_once('=') {
                        env.insert(key.trim().to_string(), value.trim().to_string());
                    }
                }
                debug!("Loaded eavs env from {}", eavs_env_path);
            }
        }

        // Enforce skdlr wrapper in Oqto sandboxed runs
        let skdlr_config_path = std::path::Path::new("/etc/oqto/skdlr-agent.toml");
        if skdlr_config_path.exists() {
            env.insert("SKDLR_OQTO_MODE".to_string(), "1".to_string());
            env.insert(
                "SKDLR_CONFIG".to_string(),
                skdlr_config_path.display().to_string(),
            );
        } else {
            warn!(
                "skdlr agent config not found at {}, scheduling will not be sandboxed",
                skdlr_config_path.display()
            );
        }

        // Agent-browser: tell the oqto-browser CLI where to find daemon sockets
        // and prevent it from auto-starting its own daemon (the backend manages that).
        if self.config.agent_browser.enabled {
            use crate::agent_browser::agent_browser_base_dir;
            env.insert(
                "AGENT_BROWSER_SOCKET_DIR_BASE".to_string(),
                agent_browser_base_dir().display().to_string(),
            );
            env.insert("OQTO_BROWSER_NO_AUTO_START".to_string(), "true".to_string());
        }

        let workspace_path = PathBuf::from(&session.workspace_path);

        info!(
            "Starting session {} via runner for user {}",
            session.id, session.user_id
        );

        // Start services via runner
        let response = runner
            .start_session(
                &session.id,
                &workspace_path,
                agent_port,
                fileserver_port,
                ttyd_port,
                session.agent.clone(),
                env,
            )
            .await
            .context("starting session via runner")?;

        info!(
            "Started local services for session {} with PIDs: {}",
            session.id, response.pids
        );

        // Update session with PIDs (stored as container_id for compatibility)
        self.repo
            .set_container_id(&session.id, &response.pids)
            .await?;

        // Wait for core services to become reachable
        if let Err(e) = self
            .readiness
            .wait_for_session_services(fileserver_port, ttyd_port)
            .await
        {
            // Best-effort cleanup: stop the session via runner
            if let Err(stop_err) = runner.stop_session(&session.id).await {
                warn!(
                    "Failed to stop runner session after readiness failure: {:?}",
                    stop_err
                );
            }
            return Err(e);
        }

        // Mark as running
        self.repo.mark_running(&session.id).await?;

        Ok(())
    }

    /// Stop a session and its services.
    ///
    /// For container mode: stops the container but does NOT remove it.
    /// For local mode: kills the processes.
    /// The session can be restarted later with `resume_session()`.
    /// To fully remove the session, use `delete_session()`.
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        let session = self
            .repo
            .get(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

        if session.is_terminal() {
            warn!(
                "Session {} is already in terminal state: {:?}",
                session_id, session.status
            );
            return Ok(());
        }

        info!(
            "Stopping session {} ({:?} mode)",
            session_id, session.runtime_mode
        );
        self.repo
            .update_status(session_id, SessionStatus::Stopping)
            .await?;

        // Note: We do NOT revoke EAVS key on stop anymore - only on delete.
        // This allows the session to be resumed without needing a new key.

        match session.runtime_mode {
            RuntimeMode::Container => {
                // Stop the container if it exists (but do NOT remove it)
                if let Some(ref container_id) = session.container_id
                    && let Some(runtime) = self.container_runtime()
                    && let Err(e) = runtime.stop_container(container_id, Some(10)).await
                {
                    warn!("Failed to stop container {}: {:?}", container_id, e);
                }
                // Container is NOT removed - it can be restarted with resume_session()
            }
            RuntimeMode::Local => {
                // Stop the local processes via runner (per-user in multi-user mode)
                match self.runner_for_user(&session.user_id) {
                    Ok(runner) => {
                        if let Err(e) = runner.stop_session(session_id).await {
                            warn!("Failed to stop local processes for {}: {:?}", session_id, e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get runner for user {}: {:?}", session.user_id, e);
                    }
                }

                // Release per-user mmry after stopping session processes.
                if self.config.mmry_enabled
                    && !self.config.single_user
                    && let Some(ref user_mmry) = self.user_mmry
                    && let Err(e) = user_mmry.release_user_mmry(&session.user_id).await
                {
                    warn!(
                        "Failed to release per-user mmry for user {}: {:?}",
                        session.user_id, e
                    );
                }
            }
        }

        self.stop_agent_browser_daemon(session_id).await;

        self.repo.mark_stopped(session_id).await?;
        info!("Session {} stopped (preserved for resume)", session_id);

        Ok(())
    }

    /// Resume a stopped session by restarting its services.
    ///
    /// For container mode: restarts the stopped container.
    /// For local mode: respawns the processes (workspace data is preserved).
    pub async fn resume_session(&self, session_id: &str) -> Result<Session> {
        self.resume_session_with_readiness(session_id).await
    }

    pub async fn resume_session_for_io(&self, session_id: &str) -> Result<Session> {
        self.resume_session_with_readiness(session_id).await
    }

    async fn resume_session_with_readiness(&self, session_id: &str) -> Result<Session> {
        let mut session = self
            .repo
            .get(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

        // Can only resume stopped sessions
        if session.status != SessionStatus::Stopped {
            anyhow::bail!(
                "cannot resume session in state {:?}, must be stopped",
                session.status
            );
        }

        // Check if image has been updated - if so, upgrade instead of resume (container mode only)
        if session.runtime_mode == RuntimeMode::Container
            && let Ok(Some(new_digest)) = self.check_for_image_update(session_id).await
        {
            info!(
                "Image update detected for session {} (new digest: {}), upgrading instead of resuming",
                session_id, new_digest
            );
            return self.upgrade_session(session_id).await;
        }

        info!(
            "Resuming session {} ({:?} mode)",
            session_id, session.runtime_mode
        );

        // Mark as starting (reassign ports first if local resume collides with active ports)
        if let Err(err) = self
            .repo
            .update_status(session_id, SessionStatus::Starting)
            .await
        {
            if session.runtime_mode == RuntimeMode::Local
                && Self::is_retryable_unique_violation(&err)
            {
                self.reassign_ports_for_resume(&mut session).await?;
                self.repo
                    .update_status(session_id, SessionStatus::Starting)
                    .await?;
            } else {
                return Err(err);
            }
        }

        // Wrap the resume logic to ensure we mark as failed on error
        let result = self.resume_session_inner(&mut session, session_id).await;

        if let Err(ref e) = result {
            error!("Failed to resume session {}: {:?}", session_id, e);
            let _ = self
                .repo
                .mark_failed(session_id, &format!("resume failed: {}", e))
                .await;
        }

        result
    }

    async fn reassign_ports_for_resume(&self, session: &mut Session) -> Result<()> {
        let max_agents = session.max_agents.unwrap_or(Self::DEFAULT_MAX_AGENTS);
        let base_port = self
            .repo
            .find_free_port_range_with_agents(self.config.base_port, max_agents)
            .await?;

        // Container mode uses a per-session mmry port (base+3) when enabled.
        // Local mode uses a per-user mmry port that must NOT be reassigned here.
        let include_mmry_port = session.runtime_mode == RuntimeMode::Container
            && self.config.mmry_enabled
            && !self.config.single_user
            && session.mmry_port.is_some();

        let new_mmry_port = if include_mmry_port {
            Some(base_port + 3)
        } else {
            session.mmry_port
        };
        let new_agent_base_port = session
            .agent_base_port
            .map(|_| base_port + if include_mmry_port { 4 } else { 3 });

        self.repo
            .update_ports(
                &session.id,
                base_port,
                base_port + 1,
                base_port + 2,
                new_mmry_port,
                new_agent_base_port,
            )
            .await?;

        session.agent_port = base_port;
        session.fileserver_port = base_port + 1;
        session.ttyd_port = base_port + 2;
        session.mmry_port = new_mmry_port;
        session.agent_base_port = new_agent_base_port;

        info!(
            "Reassigned ports for session {}: {}/{}/{}",
            session.id, session.agent_port, session.fileserver_port, session.ttyd_port
        );

        Ok(())
    }

    /// Inner resume logic - separated to allow proper error handling in the caller.
    async fn resume_session_inner(
        &self,
        session: &mut Session,
        session_id: &str,
    ) -> Result<Session> {
        match session.runtime_mode {
            RuntimeMode::Container => {
                let container_id = session
                    .container_id
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("session has no container to resume"))?;

                let runtime = self
                    .container_runtime()
                    .context("container runtime not available")?;

                // Start the existing container
                if let Err(e) = runtime.start_container(container_id).await {
                    error!(
                        "Failed to start container {} for session {}: {:?}",
                        container_id, session_id, e
                    );
                    self.repo
                        .mark_failed(session_id, &format!("resume failed: {}", e))
                        .await?;
                    return Ok(self.repo.get(session_id).await?.unwrap_or(session.clone()));
                }

                // Wait for services to become ready
                if let Err(e) = self
                    .readiness
                    .wait_for_session_services(
                        session.fileserver_port as u16,
                        session.ttyd_port as u16,
                    )
                    .await
                {
                    error!(
                        "Services not ready after resume for session {}: {:?}",
                        session_id, e
                    );
                    // Stop the container again since services didn't come up
                    let _ = runtime.stop_container(container_id, Some(5)).await;
                    self.repo
                        .mark_failed(
                            session_id,
                            &format!("services not ready after resume: {}", e),
                        )
                        .await?;
                    return Ok(self.repo.get(session_id).await?.unwrap_or(session.clone()));
                }
            }
            RuntimeMode::Local => {
                let runner = self.runner_for_user(&session.user_id)?;

                // Use local_runtime for port utilities only
                let local_runtime = self
                    .local_runtime()
                    .context("local runtime not available")?;

                let mut agent_port = session.agent_port as u16;
                let mut fileserver_port = session.fileserver_port as u16;
                let mut ttyd_port = session.ttyd_port as u16;

                if !local_runtime.check_ports_available(agent_port, fileserver_port, ttyd_port) {
                    warn!(
                        "Ports {}/{}/{} are in use for session {}, attempting cleanup...",
                        agent_port, fileserver_port, ttyd_port, session_id
                    );
                    let cleared =
                        local_runtime.clear_ports(&[agent_port, fileserver_port, ttyd_port]);
                    if local_runtime.check_ports_available(agent_port, fileserver_port, ttyd_port) {
                        info!(
                            "Cleared {} orphan process(es), ports now available for session {}",
                            cleared, session_id
                        );
                    } else {
                        let max_agents = session.max_agents.unwrap_or(Self::DEFAULT_MAX_AGENTS);
                        let base_port = self
                            .repo
                            .find_free_port_range_with_agents(self.config.base_port, max_agents)
                            .await?;
                        let new_agent_port = base_port as u16;
                        let new_fileserver_port = (base_port + 1) as u16;
                        let new_ttyd_port = (base_port + 2) as u16;
                        let new_mmry_port = session.mmry_port.map(|_| base_port + 3);
                        let new_agent_base_port = session.agent_base_port.map(|_| base_port + 4);

                        if !local_runtime.check_ports_available(
                            new_agent_port,
                            new_fileserver_port,
                            new_ttyd_port,
                        ) {
                            let cleared_new = local_runtime.clear_ports(&[
                                new_agent_port,
                                new_fileserver_port,
                                new_ttyd_port,
                            ]);
                            if !local_runtime.check_ports_available(
                                new_agent_port,
                                new_fileserver_port,
                                new_ttyd_port,
                            ) {
                                anyhow::bail!(
                                    "Ports {}/{}/{} are still in use after cleanup (cleared {} processes) for session {}",
                                    new_agent_port,
                                    new_fileserver_port,
                                    new_ttyd_port,
                                    cleared_new,
                                    session_id
                                );
                            }
                            info!(
                                "Cleared {} orphan process(es), new ports now available for session {}",
                                cleared_new, session_id
                            );
                        }

                        self.repo
                            .update_ports(
                                session_id,
                                base_port,
                                base_port + 1,
                                base_port + 2,
                                new_mmry_port,
                                new_agent_base_port,
                            )
                            .await?;

                        info!(
                            "Reassigned ports for session {}: {}/{}/{} -> {}/{}/{}",
                            session_id,
                            agent_port,
                            fileserver_port,
                            ttyd_port,
                            new_agent_port,
                            new_fileserver_port,
                            new_ttyd_port
                        );

                        session.agent_port = base_port;
                        session.fileserver_port = base_port + 1;
                        session.ttyd_port = base_port + 2;
                        session.mmry_port = new_mmry_port;
                        session.agent_base_port = new_agent_base_port;
                        agent_port = new_agent_port;
                        fileserver_port = new_fileserver_port;
                        ttyd_port = new_ttyd_port;
                    }
                }

                let mut eavs_virtual_key = None;
                if let Some(eavs) = self.eavs.as_ref() {
                    if let Some(ref key_id) = session.eavs_key_id
                        && let Err(e) = eavs.revoke_key(key_id).await
                    {
                        warn!(
                            "Failed to revoke previous EAVS key {} for session {}: {:?}",
                            key_id, session_id, e
                        );
                    }

                    match self.create_eavs_key(session_id).await {
                        Ok((key_id, key_hash, key_value)) => {
                            info!(
                                "Created new EAVS key {} for resumed local session {}",
                                key_id, session_id
                            );
                            eavs_virtual_key = Some(key_value);
                            session.eavs_key_id = Some(key_id);
                            session.eavs_key_hash = Some(key_hash);
                            self.repo
                                .update_eavs_keys(
                                    session_id,
                                    session.eavs_key_id.as_deref(),
                                    session.eavs_key_hash.as_deref(),
                                )
                                .await?;
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create EAVS key for resumed session {}: {:?}",
                                session_id, e
                            );
                        }
                    }
                }

                // Build environment variables
                let mut env = std::collections::HashMap::new();
                if let Some(ref eavs_url) = self.config.eavs_container_url {
                    env.insert("EAVS_URL".to_string(), eavs_url.clone());
                }
                if let Some(ref virtual_key) = eavs_virtual_key {
                    env.insert("EAVS_VIRTUAL_KEY".to_string(), virtual_key.clone());
                    env.insert("ANTHROPIC_API_KEY".to_string(), virtual_key.clone());
                    env.insert("OPENAI_API_KEY".to_string(), virtual_key.clone());
                }

                let workspace_path = PathBuf::from(&session.workspace_path);

                // Stop any stale session state in the runner (ignore errors - session may not exist)
                let _ = runner.stop_session(session_id).await;

                // Respawn the processes via runner
                match runner
                    .start_session(
                        session_id,
                        &workspace_path,
                        agent_port,
                        fileserver_port,
                        ttyd_port,
                        session.agent.clone(),
                        env,
                    )
                    .await
                {
                    Ok(response) => {
                        // Update with new PIDs
                        self.repo
                            .set_container_id(session_id, &response.pids)
                            .await?;
                    }
                    Err(e) => {
                        error!(
                            "Failed to resume local services for session {}: {:?}",
                            session_id, e
                        );
                        self.repo
                            .mark_failed(session_id, &format!("resume failed: {}", e))
                            .await?;
                        return Ok(self.repo.get(session_id).await?.unwrap_or(session.clone()));
                    }
                }

                // Wait for services to become ready
                if let Err(e) = self
                    .readiness
                    .wait_for_session_services(
                        session.fileserver_port as u16,
                        session.ttyd_port as u16,
                    )
                    .await
                {
                    error!(
                        "Services not ready after resume for session {}: {:?}",
                        session_id, e
                    );
                    let _ = runner.stop_session(session_id).await;
                    self.repo
                        .mark_failed(
                            session_id,
                            &format!("services not ready after resume: {}", e),
                        )
                        .await?;
                    return Ok(self.repo.get(session_id).await?.unwrap_or(session.clone()));
                }
            }
        }

        // Mark as running
        self.repo.mark_running(session_id).await?;
        self.start_agent_browser_daemon(session).await;
        info!("Session {} resumed successfully", session_id);

        Ok(self.repo.get(session_id).await?.unwrap_or(session.clone()))
    }

    /// Get a session by ID.
    pub async fn get_session(&self, session_id: &str) -> Result<Option<Session>> {
        let session = self.repo.get(session_id).await?;
        match session {
            Some(session) => Ok(Some(self.reconcile_session_container_state(session).await?)),
            None => Ok(None),
        }
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        let sessions = self.repo.list().await?;
        let mut reconciled = Vec::with_capacity(sessions.len());

        for session in sessions {
            reconciled.push(self.reconcile_session_container_state(session).await?);
        }

        Ok(reconciled)
    }

    /// Collect container stats for all container-mode sessions.
    /// Returns an empty report if no container runtime is configured (local mode).
    pub async fn collect_container_stats(&self) -> Result<ContainerStatsReport> {
        let Some(runtime) = self.container_runtime() else {
            // No container runtime in local mode - nothing to collect
            return Ok(ContainerStatsReport {
                stats: Vec::new(),
                errors: Vec::new(),
            });
        };
        let sessions = self.repo.list().await?;
        let mut stats = Vec::new();
        let mut errors = Vec::new();

        for session in sessions {
            if session.runtime_mode != RuntimeMode::Container {
                continue;
            }

            let Some(container_id) = session.container_id.clone() else {
                continue;
            };

            match runtime.get_stats(&container_id).await {
                Ok(container_stats) => stats.push(SessionContainerStats {
                    session_id: session.id,
                    container_id,
                    container_name: session.container_name,
                    stats: container_stats,
                }),
                Err(err) => errors.push(format!(
                    "stats for session {} (container {}): {}",
                    session.id, container_id, err
                )),
            }
        }

        Ok(ContainerStatsReport { stats, errors })
    }

    /// List active sessions.
    #[allow(dead_code)]
    pub async fn list_active_sessions(&self) -> Result<Vec<Session>> {
        self.repo.list_active().await
    }

    /// Delete a session and remove its services.
    ///
    /// This fully removes the container/processes and revokes any EAVS keys.
    /// The session must be stopped first (use `stop_session()`).
    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let session = self
            .repo
            .get(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

        if session.is_active() {
            anyhow::bail!("cannot delete active session, stop it first");
        }

        // Revoke EAVS key if it exists (moved from stop_session)
        if let (Some(eavs), Some(key_id)) = (&self.eavs, &session.eavs_key_id) {
            match eavs.revoke_key(key_id).await {
                Ok(()) => info!("Revoked EAVS key {} for session {}", key_id, session_id),
                Err(e) => warn!(
                    "Failed to revoke EAVS key {} for session {}: {:?}",
                    key_id, session_id, e
                ),
            }
        }

        match session.runtime_mode {
            RuntimeMode::Container => {
                // Remove the container if it exists
                if let Some(ref container_id) = session.container_id
                    && let Some(runtime) = self.container_runtime()
                {
                    // Try to stop first (in case it's somehow still running)
                    let _ = runtime.stop_container(container_id, Some(5)).await;

                    // Remove the container
                    if let Err(e) = runtime.remove_container(container_id, true).await {
                        warn!(
                            "Failed to remove container {} for session {}: {:?}",
                            container_id, session_id, e
                        );
                        // Continue with deletion even if container removal fails
                    }
                }
            }
            RuntimeMode::Local => {
                // Stop any remaining processes via runner (should already be stopped)
                match self.runner_for_user(&session.user_id) {
                    Ok(runner) => {
                        let _ = runner.stop_session(session_id).await;
                    }
                    Err(e) => {
                        warn!(
                            "Failed to get runner for user {} during delete: {:?}",
                            session.user_id, e
                        );
                    }
                }
            }
        }

        self.stop_agent_browser_daemon(session_id).await;

        self.repo.delete(session_id).await?;
        info!("Deleted session {}", session_id);

        Ok(())
    }

    /// Cleanup stale sessions (containers/processes that no longer exist).
    #[allow(dead_code)]
    pub async fn cleanup_stale_sessions(&self) -> Result<usize> {
        let active = self.repo.list_active().await?;
        let mut cleaned = 0;

        for session in active {
            let is_running = match session.runtime_mode {
                RuntimeMode::Container => {
                    if let Some(ref container_id) = session.container_id {
                        if let Some(runtime) = self.container_runtime() {
                            matches!(runtime.container_state_status(container_id).await, Ok(Some(status)) if status == "running")
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                RuntimeMode::Local => {
                    if let Some(local_runtime) = self.local_runtime() {
                        self.is_local_session_running_best_effort(local_runtime.as_ref(), &session)
                            .await
                    } else {
                        false
                    }
                }
            };

            if !is_running {
                warn!(
                    "Session {} is no longer running, marking as stopped",
                    session.id
                );
                self.repo.mark_stopped(&session.id).await?;
                cleaned += 1;
            }
        }

        Ok(cleaned)
    }

    /// Check if a newer image is available for a session.
    ///
    /// Returns `Ok(Some(new_digest))` if a newer image is available,
    /// `Ok(None)` if the session is up to date or we can't determine.
    /// Always returns `Ok(None)` for local mode sessions (no image updates).
    pub async fn check_for_image_update(&self, session_id: &str) -> Result<Option<String>> {
        let session = self
            .repo
            .get(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

        // Image updates only apply to container mode
        if session.runtime_mode == RuntimeMode::Local {
            return Ok(None);
        }

        let runtime = match self.container_runtime() {
            Some(r) => r,
            None => return Ok(None),
        };

        // Get current image digest
        let current_digest = match runtime.get_image_digest(&session.image).await {
            Ok(Some(digest)) => digest,
            Ok(None) => {
                debug!("No digest available for image {}", session.image);
                return Ok(None);
            }
            Err(e) => {
                warn!("Failed to get image digest for {}: {:?}", session.image, e);
                return Ok(None);
            }
        };

        // Compare with session's stored digest
        match &session.image_digest {
            Some(stored_digest) if stored_digest == &current_digest => {
                debug!(
                    "Session {} is up to date (digest: {})",
                    session_id, current_digest
                );
                Ok(None)
            }
            Some(stored_digest) => {
                info!(
                    "Session {} has outdated image: stored={}, current={}",
                    session_id, stored_digest, current_digest
                );
                Ok(Some(current_digest))
            }
            None => {
                // No stored digest - could be a legacy session, update it
                debug!(
                    "Session {} has no stored digest, updating to {}",
                    session_id, current_digest
                );
                self.repo
                    .update_image_digest(session_id, &current_digest)
                    .await?;
                Ok(None)
            }
        }
    }

    /// Upgrade a session's container to the latest image version.
    ///
    /// This will:
    /// 1. Stop and remove the existing container (if running)
    /// 2. Update the session's image digest
    /// 3. Create and start a new container with the same ports and volumes
    ///
    /// The session's workspace_path (user data) is preserved.
    /// Note: This is only applicable to container mode sessions.
    pub async fn upgrade_session(&self, session_id: &str) -> Result<Session> {
        let session = self
            .repo
            .get(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

        // Upgrade only applies to container mode
        if session.runtime_mode == RuntimeMode::Local {
            anyhow::bail!("upgrade is not supported for local mode sessions");
        }

        let runtime = self
            .container_runtime()
            .context("container runtime not available")?;

        info!(
            "Upgrading session {} from image {}",
            session_id, session.image
        );

        // Get the new image digest before we start
        let new_digest = runtime.get_image_digest(&session.image).await?;

        // Stop and remove the existing container if it exists
        if let Some(ref container_id) = session.container_id {
            info!("Stopping existing container {} for upgrade", container_id);

            // Try to stop gracefully first
            if let Err(e) = runtime.stop_container(container_id, Some(10)).await {
                warn!(
                    "Failed to stop container {} (may already be stopped): {:?}",
                    container_id, e
                );
            }

            // Remove the container
            if let Err(e) = runtime.remove_container(container_id, true).await {
                warn!("Failed to remove container {}: {:?}", container_id, e);
            }
        }

        // Update the session record
        self.repo.clear_container_id(session_id).await?;
        self.repo
            .update_image_and_digest(session_id, &session.image, new_digest.as_deref())
            .await?;
        self.repo
            .update_status(session_id, SessionStatus::Pending)
            .await?;

        // Refresh session from DB
        let mut session = self
            .repo
            .get(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found after update: {}", session_id))?;

        // Start a new container
        info!(
            "Starting new container for session {} with image {}",
            session_id, session.image
        );

        // Start the container (this will update session status)
        if let Err(e) = self.start_container(&session, None).await {
            error!(
                "Failed to start upgraded container for session {}: {:?}",
                session_id, e
            );
            self.repo.mark_failed(session_id, &e.to_string()).await?;
            session = self.repo.get(session_id).await?.unwrap_or(session);
            return Ok(session);
        }

        // Refresh and return the updated session
        let updated_session =
            self.repo.get(session_id).await?.ok_or_else(|| {
                anyhow::anyhow!("session not found after upgrade: {}", session_id)
            })?;

        info!(
            "Session {} upgraded successfully, new digest: {:?}",
            session_id, updated_session.image_digest
        );

        Ok(updated_session)
    }

    async fn reconcile_session_container_state(&self, session: Session) -> Result<Session> {
        if !session.is_active() {
            return Ok(session);
        }

        let Some(container_id) = session.container_id.clone() else {
            return Ok(session);
        };

        match session.runtime_mode {
            RuntimeMode::Container => {
                self.reconcile_container_mode_state(session, &container_id)
                    .await
            }
            RuntimeMode::Local => self.reconcile_local_mode_state(session).await,
        }
    }

    /// Reconcile container mode session state.
    async fn reconcile_container_mode_state(
        &self,
        session: Session,
        container_id: &str,
    ) -> Result<Session> {
        let runtime = match self.container_runtime() {
            Some(r) => r,
            None => return Ok(session),
        };

        match runtime.container_state_status(container_id).await {
            Ok(Some(status)) if status == "running" => Ok(session),
            Ok(Some(status)) if status == "created" || status == "restarting" => {
                if matches!(session.status, SessionStatus::Running) {
                    self.repo
                        .update_status(&session.id, SessionStatus::Starting)
                        .await?;
                    return Ok(self.repo.get(&session.id).await?.unwrap_or(session));
                }
                Ok(session)
            }
            // Container is stopped/exited - attempt to restart it
            Ok(Some(status)) if status == "exited" || status == "stopped" || status == "dead" => {
                info!(
                    "Container {} for session {} is {} - attempting restart",
                    container_id, session.id, status
                );

                // Mark session as starting while we restart
                self.repo
                    .update_status(&session.id, SessionStatus::Starting)
                    .await?;

                // Spawn the restart in the background to avoid blocking the request
                let service = self.clone();
                let session_id = session.id.clone();
                let container_id_owned = container_id.to_string();
                let _agent_port = session.agent_port as u16;
                let fileserver_port = session.fileserver_port as u16;
                let ttyd_port = session.ttyd_port as u16;

                tokio::spawn(async move {
                    let runtime = match service.container_runtime() {
                        Some(r) => r,
                        None => {
                            let _ = service
                                .repo
                                .mark_failed(&session_id, "container runtime not available")
                                .await;
                            return;
                        }
                    };

                    // Start the container
                    if let Err(e) = runtime.start_container(&container_id_owned).await {
                        error!(
                            "Failed to restart container {} for session {}: {:?}",
                            container_id_owned, session_id, e
                        );
                        let _ = service
                            .repo
                            .mark_failed(&session_id, &format!("restart failed: {}", e))
                            .await;
                        return;
                    }

                    info!(
                        "Container {} restarted, waiting for services",
                        container_id_owned
                    );

                    // Wait for services to become ready
                    if let Err(e) = service
                        .readiness
                        .wait_for_session_services(fileserver_port, ttyd_port)
                        .await
                    {
                        error!(
                            "Services not ready after restart for session {}: {:?}",
                            session_id, e
                        );
                        let _ = service
                            .repo
                            .mark_failed(
                                &session_id,
                                &format!("services not ready after restart: {}", e),
                            )
                            .await;
                        return;
                    }

                    // Mark as running
                    if let Err(e) = service.repo.mark_running(&session_id).await {
                        error!("Failed to mark session {} as running: {:?}", session_id, e);
                    } else {
                        info!("Session {} successfully restarted", session_id);
                    }
                });

                // Return session with Starting status
                Ok(self.repo.get(&session.id).await?.unwrap_or(session))
            }
            Ok(Some(status)) => {
                let message = format!("container not running (status={})", status);
                self.repo.mark_failed(&session.id, &message).await?;
                Ok(self.repo.get(&session.id).await?.unwrap_or(session))
            }
            Ok(None) => {
                self.repo
                    .mark_failed(&session.id, "container not found")
                    .await?;
                Ok(self.repo.get(&session.id).await?.unwrap_or(session))
            }
            Err(e) => {
                warn!(
                    "Failed to check container {} for session {}: {:?}",
                    container_id, session.id, e
                );
                Ok(session)
            }
        }
    }

    fn parse_local_session_pids(container_id: Option<&str>) -> Option<Vec<u32>> {
        let container_id = container_id?;
        let pids: Vec<u32> = container_id
            .split(',')
            .filter_map(|raw| raw.trim().parse::<u32>().ok())
            .collect();
        if pids.is_empty() {
            return None;
        }
        Some(pids)
    }

    fn are_local_session_pids_running(pids: &[u32]) -> bool {
        #[cfg(target_os = "linux")]
        {
            pids.iter()
                .all(|pid| std::path::Path::new(&format!("/proc/{}", pid)).exists())
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = pids;
            false
        }
    }

    async fn is_local_session_running_best_effort(
        &self,
        local_runtime: &LocalRuntime,
        session: &Session,
    ) -> bool {
        if local_runtime.is_session_running(&session.id).await {
            return true;
        }

        if let Some(pids) = Self::parse_local_session_pids(session.container_id.as_deref())
            && Self::are_local_session_pids_running(&pids)
        {
            return true;
        }

        // Fallback: if expected ports are still bound, treat as running.
        let ports = [
            session.agent_port as u16,
            session.fileserver_port as u16,
            session.ttyd_port as u16,
        ];
        ports
            .iter()
            .all(|port| !crate::local::is_port_available(*port))
    }

    /// Reconcile local mode session state.
    async fn reconcile_local_mode_state(&self, session: Session) -> Result<Session> {
        let local_runtime = match self.local_runtime() {
            Some(r) => r,
            None => return Ok(session),
        };

        if self
            .is_local_session_running_best_effort(local_runtime.as_ref(), &session)
            .await
        {
            Ok(session)
        } else {
            // Get detailed exit info to help debug why processes stopped
            let exit_info = local_runtime.get_session_exit_info(&session.id).await;

            if exit_info.is_empty() {
                warn!(
                    "Local processes for session {} are not running (no exit info available), marking as stopped",
                    session.id
                );
                self.repo.mark_stopped(&session.id).await?;
            } else {
                // Format exit reasons for the error message
                let reasons: Vec<String> = exit_info
                    .iter()
                    .map(|(service, reason)| format!("{}: {}", service, reason))
                    .collect();
                let error_message =
                    format!("Processes stopped unexpectedly: {}", reasons.join("; "));

                warn!(
                    "Local processes for session {} crashed: {}",
                    session.id, error_message
                );

                // Use mark_failed instead of mark_stopped to preserve the error message
                self.repo.mark_failed(&session.id, &error_message).await?;
            }

            Ok(self.repo.get(&session.id).await?.unwrap_or(session))
        }
    }

    // ========================================================================
    // Cleanup and Orphan Container Management
    // ========================================================================

    /// Default hours after which stopped containers are eligible for cleanup.
    const DEFAULT_STALE_CONTAINER_HOURS: i64 = 72; // 3 days

    /// Run cleanup at server startup.
    ///
    /// This should be called once when the server starts to clean up any
    /// orphaned containers/processes from previous runs and sync database state.
    pub async fn startup_cleanup(&self) -> Result<()> {
        info!("Running startup cleanup...");

        // 0. For local mode: clean up orphan processes on base ports
        if self.config.runtime_mode == RuntimeMode::Local
            && let (Some(local_runtime), Some(local_config)) =
                (self.local_runtime(), self.config.local_config.as_ref())
        {
            if local_config.cleanup_on_startup {
                let base_port = self.config.base_port as u16;
                local_runtime.startup_cleanup(base_port);
            } else {
                info!("Skipping local startup cleanup (preserve running sessions)");
            }
        }

        // 1. Clean up orphan containers (containers without matching sessions)
        let orphans_cleaned = self.cleanup_orphan_containers().await?;
        if orphans_cleaned > 0 {
            info!("Cleaned up {} orphan container(s)", orphans_cleaned);
        }

        // 2. Mark stale sessions as failed (sessions with missing containers/processes)
        let stale_cleaned = self.cleanup_stale_sessions().await?;
        if stale_cleaned > 0 {
            info!("Marked {} stale session(s) as failed", stale_cleaned);
        }

        // 3. Clean up old stopped containers (stopped > N hours)
        let old_stopped_cleaned = self
            .cleanup_old_stopped_containers(Self::DEFAULT_STALE_CONTAINER_HOURS)
            .await?;
        if old_stopped_cleaned > 0 {
            info!(
                "Cleaned up {} old stopped container(s) (stopped > {} hours)",
                old_stopped_cleaned,
                Self::DEFAULT_STALE_CONTAINER_HOURS
            );
        }

        info!("Startup cleanup complete");
        Ok(())
    }

    /// Manually clean up orphan local session processes.
    pub async fn cleanup_local_orphans(&self) -> Result<usize> {
        if self.config.runtime_mode != RuntimeMode::Local {
            anyhow::bail!("local cleanup is only available in local mode");
        }
        let local_runtime = self
            .local_runtime()
            .context("local runtime not available")?;
        let base_port = self.config.base_port as u16;
        Ok(local_runtime.startup_cleanup(base_port))
    }

    /// Clean up stopped containers that have been stopped for too long.
    ///
    /// This removes containers (and their sessions) that have been stopped for
    /// longer than the specified number of hours. This prevents accumulation of
    /// old stopped containers while still allowing users to resume recently
    /// stopped sessions.
    pub async fn cleanup_old_stopped_containers(&self, older_than_hours: i64) -> Result<usize> {
        let stale_sessions = self
            .repo
            .list_stale_stopped_sessions(older_than_hours)
            .await?;

        let mut cleaned = 0;
        for session in stale_sessions {
            info!(
                "Cleaning up old stopped session {} (stopped at {:?})",
                session.id, session.stopped_at
            );
            if let Err(e) = self.delete_session(&session.id).await {
                warn!(
                    "Failed to clean up old stopped session {}: {:?}",
                    session.id, e
                );
            } else {
                cleaned += 1;
            }
        }

        Ok(cleaned)
    }

    /// Find orphan containers (containers with our prefix but no matching session).
    ///
    /// An orphan container is one that:
    /// 1. Has a name starting with our prefix (e.g., "oqto-")
    /// 2. Has no corresponding session in the database (by container_id)
    ///
    /// This is safe to run even with multiple users because it only identifies
    /// containers that have NO database record at all - not containers whose
    /// sessions might be in a transient state.
    ///
    /// Returns a list of (container_id, container_name) pairs.
    /// Note: This only applies to container mode; local mode doesn't have orphan containers.
    async fn find_orphan_containers(&self) -> Result<Vec<(String, String)>> {
        let runtime = match self.container_runtime() {
            Some(r) => r,
            None => return Ok(Vec::new()), // No container runtime, no orphans
        };

        // List all containers (including stopped ones)
        let containers = runtime.list_containers(true).await?;

        // Get ALL known container IDs from the database (all sessions, not just active)
        // This ensures we don't accidentally clean up a container that belongs to
        // a stopped/failed session that hasn't been deleted yet
        let sessions = self.repo.list().await?;
        let known_container_ids: HashSet<String> = sessions
            .iter()
            .filter_map(|s| s.container_id.clone())
            .collect();

        // Find containers with our prefix that aren't in the database at all
        let mut orphans = Vec::new();
        for container in containers {
            // Check if any of the container's names match our prefix
            // Note: podman/docker may prefix names with "/" in some contexts
            let is_our_container = container.names.iter().any(|name| {
                let clean_name = name.trim_start_matches('/');
                clean_name.starts_with(CONTAINER_NAME_PREFIX)
            });

            if is_our_container && !known_container_ids.contains(&container.id) {
                let name = container
                    .names
                    .first()
                    .cloned()
                    .unwrap_or_else(|| container.id.clone());
                orphans.push((container.id.clone(), name));
            }
        }

        if !orphans.is_empty() {
            info!("Found {} orphan container(s)", orphans.len());
        }

        Ok(orphans)
    }

    /// Clean up orphan containers (stop and remove them).
    ///
    /// Returns the number of containers cleaned up.
    pub async fn cleanup_orphan_containers(&self) -> Result<usize> {
        let orphans = self.find_orphan_containers().await?;
        let mut cleaned = 0;

        for (container_id, container_name) in orphans {
            info!(
                "Removing orphan container {} ({})",
                container_name, container_id
            );
            if let Err(e) = self.cleanup_container(&container_id).await {
                warn!(
                    "Failed to remove orphan container {} ({}): {:?}",
                    container_name, container_id, e
                );
            } else {
                cleaned += 1;
            }
        }

        Ok(cleaned)
    }

    /// Stop and remove a container.
    async fn cleanup_container(&self, container_id: &str) -> Result<()> {
        let runtime = self
            .container_runtime()
            .context("container runtime not available")?;

        // Try to stop first (ignore errors if already stopped)
        if let Err(e) = runtime.stop_container(container_id, Some(5)).await {
            debug!(
                "Stop container {} (may already be stopped): {:?}",
                container_id, e
            );
        }

        // Force remove
        runtime
            .remove_container(container_id, true)
            .await
            .context("removing container")?;

        Ok(())
    }

    /// Check if a specific port range is available on the system.
    ///
    /// This checks both the database for session port allocations and
    /// running containers for actual port usage.
    #[allow(dead_code)]
    pub async fn check_ports_available(&self, base_port: u16) -> Result<bool> {
        let ports_to_check = [base_port, base_port + 1, base_port + 2];

        // Check running containers for port conflicts (container mode only)
        if let Some(runtime) = self.container_runtime() {
            let containers = runtime.list_containers(false).await?;
            for container in containers {
                for port_info in &container.ports {
                    if ports_to_check.contains(&port_info.host_port) {
                        debug!(
                            "Port {} is in use by container {}",
                            port_info.host_port,
                            container.names.first().unwrap_or(&container.id)
                        );
                        return Ok(false);
                    }
                }
            }
        }

        // Also try binding to the ports directly as a sanity check
        for port in ports_to_check {
            match std::net::TcpListener::bind(format!("0.0.0.0:{}", port)) {
                Ok(_) => {} // Port is available
                Err(_) => {
                    debug!("Port {} is not available (bind check failed)", port);
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    // ========================================================================
    // Activity Tracking and Idle Session Management
    // ========================================================================

    /// Default idle timeout in minutes.
    pub const DEFAULT_IDLE_TIMEOUT_MINUTES: i64 = 30;

    /// Default maximum concurrent sessions per user.
    pub const DEFAULT_MAX_CONCURRENT_SESSIONS: i64 = 3;

    async fn touch_session_activity_for_user(&self, user_id: &str, session_id: &str) -> Result<()> {
        let Some(session) = self.get_session_for_user(user_id, session_id).await? else {
            anyhow::bail!("Session not found");
        };
        self.repo.touch_activity(&session.id).await
    }

    /// Get or create a session for a specific workspace path.
    ///
    /// This is the preferred entry point for resuming sessions from history.
    /// It will:
    /// 1. Find an existing running session for the workspace (if any)
    /// 2. Start a new session for that workspace (if none running)
    /// 3. Enforce LRU cap by stopping oldest idle session if needed
    async fn get_or_create_session_for_workspace_for_user(
        &self,
        user_id: &str,
        workspace_path: &str,
    ) -> Result<Session> {
        // Check if we already have a running session for this workspace
        if let Some(session) = self
            .repo
            .find_running_for_workspace(user_id, workspace_path)
            .await?
        {
            info!(
                "Found existing running session {} for workspace {}",
                session.id, workspace_path
            );
            // Touch activity since user is interacting
            self.repo.touch_activity(&session.id).await?;
            return Ok(session);
        }

        // Enforce LRU cap before resuming or creating a new session
        self.enforce_session_cap(user_id).await?;

        // Resume the most recently stopped session for this workspace, if any
        if let Some(session) = self
            .repo
            .find_latest_stopped_for_workspace(user_id, workspace_path)
            .await?
        {
            info!(
                "Resuming stopped session {} for workspace {}",
                session.id, workspace_path
            );
            match self.resume_session(&session.id).await {
                Ok(resumed) => {
                    if resumed.status == SessionStatus::Failed {
                        anyhow::bail!("failed to resume session {}", resumed.id);
                    }
                    return Ok(resumed);
                }
                Err(err) => {
                    if Self::is_retryable_unique_violation(&err) {
                        warn!(
                            "Resume port conflict for session {}, creating new session instead",
                            session.id
                        );
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        // Create a new session for this workspace
        let request = CreateSessionRequest {
            workspace_path: Some(workspace_path.to_string()),
            image: None,
            agent: None,
            env: Default::default(),
        };

        self.create_session_for_user(user_id, request).await
    }

    /// Get or create a session for IO (fileserver + ttyd) for a workspace path.
    ///
    /// This does does not wait for agent readiness before returning.
    async fn get_or_create_io_session_for_workspace_for_user(
        &self,
        user_id: &str,
        workspace_path: &str,
    ) -> Result<Session> {
        if let Some(session) = self
            .repo
            .find_running_for_workspace(user_id, workspace_path)
            .await?
        {
            self.repo.touch_activity(&session.id).await?;
            return Ok(session);
        }

        self.enforce_session_cap(user_id).await?;

        if let Some(session) = self
            .repo
            .find_latest_stopped_for_workspace(user_id, workspace_path)
            .await?
        {
            let resumed = self.resume_session_with_readiness(&session.id).await?;
            if resumed.status != SessionStatus::Failed {
                return Ok(resumed);
            }
        }

        let request = CreateSessionRequest {
            workspace_path: Some(workspace_path.to_string()),
            image: None,
            agent: None,
            env: Default::default(),
        };

        self.create_session_with_readiness(user_id, request).await
    }

    /// Enforce the maximum concurrent sessions cap using LRU policy.
    ///
    /// If the user has reached the limit, stop the oldest idle session.
    async fn enforce_session_cap(&self, user_id: &str) -> Result<()> {
        if self.config.max_concurrent_sessions <= 0 {
            return Ok(());
        }

        let running_count = self.repo.count_running_for_user(user_id).await?;

        if running_count < self.config.max_concurrent_sessions {
            return Ok(());
        }

        info!(
            "User {} has {} running sessions (limit: {}), stopping oldest",
            user_id, running_count, self.config.max_concurrent_sessions
        );

        let idle_sessions = self
            .repo
            .list_idle_sessions(self.config.idle_timeout_minutes)
            .await?;
        if let Some(oldest_idle) = idle_sessions.first() {
            info!(
                "Stopping idle session {} (last activity: {:?}) to make room",
                oldest_idle.id, oldest_idle.last_activity_at
            );
            self.stop_session(&oldest_idle.id).await?;
        } else {
            anyhow::bail!(
                "active sessions at limit ({}); no idle sessions available to stop",
                self.config.max_concurrent_sessions
            );
        }

        Ok(())
    }

    /// Stop sessions that have been idle for too long.
    ///
    /// This should be called periodically (e.g., by a background task).
    /// Returns the number of sessions stopped.
    pub async fn stop_idle_sessions(&self, idle_minutes: i64) -> Result<usize> {
        let idle_sessions = self.repo.list_idle_sessions(idle_minutes).await?;
        let mut stopped = 0;

        for session in idle_sessions {
            info!(
                "Stopping idle session {} (last activity: {:?}, idle > {} min)",
                session.id, session.last_activity_at, idle_minutes
            );
            if let Err(e) = self.stop_session(&session.id).await {
                warn!("Failed to stop idle session {}: {:?}", session.id, e);
            } else {
                stopped += 1;
            }
        }

        if stopped > 0 {
            info!("Stopped {} idle session(s)", stopped);
        }

        Ok(stopped)
    }

    /// Start a background task to periodically clean up idle sessions.
    ///
    /// Returns a handle that can be used to stop the task.
    pub fn start_idle_session_cleanup_task(
        self: Arc<Self>,
        check_interval_seconds: u64,
        idle_timeout_minutes: i64,
    ) -> tokio::task::JoinHandle<()> {
        info!(
            "Starting idle session cleanup task (check every {}s, timeout {}min)",
            check_interval_seconds, idle_timeout_minutes
        );

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(check_interval_seconds));

            loop {
                interval.tick().await;

                if let Err(e) = self.stop_idle_sessions(idle_timeout_minutes).await {
                    warn!("Idle session cleanup failed: {:?}", e);
                }
            }
        })
    }
}

/// Create empty home directory structure.
fn create_empty_home_dirs(user_home: &std::path::Path) -> Result<()> {
    let dirs = [
        "",             // home itself
        "workspace",    // working directory
        ".config",      // XDG_CONFIG_HOME
        ".local/share", // XDG_DATA_HOME
        ".local/state", // XDG_STATE_HOME
        ".cache",       // XDG_CACHE_HOME
    ];
    for dir in dirs {
        let dir_path = user_home.join(dir);
        std::fs::create_dir_all(&dir_path)
            .with_context(|| format!("creating directory: {:?}", dir_path))?;
    }
    Ok(())
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{Container, ContainerRuntimeApi, ContainerStats};
    use crate::db::Database;
    use crate::eavs::{CreateKeyResponse, EavsApi, EavsResult};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRuntime {
        last_env: Mutex<HashMap<String, String>>,
    }

    #[async_trait::async_trait]
    impl ContainerRuntimeApi for FakeRuntime {
        async fn create_container(
            &self,
            config: &ContainerConfig,
        ) -> crate::container::ContainerResult<String> {
            *self.last_env.lock().unwrap() = config.env.clone();

            Ok("fake-container-id".to_string())
        }

        async fn stop_container(
            &self,
            _container_id: &str,
            _timeout_seconds: Option<u32>,
        ) -> crate::container::ContainerResult<()> {
            Ok(())
        }

        async fn start_container(
            &self,
            _container_id: &str,
        ) -> crate::container::ContainerResult<()> {
            Ok(())
        }

        async fn remove_container(
            &self,
            _container_id: &str,
            _force: bool,
        ) -> crate::container::ContainerResult<()> {
            Ok(())
        }

        async fn list_containers(
            &self,
            _all: bool,
        ) -> crate::container::ContainerResult<Vec<Container>> {
            Ok(Vec::new())
        }

        async fn container_state_status(
            &self,
            _id_or_name: &str,
        ) -> crate::container::ContainerResult<Option<String>> {
            Ok(None)
        }

        async fn get_image_digest(
            &self,
            _image: &str,
        ) -> crate::container::ContainerResult<Option<String>> {
            Ok(None)
        }

        async fn get_stats(
            &self,
            container_id: &str,
        ) -> crate::container::ContainerResult<ContainerStats> {
            Ok(ContainerStats {
                container_id: container_id.to_string(),
                name: String::new(),
                cpu_percent: String::new(),
                mem_usage: String::new(),
                mem_percent: String::new(),
                net_io: String::new(),
                block_io: String::new(),
                pids: String::new(),
            })
        }

        async fn exec_detached(
            &self,
            _container_id: &str,
            _command: &[&str],
        ) -> crate::container::ContainerResult<()> {
            Ok(())
        }

        async fn exec_output(
            &self,
            _container_id: &str,
            _command: &[&str],
        ) -> crate::container::ContainerResult<String> {
            Ok(String::new())
        }
    }

    #[derive(Default)]
    struct NoopReadiness;

    #[async_trait]
    impl SessionReadiness for NoopReadiness {
        async fn wait_for_session_services(
            &self,
            _fileserver_port: u16,
            _ttyd_port: u16,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeEavs;

    #[async_trait]
    impl EavsApi for FakeEavs {
        async fn create_key(&self, _request: CreateKeyRequest) -> EavsResult<CreateKeyResponse> {
            Ok(CreateKeyResponse {
                key: "vk_test_123".to_string(),
                key_id: "cold-lamp".to_string(),
                key_hash: "hash_123".to_string(),
                name: None,
                created_at: Utc::now(),
                expires_at: None,
                permissions: KeyPermissions::default(),
            })
        }

        async fn revoke_key(&self, _key_id_or_hash: &str) -> EavsResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn create_session_never_persists_eavs_virtual_key() {
        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());
        let fake_runtime = Arc::new(FakeRuntime::default());
        let runtime: Arc<dyn ContainerRuntimeApi> = fake_runtime.clone();
        let eavs: Arc<dyn EavsApi> = Arc::new(FakeEavs);
        let workspace_dir = tempfile::tempdir().unwrap();

        let config = SessionServiceConfig {
            default_image: "test-image:latest".to_string(),
            base_port: 41820,
            user_data_path: workspace_dir.path().to_string_lossy().to_string(),
            skel_path: None,
            default_session_budget_usd: Some(10.0),
            default_session_rpm: Some(60),
            eavs_container_url: Some("http://eavs".to_string()),
            runtime_mode: RuntimeMode::Container,
            local_config: None,
            single_user: false,
            mmry_enabled: false,
            mmry_container_url: None,
            max_concurrent_sessions: SessionService::DEFAULT_MAX_CONCURRENT_SESSIONS,
            idle_timeout_minutes: SessionService::DEFAULT_IDLE_TIMEOUT_MINUTES,
            idle_check_interval_seconds: 5 * 60,
            pi_bridge_enabled: false,
            pi_provider: None,
            pi_model: None,
            agent_browser: AgentBrowserConfig::default(),
            runner_socket_pattern: None,
            linux_user_prefix: None,
        };

        let mut service = SessionService::with_eavs(repo.clone(), runtime.clone(), eavs, config);
        service.readiness = Arc::new(NoopReadiness);

        let session = service
            .for_user("test")
            .create_session(CreateSessionRequest {
                workspace_path: None,
                image: None,
                agent: None,
                env: Default::default(),
            })
            .await
            .unwrap();

        assert_eq!(session.status, SessionStatus::Running);
        assert!(session.container_id.is_some());
        assert!(session.eavs_key_id.is_some());
        assert!(session.eavs_key_hash.is_some());
        assert!(session.eavs_virtual_key.is_none());

        let stored = repo.get(&session.id).await.unwrap().unwrap();
        assert!(stored.eavs_virtual_key.is_none());

        let last_env = fake_runtime.last_env.lock().unwrap();
        assert_eq!(
            last_env.get("EAVS_VIRTUAL_KEY"),
            Some(&"vk_test_123".to_string())
        );
    }

    #[tokio::test]
    async fn collect_container_stats_returns_sessions() {
        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());
        let runtime: Arc<dyn ContainerRuntimeApi> = Arc::new(FakeRuntime::default());
        let service = SessionService::new(repo.clone(), runtime, SessionServiceConfig::default());

        let session = Session {
            id: "session-1".to_string(),
            readable_id: None,

            container_id: Some("container-1".to_string()),
            container_name: "oqto-session-1".to_string(),
            user_id: "user-1".to_string(),
            workspace_path: "/tmp/workspace".to_string(),
            agent: None,
            image: "oqto-dev:latest".to_string(),
            image_digest: None,
            agent_port: 41821,
            fileserver_port: 41822,
            ttyd_port: 41823,
            eavs_port: None,
            agent_base_port: None,
            max_agents: Some(10),
            eavs_key_id: None,
            eavs_key_hash: None,
            eavs_virtual_key: None,
            mmry_port: None,
            status: SessionStatus::Running,
            runtime_mode: RuntimeMode::Container,
            created_at: Utc::now().to_rfc3339(),
            started_at: None,
            stopped_at: None,
            last_activity_at: Some(Utc::now().to_rfc3339()),
            error_message: None,
        };

        repo.create(&session).await.unwrap();

        let report = service.collect_container_stats().await.unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.stats.len(), 1);
        assert_eq!(report.stats[0].session_id, "session-1");
        assert_eq!(report.stats[0].container_id, "container-1");
    }

    #[tokio::test]
    async fn resolve_workspace_path_enforces_allowed_roots() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_root = temp_dir.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root).unwrap();

        let allowed = workspace_root.join("project");
        std::fs::create_dir_all(&allowed).unwrap();

        let outside = temp_dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();

        let local_config = LocalRuntimeConfig {
            workspace_dir: workspace_root.to_string_lossy().to_string(),
            single_user: true,
            ..Default::default()
        };
        let local_runtime = LocalRuntime::new(local_config.clone());
        let config = SessionServiceConfig {
            runtime_mode: RuntimeMode::Local,
            local_config: Some(local_config),
            single_user: true,
            ..Default::default()
        };

        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());
        let runner = RunnerClient::default();
        let service = SessionService::with_runner(repo, runner, local_runtime, config);

        let user_id = "test-user";
        let resolved = service
            .resolve_workspace_path(user_id, allowed.to_string_lossy().as_ref())
            .await
            .unwrap();
        assert_eq!(resolved, allowed.canonicalize().unwrap());

        let relative = service
            .resolve_workspace_path(user_id, "project")
            .await
            .unwrap();
        assert_eq!(relative, allowed.canonicalize().unwrap());

        let err = service
            .resolve_workspace_path(user_id, outside.to_string_lossy().as_ref())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("outside allowed roots"));
    }

    #[test]
    fn test_session_service_config_default() {
        let config = SessionServiceConfig::default();
        assert_eq!(config.runtime_mode, RuntimeMode::Container);
        assert!(config.local_config.is_none());
        assert_eq!(config.default_image, DEFAULT_IMAGE);
        assert_eq!(config.base_port, DEFAULT_BASE_PORT);
    }

    #[test]
    fn test_session_service_config_with_local_mode() {
        let local_config = LocalRuntimeConfig::default();
        let config = SessionServiceConfig {
            runtime_mode: RuntimeMode::Local,
            local_config: Some(local_config.clone()),
            ..Default::default()
        };

        assert_eq!(config.runtime_mode, RuntimeMode::Local);
        assert!(config.local_config.is_some());
        assert_eq!(
            config.local_config.unwrap().fileserver_binary,
            local_config.fileserver_binary
        );
    }

    #[tokio::test]
    async fn test_session_service_with_runner_constructor() {
        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());

        let local_config = LocalRuntimeConfig::default();
        let local_runtime = LocalRuntime::new(local_config);
        let runner = RunnerClient::default();

        let config = SessionServiceConfig {
            runtime_mode: RuntimeMode::Local,
            local_config: None,
            ..Default::default()
        };

        let service = SessionService::with_runner(repo, runner, local_runtime, config);

        // Verify local runtime and runner are set
        assert!(service.local_runtime().is_some());
        assert!(service.runner.is_some());
        assert!(service.container_runtime().is_none());
    }

    #[tokio::test]
    async fn test_session_service_with_runner_and_eavs_constructor() {
        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());

        let local_config = LocalRuntimeConfig::default();
        let local_runtime = LocalRuntime::new(local_config);
        let runner = RunnerClient::default();
        let eavs: Arc<dyn EavsApi> = Arc::new(FakeEavs);

        let config = SessionServiceConfig {
            runtime_mode: RuntimeMode::Local,
            local_config: None,
            ..Default::default()
        };

        let service =
            SessionService::with_runner_and_eavs(repo, runner, local_runtime, eavs, config);

        // Verify runner, local runtime, and EAVS are set
        assert!(service.local_runtime().is_some());
        assert!(service.runner.is_some());
        assert!(service.container_runtime().is_none());
        assert!(service.eavs.is_some());
    }

    #[test]
    fn test_runtime_mode_default() {
        assert_eq!(RuntimeMode::default(), RuntimeMode::Container);
    }

    #[test]
    fn test_runtime_mode_display() {
        assert_eq!(format!("{}", RuntimeMode::Container), "container");
        assert_eq!(format!("{}", RuntimeMode::Local), "local");
    }

    #[test]
    fn test_runtime_mode_from_str() {
        assert_eq!(
            "container".parse::<RuntimeMode>().unwrap(),
            RuntimeMode::Container
        );
        assert_eq!("local".parse::<RuntimeMode>().unwrap(), RuntimeMode::Local);
        assert!("invalid".parse::<RuntimeMode>().is_err());
    }

    #[test]
    fn test_runtime_mode_serialization() {
        // Test that RuntimeMode serializes to lowercase string
        let container = RuntimeMode::Container;
        let local = RuntimeMode::Local;

        let container_json = serde_json::to_string(&container).unwrap();
        let local_json = serde_json::to_string(&local).unwrap();

        assert_eq!(container_json, "\"container\"");
        assert_eq!(local_json, "\"local\"");

        // Test deserialization
        let parsed_container: RuntimeMode = serde_json::from_str(&container_json).unwrap();
        let parsed_local: RuntimeMode = serde_json::from_str(&local_json).unwrap();

        assert_eq!(parsed_container, RuntimeMode::Container);
        assert_eq!(parsed_local, RuntimeMode::Local);
    }

    /// A fake runtime that can simulate failures for testing error handling.
    #[derive(Default)]
    struct FailingRuntime {
        fail_start: Mutex<bool>,
    }

    impl FailingRuntime {
        fn new(fail_start: bool) -> Self {
            Self {
                fail_start: Mutex::new(fail_start),
            }
        }
    }

    #[async_trait::async_trait]
    impl ContainerRuntimeApi for FailingRuntime {
        async fn create_container(
            &self,
            _config: &ContainerConfig,
        ) -> crate::container::ContainerResult<String> {
            Ok("fake-container-id".to_string())
        }

        async fn stop_container(
            &self,
            _container_id: &str,
            _timeout_seconds: Option<u32>,
        ) -> crate::container::ContainerResult<()> {
            Ok(())
        }

        async fn start_container(
            &self,
            _container_id: &str,
        ) -> crate::container::ContainerResult<()> {
            if *self.fail_start.lock().unwrap() {
                Err(crate::container::ContainerError::CommandFailed {
                    command: "start".to_string(),
                    message: "simulated start failure".to_string(),
                })
            } else {
                Ok(())
            }
        }

        async fn remove_container(
            &self,
            _container_id: &str,
            _force: bool,
        ) -> crate::container::ContainerResult<()> {
            Ok(())
        }

        async fn list_containers(
            &self,
            _all: bool,
        ) -> crate::container::ContainerResult<Vec<Container>> {
            Ok(Vec::new())
        }

        async fn container_state_status(
            &self,
            _id_or_name: &str,
        ) -> crate::container::ContainerResult<Option<String>> {
            Ok(Some("running".to_string()))
        }

        async fn get_image_digest(
            &self,
            _image: &str,
        ) -> crate::container::ContainerResult<Option<String>> {
            Ok(None)
        }

        async fn get_stats(
            &self,
            container_id: &str,
        ) -> crate::container::ContainerResult<ContainerStats> {
            Ok(ContainerStats {
                container_id: container_id.to_string(),
                name: String::new(),
                cpu_percent: String::new(),
                mem_usage: String::new(),
                mem_percent: String::new(),
                net_io: String::new(),
                block_io: String::new(),
                pids: String::new(),
            })
        }

        async fn exec_detached(
            &self,
            _container_id: &str,
            _command: &[&str],
        ) -> crate::container::ContainerResult<()> {
            Ok(())
        }

        async fn exec_output(
            &self,
            _container_id: &str,
            _command: &[&str],
        ) -> crate::container::ContainerResult<String> {
            Ok(String::new())
        }
    }

    /// A readiness checker that always fails - for testing error handling.
    #[derive(Default)]
    struct FailingReadiness;

    #[async_trait]
    impl SessionReadiness for FailingReadiness {
        async fn wait_for_session_services(
            &self,
            _fileserver_port: u16,
            _ttyd_port: u16,
        ) -> Result<()> {
            anyhow::bail!("simulated readiness failure")
        }
    }

    #[tokio::test]
    async fn test_resume_session_marks_failed_on_container_start_error() {
        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());
        let runtime: Arc<dyn ContainerRuntimeApi> = Arc::new(FailingRuntime::new(true));

        let config = SessionServiceConfig {
            default_image: "test-image:latest".to_string(),
            base_port: 41820,
            runtime_mode: RuntimeMode::Container,
            ..Default::default()
        };

        let mut service = SessionService::new(repo.clone(), runtime.clone(), config);
        service.readiness = Arc::new(NoopReadiness);

        // Create a stopped session in the database
        let session = Session {
            id: "test-session-1".to_string(),
            readable_id: None,

            container_id: Some("container-1".to_string()),
            container_name: "oqto-test-1".to_string(),
            user_id: "user-1".to_string(),
            workspace_path: "/tmp/workspace".to_string(),
            agent: None,
            image: "test-image:latest".to_string(),
            image_digest: None,
            agent_port: 41821,
            fileserver_port: 41822,
            ttyd_port: 41823,
            eavs_port: None,
            agent_base_port: None,
            max_agents: Some(10),
            eavs_key_id: None,
            eavs_key_hash: None,
            eavs_virtual_key: None,
            mmry_port: None,
            status: SessionStatus::Stopped,
            runtime_mode: RuntimeMode::Container,
            created_at: Utc::now().to_rfc3339(),
            started_at: None,
            stopped_at: Some(Utc::now().to_rfc3339()),
            last_activity_at: None,
            error_message: None,
        };

        repo.create(&session).await.unwrap();

        // Try to resume - should fail and mark as failed
        let result = service.resume_session("test-session-1").await;
        assert!(result.is_ok()); // Returns Ok with the failed session

        let failed_session = result.unwrap();
        assert_eq!(failed_session.status, SessionStatus::Failed);
        assert!(failed_session.error_message.is_some());
        assert!(
            failed_session
                .error_message
                .unwrap()
                .contains("resume failed")
        );

        // Verify in database
        let stored = repo.get("test-session-1").await.unwrap().unwrap();
        assert_eq!(stored.status, SessionStatus::Failed);
    }

    #[tokio::test]
    async fn test_resume_session_marks_failed_on_readiness_timeout() {
        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());
        let runtime: Arc<dyn ContainerRuntimeApi> = Arc::new(FakeRuntime::default());

        let config = SessionServiceConfig {
            default_image: "test-image:latest".to_string(),
            base_port: 41820,
            runtime_mode: RuntimeMode::Container,
            ..Default::default()
        };

        let mut service = SessionService::new(repo.clone(), runtime.clone(), config);
        // Use failing readiness to simulate timeout
        service.readiness = Arc::new(FailingReadiness);

        // Create a stopped session
        let session = Session {
            id: "test-session-2".to_string(),
            readable_id: None,

            container_id: Some("container-2".to_string()),
            container_name: "oqto-test-2".to_string(),
            user_id: "user-1".to_string(),
            workspace_path: "/tmp/workspace2".to_string(),
            agent: None,
            image: "test-image:latest".to_string(),
            image_digest: None,
            agent_port: 41824,
            fileserver_port: 41825,
            ttyd_port: 41826,
            eavs_port: None,
            agent_base_port: None,
            max_agents: Some(10),
            eavs_key_id: None,
            eavs_key_hash: None,
            eavs_virtual_key: None,
            mmry_port: None,
            status: SessionStatus::Stopped,
            runtime_mode: RuntimeMode::Container,
            created_at: Utc::now().to_rfc3339(),
            started_at: None,
            stopped_at: Some(Utc::now().to_rfc3339()),
            last_activity_at: None,
            error_message: None,
        };

        repo.create(&session).await.unwrap();

        // Try to resume - should fail on readiness and mark as failed
        let result = service.resume_session("test-session-2").await;
        assert!(result.is_ok());

        let failed_session = result.unwrap();
        assert_eq!(failed_session.status, SessionStatus::Failed);
        assert!(failed_session.error_message.is_some());

        // Verify in database
        let stored = repo.get("test-session-2").await.unwrap().unwrap();
        assert_eq!(stored.status, SessionStatus::Failed);
    }

    #[tokio::test]
    async fn test_resume_session_not_stopped_returns_error() {
        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());
        let runtime: Arc<dyn ContainerRuntimeApi> = Arc::new(FakeRuntime::default());

        let config = SessionServiceConfig::default();
        let service = SessionService::new(repo.clone(), runtime, config);

        // Create a running session
        let session = Session {
            id: "test-session-3".to_string(),
            readable_id: None,

            container_id: Some("container-3".to_string()),
            container_name: "oqto-test-3".to_string(),
            user_id: "user-1".to_string(),
            workspace_path: "/tmp/workspace3".to_string(),
            agent: None,
            image: "test-image:latest".to_string(),
            image_digest: None,
            agent_port: 41827,
            fileserver_port: 41828,
            ttyd_port: 41829,
            eavs_port: None,
            agent_base_port: None,
            max_agents: Some(10),
            eavs_key_id: None,
            eavs_key_hash: None,
            eavs_virtual_key: None,
            mmry_port: None,
            status: SessionStatus::Running, // Already running!
            runtime_mode: RuntimeMode::Container,
            created_at: Utc::now().to_rfc3339(),
            started_at: Some(Utc::now().to_rfc3339()),
            stopped_at: None,
            last_activity_at: None,
            error_message: None,
        };

        repo.create(&session).await.unwrap();

        // Try to resume a running session - should return error
        let result = service.resume_session("test-session-3").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("must be stopped"));
    }

    #[tokio::test]
    async fn test_resume_session_not_found_returns_error() {
        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());
        let runtime: Arc<dyn ContainerRuntimeApi> = Arc::new(FakeRuntime::default());

        let config = SessionServiceConfig::default();
        let service = SessionService::new(repo, runtime, config);

        // Try to resume non-existent session
        let result = service.resume_session("nonexistent-session").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_resume_session_success() {
        let db = Database::in_memory().await.unwrap();
        let repo = SessionRepository::new(db.pool().clone());
        let runtime: Arc<dyn ContainerRuntimeApi> = Arc::new(FakeRuntime::default());

        let config = SessionServiceConfig {
            default_image: "test-image:latest".to_string(),
            base_port: 41820,
            runtime_mode: RuntimeMode::Container,
            ..Default::default()
        };

        let mut service = SessionService::new(repo.clone(), runtime.clone(), config);
        service.readiness = Arc::new(NoopReadiness);

        // Create a stopped session
        let session = Session {
            id: "test-session-4".to_string(),
            readable_id: None,

            container_id: Some("container-4".to_string()),
            container_name: "oqto-test-4".to_string(),
            user_id: "user-1".to_string(),
            workspace_path: "/tmp/workspace4".to_string(),
            agent: None,
            image: "test-image:latest".to_string(),
            image_digest: None,
            agent_port: 41830,
            fileserver_port: 41831,
            ttyd_port: 41832,
            eavs_port: None,
            agent_base_port: None,
            max_agents: Some(10),
            eavs_key_id: None,
            eavs_key_hash: None,
            eavs_virtual_key: None,
            mmry_port: None,
            status: SessionStatus::Stopped,
            runtime_mode: RuntimeMode::Container,
            created_at: Utc::now().to_rfc3339(),
            started_at: None,
            stopped_at: Some(Utc::now().to_rfc3339()),
            last_activity_at: None,
            error_message: None,
        };

        repo.create(&session).await.unwrap();

        // Resume should succeed
        let result = service.resume_session("test-session-4").await;
        assert!(result.is_ok());

        let resumed = result.unwrap();
        assert_eq!(resumed.status, SessionStatus::Running);
        assert!(resumed.error_message.is_none());

        // Verify in database
        let stored = repo.get("test-session-4").await.unwrap().unwrap();
        assert_eq!(stored.status, SessionStatus::Running);
    }
}

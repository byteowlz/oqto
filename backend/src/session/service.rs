//! Session service - orchestrates container lifecycle.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use log::{debug, error, info, warn};
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use crate::container::{ContainerConfig, ContainerRuntimeApi};
use crate::eavs::{CreateKeyRequest, EavsApi, KeyPermissions};
use crate::wordlist;

use super::models::{CreateSessionRequest, Session, SessionStatus};
use super::repository::SessionRepository;

/// Prefix used for container names managed by this orchestrator.
const CONTAINER_NAME_PREFIX: &str = "octo-";

/// Default container image.
const DEFAULT_IMAGE: &str = "octo-dev:latest";

/// Default base port.
const DEFAULT_BASE_PORT: i64 = 41820;

#[async_trait]
trait SessionReadiness: Send + Sync {
    async fn wait_for_session_services(&self, opencode_port: u16, ttyd_port: u16) -> Result<()>;
}

#[derive(Debug, Default)]
struct HttpSessionReadiness;

#[async_trait]
impl SessionReadiness for HttpSessionReadiness {
    async fn wait_for_session_services(&self, opencode_port: u16, ttyd_port: u16) -> Result<()> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("building readiness HTTP client")?;

        let opencode_url = format!("http://localhost:{}/session", opencode_port);
        let ttyd_url = format!("http://localhost:{}/", ttyd_port);

        let start = tokio::time::Instant::now();
        // Increased timeout to 60s because opencode may need to download plugins
        // on first request, which can take time depending on network conditions.
        let timeout = tokio::time::Duration::from_secs(60);
        let mut attempts: u32 = 0;

        loop {
            attempts += 1;

            let opencode_ok = client
                .get(&opencode_url)
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

            if opencode_ok && ttyd_ok {
                return Ok(());
            }

            if start.elapsed() >= timeout {
                anyhow::bail!(
                    "session services not ready after {} attempts over {:?} (opencode_ok={}, ttyd_ok={})",
                    attempts,
                    timeout,
                    opencode_ok,
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
    /// Default container image to use.
    pub default_image: String,
    /// Base port for allocating session ports.
    pub base_port: i64,
    /// Base directory for user home directories. Each user gets {base}/home/{user_id}/.
    pub user_data_path: String,
    /// Path to skeleton directory to copy into new user homes. If None, empty dirs are created.
    pub skel_path: Option<String>,
    /// Default user ID for sessions.
    pub default_user_id: String,
    /// Default budget limit per session in USD.
    pub default_session_budget_usd: Option<f64>,
    /// Default rate limit per session (requests per minute).
    pub default_session_rpm: Option<u32>,
    /// URL for containers to reach EAVS (e.g., http://host.docker.internal:41800).
    pub eavs_container_url: Option<String>,
}

impl Default for SessionServiceConfig {
    fn default() -> Self {
        Self {
            default_image: DEFAULT_IMAGE.to_string(),
            base_port: DEFAULT_BASE_PORT,
            user_data_path: "./data".to_string(),
            skel_path: None,
            default_user_id: "default".to_string(),
            default_session_budget_usd: Some(10.0),
            default_session_rpm: Some(60),
            eavs_container_url: None,
        }
    }
}

/// Service for managing container sessions.
#[derive(Clone)]
pub struct SessionService {
    repo: SessionRepository,
    runtime: Arc<dyn ContainerRuntimeApi>,
    eavs: Option<Arc<dyn EavsApi>>,
    readiness: Arc<dyn SessionReadiness>,
    config: SessionServiceConfig,
}

impl SessionService {
    /// Create a new session service.
    pub fn new(
        repo: SessionRepository,
        runtime: Arc<dyn ContainerRuntimeApi>,
        config: SessionServiceConfig,
    ) -> Self {
        Self {
            repo,
            runtime,
            eavs: None,
            readiness: Arc::new(HttpSessionReadiness::default()),
            config,
        }
    }

    /// Create a new session service with EAVS integration.
    pub fn with_eavs(
        repo: SessionRepository,
        runtime: Arc<dyn ContainerRuntimeApi>,
        eavs: Arc<dyn EavsApi>,
        config: SessionServiceConfig,
    ) -> Self {
        Self {
            repo,
            runtime,
            eavs: Some(eavs),
            readiness: Arc::new(HttpSessionReadiness::default()),
            config,
        }
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
    pub async fn get_or_create_session(&self, request: CreateSessionRequest) -> Result<Session> {
        let user_id = &self.config.default_user_id;

        // Check for running sessions that need upgrading
        let running_sessions = self.repo.list_running_for_user(user_id).await?;
        for session in running_sessions {
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
            // Verify the container still exists
            if let Some(ref container_id) = stopped_session.container_id {
                match self.runtime.container_state_status(container_id).await {
                    Ok(Some(status)) if status == "exited" || status == "stopped" => {
                        info!(
                            "Found resumable session {} for user {}, resuming...",
                            stopped_session.id, user_id
                        );
                        return self.resume_session(&stopped_session.id).await;
                    }
                    Ok(Some(status)) => {
                        debug!(
                            "Stopped session {} has container in unexpected state: {}",
                            stopped_session.id, status
                        );
                    }
                    Ok(None) => {
                        debug!(
                            "Container {} for stopped session {} no longer exists",
                            container_id, stopped_session.id
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to check container state for session {}: {:?}",
                            stopped_session.id, e
                        );
                    }
                }
            }
        }

        // No resumable session found, create a new one
        self.create_session(request).await
    }

    /// Create and start a new session.
    ///
    /// This method handles port allocation with retry logic to handle race conditions
    /// when multiple users create sessions simultaneously. The database has partial unique
    /// indexes on ports for active sessions, so if two requests try to allocate the same
    /// ports concurrently, one will fail and retry with a different range.
    ///
    /// Security: the EAVS virtual key is never persisted to the database; it is passed
    /// directly into container env and then dropped.
    pub async fn create_session(&self, request: CreateSessionRequest) -> Result<Session> {
        let image = request
            .image
            .unwrap_or_else(|| self.config.default_image.clone());

        // Get current image digest for tracking upgrades (best-effort).
        let image_digest = match self.runtime.get_image_digest(&image).await {
            Ok(digest) => digest,
            Err(e) => {
                warn!("Failed to get image digest for {}: {:?}", image, e);
                None
            }
        };

        // Determine user home path - either provided or create per-user home directory.
        let user_home_path = if let Some(path) = request.workspace_path {
            if !std::path::Path::new(&path).exists() {
                anyhow::bail!("workspace path does not exist: {}", path);
            }
            path
        } else {
            let user_id = &self.config.default_user_id;
            let user_home = std::path::Path::new(&self.config.user_data_path)
                .join("home")
                .join(user_id);

            if !user_home.exists() {
                // If skel_path is configured, copy it; otherwise create empty dirs
                if let Some(ref skel_path) = self.config.skel_path {
                    let skel = std::path::Path::new(skel_path);
                    if skel.exists() {
                        copy_dir_recursive(skel, &user_home)
                            .with_context(|| format!("copying skel from {:?} to {:?}", skel, user_home))?;
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

        let mut last_error = None;
        for attempt in 0..Self::MAX_PORT_ALLOCATION_RETRIES {
            match self
                .try_create_session(&user_home_path, &image, image_digest.as_deref(), attempt)
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
            if let Some(sqlx_error) = cause.downcast_ref::<sqlx::Error>() {
                if let sqlx::Error::Database(db_err) = sqlx_error {
                    if db_err.is_unique_violation() {
                        return true;
                    }
                }
            }
        }

        let error_str = error.to_string().to_lowercase();
        error_str.contains("unique constraint")
            || error_str.contains("unique_violation")
            || error_str.contains("duplicate")
    }

    /// Internal method to attempt session creation with a specific port range.
    async fn try_create_session(
        &self,
        user_home_path: &str,
        image: &str,
        image_digest: Option<&str>,
        attempt: u32,
    ) -> Result<Session> {
        let session_id = Uuid::new_v4().to_string();
        let container_name = format!("{}{}", CONTAINER_NAME_PREFIX, &session_id[..8]);

        let readable_id = self.generate_unique_readable_id().await?;

        // Find available ports (opencode, fileserver, ttyd). On retry, offset the search window.
        let search_start = self.config.base_port + (attempt as i64 * 4);
        let base_port = self.repo.find_free_port_range(search_start).await?;
        let opencode_port = base_port;
        let fileserver_port = base_port + 1;
        let ttyd_port = base_port + 2;

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

        let session = Session {
            id: session_id.clone(),
            readable_id: Some(readable_id),
            container_id: None,
            container_name: container_name.clone(),
            user_id: self.config.default_user_id.clone(),
            workspace_path: user_home_path.to_string(),
            image: image.to_string(),
            image_digest: image_digest.map(ToString::to_string),
            opencode_port,
            fileserver_port,
            ttyd_port,
            eavs_port: None,
            eavs_key_id,
            eavs_key_hash,
            eavs_virtual_key: None,
            status: SessionStatus::Pending,
            created_at: Utc::now().to_rfc3339(),
            started_at: None,
            stopped_at: None,
            error_message: None,
        };

        // Persist the session. This will fail with a unique constraint violation if another
        // session grabbed these ports/readable_id between our check and insert.
        self.repo.create(&session).await?;

        info!(
            "Created session {} with ports {}/{}/{}",
            session_id, opencode_port, fileserver_port, ttyd_port
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
            if let (Some(eavs), Some(key_id)) = (&self.eavs, &session.eavs_key_id) {
                if let Err(revoke_err) = eavs.revoke_key(key_id).await {
                    warn!(
                        "Failed to revoke EAVS key {} after startup failure: {:?}",
                        key_id, revoke_err
                    );
                }
            }

            return Err(e);
        }

        Ok(self.repo.get(&session.id).await?.unwrap_or(session))
    }

    /// Generate a unique human-readable ID for a session.
    async fn generate_unique_readable_id(&self) -> Result<String> {
        let mut attempts = 0;
        loop {
            let readable_id = wordlist::generate_readable_id();

            // Check if this ID already exists
            if !self.repo.readable_id_exists(&readable_id).await? {
                return Ok(readable_id);
            }

            attempts += 1;
            if attempts > 100 {
                // After many attempts, add a random suffix
                let suffix: u16 = rand::random::<u16>() % 1000;
                return Ok(format!("{}-{}", wordlist::generate_readable_id(), suffix));
            }
        }
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
                "created_by": "octo"
            }));

        let response = eavs.create_key(request).await?;

        Ok((response.key_id, response.key_hash, response.key))
    }

    /// Start a container for the given session.
    async fn start_container(
        &self,
        session: &Session,
        eavs_virtual_key: Option<&str>,
    ) -> Result<()> {
        debug!("Starting container for session {}", session.id);

        // Build container config
        // Mount the full user home directory so dotfiles and tool state persist across restarts.
        let mut config = ContainerConfig::new(&session.image)
            .name(&session.container_name)
            .hostname(&session.container_name)
            .port(session.opencode_port as u16, 41820)
            .port(session.fileserver_port as u16, 41821)
            .port(session.ttyd_port as u16, 41822)
            .volume(&session.workspace_path, "/home/dev")
            .env("OPENCODE_PORT", "41820")
            .env("FILESERVER_PORT", "41821")
            .env("TTYD_PORT", "41822");

        // Pass EAVS URL and virtual key to container if available
        if let Some(ref eavs_url) = self.config.eavs_container_url {
            config = config.env("EAVS_URL", eavs_url);
        }
        if let Some(virtual_key) = eavs_virtual_key {
            config = config.env("EAVS_VIRTUAL_KEY", virtual_key);
        }

        // Create and start the container
        let container_id = self
            .runtime
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
            .wait_for_session_services(session.opencode_port as u16, session.ttyd_port as u16)
            .await
        {
            // Best-effort cleanup: stop/remove the container, then surface the error.
            if let Err(stop_err) = self.runtime.stop_container(&container_id, Some(10)).await {
                warn!(
                    "Failed to stop container {} after readiness failure: {:?}",
                    container_id, stop_err
                );
            }
            if let Err(rm_err) = self.runtime.remove_container(&container_id, true).await {
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

    /// Stop a session and its container.
    ///
    /// This only stops the container, it does NOT remove it. The container can be
    /// restarted later with `resume_session()`. To fully remove the container,
    /// use `delete_session()`.
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

        info!("Stopping session {}", session_id);
        self.repo
            .update_status(session_id, SessionStatus::Stopping)
            .await?;

        // Note: We do NOT revoke EAVS key on stop anymore - only on delete.
        // This allows the session to be resumed without needing a new key.

        // Stop the container if it exists (but do NOT remove it)
        if let Some(ref container_id) = session.container_id {
            if let Err(e) = self.runtime.stop_container(container_id, Some(10)).await {
                warn!("Failed to stop container {}: {:?}", container_id, e);
            }
            // Container is NOT removed - it can be restarted with resume_session()
        }

        self.repo.mark_stopped(session_id).await?;
        info!("Session {} stopped (container preserved for resume)", session_id);

        Ok(())
    }

    /// Resume a stopped session by restarting its container.
    ///
    /// This is faster than creating a new session because the container already exists
    /// with all its state (opencode sessions, MCP servers, etc.).
    pub async fn resume_session(&self, session_id: &str) -> Result<Session> {
        let session = self
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

        // Check if image has been updated - if so, upgrade instead of resume
        if let Ok(Some(new_digest)) = self.check_for_image_update(session_id).await {
            info!(
                "Image update detected for session {} (new digest: {}), upgrading instead of resuming",
                session_id, new_digest
            );
            return self.upgrade_session(session_id).await;
        }

        let container_id = session
            .container_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session has no container to resume"))?;

        info!("Resuming session {} (container {})", session_id, container_id);

        // Mark as starting
        self.repo
            .update_status(session_id, SessionStatus::Starting)
            .await?;

        // Start the existing container
        if let Err(e) = self.runtime.start_container(container_id).await {
            error!(
                "Failed to start container {} for session {}: {:?}",
                container_id, session_id, e
            );
            self.repo
                .mark_failed(session_id, &format!("resume failed: {}", e))
                .await?;
            return Ok(self.repo.get(session_id).await?.unwrap_or(session));
        }

        // Wait for services to become ready
        if let Err(e) = self
            .readiness
            .wait_for_session_services(session.opencode_port as u16, session.ttyd_port as u16)
            .await
        {
            error!(
                "Services not ready after resume for session {}: {:?}",
                session_id, e
            );
            // Stop the container again since services didn't come up
            let _ = self.runtime.stop_container(container_id, Some(5)).await;
            self.repo
                .mark_failed(session_id, &format!("services not ready after resume: {}", e))
                .await?;
            return Ok(self.repo.get(session_id).await?.unwrap_or(session));
        }

        // Mark as running
        self.repo.mark_running(session_id).await?;
        info!("Session {} resumed successfully", session_id);

        Ok(self.repo.get(session_id).await?.unwrap_or(session))
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

    /// List active sessions.
    #[allow(dead_code)]
    pub async fn list_active_sessions(&self) -> Result<Vec<Session>> {
        self.repo.list_active().await
    }

    /// Delete a session and remove its container.
    ///
    /// This fully removes the container and revokes any EAVS keys. The session
    /// must be stopped first (use `stop_session()`).
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

        // Remove the container if it exists
        if let Some(ref container_id) = session.container_id {
            // Try to stop first (in case it's somehow still running)
            let _ = self.runtime.stop_container(container_id, Some(5)).await;
            
            // Remove the container
            if let Err(e) = self.runtime.remove_container(container_id, true).await {
                warn!(
                    "Failed to remove container {} for session {}: {:?}",
                    container_id, session_id, e
                );
                // Continue with deletion even if container removal fails
            }
        }

        self.repo.delete(session_id).await?;
        info!("Deleted session {} and removed container", session_id);

        Ok(())
    }

    /// Cleanup stale sessions (containers that no longer exist).
    #[allow(dead_code)]
    pub async fn cleanup_stale_sessions(&self) -> Result<usize> {
        let active = self.repo.list_active().await?;
        let mut cleaned = 0;

        for session in active {
            if let Some(ref container_id) = session.container_id {
                // Check if container still exists
                match self.runtime.container_state_status(container_id).await {
                    Ok(Some(status)) if status == "running" => continue, // Container exists, skip
                    Ok(Some(_)) | Ok(None) => {
                        warn!(
                            "Container {} for session {} is no longer running, marking as stopped",
                            container_id, session.id
                        );
                        self.repo.mark_stopped(&session.id).await?;
                        cleaned += 1;
                    }
                    Err(e) => {
                        warn!(
                            "Failed to check container {} for session {}: {:?}",
                            container_id, session.id, e
                        );
                    }
                }
            }
        }

        Ok(cleaned)
    }

    /// Check if a newer image is available for a session.
    ///
    /// Returns `Ok(Some(new_digest))` if a newer image is available,
    /// `Ok(None)` if the session is up to date or we can't determine.
    pub async fn check_for_image_update(&self, session_id: &str) -> Result<Option<String>> {
        let session = self
            .repo
            .get(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

        // Get current image digest
        let current_digest = match self.runtime.get_image_digest(&session.image).await {
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
    pub async fn upgrade_session(&self, session_id: &str) -> Result<Session> {
        let session = self
            .repo
            .get(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

        info!(
            "Upgrading session {} from image {}",
            session_id, session.image
        );

        // Get the new image digest before we start
        let new_digest = self.runtime.get_image_digest(&session.image).await?;

        // Stop and remove the existing container if it exists
        if let Some(ref container_id) = session.container_id {
            info!("Stopping existing container {} for upgrade", container_id);

            // Try to stop gracefully first
            if let Err(e) = self.runtime.stop_container(container_id, Some(10)).await {
                warn!(
                    "Failed to stop container {} (may already be stopped): {:?}",
                    container_id, e
                );
            }

            // Remove the container
            if let Err(e) = self.runtime.remove_container(container_id, true).await {
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

    /// Check all active sessions for available image updates.
    ///
    /// Returns a list of (session_id, new_digest) pairs for sessions that have updates available.
    pub async fn check_all_for_updates(&self) -> Result<Vec<(String, String)>> {
        let sessions = self.repo.list_active().await?;
        let mut updates = Vec::new();

        for session in sessions {
            if let Ok(Some(new_digest)) = self.check_for_image_update(&session.id).await {
                updates.push((session.id, new_digest));
            }
        }

        Ok(updates)
    }

    async fn reconcile_session_container_state(&self, session: Session) -> Result<Session> {
        if !session.is_active() {
            return Ok(session);
        }

        let Some(ref container_id) = session.container_id else {
            return Ok(session);
        };

        match self.runtime.container_state_status(container_id).await {
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
                let container_id_owned = container_id.clone();
                let opencode_port = session.opencode_port as u16;
                let ttyd_port = session.ttyd_port as u16;

                tokio::spawn(async move {
                    // Start the container
                    if let Err(e) = service.runtime.start_container(&container_id_owned).await {
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
                        .wait_for_session_services(opencode_port, ttyd_port)
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

    // ========================================================================
    // Cleanup and Orphan Container Management
    // ========================================================================

    /// Default hours after which stopped containers are eligible for cleanup.
    const DEFAULT_STALE_CONTAINER_HOURS: i64 = 72; // 3 days

    /// Run cleanup at server startup.
    ///
    /// This should be called once when the server starts to clean up any
    /// orphaned containers from previous runs and sync database state.
    pub async fn startup_cleanup(&self) -> Result<()> {
        info!("Running startup cleanup...");

        // 1. Clean up orphan containers (containers without matching sessions)
        let orphans_cleaned = self.cleanup_orphan_containers().await?;
        if orphans_cleaned > 0 {
            info!("Cleaned up {} orphan container(s)", orphans_cleaned);
        }

        // 2. Mark stale sessions as failed (sessions with missing containers)
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
    /// 1. Has a name starting with our prefix (e.g., "opencode-")
    /// 2. Has no corresponding session in the database (by container_id)
    ///
    /// This is safe to run even with multiple users because it only identifies
    /// containers that have NO database record at all - not containers whose
    /// sessions might be in a transient state.
    ///
    /// Returns a list of (container_id, container_name) pairs.
    async fn find_orphan_containers(&self) -> Result<Vec<(String, String)>> {
        // List all containers (including stopped ones)
        let containers = self.runtime.list_containers(true).await?;

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
        // Try to stop first (ignore errors if already stopped)
        if let Err(e) = self.runtime.stop_container(container_id, Some(5)).await {
            debug!(
                "Stop container {} (may already be stopped): {:?}",
                container_id, e
            );
        }

        // Force remove
        self.runtime
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

        // Check running containers for port conflicts
        let containers = self.runtime.list_containers(false).await?;
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
    use crate::container::{Container, ContainerRuntimeApi};
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
            _opencode_port: u16,
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
        let eavs: Arc<dyn EavsApi> = Arc::new(FakeEavs::default());

        let config = SessionServiceConfig {
            default_image: "test-image:latest".to_string(),
            base_port: 41820,
            user_data_path: "./data".to_string(),
            skel_path: None,
            default_user_id: "default".to_string(),
            default_session_budget_usd: Some(10.0),
            default_session_rpm: Some(60),
            eavs_container_url: Some("http://eavs".to_string()),
        };

        let mut service = SessionService::with_eavs(repo.clone(), runtime.clone(), eavs, config);
        service.readiness = Arc::new(NoopReadiness::default());

        let workspace_dir = tempfile::tempdir().unwrap();
        let session = service
            .create_session(CreateSessionRequest {
                workspace_path: Some(workspace_dir.path().to_string_lossy().to_string()),
                image: None,
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
}

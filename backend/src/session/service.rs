//! Session service - orchestrates container lifecycle.

use anyhow::{Context, Result};
use chrono::Utc;
use log::{debug, error, info, warn};
use std::sync::Arc;
use uuid::Uuid;

use crate::container::{ContainerConfig, ContainerRuntime};
use crate::eavs::{CreateKeyRequest, EavsClient, KeyPermissions};

use super::models::{CreateSessionRequest, Session, SessionStatus};
use super::repository::SessionRepository;

/// Default container image.
const DEFAULT_IMAGE: &str = "opencode-dev:latest";

/// Default base port.
const DEFAULT_BASE_PORT: i64 = 41820;

/// Session service configuration.
#[derive(Debug, Clone)]
pub struct SessionServiceConfig {
    /// Default container image to use.
    pub default_image: String,
    /// Base port for allocating session ports.
    pub base_port: i64,
    /// Default workspace directory to mount when none is provided.
    pub default_workspace_path: String,
    /// Default user ID for sessions.
    pub default_user_id: String,
    /// Default budget limit per session in USD.
    pub default_session_budget_usd: Option<f64>,
    /// Default rate limit per session (requests per minute).
    pub default_session_rpm: Option<u32>,
}

impl Default for SessionServiceConfig {
    fn default() -> Self {
        Self {
            default_image: DEFAULT_IMAGE.to_string(),
            base_port: DEFAULT_BASE_PORT,
            default_workspace_path: ".".to_string(),
            default_user_id: "default".to_string(),
            default_session_budget_usd: Some(10.0),
            default_session_rpm: Some(60),
        }
    }
}

/// Service for managing container sessions.
#[derive(Clone)]
pub struct SessionService {
    repo: SessionRepository,
    runtime: Arc<ContainerRuntime>,
    eavs: Option<Arc<EavsClient>>,
    config: SessionServiceConfig,
}

impl SessionService {
    /// Create a new session service.
    pub fn new(repo: SessionRepository, runtime: ContainerRuntime, config: SessionServiceConfig) -> Self {
        Self {
            repo,
            runtime: Arc::new(runtime),
            eavs: None,
            config,
        }
    }

    /// Create a new session service with EAVS integration.
    pub fn with_eavs(
        repo: SessionRepository,
        runtime: ContainerRuntime,
        eavs: EavsClient,
        config: SessionServiceConfig,
    ) -> Self {
        Self {
            repo,
            runtime: Arc::new(runtime),
            eavs: Some(Arc::new(eavs)),
            config,
        }
    }

    /// Create and start a new session.
    pub async fn create_session(&self, request: CreateSessionRequest) -> Result<Session> {
        let session_id = Uuid::new_v4().to_string();
        let container_name = format!("opencode-{}", &session_id[..8]);

        let workspace_path = request
            .workspace_path
            .unwrap_or_else(|| self.config.default_workspace_path.clone());

        if !std::path::Path::new(&workspace_path).exists() {
            anyhow::bail!("workspace path does not exist: {}", workspace_path);
        }

        // Find available ports (now 4 ports: opencode, fileserver, ttyd, eavs)
        let base_port = self
            .repo
            .find_free_port_range(self.config.base_port)
            .await?;
        let opencode_port = base_port;
        let fileserver_port = base_port + 1;
        let ttyd_port = base_port + 2;
        let eavs_port = base_port + 3;

        let image = request
            .image
            .unwrap_or_else(|| self.config.default_image.clone());

        // Create EAVS virtual key if EAVS is configured
        let (eavs_key_id, eavs_key_hash, eavs_virtual_key) = if let Some(ref eavs) = self.eavs {
            match self.create_eavs_key(&session_id).await {
                Ok((key_id, key_hash, key_value)) => {
                    info!("Created EAVS key {} for session {}", key_id, session_id);
                    (Some(key_id), Some(key_hash), Some(key_value))
                }
                Err(e) => {
                    warn!("Failed to create EAVS key for session {}: {:?}", session_id, e);
                    (None, None, None)
                }
            }
        } else {
            (None, None, None)
        };

        // Create session record
        let session = Session {
            id: session_id.clone(),
            container_id: None,
            container_name: container_name.clone(),
            user_id: self.config.default_user_id.clone(),
            workspace_path,
            image: image.clone(),
            opencode_port,
            fileserver_port,
            ttyd_port,
            eavs_port: Some(eavs_port),
            eavs_key_id,
            eavs_key_hash,
            eavs_virtual_key,
            status: SessionStatus::Pending,
            created_at: Utc::now().to_rfc3339(),
            started_at: None,
            stopped_at: None,
            error_message: None,
        };

        // Persist the session
        self.repo.create(&session).await?;

        info!(
            "Created session {} with ports {}/{}/{}/{}",
            session_id, opencode_port, fileserver_port, ttyd_port, eavs_port
        );

        // Start the container in the background
        let service = self.clone();
        let session_clone = session.clone();
        tokio::spawn(async move {
            if let Err(e) = service.start_container(&session_clone).await {
                error!(
                    "Failed to start container for session {}: {:?}",
                    session_clone.id, e
                );
                let _ = service
                    .repo
                    .mark_failed(&session_clone.id, &e.to_string())
                    .await;
            }
        });

        Ok(session)
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
                "created_by": "workspace-backend"
            }));

        let response = eavs.create_key(request).await?;

        Ok((response.key_id, response.key_hash, response.key))
    }

    /// Start a container for the given session.
    async fn start_container(&self, session: &Session) -> Result<()> {
        debug!("Starting container for session {}", session.id);

        let eavs_port = session.eavs_port.unwrap_or(41823);

        // Build container config
        let mut config = ContainerConfig::new(&session.image)
            .name(&session.container_name)
            .hostname(&session.container_name)
            .port(session.opencode_port as u16, 41820)
            .port(session.fileserver_port as u16, 41821)
            .port(session.ttyd_port as u16, 41822)
            .port(eavs_port as u16, 41823)
            .volume(&session.workspace_path, "/home/dev/workspace")
            .env("OPENCODE_PORT", "41820")
            .env("FILESERVER_PORT", "41821")
            .env("TTYD_PORT", "41822")
            .env("EAVS_PORT", "41823");

        // Pass EAVS virtual key to container if available
        if let Some(ref virtual_key) = session.eavs_virtual_key {
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

        // Clear the virtual key from the session record (security: don't persist it)
        self.repo.clear_eavs_virtual_key(&session.id).await?;

        // Wait a moment for services to start
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Mark as running
        self.repo.mark_running(&session.id).await?;

        Ok(())
    }

    /// Stop a session and its container.
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

        // Stop the container if it exists
        if let Some(ref container_id) = session.container_id {
            if let Err(e) = self.runtime.stop_container(container_id, Some(10)).await {
                warn!("Failed to stop container {}: {:?}", container_id, e);
            }

            // Remove the container
            if let Err(e) = self.runtime.remove_container(container_id, true).await {
                warn!("Failed to remove container {}: {:?}", container_id, e);
            }
        }

        self.repo.mark_stopped(session_id).await?;
        info!("Session {} stopped", session_id);

        Ok(())
    }

    /// Get a session by ID.
    pub async fn get_session(&self, session_id: &str) -> Result<Option<Session>> {
        self.repo.get(session_id).await
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.repo.list().await
    }

    /// List active sessions.
    #[allow(dead_code)]
    pub async fn list_active_sessions(&self) -> Result<Vec<Session>> {
        self.repo.list_active().await
    }

    /// Delete a session (must be stopped first).
    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let session = self
            .repo
            .get(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

        if session.is_active() {
            anyhow::bail!("cannot delete active session, stop it first");
        }

        self.repo.delete(session_id).await?;
        info!("Deleted session {}", session_id);

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
                match self.runtime.get_container(container_id).await {
                    Ok(Some(_)) => continue, // Container exists, skip
                    Ok(None) => {
                        warn!(
                            "Container {} for session {} no longer exists, marking as stopped",
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
}

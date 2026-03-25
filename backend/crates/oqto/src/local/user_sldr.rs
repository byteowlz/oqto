//! Per-user sldr-server process manager for local multi-user mode.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::runner::client::{RunnerClient, RunnerEndpointPattern};
use crate::user::UserRepository;

#[derive(Debug, Clone)]
pub struct UserSldrConfig {
    pub sldr_binary: String,
    pub base_port: u16,
    pub port_range: u16,
    /// Runner endpoint pattern.
    pub runner_endpoint: Option<RunnerEndpointPattern>,
}

#[derive(Debug, Clone)]
struct UserSldrInstance {
    port: u16,
}

#[derive(Debug, Default)]
struct UserSldrState {
    instances: HashMap<String, UserSldrInstance>,
}

/// Tracks and spawns per-user sldr-server instances.
#[derive(Clone)]
pub struct UserSldrManager {
    config: UserSldrConfig,
    state: Arc<Mutex<UserSldrState>>,
    linux_username_for_user_id: Arc<dyn Fn(&str) -> String + Send + Sync>,
    user_repo: UserRepository,
}

impl std::fmt::Debug for UserSldrManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserSldrManager")
            .field("config", &self.config)
            .finish()
    }
}

impl UserSldrManager {
    pub fn new(
        config: UserSldrConfig,
        linux_username_for_user_id: impl Fn(&str) -> String + Send + Sync + 'static,
        user_repo: UserRepository,
    ) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(UserSldrState::default())),
            linux_username_for_user_id: Arc::new(linux_username_for_user_id),
            user_repo,
        }
    }

    fn runner_client_for_linux_user(&self, linux_username: &str) -> Result<RunnerClient> {
        if let Some(endpoint) = self.config.runner_endpoint.as_ref() {
            RunnerClient::for_user_with_endpoint(linux_username, endpoint)
        } else {
            Ok(RunnerClient::default())
        }
    }

    fn process_id_for_user(user_id: &str) -> String {
        format!("sldr-{}", user_id)
    }

    async fn port_for_user(&self, user_id: &str) -> Result<u16> {
        let p = self
            .user_repo
            .ensure_sldr_port(user_id, self.config.base_port, self.config.port_range)
            .await
            .context("ensuring user sldr port")?;
        let p_u16: u16 = p.try_into().context("sldr_port out of range")?;
        Ok(p_u16)
    }

    async fn wait_for_port_ready(port: u16) -> bool {
        use tokio::net::TcpStream;
        use tokio::time::{Duration, Instant, sleep};

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                return true;
            }
            sleep(Duration::from_millis(100)).await;
        }
        false
    }

    async fn spawn_sldr(&self, client: &RunnerClient, process_id: &str, port: u16) -> Result<()> {
        let mut env = std::collections::HashMap::new();
        env.insert("SLDR_SERVER_ADDR".to_string(), format!("127.0.0.1:{port}"));

        client
            .spawn_process(
                process_id,
                self.config.sldr_binary.clone(),
                Vec::new(),
                PathBuf::from("/"),
                env,
                false,
            )
            .await
            .map(|_| ())
            .context("spawning sldr via runner")
    }

    /// Ensure a per-user sldr-server instance is running and return its port.
    pub async fn ensure_user_sldr(&self, user_id: &str) -> Result<u16> {
        let mut state = self.state.lock().await;

        if let Some(inst) = state.instances.get(user_id) {
            return Ok(inst.port);
        }

        let port = self.port_for_user(user_id).await?;
        let linux_username = (self.linux_username_for_user_id)(user_id);
        let client = self.runner_client_for_linux_user(&linux_username)?;
        let process_id = Self::process_id_for_user(user_id);

        debug!("Spawning sldr for {} on port {}", user_id, port);
        self.spawn_sldr(&client, &process_id, port).await?;

        if !Self::wait_for_port_ready(port).await {
            warn!("sldr did not become ready on port {}", port);
        } else {
            info!("sldr ready on port {} for user {}", port, user_id);
        }

        state
            .instances
            .insert(user_id.to_string(), UserSldrInstance { port });
        Ok(port)
    }
}

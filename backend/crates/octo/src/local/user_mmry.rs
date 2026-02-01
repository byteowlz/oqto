//! Per-user mmry process manager for local multi-user mode.
//!
//! In local multi-user mode, each platform user gets their own mmry instance.
//! The manager first checks if mmry is already running (e.g., via systemd or
//! manual start) and reuses it. Only spawns via octo-runner if no existing
//! instance is found.
//!
//! Future: shared/remote workspaces will need separate mmry instances with
//! isolated stores.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::runner::client::RunnerClient;
use crate::user::UserRepository;

#[derive(Debug, Clone)]
pub struct UserMmryConfig {
    pub mmry_binary: String,
    pub base_port: u16,
    pub port_range: u16,
    /// Runner socket path pattern.
    /// Supports `{user}` (Linux username) and `{uid}`.
    pub runner_socket_pattern: Option<String>,
}

#[derive(Debug, Clone)]
struct UserMmryInstance {
    port: u16,
    session_count: usize,
    pinned_count: usize,
}

#[derive(Debug, Default)]
struct UserMmryState {
    instances: HashMap<String, UserMmryInstance>,
}

/// Tracks and spawns per-user mmry instances.
#[derive(Clone)]
pub struct UserMmryManager {
    config: UserMmryConfig,
    state: Arc<Mutex<UserMmryState>>,
    linux_username_for_user_id: Arc<dyn Fn(&str) -> String + Send + Sync>,
    user_repo: UserRepository,
}

impl std::fmt::Debug for UserMmryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserMmryManager")
            .field("config", &self.config)
            .finish()
    }
}

impl UserMmryManager {
    pub fn new(
        config: UserMmryConfig,
        linux_username_for_user_id: impl Fn(&str) -> String + Send + Sync + 'static,
        user_repo: UserRepository,
    ) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(UserMmryState::default())),
            linux_username_for_user_id: Arc::new(linux_username_for_user_id),
            user_repo,
        }
    }

    fn linux_user_uid(linux_username: &str) -> Result<u32> {
        let output = Command::new("id")
            .args(["-u", linux_username])
            .output()
            .with_context(|| format!("getting uid for linux user '{}'", linux_username))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "failed to get uid for linux user '{}': {}",
                linux_username,
                stderr.trim()
            );
        }

        let uid_str = String::from_utf8_lossy(&output.stdout);
        let uid = uid_str.trim().parse::<u32>().context("parsing uid")?;
        Ok(uid)
    }

    fn runner_client_for_linux_user(&self, linux_username: &str) -> Result<RunnerClient> {
        if let Some(pattern) = self.config.runner_socket_pattern.as_deref() {
            let mut socket = pattern.replace("{user}", linux_username);
            if socket.contains("{uid}") {
                let uid = Self::linux_user_uid(linux_username)?;
                socket = socket.replace("{uid}", &uid.to_string());
            }
            Ok(RunnerClient::new(socket))
        } else {
            // Fallback: same-user runner socket.
            // This only works if the backend can access the runner socket.
            Ok(RunnerClient::default())
        }
    }

    fn process_id_for_user(user_id: &str) -> String {
        format!("mmry-{}", user_id)
    }

    async fn port_for_user(&self, user_id: &str) -> Result<u16> {
        let p = self
            .user_repo
            .ensure_mmry_port(user_id, self.config.base_port, self.config.port_range)
            .await
            .context("ensuring user mmry port")?;
        let p_u16: u16 = p.try_into().context("mmry_port out of range")?;
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

    /// Check if mmry service is already running for a user and return its HTTP port.
    ///
    /// Runs `mmry service status` as the target user and parses the HTTP port from output.
    /// Returns `Some(port)` if running with HTTP enabled, `None` otherwise.
    fn check_existing_mmry_service(mmry_binary: &str, linux_username: &str) -> Option<u16> {
        // Get current username from environment
        let current_user = std::env::var("USER").unwrap_or_default();

        // Run as the target user to check their mmry service
        let output = if linux_username == current_user {
            Command::new(mmry_binary)
                .args(["service", "status"])
                .output()
        } else {
            Command::new("sudo")
                .args(["-u", linux_username, mmry_binary, "service", "status"])
                .output()
        };

        let output = match output {
            Ok(o) => o,
            Err(e) => {
                debug!("Failed to check mmry status for {}: {}", linux_username, e);
                return None;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check if service is running
        if !stdout.contains("Service is running") {
            return None;
        }

        // Parse HTTP port from output like:
        // Service is running
        //   PID: 2429525
        //   gRPC port: 45443
        //   HTTP port: 8081 (127.0.0.1:8081)
        //   Status: Healthy
        let http_port_re = Regex::new(r"HTTP port:\s*(\d+)").ok()?;
        if let Some(caps) = http_port_re.captures(&stdout)
            && let Some(port_str) = caps.get(1)
            && let Ok(port) = port_str.as_str().parse::<u16>()
        {
            info!(
                "Found existing mmry service for {} with HTTP port {}",
                linux_username, port
            );
            return Some(port);
        }

        // HTTP API not enabled or couldn't parse
        debug!(
            "mmry service running for {} but HTTP port not found in status",
            linux_username
        );
        None
    }

    async fn drain_stdout_best_effort(client: &RunnerClient, process_id: &str) -> String {
        let mut out = String::new();
        for _ in 0..16 {
            match client.read_stdout(process_id.to_string(), 0).await {
                Ok(resp) => {
                    if !resp.data.is_empty() {
                        out.push_str(&resp.data);
                    }
                    if !resp.has_more {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        out
    }

    async fn spawn_mmry(
        &self,
        client: &RunnerClient,
        process_id: &str,
        linux_username: &str,
        port: u16,
    ) -> Result<()> {
        // IMPORTANT: do not write config files here.
        // In local multi-user mode, the backend may not have permissions to write into
        // the user's home directory. Instead, we rely on mmry's existing config loading
        // (from the target user's ~/.config/mmry/config.toml) and override just the
        // per-user external API bind via environment variables.
        let _ = linux_username;

        let mut env = std::collections::HashMap::new();
        env.insert("MMRY__EXTERNAL_API__PORT".to_string(), port.to_string());
        // NOTE: Do not set MMRY__EXTERNAL_API__HOST here.
        // The mmry config loader uses `try_parsing(true)` for env values and some
        // shells/configurations can cause host parsing to fail. Port override alone
        // is sufficient; host defaults are taken from the user's config.

        client
            .spawn_rpc_process(
                process_id,
                self.config.mmry_binary.clone(),
                vec!["service".to_string(), "run".to_string()],
                PathBuf::from("/"),
                env,
                false,
            )
            .await
            .map(|_| ())
            .context("spawning mmry via runner")
    }

    /// Ensure a per-user mmry instance is running and increment refcount.
    /// Returns the port the instance is listening on.
    ///
    /// First checks if mmry is already running for the user (e.g., via systemd).
    /// Only spawns via runner if no existing instance is found.
    pub async fn ensure_user_mmry(&self, user_id: &str) -> Result<u16> {
        let mut state = self.state.lock().await;

        if let Some(inst) = state.instances.get_mut(user_id) {
            inst.session_count += 1;
            debug!(
                "Reusing user mmry for {} on port {} (sessions={})",
                user_id, inst.port, inst.session_count
            );
            return Ok(inst.port);
        }

        let linux_username = (self.linux_username_for_user_id)(user_id);

        // First, check if mmry is already running for this user with HTTP enabled
        if let Some(http_port) =
            Self::check_existing_mmry_service(&self.config.mmry_binary, &linux_username)
        {
            info!(
                "Using existing mmry service for user {} on HTTP port {}",
                user_id, http_port
            );
            state.instances.insert(
                user_id.to_string(),
                UserMmryInstance {
                    port: http_port,
                    session_count: 1,
                    pinned_count: 0,
                },
            );
            return Ok(http_port);
        }

        // No existing service, spawn via runner
        let port = self.port_for_user(user_id).await?;
        let client = self
            .runner_client_for_linux_user(&linux_username)
            .context("building runner client")?;
        let process_id = Self::process_id_for_user(user_id);

        info!(
            "Spawning mmry for user {} (linux user {}, port {}, runner socket {:?})",
            user_id,
            linux_username,
            port,
            client.socket_path()
        );

        if let Err(err) = self
            .spawn_mmry(&client, &process_id, &linux_username, port)
            .await
        {
            // Common case after backend restart: runner still has process id.
            // Don't rely on string matching; check runner status.
            match client.get_status(&process_id).await {
                Ok(status) if status.running => {
                    warn!(
                        "mmry process {} already exists for user {}, reusing (pid={:?})",
                        process_id, user_id, status.pid
                    );
                }
                Ok(_status) => {
                    // Stale process entry or crashed process; kill then respawn.
                    let _ = client.kill_process(&process_id, true).await;
                    self.spawn_mmry(&client, &process_id, &linux_username, port)
                        .await
                        .with_context(|| {
                            format!("respawning mmry for user {} after stale process", user_id)
                        })?;
                }
                Err(status_err) => {
                    return Err(err).context(format!(
                        "mmry spawn failed and status check failed: {}",
                        status_err
                    ));
                }
            }
        }

        state.instances.insert(
            user_id.to_string(),
            UserMmryInstance {
                port,
                session_count: 1,
                pinned_count: 0,
            },
        );

        drop(state);

        if !Self::wait_for_port_ready(port).await {
            // Try a one-time restart (common case: config port collision / stale process).
            let _ = client.kill_process(&process_id, true).await;
            let _ = self
                .spawn_mmry(&client, &process_id, &linux_username, port)
                .await;

            if !Self::wait_for_port_ready(port).await {
                let status = client.get_status(&process_id).await.ok();
                let logs = Self::drain_stdout_best_effort(&client, &process_id).await;
                anyhow::bail!(
                    "mmry did not become ready on port {} (status={:?})\n{}",
                    port,
                    status,
                    logs
                );
            }
        }

        Ok(port)
    }

    /// Ensure a per-user mmry instance is running and keep it pinned.
    ///
    /// This is intended for non-session use (e.g. main chat workspace memories) where we
    /// don't have a clean "stop" lifecycle. This call is idempotent per user.
    ///
    /// First checks if mmry is already running for the user (e.g., via systemd).
    /// Only spawns via runner if no existing instance is found.
    pub async fn pin_user_mmry(&self, user_id: &str) -> Result<u16> {
        let mut state = self.state.lock().await;

        if let Some(inst) = state.instances.get_mut(user_id) {
            if inst.pinned_count == 0 {
                inst.pinned_count = 1;
            }
            return Ok(inst.port);
        }

        let linux_username = (self.linux_username_for_user_id)(user_id);

        // First, check if mmry is already running for this user
        if let Some(existing_port) =
            Self::check_existing_mmry_service(&self.config.mmry_binary, &linux_username)
        {
            info!(
                "Pinning existing mmry service for user {} on port {}",
                user_id, existing_port
            );
            state.instances.insert(
                user_id.to_string(),
                UserMmryInstance {
                    port: existing_port,
                    session_count: 0,
                    pinned_count: 1,
                },
            );
            return Ok(existing_port);
        }

        // No existing service, spawn via runner
        let port = self.port_for_user(user_id).await?;
        let client = self
            .runner_client_for_linux_user(&linux_username)
            .context("building runner client")?;
        let process_id = Self::process_id_for_user(user_id);

        info!(
            "Spawning pinned mmry for user {} (linux user {}, port {}, runner socket {:?})",
            user_id,
            linux_username,
            port,
            client.socket_path()
        );

        if let Err(err) = self
            .spawn_mmry(&client, &process_id, &linux_username, port)
            .await
        {
            match client.get_status(&process_id).await {
                Ok(status) if status.running => {
                    warn!(
                        "mmry process {} already exists for user {}, pinning (pid={:?})",
                        process_id, user_id, status.pid
                    );
                }
                Ok(_status) => {
                    // Stale process entry or crashed process; kill then respawn.
                    let _ = client.kill_process(&process_id, true).await;
                    self.spawn_mmry(&client, &process_id, &linux_username, port)
                        .await
                        .with_context(|| {
                            format!(
                                "respawning pinned mmry for user {} after stale process",
                                user_id
                            )
                        })?;
                }
                Err(status_err) => {
                    return Err(err).context(format!(
                        "pinning mmry via runner failed and status check failed: {}",
                        status_err
                    ));
                }
            }
        }

        state.instances.insert(
            user_id.to_string(),
            UserMmryInstance {
                port,
                session_count: 0,
                pinned_count: 1,
            },
        );

        drop(state);

        if !Self::wait_for_port_ready(port).await {
            // Try a one-time restart.
            let _ = client.kill_process(&process_id, true).await;
            let _ = self
                .spawn_mmry(&client, &process_id, &linux_username, port)
                .await;

            if !Self::wait_for_port_ready(port).await {
                let status = client.get_status(&process_id).await.ok();
                let logs = Self::drain_stdout_best_effort(&client, &process_id).await;
                anyhow::bail!(
                    "pinned mmry did not become ready on port {} (status={:?})\n{}",
                    port,
                    status,
                    logs
                );
            }
        }

        Ok(port)
    }

    /// Decrement refcount and stop the per-user mmry instance when it reaches 0.
    pub async fn release_user_mmry(&self, user_id: &str) -> Result<()> {
        let mut state = self.state.lock().await;
        let Some(inst) = state.instances.get_mut(user_id) else {
            return Ok(());
        };

        inst.session_count = inst.session_count.saturating_sub(1);
        if inst.session_count > 0 || inst.pinned_count > 0 {
            debug!(
                "Released user mmry for {} on port {} (sessions={})",
                user_id, inst.port, inst.session_count
            );
            return Ok(());
        }

        let port = inst.port;
        state.instances.remove(user_id);

        let linux_username = (self.linux_username_for_user_id)(user_id);
        let client = self
            .runner_client_for_linux_user(&linux_username)
            .context("building runner client")?;
        let process_id = Self::process_id_for_user(user_id);

        info!(
            "Stopping mmry for user {} (linux user {}, port {})",
            user_id, linux_username, port
        );

        match client.kill_process(&process_id, false).await {
            Ok(_) => Ok(()),
            Err(e) => {
                // If it's already gone, that's fine.
                if e.to_string().contains("ProcessNotFound") {
                    Ok(())
                } else {
                    Err(e).context("stopping mmry via runner")
                }
            }
        }
    }

    // Intentionally no public "get port" API: callers should pin/ensure and use the returned port.
}

//! Per-user hstry process manager for local multi-user mode.
//!
//! In local multi-user mode, each platform user gets their own hstry instance.
//! The manager first checks if hstry is already running (e.g., via systemd or
//! manual start) and reuses it. Only spawns via oqto-runner if no existing
//! instance is found.
//!
//! Unlike mmry which uses HTTP ports, hstry uses Unix sockets at a fixed path
//! in the user's XDG_RUNTIME_DIR or state directory.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::runner::client::RunnerClient;

#[derive(Debug, Clone)]
pub struct UserHstryConfig {
    pub hstry_binary: String,
    /// Runner socket path pattern.
    /// Supports `{user}` (Linux username) and `{uid}`.
    pub runner_socket_pattern: Option<String>,
}

impl Default for UserHstryConfig {
    fn default() -> Self {
        Self {
            hstry_binary: "hstry".to_string(),
            runner_socket_pattern: None,
        }
    }
}

#[derive(Debug, Clone)]
struct UserHstryInstance {
    /// Unix socket path for this user's hstry.
    socket_path: PathBuf,
    /// Number of active sessions using this instance.
    session_count: usize,
}

#[derive(Debug, Default)]
struct UserHstryState {
    instances: HashMap<String, UserHstryInstance>,
}

/// Tracks and spawns per-user hstry instances.
#[derive(Clone)]
pub struct UserHstryManager {
    config: UserHstryConfig,
    state: Arc<Mutex<UserHstryState>>,
    linux_username_for_user_id: Arc<dyn Fn(&str) -> String + Send + Sync>,
}

impl std::fmt::Debug for UserHstryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserHstryManager")
            .field("config", &self.config)
            .finish()
    }
}

impl UserHstryManager {
    pub fn new(
        config: UserHstryConfig,
        linux_username_for_user_id: impl Fn(&str) -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(UserHstryState::default())),
            linux_username_for_user_id: Arc::new(linux_username_for_user_id),
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
            Ok(RunnerClient::default())
        }
    }

    fn process_id_for_user(user_id: &str) -> String {
        format!("hstry-{}", user_id)
    }

    /// Get the Unix socket path for a user's hstry instance.
    fn socket_path_for_user(linux_username: &str) -> PathBuf {
        // hstry uses XDG_RUNTIME_DIR if available, otherwise falls back to state dir
        // Check XDG_RUNTIME_DIR first (typically /run/user/<uid>)
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            return PathBuf::from(runtime_dir).join("hstry.sock");
        }

        // For other users, construct the path based on their uid
        if let Ok(uid) = Self::linux_user_uid(linux_username) {
            let runtime_path = PathBuf::from(format!("/run/user/{}/hstry.sock", uid));
            if runtime_path.parent().map(|p| p.exists()).unwrap_or(false) {
                return runtime_path;
            }
        }

        // Fallback to state directory
        // This would be ~/.local/state/hstry/hstry.sock for the user
        // But we can't easily determine another user's home, so use a convention
        PathBuf::from(format!(
            "/home/{}/.local/state/hstry/hstry.sock",
            linux_username
        ))
    }

    /// Check if hstry service is already running for a user.
    ///
    /// Runs `hstry service status` as the target user and checks if it's running.
    fn check_existing_hstry_service(hstry_binary: &str, linux_username: &str) -> bool {
        let current_user = std::env::var("USER").unwrap_or_default();

        let output = if linux_username == current_user {
            Command::new(hstry_binary)
                .args(["service", "status"])
                .output()
        } else {
            Command::new("sudo")
                .args(["-u", linux_username, hstry_binary, "service", "status"])
                .output()
        };

        let output = match output {
            Ok(o) => o,
            Err(e) => {
                debug!("Failed to check hstry status for {}: {}", linux_username, e);
                return false;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check for "running" in the status output
        if stdout.contains("running") && !stdout.contains("stopped") {
            info!("Found existing hstry service for {}", linux_username);
            return true;
        }

        false
    }

    async fn spawn_hstry(
        &self,
        client: &RunnerClient,
        process_id: &str,
        _linux_username: &str,
    ) -> Result<()> {
        // hstry uses its own config from the user's home directory
        // No environment overrides needed (unlike mmry which needs port override)
        let env = std::collections::HashMap::new();

        client
            .spawn_rpc_process(
                process_id,
                self.config.hstry_binary.clone(),
                vec!["service".to_string(), "run".to_string()],
                PathBuf::from("/"),
                env,
                false,
            )
            .await
            .map(|_| ())
            .context("spawning hstry via runner")
    }

    /// Wait for hstry socket to become available.
    async fn wait_for_socket_ready(socket_path: &Path) -> bool {
        use tokio::time::{Duration, Instant, sleep};

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if socket_path.exists() {
                return true;
            }
            sleep(Duration::from_millis(100)).await;
        }
        false
    }

    /// Ensure a per-user hstry instance is running and increment refcount.
    /// Returns the Unix socket path for the instance.
    ///
    /// First checks if hstry is already running for the user (e.g., via systemd).
    /// Only spawns via runner if no existing instance is found.
    pub async fn ensure_user_hstry(&self, user_id: &str) -> Result<PathBuf> {
        let mut state = self.state.lock().await;

        if let Some(inst) = state.instances.get_mut(user_id) {
            inst.session_count += 1;
            debug!(
                "Reusing user hstry for {} at {:?} (sessions={})",
                user_id, inst.socket_path, inst.session_count
            );
            return Ok(inst.socket_path.clone());
        }

        let linux_username = (self.linux_username_for_user_id)(user_id);
        let socket_path = Self::socket_path_for_user(&linux_username);

        // First, check if hstry is already running for this user
        if Self::check_existing_hstry_service(&self.config.hstry_binary, &linux_username) {
            info!(
                "Using existing hstry service for user {} at {:?}",
                user_id, socket_path
            );
            state.instances.insert(
                user_id.to_string(),
                UserHstryInstance {
                    socket_path: socket_path.clone(),
                    session_count: 1,
                },
            );
            return Ok(socket_path);
        }

        // No existing service, spawn via runner
        let client = self
            .runner_client_for_linux_user(&linux_username)
            .context("building runner client")?;
        let process_id = Self::process_id_for_user(user_id);

        info!(
            "Spawning hstry for user {} (linux user {}, runner socket {:?})",
            user_id,
            linux_username,
            client.socket_path()
        );

        if let Err(err) = self
            .spawn_hstry(&client, &process_id, &linux_username)
            .await
        {
            // Common case after backend restart: runner still has process id.
            match client.get_status(&process_id).await {
                Ok(status) if status.running => {
                    warn!(
                        "hstry process {} already exists for user {}, reusing (pid={:?})",
                        process_id, user_id, status.pid
                    );
                }
                Ok(_status) => {
                    // Stale process entry or crashed process; kill then respawn.
                    let _ = client.kill_process(&process_id, true).await;
                    self.spawn_hstry(&client, &process_id, &linux_username)
                        .await
                        .with_context(|| {
                            format!("respawning hstry for user {} after stale process", user_id)
                        })?;
                }
                Err(_) => {
                    return Err(err).context("spawning hstry via runner");
                }
            }
        }

        // Wait for socket to be ready
        if !Self::wait_for_socket_ready(&socket_path).await {
            warn!(
                "hstry socket not ready after spawn for user {} at {:?}",
                user_id, socket_path
            );
        }

        state.instances.insert(
            user_id.to_string(),
            UserHstryInstance {
                socket_path: socket_path.clone(),
                session_count: 1,
            },
        );

        info!("hstry ready for user {} at {:?}", user_id, socket_path);

        Ok(socket_path)
    }

    /// Release a per-user hstry instance (decrement refcount).
    /// Does NOT stop the daemon - it should persist across sessions.
    pub async fn release_user_hstry(&self, user_id: &str) {
        let mut state = self.state.lock().await;
        if let Some(inst) = state.instances.get_mut(user_id) {
            inst.session_count = inst.session_count.saturating_sub(1);
            debug!(
                "Released user hstry for {} (sessions={})",
                user_id, inst.session_count
            );
            // Note: We don't remove the instance or stop the daemon.
            // hstry should persist to maintain chat history across sessions.
        }
    }

    /// Get the socket path for a user's hstry instance if it's tracked.
    pub async fn get_user_hstry_socket(&self, user_id: &str) -> Option<PathBuf> {
        let state = self.state.lock().await;
        state.instances.get(user_id).map(|i| i.socket_path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path_construction() {
        let path = UserHstryManager::socket_path_for_user("testuser");
        assert!(path.to_string_lossy().contains("hstry.sock"));
    }

    #[test]
    fn test_process_id() {
        let id = UserHstryManager::process_id_for_user("user123");
        assert_eq!(id, "hstry-user123");
    }
}

//! hstry service manager for auto-starting the hstry daemon.
//!
//! In single-user mode, Octo automatically starts the hstry daemon if it's not
//! already running. The daemon runs in the background and persists until stopped.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Configuration for the hstry service manager.
#[derive(Debug, Clone)]
pub struct HstryServiceConfig {
    /// Path to the hstry binary.
    pub binary: String,
    /// Whether to auto-start the daemon if not running.
    pub auto_start: bool,
    /// Timeout for waiting for the daemon to become ready.
    pub startup_timeout: Duration,
}

impl Default for HstryServiceConfig {
    fn default() -> Self {
        Self {
            binary: "hstry".to_string(),
            auto_start: true,
            startup_timeout: Duration::from_secs(10),
        }
    }
}

/// Manages the hstry daemon lifecycle for single-user mode.
#[derive(Clone)]
pub struct HstryServiceManager {
    config: HstryServiceConfig,
    /// Child process handle (if we spawned it).
    child: Arc<Mutex<Option<Child>>>,
}

impl HstryServiceManager {
    /// Create a new service manager.
    pub fn new(config: HstryServiceConfig) -> Self {
        Self {
            config,
            child: Arc::new(Mutex::new(None)),
        }
    }

    /// Check if the hstry daemon is already running.
    ///
    /// Checks for the port file or Unix socket that hstry creates when running.
    pub fn is_running(&self) -> bool {
        // Check Unix socket first (preferred)
        let socket_path = hstry_core::paths::service_socket_path();
        if socket_path.exists() {
            return true;
        }

        // Check TCP port file
        let port_path = hstry_core::paths::service_port_path();
        if port_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&port_path) {
                if content.trim().parse::<u16>().is_ok() {
                    return true;
                }
            }
        }

        false
    }

    /// Ensure the hstry daemon is running.
    ///
    /// If already running, returns immediately.
    /// If not running and auto_start is enabled, spawns the daemon.
    pub async fn ensure_running(&self) -> Result<()> {
        if self.is_running() {
            debug!("hstry daemon already running");
            return Ok(());
        }

        if !self.config.auto_start {
            anyhow::bail!(
                "hstry daemon is not running and auto_start is disabled. \
                 Start it manually with: hstry service start"
            );
        }

        self.start().await
    }

    /// Start the hstry daemon.
    async fn start(&self) -> Result<()> {
        info!("Starting hstry daemon...");

        let mut cmd = Command::new(&self.config.binary);
        cmd.args(["service", "run"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        // Spawn as a detached process
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Create new process group so it survives parent exit
            unsafe {
                cmd.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }

        let child = cmd.spawn().context("Failed to spawn hstry daemon")?;

        let pid = child.id();
        info!("hstry daemon spawned with PID {:?}", pid);

        // Store the child handle
        *self.child.lock().await = Some(child);

        // Wait for the daemon to become ready
        self.wait_for_ready().await?;

        info!("hstry daemon is ready");
        Ok(())
    }

    /// Wait for the daemon to become ready (socket/port file exists).
    async fn wait_for_ready(&self) -> Result<()> {
        let start = std::time::Instant::now();
        let check_interval = Duration::from_millis(100);

        while start.elapsed() < self.config.startup_timeout {
            if self.is_running() {
                return Ok(());
            }
            tokio::time::sleep(check_interval).await;
        }

        anyhow::bail!(
            "hstry daemon did not become ready within {:?}",
            self.config.startup_timeout
        )
    }

    /// Stop the hstry daemon if we started it.
    pub async fn stop(&self) -> Result<()> {
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            info!("Stopping hstry daemon...");
            child.kill().await.context("Failed to kill hstry daemon")?;
            child
                .wait()
                .await
                .context("Failed to wait for hstry daemon")?;
            info!("hstry daemon stopped");
        }
        Ok(())
    }
}

impl Drop for HstryServiceManager {
    fn drop(&mut self) {
        // Note: We don't stop the daemon on drop because it should persist
        // across Octo restarts. The daemon manages its own lifecycle.
        // Use stop() explicitly if needed.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = HstryServiceConfig::default();
        assert_eq!(config.binary, "hstry");
        assert!(config.auto_start);
    }
}

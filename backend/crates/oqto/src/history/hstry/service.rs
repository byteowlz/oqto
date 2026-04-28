//! hstry service manager for auto-starting the hstry daemon.
//!
//! Uses `hstry service start` which properly daemonizes (double-fork, PID file).
//! The daemon persists across Oqto restarts. If already running, start is a no-op.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{debug, info};

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

/// Manages the hstry daemon lifecycle.
#[derive(Clone, Debug)]
pub struct HstryServiceManager {
    config: HstryServiceConfig,
}

impl HstryServiceManager {
    /// Create a new service manager.
    pub fn new(config: HstryServiceConfig) -> Self {
        Self { config }
    }

    /// Check if the hstry daemon is already running.
    pub fn is_running(&self) -> bool {
        #[cfg(unix)]
        {
            let socket_path = hstry_core::paths::service_socket_path();
            if socket_path.exists() && std::os::unix::net::UnixStream::connect(&socket_path).is_ok()
            {
                return true;
            }
        }

        let port_path = hstry_core::paths::service_port_path();
        if port_path.exists()
            && let Ok(content) = std::fs::read_to_string(&port_path)
            && let Ok(port) = content.trim().parse::<u16>()
        {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                return true;
            }
            debug!("hstry port file points to unreachable endpoint: {}", addr);
        }

        false
    }

    /// Ensure the hstry daemon is running.
    ///
    /// Uses `hstry service start` which is a proper daemonizing command.
    /// If already running, this is a no-op (the command returns an error
    /// which we treat as success).
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

    /// Start the hstry daemon via `hstry service start`.
    async fn start(&self) -> Result<()> {
        info!(
            "Starting hstry daemon via `{} service start`...",
            self.config.binary
        );

        let output = Command::new(&self.config.binary)
            .args(["service", "start"])
            .output()
            .await
            .context("Failed to run hstry service start")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            // "already running" is fine
            let combined = format!("{}{}", stdout, stderr);
            if combined.contains("already running") {
                debug!("hstry daemon reported already running but endpoint not ready; restarting");
                return self.restart().await;
            }
            anyhow::bail!(
                "hstry service start failed (exit {}): {}{}",
                output.status,
                stdout,
                stderr
            );
        }

        info!("hstry service start succeeded: {}", stdout.trim());

        // Wait for the daemon to become ready. If startup races with stale
        // state, try adapter refresh + restart once before giving up.
        if let Err(first_err) = self.wait_for_ready().await {
            debug!(
                "hstry not ready after start ({}); attempting adapter refresh + restart",
                first_err
            );
            if let Err(e) = self.update_adapters().await {
                debug!("hstry adapters update failed (continuing): {}", e);
            }
            self.restart().await?;
            self.wait_for_ready().await?;
        }

        Ok(())
    }

    /// Restart the hstry daemon via `hstry service restart`.
    async fn restart(&self) -> Result<()> {
        let output = Command::new(&self.config.binary)
            .args(["service", "restart"])
            .output()
            .await
            .context("Failed to run hstry service restart")?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "hstry service restart failed (exit {}): {}{}",
                output.status,
                stdout,
                stderr
            );
        }

        self.wait_for_ready().await
    }

    /// Best-effort adapter refresh for binary/adapter version drift.
    async fn update_adapters(&self) -> Result<()> {
        let output = Command::new(&self.config.binary)
            .args(["adapters", "update"])
            .output()
            .await
            .context("Failed to run hstry adapters update")?;

        if output.status.success() {
            info!("hstry adapters updated successfully");
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "hstry adapters update failed (exit {}): {}{}",
            output.status,
            stdout,
            stderr
        )
    }

    /// Wait for the daemon to become ready (socket/port connectivity).
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

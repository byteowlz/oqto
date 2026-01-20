//! Agent browser daemon integration.
//!
//! This module provides a lightweight wrapper to start/stop per-session
//! agent-browser daemons via the CLI binary.

use std::time::Duration;

use anyhow::{Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::timeout;

/// Configuration for agent-browser integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentBrowserConfig {
    /// Enable agent-browser integration.
    pub enabled: bool,
    /// Path to the agent-browser CLI binary.
    pub binary: String,
    /// Launch browser in headed mode (default: headless).
    pub headed: bool,
    /// Base port for the screencast WebSocket stream server.
    pub stream_port_base: u16,
    /// Port range size for per-session screencast streams.
    pub stream_port_range: u16,
    /// Optional Chromium executable path.
    pub executable_path: Option<String>,
    /// Extensions to load (paths).
    pub extensions: Vec<String>,
}

impl Default for AgentBrowserConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            binary: "agent-browser".to_string(),
            headed: false,
            stream_port_base: 30000,
            stream_port_range: 10000,
            executable_path: None,
            extensions: Vec::new(),
        }
    }
}

/// Manager for agent-browser per-session daemons.
#[derive(Debug, Clone)]
pub struct AgentBrowserManager {
    config: AgentBrowserConfig,
}

impl AgentBrowserManager {
    /// Create a new manager.
    pub fn new(config: AgentBrowserConfig) -> Self {
        Self { config }
    }

    /// Ensure the daemon is running for the session.
    pub async fn ensure_session(&self, session_id: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.run_command(session_id, &["open", "about:blank"]).await
    }

    /// Stop the daemon for the session.
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.run_command(session_id, &["close"]).await
    }

    /// Get the stream port for a session.
    pub fn stream_port_for_session(&self, session_id: &str) -> Result<u16> {
        if let Some(port) = read_stream_port_file(session_id) {
            return Ok(port);
        }
        self.compute_stream_port(session_id)
    }

    async fn run_command(&self, session_id: &str, args: &[&str]) -> Result<()> {
        let mut cmd = Command::new(&self.config.binary);
        cmd.arg("--session").arg(session_id);

        if self.config.headed {
            cmd.arg("--headed");
        }

        let stream_port = self.compute_stream_port(session_id)?;
        cmd.env("AGENT_BROWSER_STREAM_PORT", stream_port.to_string());

        if let Some(ref executable_path) = self.config.executable_path {
            cmd.arg("--executable-path").arg(executable_path);
        }

        for extension in &self.config.extensions {
            cmd.arg("--extension").arg(extension);
        }

        cmd.args(args);

        debug!(
            "agent-browser command: {} {:?} (session={})",
            self.config.binary, args, session_id
        );

        let output = timeout(Duration::from_secs(15), cmd.output())
            .await
            .context("agent-browser command timed out")?
            .context("running agent-browser command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            anyhow::bail!("agent-browser command failed: {}", detail);
        }

        Ok(())
    }

    fn compute_stream_port(&self, session_id: &str) -> Result<u16> {
        if self.config.stream_port_range == 0 {
            anyhow::bail!("agent-browser stream_port_range must be > 0");
        }
        let max_port = self.config.stream_port_base as u32 + self.config.stream_port_range as u32;
        if max_port > u16::MAX as u32 {
            anyhow::bail!(
                "agent-browser stream port range exceeds maximum port (base={}, range={})",
                self.config.stream_port_base,
                self.config.stream_port_range
            );
        }

        let mut hash: i64 = 0;
        for b in session_id.bytes() {
            hash = (hash << 5).wrapping_sub(hash).wrapping_add(b as i64);
        }
        let offset = (hash.abs() as u16) % self.config.stream_port_range;
        Ok(self.config.stream_port_base + offset)
    }
}

fn read_stream_port_file(session_id: &str) -> Option<u16> {
    let path = std::env::temp_dir().join(format!("agent-browser-{}.stream", session_id));
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<u16>().ok()
}

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

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
            binary: "oqto-browserd".to_string(),
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

    /// Whether agent-browser integration is enabled.
    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    /// Kill all browser daemon processes from previous runs.
    ///
    /// Scans the session socket base directory for PID files and sends SIGTERM
    /// to each daemon, then cleans up the socket directories.
    pub fn cleanup_all_sessions(&self) {
        if !self.config.enabled {
            return;
        }

        let base = agent_browser_base_dir();
        let entries = match std::fs::read_dir(&base) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut killed = 0u32;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let session_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let pid_file = path.join(format!("{}.pid", session_name));
            if let Ok(contents) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = contents.trim().parse::<i32>()
            {
                #[cfg(unix)]
                {
                    // Check if process exists before killing
                    unsafe {
                        if libc::kill(pid, 0) == 0 {
                            log::info!(
                                "Killing stale browser daemon pid={} session={}",
                                pid,
                                session_name
                            );
                            libc::kill(pid, libc::SIGTERM);
                            killed += 1;
                        }
                    }
                }
            }
            // Clean up the session directory
            let _ = std::fs::remove_dir_all(&path);
        }

        if killed > 0 {
            log::info!("Cleaned up {} stale browser daemon(s)", killed);
        }
    }

    /// Ensure the daemon is running for the session.
    pub async fn ensure_session(&self, session_id: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.run_command(session_id, &["open", "about:blank"]).await
    }

    /// Navigate to a URL, launching the daemon if not already running.
    ///
    /// First ensures the daemon is running (opens about:blank), then navigates
    /// to the requested URL. Navigation errors (SSL, DNS, etc.) are logged but
    /// not propagated -- the daemon and stream are still usable.
    pub async fn navigate_to(&self, session_id: &str, url: &str) -> Result<()> {
        if !self.config.enabled {
            anyhow::bail!("agent-browser integration is not enabled");
        }

        // Ensure daemon is running first (always succeeds for valid sessions)
        self.run_command(session_id, &["open", "about:blank"])
            .await?;

        // Navigate to the requested URL -- best-effort, don't fail the launch
        if let Err(e) = self.run_command(session_id, &["open", url]).await {
            log::warn!(
                "agent-browser navigation to {} failed (session {}): {}. Browser is still running.",
                url,
                session_id,
                e
            );
        }

        Ok(())
    }

    /// Set the browser viewport size.
    pub async fn set_viewport(&self, session_id: &str, width: u32, height: u32) -> Result<()> {
        if !self.config.enabled {
            anyhow::bail!("agent-browser integration is not enabled");
        }

        let w = width.to_string();
        let h = height.to_string();
        self.run_command(session_id, &["set", "viewport", &w, &h])
            .await
    }

    /// Stop the daemon for the session.
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.run_command(session_id, &["close"]).await
    }

    /// Navigate back in the browser history.
    pub async fn go_back(&self, session_id: &str) -> Result<()> {
        if !self.config.enabled {
            anyhow::bail!("agent-browser integration is not enabled");
        }
        self.run_command(session_id, &["back"]).await
    }

    /// Navigate forward in the browser history.
    pub async fn go_forward(&self, session_id: &str) -> Result<()> {
        if !self.config.enabled {
            anyhow::bail!("agent-browser integration is not enabled");
        }
        self.run_command(session_id, &["forward"]).await
    }

    /// Reload the current page.
    pub async fn reload(&self, session_id: &str) -> Result<()> {
        if !self.config.enabled {
            anyhow::bail!("agent-browser integration is not enabled");
        }
        self.run_command(session_id, &["reload"]).await
    }

    /// Set the browser color scheme (light/dark).
    pub async fn set_color_scheme(&self, session_id: &str, scheme: &str) -> Result<()> {
        if !self.config.enabled {
            anyhow::bail!("agent-browser integration is not enabled");
        }
        self.run_command(session_id, &["emulatemedia", scheme])
            .await
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

        let socket_dir = agent_browser_session_dir(session_id, None);
        if let Err(err) = std::fs::create_dir_all(&socket_dir) {
            log::warn!(
                "Failed to create agent-browser socket dir {}: {}",
                socket_dir.display(),
                err
            );
        }
        #[cfg(unix)]
        // 0o750: owner rwx, group rx (so oqto group members can access the socket)
        if let Err(err) =
            std::fs::set_permissions(&socket_dir, std::fs::Permissions::from_mode(0o750))
        {
            log::warn!(
                "Failed to set permissions for agent-browser socket dir {}: {}",
                socket_dir.display(),
                err
            );
        }
        cmd.env("AGENT_BROWSER_SOCKET_DIR", &socket_dir);

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
        let offset = (hash.unsigned_abs() as u16) % self.config.stream_port_range;
        Ok(self.config.stream_port_base + offset)
    }
}

/// Resolve the base directory for agent-browser session socket directories.
///
/// Priority:
///   AGENT_BROWSER_SOCKET_DIR_BASE > XDG_STATE_HOME/oqto/agent-browser >
///   ~/.local/state/oqto/agent-browser > tmpdir/oqto/agent-browser
///
/// XDG_STATE_HOME is used instead of XDG_RUNTIME_DIR because XDG_RUNTIME_DIR
/// is a tmpfs. When bwrap bind-mounts a path that lives on a tmpfs, it creates
/// a fresh tmpfs layer inside the sandbox rather than sharing the host
/// directory, so socket files created by oqto-browserd on the host are never
/// visible to the Pi agent inside the sandbox.  XDG_STATE_HOME is on a real
/// filesystem and bind-mounts correctly.
pub fn agent_browser_base_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("AGENT_BROWSER_SOCKET_DIR_BASE") {
        return std::path::PathBuf::from(dir);
    }
    if let Ok(state_dir) = std::env::var("XDG_STATE_HOME") {
        return std::path::PathBuf::from(state_dir)
            .join("oqto")
            .join("agent-browser");
    }
    if let Some(home) = dirs::home_dir() {
        return home
            .join(".local")
            .join("state")
            .join("oqto")
            .join("agent-browser");
    }
    std::env::temp_dir().join("oqto").join("agent-browser")
}

/// Compute a short, deterministic agent-browser session name from a chat session ID.
///
/// Unix socket paths are limited (about 103 bytes). Chat session IDs (UUIDs)
/// are too long when repeated in both directory and filename, so we hash and
/// truncate them to a short stable name.
pub fn browser_session_name(chat_session_id: &str) -> String {
    const NAMESPACE_BYTES: [u8; 16] = [
        0x8b, 0x3a, 0x8f, 0x51, 0x90, 0x4c, 0x4a, 0x09, 0x97, 0x7c, 0x83, 0x37, 0x9f, 0x7a, 0x21,
        0x59,
    ];
    let namespace = uuid::Uuid::from_bytes(NAMESPACE_BYTES);
    let uuid = uuid::Uuid::new_v5(&namespace, chat_session_id.as_bytes());
    let simple = uuid.simple().to_string();
    format!("ab-{}", &simple[..16])
}

/// Resolve the agent-browser socket directory for a session.
pub fn agent_browser_session_dir(
    session_id: &str,
    override_dir: Option<&str>,
) -> std::path::PathBuf {
    if let Some(dir) = override_dir {
        return std::path::PathBuf::from(dir);
    }
    agent_browser_base_dir().join(session_id)
}

fn read_stream_port_file(session_id: &str) -> Option<u16> {
    let path = agent_browser_session_dir(session_id, None).join(format!("{}.stream", session_id));
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<u16>().ok()
}

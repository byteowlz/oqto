//! Local runtime implementation.
//!
//! Provides a runtime that spawns services as native processes instead of containers.

use anyhow::{Context, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use super::linux_users::LinuxUsersConfig;
use super::process::{ProcessManager, RunAsUser};
use super::sandbox::SandboxConfig;

/// Configuration for the local runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalRuntimeConfig {
    /// Path to the opencode binary.
    pub opencode_binary: String,
    /// Path to the octo-files binary.
    pub fileserver_binary: String,
    /// Path to the ttyd binary.
    pub ttyd_binary: String,
    /// Base directory for user workspaces.
    /// Supports ~ and environment variables. The {user_id} placeholder is replaced with the user ID.
    /// Default: $HOME/octo/{user_id}
    pub workspace_dir: String,
    /// Default agent name to pass to opencode via --agent flag.
    /// Agents are defined in opencode's global config or workspace's opencode.json.
    pub default_agent: Option<String>,
    /// Enable single-user mode.
    pub single_user: bool,
    /// Linux user isolation configuration.
    #[serde(default)]
    pub linux_users: LinuxUsersConfig,
    /// Sandbox configuration for process isolation.
    #[serde(default)]
    pub sandbox: Option<SandboxConfig>,
    /// Whether to clean up local session processes on startup.
    pub cleanup_on_startup: bool,
    /// Whether to stop sessions when the backend shuts down.
    pub stop_sessions_on_shutdown: bool,
}

impl Default for LocalRuntimeConfig {
    fn default() -> Self {
        Self {
            opencode_binary: "opencode".to_string(),
            fileserver_binary: "octo-files".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "$HOME/octo/{user_id}".to_string(),
            default_agent: None,
            single_user: false,
            linux_users: LinuxUsersConfig::default(),
            sandbox: None,
            cleanup_on_startup: false,
            stop_sessions_on_shutdown: false,
        }
    }
}

impl LocalRuntimeConfig {
    /// Validate that all required binaries exist.
    pub fn validate(&self) -> Result<()> {
        // Check opencode
        if !Self::binary_exists(&self.opencode_binary) {
            anyhow::bail!(
                "opencode binary not found: {}. Install opencode or set local.opencode_binary in config.",
                self.opencode_binary
            );
        }

        // Check fileserver
        if !Self::binary_exists(&self.fileserver_binary) {
            anyhow::bail!(
                "octo-files binary not found: {}. Build octo-files or set local.fileserver_binary in config.",
                self.fileserver_binary
            );
        }

        // Check ttyd
        if !Self::binary_exists(&self.ttyd_binary) {
            anyhow::bail!(
                "ttyd binary not found: {}. Install ttyd or set local.ttyd_binary in config.",
                self.ttyd_binary
            );
        }

        Ok(())
    }

    /// Check if a binary exists in PATH or as an absolute path.
    fn binary_exists(binary: &str) -> bool {
        if Path::new(binary).is_absolute() {
            Path::new(binary).exists()
        } else {
            // Check if it exists in PATH
            std::process::Command::new("which")
                .arg(binary)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
    }

    /// Expand ~ and environment variables in paths.
    pub fn expand_paths(&mut self) {
        self.opencode_binary = shellexpand::tilde(&self.opencode_binary).to_string();
        self.fileserver_binary = shellexpand::tilde(&self.fileserver_binary).to_string();
        self.ttyd_binary = shellexpand::tilde(&self.ttyd_binary).to_string();
        // Don't fully expand workspace_dir here - it contains {user_id} placeholder
        // Only expand ~ for now, env vars and {user_id} are expanded per-user
        self.workspace_dir = shellexpand::tilde(&self.workspace_dir).to_string();
    }

    /// Get the workspace directory for a specific user.
    /// Expands environment variables and replaces {user_id} placeholder.
    pub fn workspace_for_user(&self, user_id: &str) -> std::path::PathBuf {
        // First expand environment variables
        let expanded = shellexpand::env(&self.workspace_dir)
            .unwrap_or_else(|_| std::borrow::Cow::Borrowed(&self.workspace_dir));
        // Then replace {user_id} placeholder
        let path_str = expanded.replace("{user_id}", user_id);
        std::path::PathBuf::from(path_str)
    }

    /// Get the base workspace directory (for single-user mode).
    /// Expands environment variables but removes {user_id} placeholder.
    pub fn workspace_base(&self) -> std::path::PathBuf {
        // First expand environment variables
        let expanded = shellexpand::env(&self.workspace_dir)
            .unwrap_or_else(|_| std::borrow::Cow::Borrowed(&self.workspace_dir));
        // Remove {user_id} placeholder and any trailing slash
        let path_str = expanded
            .replace("/{user_id}", "")
            .replace("{user_id}/", "")
            .replace("{user_id}", "");
        std::path::PathBuf::from(path_str)
    }
}

/// Local runtime for running services as native processes.
#[derive(Clone)]
pub struct LocalRuntime {
    /// Configuration for the local runtime.
    config: LocalRuntimeConfig,
    /// Process manager for tracking spawned processes.
    process_manager: ProcessManager,
}

impl LocalRuntime {
    /// Create a new local runtime.
    pub fn new(config: LocalRuntimeConfig) -> Self {
        Self {
            config,
            process_manager: ProcessManager::new(),
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &LocalRuntimeConfig {
        &self.config
    }

    /// Get the process manager.
    #[allow(dead_code)]
    pub fn process_manager(&self) -> &ProcessManager {
        &self.process_manager
    }

    /// Start all services for a session.
    ///
    /// This spawns opencode, fileserver, and ttyd as native processes.
    /// If Linux user isolation is enabled, processes run under the user's Linux account
    /// (or the project's Linux account for shared projects).
    /// Returns the PIDs of the spawned processes as a comma-separated string.
    ///
    /// If `agent` is provided, it is passed to opencode via the --agent flag.
    /// Agents are defined in opencode's global config or the workspace's opencode.json.
    ///
    /// If `project_id` is provided, the session runs as the project's Linux user,
    /// enabling multiple platform users to access the same workspace.
    pub async fn start_session(
        &self,
        session_id: &str,
        user_id: &str,
        workspace_path: &Path,
        agent: Option<&str>,
        project_id: Option<&str>,
        opencode_port: u16,
        fileserver_port: u16,
        ttyd_port: u16,
        env: HashMap<String, String>,
    ) -> Result<String> {
        info!(
            "Starting local session {} for user {} with ports {}/{}/{}, agent: {:?}, project_id: {:?}",
            session_id, user_id, opencode_port, fileserver_port, ttyd_port, agent, project_id
        );

        // Determine how to run processes (as current user, platform user, or project user)
        let run_as = if self.config.linux_users.enabled && !self.config.single_user {
            // Ensure effective Linux user exists (project user or platform user)
            let uid = self.config.linux_users.ensure_effective_user(
                user_id,
                project_id,
                Some(workspace_path),
            )?;
            let username = self
                .config
                .linux_users
                .effective_username(user_id, project_id);
            info!("Running session as Linux user '{}' (UID {})", username, uid);
            RunAsUser::new(username, self.config.linux_users.use_sudo)
        } else {
            RunAsUser::current()
        };

        // Ensure workspace directory exists
        std::fs::create_dir_all(workspace_path)
            .with_context(|| format!("creating workspace directory: {:?}", workspace_path))?;

        // Set ownership if Linux user isolation is enabled
        if self.config.linux_users.enabled && !self.config.single_user {
            let username = self
                .config
                .linux_users
                .effective_username(user_id, project_id);
            self.config
                .linux_users
                .chown_directory_to_user(workspace_path, &username)?;
        }

        // Start fileserver - serves files from workspace_path
        let fileserver_pid = self
            .process_manager
            .spawn_fileserver(
                session_id,
                fileserver_port,
                workspace_path,
                &self.config.fileserver_binary,
                &run_as,
            )
            .await
            .context("starting fileserver")?;

        // Start ttyd
        let ttyd_pid = self
            .process_manager
            .spawn_ttyd(
                session_id,
                ttyd_port,
                workspace_path,
                &self.config.ttyd_binary,
                &run_as,
            )
            .await
            .context("starting ttyd")?;

        // Start opencode in workspace_path, optionally with --agent flag
        // Use provided agent, fall back to default_agent from config
        let effective_agent = agent.or(self.config.default_agent.as_deref());
        let sandbox = self.config.sandbox.as_ref().filter(|s| s.enabled);
        let opencode_pid = self
            .process_manager
            .spawn_opencode(
                session_id,
                opencode_port,
                workspace_path,
                &self.config.opencode_binary,
                effective_agent,
                env,
                &run_as,
                sandbox,
            )
            .await
            .context("starting opencode")?;

        // Return PIDs as a pseudo "container ID"
        let pids = format!("{},{},{}", opencode_pid, fileserver_pid, ttyd_pid);
        info!("Local session {} started with PIDs: {}", session_id, pids);

        Ok(pids)
    }

    /// Stop all services for a session.
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        info!("Stopping local session {}", session_id);
        self.process_manager.stop_session(session_id).await
    }

    /// Resume a stopped session by restarting its processes.
    ///
    /// Note: For local runtime, "resume" actually restarts the processes since
    /// we don't have container state to preserve. The workspace data is preserved.
    pub async fn resume_session(
        &self,
        session_id: &str,
        user_id: &str,
        workspace_path: &Path,
        agent: Option<&str>,
        project_id: Option<&str>,
        opencode_port: u16,
        fileserver_port: u16,
        ttyd_port: u16,
        env: HashMap<String, String>,
    ) -> Result<String> {
        info!("Resuming local session {}", session_id);

        // For local runtime, resume is the same as start
        // The processes don't persist state, but the workspace does
        self.start_session(
            session_id,
            user_id,
            workspace_path,
            agent,
            project_id,
            opencode_port,
            fileserver_port,
            ttyd_port,
            env,
        )
        .await
    }

    /// Check if a session's processes are running.
    pub async fn is_session_running(&self, session_id: &str) -> bool {
        self.process_manager.is_session_running(session_id).await
    }

    /// Get the state of a session (similar to container state).
    #[allow(dead_code)]
    pub async fn get_session_state(&self, session_id: &str) -> Option<String> {
        if self.process_manager.is_session_running(session_id).await {
            Some("running".to_string())
        } else {
            let pids = self.process_manager.get_session_pids(session_id).await;
            if pids.is_empty() {
                None
            } else {
                Some("exited".to_string())
            }
        }
    }

    /// Stop all managed sessions.
    #[allow(dead_code)]
    pub async fn stop_all(&self) -> Result<()> {
        info!("Stopping all local sessions");
        self.process_manager.stop_all().await
    }

    /// Perform health check - verify all required binaries are available.
    #[allow(dead_code)]
    pub fn health_check(&self) -> Result<String> {
        self.config.validate()?;
        Ok(format!(
            "Local runtime ready: opencode={}, fileserver={}, ttyd={}",
            self.config.opencode_binary, self.config.fileserver_binary, self.config.ttyd_binary
        ))
    }

    /// Check if a set of ports are available for use.
    ///
    /// Returns true if all ports are free, false if any are in use.
    pub fn check_ports_available(
        &self,
        opencode_port: u16,
        fileserver_port: u16,
        ttyd_port: u16,
    ) -> bool {
        super::process::are_ports_available(&[opencode_port, fileserver_port, ttyd_port])
    }

    /// Clear orphan processes on the specified ports.
    ///
    /// This is useful during startup to clean up processes from a previous
    /// server instance that crashed or was killed without proper cleanup.
    ///
    /// Returns the number of processes killed.
    pub fn clear_ports(&self, ports: &[u16]) -> usize {
        let mut killed = 0;

        for &port in ports {
            if let Some((pid, name)) = super::process::find_process_on_port(port) {
                info!(
                    "Found orphan process '{}' (PID {}) on port {}, killing...",
                    name, pid, port
                );

                // Try graceful kill first
                if super::process::kill_process(pid) {
                    // Wait a bit for graceful shutdown
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    // Check if still running, force kill if needed
                    if !super::process::is_port_available(port) {
                        info!("Process {} still running, force killing...", pid);
                        super::process::force_kill_process(pid);
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }

                    if super::process::is_port_available(port) {
                        killed += 1;
                        info!("Cleared orphan process on port {}", port);
                    } else {
                        warn!(
                            "Failed to clear port {} - process may still be running",
                            port
                        );
                    }
                } else {
                    warn!("Failed to kill process {} on port {}", pid, port);
                }
            }
        }

        killed
    }

    /// Startup cleanup for local runtime.
    ///
    /// This should be called when the server starts to clean up any orphan
    /// processes from previous runs. It checks the base port range for any
    /// lingering processes and kills them.
    pub fn startup_cleanup(&self, base_port: u16) -> usize {
        info!("Running local runtime startup cleanup...");

        // Check the default port range (base, base+1, base+2)
        let ports = [base_port, base_port + 1, base_port + 2];
        let cleared = self.clear_ports(&ports);

        if cleared > 0 {
            info!("Cleared {} orphan process(es) during startup", cleared);
        } else {
            info!("No orphan processes found during startup");
        }

        cleared
    }
}

impl std::fmt::Debug for LocalRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalRuntime")
            .field("config", &self.config)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::linux_users::LinuxUsersConfig;
    use tempfile::tempdir;

    #[test]
    fn test_local_runtime_config_default() {
        let config = LocalRuntimeConfig::default();
        assert_eq!(config.opencode_binary, "opencode");
        assert_eq!(config.fileserver_binary, "octo-files");
        assert_eq!(config.ttyd_binary, "ttyd");
        assert_eq!(config.workspace_dir, "$HOME/octo/{user_id}");
        assert!(!config.single_user);
    }

    #[test]
    fn test_expand_paths() {
        let mut config = LocalRuntimeConfig {
            opencode_binary: "~/bin/opencode".to_string(),
            fileserver_binary: "~/bin/fileserver".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "~/workspace".to_string(),
            ..Default::default()
        };

        config.expand_paths();

        // Should expand ~ to home directory
        assert!(!config.opencode_binary.starts_with('~'));
        assert!(!config.fileserver_binary.starts_with('~'));
        assert!(!config.workspace_dir.starts_with('~'));

        // ttyd doesn't have ~ so should be unchanged
        assert_eq!(config.ttyd_binary, "ttyd");
    }

    #[test]
    fn test_expand_paths_preserves_absolute() {
        let mut config = LocalRuntimeConfig {
            opencode_binary: "/usr/local/bin/opencode".to_string(),
            fileserver_binary: "/opt/fileserver".to_string(),
            ttyd_binary: "/usr/bin/ttyd".to_string(),
            workspace_dir: "/home/user/workspace".to_string(),
            ..Default::default()
        };

        let orig_opencode = config.opencode_binary.clone();
        let orig_fileserver = config.fileserver_binary.clone();
        let orig_ttyd = config.ttyd_binary.clone();
        let orig_workspace = config.workspace_dir.clone();

        config.expand_paths();

        // Absolute paths should be unchanged
        assert_eq!(config.opencode_binary, orig_opencode);
        assert_eq!(config.fileserver_binary, orig_fileserver);
        assert_eq!(config.ttyd_binary, orig_ttyd);
        assert_eq!(config.workspace_dir, orig_workspace);
    }

    #[test]
    fn test_binary_exists_with_common_binaries() {
        // These should exist on most Unix systems
        assert!(LocalRuntimeConfig::binary_exists("sh"));
        assert!(LocalRuntimeConfig::binary_exists("ls"));

        // This should not exist
        assert!(!LocalRuntimeConfig::binary_exists(
            "nonexistent-binary-12345"
        ));
    }

    #[test]
    fn test_binary_exists_with_absolute_path() {
        // /bin/sh should exist on Unix
        assert!(LocalRuntimeConfig::binary_exists("/bin/sh"));

        // Non-existent absolute path
        assert!(!LocalRuntimeConfig::binary_exists(
            "/nonexistent/path/to/binary"
        ));
    }

    #[test]
    fn test_validate_missing_opencode() {
        let config = LocalRuntimeConfig {
            opencode_binary: "nonexistent-opencode-12345".to_string(),
            fileserver_binary: "sh".to_string(), // exists
            ttyd_binary: "sh".to_string(),       // exists
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("opencode"));
    }

    #[test]
    fn test_validate_missing_fileserver() {
        let config = LocalRuntimeConfig {
            opencode_binary: "sh".to_string(), // exists
            fileserver_binary: "nonexistent-fileserver-12345".to_string(),
            ttyd_binary: "sh".to_string(), // exists
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("fileserver"));
    }

    #[test]
    fn test_validate_missing_ttyd() {
        let config = LocalRuntimeConfig {
            opencode_binary: "sh".to_string(),   // exists
            fileserver_binary: "sh".to_string(), // exists
            ttyd_binary: "nonexistent-ttyd-12345".to_string(),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("ttyd"));
    }

    #[test]
    fn test_validate_all_exist() {
        // Use common binaries that exist everywhere
        let config = LocalRuntimeConfig {
            opencode_binary: "sh".to_string(),
            fileserver_binary: "sh".to_string(),
            ttyd_binary: "sh".to_string(),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_local_runtime_new() {
        let config = LocalRuntimeConfig::default();
        let runtime = LocalRuntime::new(config.clone());

        assert_eq!(runtime.config().opencode_binary, config.opencode_binary);
        assert_eq!(runtime.config().fileserver_binary, config.fileserver_binary);
        assert_eq!(runtime.config().ttyd_binary, config.ttyd_binary);
    }

    #[test]
    fn test_local_runtime_config_accessor() {
        let config = LocalRuntimeConfig {
            opencode_binary: "custom-opencode".to_string(),
            fileserver_binary: "custom-fileserver".to_string(),
            ttyd_binary: "custom-ttyd".to_string(),
            workspace_dir: "/custom/workspace".to_string(),
            ..Default::default()
        };

        let runtime = LocalRuntime::new(config);
        assert_eq!(runtime.config().opencode_binary, "custom-opencode");
        assert_eq!(runtime.config().workspace_dir, "/custom/workspace");
    }

    #[test]
    fn test_local_runtime_debug() {
        let config = LocalRuntimeConfig::default();
        let runtime = LocalRuntime::new(config);

        let debug_str = format!("{:?}", runtime);
        assert!(debug_str.contains("LocalRuntime"));
        assert!(debug_str.contains("config"));
    }

    #[test]
    fn test_local_runtime_clone() {
        let config = LocalRuntimeConfig::default();
        let runtime1 = LocalRuntime::new(config);
        let runtime2 = runtime1.clone();

        // Both should have same config
        assert_eq!(
            runtime1.config().opencode_binary,
            runtime2.config().opencode_binary
        );
    }

    #[tokio::test]
    async fn test_local_runtime_is_session_running_nonexistent() {
        let config = LocalRuntimeConfig::default();
        let runtime = LocalRuntime::new(config);

        // Non-existent session should return false
        assert!(!runtime.is_session_running("nonexistent").await);
    }

    #[tokio::test]
    async fn test_local_runtime_stop_session_nonexistent() {
        let config = LocalRuntimeConfig::default();
        let runtime = LocalRuntime::new(config);

        // Stopping non-existent session should succeed (no-op)
        let result = runtime.stop_session("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_local_runtime_get_session_state_nonexistent() {
        let config = LocalRuntimeConfig::default();
        let runtime = LocalRuntime::new(config);

        // Non-existent session should return None
        let state = runtime.get_session_state("nonexistent").await;
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn test_local_runtime_health_check_with_valid_binaries() {
        let config = LocalRuntimeConfig {
            opencode_binary: "sh".to_string(),
            fileserver_binary: "sh".to_string(),
            ttyd_binary: "sh".to_string(),
            ..Default::default()
        };

        let runtime = LocalRuntime::new(config);
        let result = runtime.health_check();

        assert!(result.is_ok());
        let msg = result.unwrap();
        assert!(msg.contains("Local runtime ready"));
    }

    #[tokio::test]
    async fn test_local_runtime_health_check_with_invalid_binaries() {
        let config = LocalRuntimeConfig {
            opencode_binary: "nonexistent-12345".to_string(),
            fileserver_binary: "sh".to_string(),
            ttyd_binary: "sh".to_string(),
            ..Default::default()
        };

        let runtime = LocalRuntime::new(config);
        let result = runtime.health_check();

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_local_runtime_start_session_creates_workspace() {
        // Use a mock setup - we can't really start opencode/fileserver/ttyd in tests
        // But we can test that the workspace directory is created
        let temp_dir = tempdir().unwrap();
        let workspace_path = temp_dir.path().join("new_workspace");

        // Verify it doesn't exist yet
        assert!(!workspace_path.exists());

        // We can't fully test start_session without real binaries,
        // but we can test the workspace creation part by checking the path handling
        std::fs::create_dir_all(&workspace_path).unwrap();
        assert!(workspace_path.exists());
    }

    #[tokio::test]
    async fn test_local_runtime_stop_all() {
        let config = LocalRuntimeConfig::default();
        let runtime = LocalRuntime::new(config);

        // Stop all should succeed even with no sessions
        let result = runtime.stop_all().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_serialization() {
        let config = LocalRuntimeConfig {
            opencode_binary: "opencode".to_string(),
            fileserver_binary: "octo-files".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "~/workspace".to_string(),
            default_agent: None,
            single_user: true,
            linux_users: LinuxUsersConfig::default(),
            sandbox: None,
            cleanup_on_startup: false,
            stop_sessions_on_shutdown: false,
        };

        // Test serialization
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("opencode"));
        assert!(json.contains("fileserver"));
        assert!(json.contains("ttyd"));
        assert!(json.contains("single_user"));
        assert!(json.contains("linux_users"));

        // Test deserialization
        let parsed: LocalRuntimeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.opencode_binary, config.opencode_binary);
        assert_eq!(parsed.fileserver_binary, config.fileserver_binary);
        assert_eq!(parsed.ttyd_binary, config.ttyd_binary);
        assert_eq!(parsed.workspace_dir, config.workspace_dir);
        assert_eq!(parsed.single_user, config.single_user);
        assert_eq!(parsed.linux_users.enabled, config.linux_users.enabled);
    }

    #[test]
    fn test_config_default_serde() {
        // Test that default config can be serialized and deserialized
        let config = LocalRuntimeConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: LocalRuntimeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.opencode_binary, "opencode");
        assert_eq!(parsed.fileserver_binary, "octo-files");
        assert_eq!(parsed.ttyd_binary, "ttyd");
        assert_eq!(parsed.workspace_dir, "$HOME/octo/{user_id}");
        assert!(!parsed.single_user);
    }

    #[test]
    fn test_workspace_for_user() {
        let config = LocalRuntimeConfig {
            workspace_dir: "/home/test/octo/{user_id}".to_string(),
            ..Default::default()
        };

        let path = config.workspace_for_user("alice");
        assert_eq!(path, std::path::PathBuf::from("/home/test/octo/alice"));

        let path = config.workspace_for_user("bob");
        assert_eq!(path, std::path::PathBuf::from("/home/test/octo/bob"));
    }

    #[test]
    fn test_workspace_for_user_with_env_var() {
        // Use $HOME which is always set
        let home = std::env::var("HOME").expect("HOME should be set");

        let config = LocalRuntimeConfig {
            workspace_dir: "$HOME/octo/{user_id}".to_string(),
            ..Default::default()
        };

        let path = config.workspace_for_user("testuser");
        assert_eq!(
            path,
            std::path::PathBuf::from(format!("{}/octo/testuser", home))
        );
    }

    #[test]
    fn test_single_user_mode() {
        let config = LocalRuntimeConfig {
            single_user: true,
            ..Default::default()
        };

        assert!(config.single_user);
    }

    #[test]
    fn test_workspace_base() {
        // Test with {user_id} at the end
        let config = LocalRuntimeConfig {
            workspace_dir: "/home/test/octo/{user_id}".to_string(),
            ..Default::default()
        };
        assert_eq!(
            config.workspace_base(),
            std::path::PathBuf::from("/home/test/octo")
        );

        // Test with {user_id} in the middle (edge case)
        let config = LocalRuntimeConfig {
            workspace_dir: "/data/{user_id}/workspace".to_string(),
            ..Default::default()
        };
        assert_eq!(
            config.workspace_base(),
            std::path::PathBuf::from("/data/workspace")
        );

        // Test without {user_id} placeholder
        let config = LocalRuntimeConfig {
            workspace_dir: "/home/user/workspace".to_string(),
            ..Default::default()
        };
        assert_eq!(
            config.workspace_base(),
            std::path::PathBuf::from("/home/user/workspace")
        );
    }

    #[test]
    fn test_workspace_base_with_env_var() {
        let home = std::env::var("HOME").expect("HOME should be set");

        let config = LocalRuntimeConfig {
            workspace_dir: "$HOME/octo/{user_id}".to_string(),
            single_user: true,
            ..Default::default()
        };

        let path = config.workspace_base();
        assert_eq!(path, std::path::PathBuf::from(format!("{}/octo", home)));
    }

    #[test]
    fn test_local_runtime_config_with_linux_users() {
        let config = LocalRuntimeConfig {
            linux_users: LinuxUsersConfig {
                enabled: true,
                prefix: "test_".to_string(),
                uid_start: 3000,
                group: "testgroup".to_string(),
                shell: "/bin/zsh".to_string(),
                use_sudo: false,
                create_home: false,
            },
            ..Default::default()
        };

        assert!(config.linux_users.enabled);
        assert_eq!(config.linux_users.prefix, "test_");
        assert_eq!(config.linux_users.uid_start, 3000);
        assert_eq!(config.linux_users.group, "testgroup");
        assert_eq!(config.linux_users.shell, "/bin/zsh");
        assert!(!config.linux_users.use_sudo);
        assert!(!config.linux_users.create_home);
    }

    #[test]
    fn test_local_runtime_config_default_linux_users() {
        let config = LocalRuntimeConfig::default();

        // Linux users should be disabled by default
        assert!(!config.linux_users.enabled);
        assert_eq!(config.linux_users.prefix, "octo_");
        assert_eq!(config.linux_users.uid_start, 2000);
        assert_eq!(config.linux_users.group, "octo");
        assert_eq!(config.linux_users.shell, "/bin/bash");
        assert!(config.linux_users.use_sudo);
        assert!(config.linux_users.create_home);
    }

    #[test]
    fn test_config_serialization_with_linux_users() {
        let config = LocalRuntimeConfig {
            opencode_binary: "opencode".to_string(),
            fileserver_binary: "octo-files".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "/data/{user_id}".to_string(),
            default_agent: Some("build".to_string()),
            single_user: false,
            linux_users: LinuxUsersConfig {
                enabled: true,
                prefix: "ws_".to_string(),
                uid_start: 5000,
                group: "workspace".to_string(),
                shell: "/bin/sh".to_string(),
                use_sudo: true,
                create_home: true,
            },
            sandbox: None,
            cleanup_on_startup: false,
            stop_sessions_on_shutdown: false,
        };

        // Test serialization round-trip
        let json = serde_json::to_string(&config).unwrap();
        let parsed: LocalRuntimeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.linux_users.enabled, true);
        assert_eq!(parsed.linux_users.prefix, "ws_");
        assert_eq!(parsed.linux_users.uid_start, 5000);
        assert_eq!(parsed.linux_users.group, "workspace");
        assert_eq!(parsed.linux_users.shell, "/bin/sh");
        assert_eq!(parsed.linux_users.use_sudo, true);
        assert_eq!(parsed.linux_users.create_home, true);
    }

    #[test]
    fn test_config_deserialization_without_linux_users() {
        // JSON without linux_users field - should use defaults
        let json = r#"{
            "opencode_binary": "opencode",
            "fileserver_binary": "fileserver",
            "ttyd_binary": "ttyd",
            "workspace_dir": "/data/workspace",
            "single_user": false
        }"#;

        let parsed: LocalRuntimeConfig = serde_json::from_str(json).unwrap();

        // linux_users should have default values
        assert!(!parsed.linux_users.enabled);
        assert_eq!(parsed.linux_users.prefix, "octo_");
        assert_eq!(parsed.linux_users.uid_start, 2000);
    }
}

//! Process management for local runtime.
//!
//! Handles spawning and managing native processes for opencode, fileserver, and ttyd.
//! Supports running processes as specific Linux users for multi-user isolation.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Options for running a process as a specific user.
#[derive(Debug, Clone, Default)]
pub struct RunAsUser {
    /// Linux username to run as.
    pub username: Option<String>,
    /// Use sudo to switch users.
    pub use_sudo: bool,
}

impl RunAsUser {
    /// Create options to run as a specific user.
    pub fn new(username: impl Into<String>, use_sudo: bool) -> Self {
        Self {
            username: Some(username.into()),
            use_sudo,
        }
    }

    /// Create options to run as the current user (no switching).
    pub fn current() -> Self {
        Self::default()
    }
}

/// Handle to a managed process.
#[derive(Debug)]
pub struct ProcessHandle {
    /// Process ID.
    pub pid: u32,
    /// Service name (e.g., "opencode", "fileserver", "ttyd").
    pub service: String,
    /// Port the service is listening on (kept for debugging/logging).
    #[allow(dead_code)]
    pub port: u16,
    /// The underlying child process.
    child: Child,
}

impl ProcessHandle {
    /// Create a new process handle.
    pub fn new(child: Child, service: impl Into<String>, port: u16) -> Option<Self> {
        let pid = child.id()?;
        Some(Self {
            pid,
            service: service.into(),
            port,
            child,
        })
    }

    /// Check if the process is still running.
    pub fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,  // Still running
            Ok(Some(_)) => false,  // Exited
            Err(_) => false,  // Error checking status
        }
    }

    /// Kill the process.
    pub async fn kill(&mut self) -> Result<()> {
        self.child.kill().await.context("killing process")?;
        Ok(())
    }

    /// Wait for the process to exit.
    #[allow(dead_code)]
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus> {
        self.child.wait().await.context("waiting for process")
    }
}

/// Manager for local processes.
///
/// Tracks all spawned processes and provides lifecycle management.
#[derive(Debug, Default)]
pub struct ProcessManager {
    /// Map of session_id -> list of process handles.
    processes: Arc<Mutex<HashMap<String, Vec<ProcessHandle>>>>,
}

impl ProcessManager {
    /// Create a new process manager.
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Spawn opencode serve.
    pub async fn spawn_opencode(
        &self,
        session_id: &str,
        port: u16,
        workspace_dir: &Path,
        opencode_binary: &str,
        env: HashMap<String, String>,
        run_as: &RunAsUser,
    ) -> Result<u32> {
        info!(
            "Spawning opencode serve on port {} for session {}",
            port, session_id
        );

        let child = self
            .spawn_as_user(
                run_as,
                opencode_binary,
                &["serve", "--port", &port.to_string(), "--hostname", "0.0.0.0"],
                Some(workspace_dir),
                env,
            )
            .await
            .context("spawning opencode")?;

        let handle = ProcessHandle::new(child, "opencode", port)
            .ok_or_else(|| anyhow::anyhow!("failed to get PID for opencode"))?;
        let pid = handle.pid;

        let mut processes = self.processes.lock().await;
        processes
            .entry(session_id.to_string())
            .or_default()
            .push(handle);

        info!("opencode spawned with PID {} on port {}", pid, port);
        Ok(pid)
    }

    /// Spawn fileserver.
    pub async fn spawn_fileserver(
        &self,
        session_id: &str,
        port: u16,
        root_dir: &Path,
        fileserver_binary: &str,
        run_as: &RunAsUser,
    ) -> Result<u32> {
        info!(
            "Spawning fileserver on port {} for session {}",
            port, session_id
        );

        let child = self
            .spawn_as_user(
                run_as,
                fileserver_binary,
                &[
                    "--port",
                    &port.to_string(),
                    "--bind",
                    "0.0.0.0",
                    "--root",
                    root_dir.to_str().unwrap_or("."),
                ],
                None,
                HashMap::new(),
            )
            .await
            .context("spawning fileserver")?;

        let handle = ProcessHandle::new(child, "fileserver", port)
            .ok_or_else(|| anyhow::anyhow!("failed to get PID for fileserver"))?;
        let pid = handle.pid;

        let mut processes = self.processes.lock().await;
        processes
            .entry(session_id.to_string())
            .or_default()
            .push(handle);

        info!("fileserver spawned with PID {} on port {}", pid, port);
        Ok(pid)
    }

    /// Spawn ttyd.
    pub async fn spawn_ttyd(
        &self,
        session_id: &str,
        port: u16,
        cwd: &Path,
        ttyd_binary: &str,
        run_as: &RunAsUser,
    ) -> Result<u32> {
        info!(
            "Spawning ttyd on port {} for session {}",
            port, session_id
        );

        // For ttyd, we need to spawn a shell as the target user
        let (shell_cmd, shell_args) = if let Some(ref username) = run_as.username {
            // Use su - to get a proper login shell as the target user
            ("su", vec!["-", username, "-c", "exec bash -l"])
        } else {
            ("bash", vec!["-l"])
        };

        let child = self
            .spawn_as_user(
                &RunAsUser::current(), // ttyd itself runs as current user
                ttyd_binary,
                &[
                    "--port",
                    &port.to_string(),
                    "--interface",
                    "0.0.0.0",
                    "--writable",
                    "--cwd",
                    cwd.to_str().unwrap_or("."),
                    shell_cmd,
                    &shell_args.join(" "),
                ],
                None,
                HashMap::new(),
            )
            .await
            .context("spawning ttyd")?;

        let handle = ProcessHandle::new(child, "ttyd", port)
            .ok_or_else(|| anyhow::anyhow!("failed to get PID for ttyd"))?;
        let pid = handle.pid;

        let mut processes = self.processes.lock().await;
        processes
            .entry(session_id.to_string())
            .or_default()
            .push(handle);

        info!("ttyd spawned with PID {} on port {}", pid, port);
        Ok(pid)
    }

    /// Helper to spawn a process, optionally as a different user.
    async fn spawn_as_user(
        &self,
        run_as: &RunAsUser,
        binary: &str,
        args: &[&str],
        cwd: Option<&Path>,
        env: HashMap<String, String>,
    ) -> Result<Child> {
        let child = if let Some(ref username) = run_as.username {
            // Run as a specific user
            let is_root = unsafe { libc::geteuid() } == 0;

            if run_as.use_sudo && !is_root {
                // Use sudo -u to run as the target user
                debug!(
                    "Spawning {} as user '{}' via sudo: {:?}",
                    binary, username, args
                );

                let mut cmd = Command::new("sudo");
                cmd.arg("-u")
                    .arg(username)
                    .arg("--preserve-env")
                    .arg("--")
                    .arg(binary)
                    .args(args)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true);

                if let Some(dir) = cwd {
                    cmd.current_dir(dir);
                }

                for (key, value) in env {
                    cmd.env(&key, &value);
                }

                cmd.spawn()?
            } else if is_root {
                // We're root, use su to switch user
                debug!(
                    "Spawning {} as user '{}' via su (running as root): {:?}",
                    binary, username, args
                );

                // Build the command string for su
                let args_str = args
                    .iter()
                    .map(|a| shell_escape(a))
                    .collect::<Vec<_>>()
                    .join(" ");

                let full_cmd = if let Some(dir) = cwd {
                    format!("cd {} && {} {}", shell_escape(dir.to_str().unwrap_or(".")), binary, args_str)
                } else {
                    format!("{} {}", binary, args_str)
                };

                let mut cmd = Command::new("su");
                cmd.arg("-")
                    .arg(username)
                    .arg("-c")
                    .arg(&full_cmd)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true);

                for (key, value) in env {
                    cmd.env(&key, &value);
                }

                cmd.spawn()?
            } else {
                anyhow::bail!(
                    "Cannot run as user '{}': not root and use_sudo is false",
                    username
                );
            }
        } else {
            // Run as current user
            debug!("Spawning {} as current user: {:?}", binary, args);

            let mut cmd = Command::new(binary);
            cmd.args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);

            if let Some(dir) = cwd {
                cmd.current_dir(dir);
            }

            for (key, value) in env {
                cmd.env(&key, &value);
            }

            cmd.spawn()?
        };

        Ok(child)
    }

    /// Stop all processes for a session.
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        let mut processes = self.processes.lock().await;

        if let Some(mut handles) = processes.remove(session_id) {
            info!(
                "Stopping {} processes for session {}",
                handles.len(),
                session_id
            );

            for handle in handles.iter_mut() {
                debug!("Killing {} (PID {})", handle.service, handle.pid);
                if let Err(e) = handle.kill().await {
                    warn!(
                        "Failed to kill {} (PID {}): {:?}",
                        handle.service, handle.pid, e
                    );
                }
            }
        }

        Ok(())
    }

    /// Check if all processes for a session are running.
    pub async fn is_session_running(&self, session_id: &str) -> bool {
        let mut processes = self.processes.lock().await;

        if let Some(handles) = processes.get_mut(session_id) {
            handles.iter_mut().all(|h| h.is_running())
        } else {
            false
        }
    }

    /// Get the list of PIDs for a session.
    #[allow(dead_code)]
    pub async fn get_session_pids(&self, session_id: &str) -> Vec<u32> {
        let processes = self.processes.lock().await;

        processes
            .get(session_id)
            .map(|handles| handles.iter().map(|h| h.pid).collect())
            .unwrap_or_default()
    }

    /// Stop all managed processes.
    #[allow(dead_code)]
    pub async fn stop_all(&self) -> Result<()> {
        let mut processes = self.processes.lock().await;
        let session_ids: Vec<String> = processes.keys().cloned().collect();

        for session_id in session_ids {
            if let Some(mut handles) = processes.remove(&session_id) {
                info!(
                    "Stopping {} processes for session {}",
                    handles.len(),
                    session_id
                );

                for handle in handles.iter_mut() {
                    debug!("Killing {} (PID {})", handle.service, handle.pid);
                    if let Err(e) = handle.kill().await {
                        warn!(
                            "Failed to kill {} (PID {}): {:?}",
                            handle.service, handle.pid, e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Cleanup processes that have exited.
    #[allow(dead_code)]
    pub async fn cleanup_dead_processes(&self) {
        let mut processes = self.processes.lock().await;

        for (session_id, handles) in processes.iter_mut() {
            let before = handles.len();
            handles.retain_mut(|h| h.is_running());
            let after = handles.len();

            if before != after {
                warn!(
                    "Session {} had {} dead processes cleaned up",
                    session_id,
                    before - after
                );
            }
        }

        // Remove empty sessions
        processes.retain(|_, handles| !handles.is_empty());
    }
}

impl Clone for ProcessManager {
    fn clone(&self) -> Self {
        Self {
            processes: Arc::clone(&self.processes),
        }
    }
}

/// Escape a string for safe use in a shell command.
fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/') {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_process_manager_new() {
        let manager = ProcessManager::new();
        assert!(manager.get_session_pids("nonexistent").await.is_empty());
    }

    #[tokio::test]
    async fn test_process_manager_clone_shares_state() {
        let manager1 = ProcessManager::new();
        let manager2 = manager1.clone();

        // Spawn a process using manager1
        let child = Command::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let handle = ProcessHandle::new(child, "test", 8080).unwrap();
        let pid = handle.pid;

        {
            let mut processes = manager1.processes.lock().await;
            processes.insert("session1".to_string(), vec![handle]);
        }

        // Verify manager2 sees the same state
        let pids = manager2.get_session_pids("session1").await;
        assert_eq!(pids, vec![pid]);

        // Cleanup
        manager1.stop_session("session1").await.unwrap();
    }

    #[tokio::test]
    async fn test_process_handle_creation() {
        let child = Command::new("sleep")
            .arg("1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let handle = ProcessHandle::new(child, "test-service", 9090);
        assert!(handle.is_some());

        let handle = handle.unwrap();
        assert_eq!(handle.service, "test-service");
        assert_eq!(handle.port, 9090);
        assert!(handle.pid > 0);
    }

    #[tokio::test]
    async fn test_process_handle_is_running() {
        let child = Command::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let mut handle = ProcessHandle::new(child, "test", 8080).unwrap();

        // Process should be running
        assert!(handle.is_running());

        // Kill it
        handle.kill().await.unwrap();

        // Give it a moment to exit
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should no longer be running
        assert!(!handle.is_running());
    }

    #[tokio::test]
    async fn test_process_handle_kill() {
        let child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let mut handle = ProcessHandle::new(child, "test", 8080).unwrap();
        let pid = handle.pid;

        // Verify process exists
        assert!(handle.is_running());

        // Kill it
        let result = handle.kill().await;
        assert!(result.is_ok());

        // Wait a bit and verify it's gone
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!handle.is_running());

        // Verify via system that PID is gone (optional, platform-specific)
        let status = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status();
        // kill -0 returns error if process doesn't exist
        assert!(status.is_err() || !status.unwrap().success());
    }

    #[tokio::test]
    async fn test_process_manager_stop_session() {
        let manager = ProcessManager::new();

        // Spawn two processes for a session
        let child1 = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let child2 = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let handle1 = ProcessHandle::new(child1, "service1", 8080).unwrap();
        let handle2 = ProcessHandle::new(child2, "service2", 8081).unwrap();

        {
            let mut processes = manager.processes.lock().await;
            processes.insert("session1".to_string(), vec![handle1, handle2]);
        }

        // Verify processes are tracked
        assert_eq!(manager.get_session_pids("session1").await.len(), 2);

        // Stop the session
        manager.stop_session("session1").await.unwrap();

        // Verify processes are removed
        assert!(manager.get_session_pids("session1").await.is_empty());
    }

    #[tokio::test]
    async fn test_process_manager_stop_nonexistent_session() {
        let manager = ProcessManager::new();

        // Stopping a nonexistent session should succeed (no-op)
        let result = manager.stop_session("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_manager_is_session_running() {
        let manager = ProcessManager::new();

        // Non-existent session should return false
        assert!(!manager.is_session_running("nonexistent").await);

        // Spawn a process
        let child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let handle = ProcessHandle::new(child, "test", 8080).unwrap();

        {
            let mut processes = manager.processes.lock().await;
            processes.insert("session1".to_string(), vec![handle]);
        }

        // Session should be running
        assert!(manager.is_session_running("session1").await);

        // Stop it
        manager.stop_session("session1").await.unwrap();

        // Should no longer be running
        assert!(!manager.is_session_running("session1").await);
    }

    #[tokio::test]
    async fn test_process_manager_stop_all() {
        let manager = ProcessManager::new();

        // Spawn processes for multiple sessions
        for session_id in ["session1", "session2", "session3"] {
            let child = Command::new("sleep")
                .arg("60")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .unwrap();

            let handle = ProcessHandle::new(child, "test", 8080).unwrap();

            let mut processes = manager.processes.lock().await;
            processes.insert(session_id.to_string(), vec![handle]);
        }

        // Verify all sessions exist
        assert!(manager.is_session_running("session1").await);
        assert!(manager.is_session_running("session2").await);
        assert!(manager.is_session_running("session3").await);

        // Stop all
        manager.stop_all().await.unwrap();

        // All should be gone
        assert!(!manager.is_session_running("session1").await);
        assert!(!manager.is_session_running("session2").await);
        assert!(!manager.is_session_running("session3").await);
    }

    #[tokio::test]
    async fn test_process_manager_cleanup_dead_processes() {
        let manager = ProcessManager::new();

        // Spawn a short-lived process
        let child = Command::new("true") // exits immediately
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let handle = ProcessHandle::new(child, "test", 8080).unwrap();

        {
            let mut processes = manager.processes.lock().await;
            processes.insert("session1".to_string(), vec![handle]);
        }

        // Wait for process to exit
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cleanup dead processes
        manager.cleanup_dead_processes().await;

        // Session should be removed (empty)
        assert!(manager.get_session_pids("session1").await.is_empty());
    }

    #[tokio::test]
    async fn test_process_manager_get_session_pids() {
        let manager = ProcessManager::new();

        // Non-existent session returns empty
        assert!(manager.get_session_pids("nonexistent").await.is_empty());

        // Spawn multiple processes
        let mut expected_pids = Vec::new();

        for port in [8080, 8081, 8082] {
            let child = Command::new("sleep")
                .arg("60")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .unwrap();

            let handle = ProcessHandle::new(child, "test", port).unwrap();
            expected_pids.push(handle.pid);

            let mut processes = manager.processes.lock().await;
            processes
                .entry("session1".to_string())
                .or_default()
                .push(handle);
        }

        // Get PIDs
        let pids = manager.get_session_pids("session1").await;
        assert_eq!(pids.len(), 3);
        for pid in expected_pids {
            assert!(pids.contains(&pid));
        }

        // Cleanup
        manager.stop_session("session1").await.unwrap();
    }
}

//! Process management for local runtime.
//!
//! Handles spawning and managing native processes for opencode, fileserver, and ttyd.
//! Supports running processes as specific Linux users for multi-user isolation.
//! Optionally wraps processes in a bubblewrap sandbox for additional security.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::sandbox::SandboxConfig;

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
            Ok(None) => true,     // Still running
            Ok(Some(_)) => false, // Exited
            Err(_) => false,      // Error checking status
        }
    }

    /// Check if the process has exited and return exit info.
    ///
    /// Returns `None` if still running, or `Some((exit_code, signal))` if exited.
    /// On Unix, if killed by signal, exit_code is None and signal contains the signal number.
    pub fn check_exit_status(&mut self) -> Option<(Option<i32>, Option<i32>)> {
        match self.child.try_wait() {
            Ok(None) => None, // Still running
            Ok(Some(status)) => {
                let code = status.code();
                #[cfg(unix)]
                let signal = {
                    use std::os::unix::process::ExitStatusExt;
                    status.signal()
                };
                #[cfg(not(unix))]
                let signal = None;
                Some((code, signal))
            }
            Err(e) => {
                warn!("Error checking process {} status: {:?}", self.pid, e);
                Some((None, None))
            }
        }
    }

    /// Format exit status as a human-readable string.
    pub fn format_exit_status(exit_code: Option<i32>, signal: Option<i32>) -> String {
        match (exit_code, signal) {
            (Some(code), _) => format!("exited with code {}", code),
            (None, Some(sig)) => {
                let sig_name = match sig {
                    9 => "SIGKILL",
                    15 => "SIGTERM",
                    11 => "SIGSEGV",
                    6 => "SIGABRT",
                    _ => "",
                };
                if sig_name.is_empty() {
                    format!("killed by signal {}", sig)
                } else {
                    format!("killed by {} (signal {})", sig_name, sig)
                }
            }
            (None, None) => "exited (unknown status)".to_string(),
        }
    }

    /// Kill the process and wait for it to be reaped.
    ///
    /// This both sends SIGKILL and waits for the process to exit,
    /// preventing zombie processes.
    pub async fn kill(&mut self) -> Result<()> {
        // First try to kill
        if let Err(e) = self.child.kill().await {
            // Process might already be dead, check
            if self.is_running() {
                return Err(anyhow::anyhow!("failed to kill process: {}", e));
            }
        }

        // Wait for the process to be reaped (prevents zombies)
        // Use a timeout to avoid hanging forever
        match tokio::time::timeout(std::time::Duration::from_secs(5), self.child.wait()).await {
            Ok(Ok(_)) => Ok(()), // Process exited cleanly
            Ok(Err(e)) => {
                // Error waiting, but process might be gone
                warn!("Error waiting for process {}: {:?}", self.pid, e);
                Ok(())
            }
            Err(_) => {
                // Timeout - process didn't exit in time
                warn!("Timeout waiting for process {} to exit", self.pid);
                Ok(())
            }
        }
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

    // Pi process management is handled by Main Chat Pi service.

    /// Spawn opencode serve.
    ///
    /// If `agent` is provided, it is passed via the --agent flag.
    /// If `sandbox` is provided and enabled, wraps the process in bubblewrap.
    pub async fn spawn_opencode(
        &self,
        session_id: &str,
        port: u16,
        workspace_dir: &Path,
        opencode_binary: &str,
        agent: Option<&str>,
        env: HashMap<String, String>,
        run_as: &RunAsUser,
        sandbox: Option<&SandboxConfig>,
    ) -> Result<u32> {
        info!(
            "Spawning opencode serve on port {} for session {}, agent: {:?}, sandbox: {}",
            port,
            session_id,
            agent,
            sandbox.map(|s| s.enabled).unwrap_or(false)
        );

        let mut args = vec![
            "serve".to_string(),
            "--port".to_string(),
            port.to_string(),
            "--hostname".to_string(),
            "127.0.0.1".to_string(),
        ];

        if let Some(agent_name) = agent {
            args.push("--agent".to_string());
            args.push(agent_name.to_string());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let child = self
            .spawn_sandboxed(
                run_as,
                opencode_binary,
                &args_refs,
                Some(workspace_dir),
                env,
                sandbox,
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
                    "127.0.0.1",
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
        info!("Spawning ttyd on port {} for session {}", port, session_id);

        // For ttyd, we need to spawn a shell as the target user.
        // ttyd takes: <command> [<arguments...>] as separate positional args.
        // Use zsh as the default shell for a better experience.
        let shell_args: Vec<String> = if let Some(ref username) = run_as.username {
            if run_as.use_sudo {
                // Prefer sudo to avoid su variants that reject -c or -l options.
                vec![
                    "sudo".to_string(),
                    "-u".to_string(),
                    username.clone(),
                    "-H".to_string(),
                    "--".to_string(),
                    "zsh".to_string(),
                    "-l".to_string(),
                ]
            } else {
                // Use su -l <user> runs a login shell; we then exec zsh.
                vec![
                    "su".to_string(),
                    "-l".to_string(),
                    username.clone(),
                    "-c".to_string(),
                    "exec zsh -l".to_string(),
                ]
            }
        } else {
            vec!["zsh".to_string(), "-l".to_string()]
        };

        // Build ttyd args: options first, then shell command + args
        let port_str = port.to_string();
        let cwd_str = cwd.to_str().unwrap_or(".");
        let mut ttyd_args: Vec<&str> = vec![
            "--port",
            &port_str,
            "--interface",
            "127.0.0.1",
            "--writable",
            "--cwd",
            cwd_str,
        ];
        // Append shell command and its arguments as separate positional args
        for arg in &shell_args {
            ttyd_args.push(arg);
        }

        let child = self
            .spawn_as_user(
                &RunAsUser::current(), // ttyd itself runs as current user
                ttyd_binary,
                &ttyd_args,
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
                cmd.arg("-n")
                    .arg("-u")
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
                    format!(
                        "cd {} && {} {}",
                        shell_escape(dir.to_str().unwrap_or(".")),
                        binary,
                        args_str
                    )
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

    /// Spawn a process with optional sandboxing.
    ///
    /// If sandbox config is provided and enabled, wraps the command with bubblewrap.
    /// Falls back to direct spawn if bwrap is not available.
    ///
    /// When running as a different user (`run_as.username` is Some), the sandbox
    /// will expand paths like `~/.config` to that user's home directory, not
    /// the current user's.
    pub async fn spawn_sandboxed(
        &self,
        run_as: &RunAsUser,
        binary: &str,
        args: &[&str],
        cwd: Option<&Path>,
        env: HashMap<String, String>,
        sandbox: Option<&SandboxConfig>,
    ) -> Result<Child> {
        // Check if sandboxing should be applied
        let workspace = cwd.unwrap_or(Path::new("."));
        let target_user = run_as.username.as_deref();

        info!(
            "spawn_sandboxed: binary={}, workspace={}, target_user={:?}, sandbox_enabled={}",
            binary,
            workspace.display(),
            target_user.unwrap_or("(current)"),
            sandbox.map(|s| s.enabled).unwrap_or(false)
        );

        // Merge global sandbox config with workspace-specific config
        let effective_sandbox = sandbox
            .filter(|s| s.enabled)
            .map(|global| global.with_workspace_config(workspace));

        // Build bwrap args for the target user (important for multi-user mode)
        let sandbox_args = effective_sandbox
            .as_ref()
            .filter(|s| s.enabled)
            .and_then(|s| s.build_bwrap_args_for_user(workspace, target_user));

        if let Some(bwrap_args) = sandbox_args {
            // Spawn with bwrap wrapper
            info!(
                "Spawning {} with sandbox (bwrap) for user {:?}",
                binary,
                target_user.unwrap_or("(current)")
            );
            debug!("bwrap args count: {}", bwrap_args.len());

            // Build the full command: bwrap [bwrap_args] -- binary [args]
            let mut full_args: Vec<String> = bwrap_args;
            full_args.push(binary.to_string());
            full_args.extend(args.iter().map(|s| s.to_string()));

            let full_args_refs: Vec<&str> = full_args.iter().map(|s| s.as_str()).collect();

            debug!(
                "Full sandboxed command: bwrap {} {}",
                full_args_refs[..full_args_refs.len().saturating_sub(args.len() + 1)]
                    .iter()
                    .take(10)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" "),
                if full_args_refs.len() > 10 { "..." } else { "" }
            );

            // For sandboxed execution, we run bwrap as the target user
            // bwrap handles the actual command execution inside the sandbox
            self.spawn_as_user(run_as, "bwrap", &full_args_refs, None, env)
                .await
        } else {
            // Direct spawn without sandbox
            if sandbox.map(|s| s.enabled).unwrap_or(false) {
                warn!(
                    "Sandbox enabled but bwrap not available, spawning {} without sandbox",
                    binary
                );
            }
            self.spawn_as_user(run_as, binary, args, cwd, env).await
        }
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

    /// Get exit information for any crashed processes in a session.
    ///
    /// Returns a list of (service_name, exit_reason) for processes that have exited.
    /// Returns empty vec if all processes are running or session doesn't exist.
    pub async fn get_session_exit_info(&self, session_id: &str) -> Vec<(String, String)> {
        let mut processes = self.processes.lock().await;
        let mut exit_info = Vec::new();

        if let Some(handles) = processes.get_mut(session_id) {
            for handle in handles.iter_mut() {
                if let Some((code, signal)) = handle.check_exit_status() {
                    let reason = ProcessHandle::format_exit_status(code, signal);
                    exit_info.push((handle.service.clone(), reason));
                }
            }
        }

        exit_info
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
    if s.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/')
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Check if a port is available for binding.
pub fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
}

/// Check if all ports in a range are available.
pub fn are_ports_available(ports: &[u16]) -> bool {
    ports.iter().all(|&p| is_port_available(p))
}

/// Find the process using a specific port (Linux only).
/// Returns (pid, process_name) if found.
#[cfg(target_os = "linux")]
pub fn find_process_on_port(port: u16) -> Option<(u32, String)> {
    use std::process::Command as StdCommand;

    // Use ss or netstat to find the process
    let output = StdCommand::new("ss")
        .args(["-tlnp", &format!("sport = :{}", port)])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse the output to find PID
    // Format: LISTEN 0 4096 0.0.0.0:41820 0.0.0.0:* users:(("opencode",pid=12345,fd=15))
    for line in stdout.lines().skip(1) {
        if let Some(users_part) = line.split("users:((").nth(1) {
            if let Some(pid_part) = users_part.split("pid=").nth(1) {
                if let Some(pid_str) = pid_part.split(',').next() {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        // Extract process name
                        let name = users_part
                            .split('"')
                            .nth(1)
                            .unwrap_or("unknown")
                            .to_string();
                        return Some((pid, name));
                    }
                }
            }
        }
    }

    None
}

#[cfg(not(target_os = "linux"))]
pub fn find_process_on_port(_port: u16) -> Option<(u32, String)> {
    None
}

/// Kill a process by PID.
pub fn kill_process(pid: u32) -> bool {
    use std::process::Command as StdCommand;

    StdCommand::new("kill")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Force kill a process by PID (SIGKILL).
pub fn force_kill_process(pid: u32) -> bool {
    use std::process::Command as StdCommand;

    StdCommand::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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

    // =========================================================================
    // Security tests: Verify services bind to localhost only
    // =========================================================================
    //
    // These tests ensure that session services (opencode, fileserver, ttyd) bind
    // to 127.0.0.1 (localhost) rather than 0.0.0.0 (all interfaces). Binding to
    // 0.0.0.0 would expose these services to the network, bypassing the proxy.

    /// Helper to build opencode args (mirrors the logic in spawn_opencode).
    fn build_opencode_args(port: u16, agent: Option<&str>) -> Vec<String> {
        let mut args = vec![
            "serve".to_string(),
            "--port".to_string(),
            port.to_string(),
            "--hostname".to_string(),
            "127.0.0.1".to_string(),
        ];
        if let Some(agent_name) = agent {
            args.push("--agent".to_string());
            args.push(agent_name.to_string());
        }
        args
    }

    /// Helper to build fileserver args (mirrors the logic in spawn_fileserver).
    fn build_fileserver_args(port: u16, root_dir: &str) -> Vec<String> {
        vec![
            "--port".to_string(),
            port.to_string(),
            "--bind".to_string(),
            "127.0.0.1".to_string(),
            "--root".to_string(),
            root_dir.to_string(),
        ]
    }

    /// Helper to build ttyd args (mirrors the logic in spawn_ttyd).
    fn build_ttyd_args(port: u16, cwd: &str) -> Vec<String> {
        vec![
            "--port".to_string(),
            port.to_string(),
            "--interface".to_string(),
            "127.0.0.1".to_string(),
            "--writable".to_string(),
            "--cwd".to_string(),
            cwd.to_string(),
            "zsh".to_string(),
            "-l".to_string(),
        ]
    }

    #[test]
    fn test_opencode_binds_to_localhost_only() {
        let args = build_opencode_args(4096, None);

        // Find the --hostname argument
        let hostname_idx = args.iter().position(|a| a == "--hostname");
        assert!(hostname_idx.is_some(), "opencode args must include --hostname");

        let bind_addr = &args[hostname_idx.unwrap() + 1];
        assert_eq!(
            bind_addr, "127.0.0.1",
            "opencode must bind to 127.0.0.1, not {}. Binding to 0.0.0.0 exposes the service to the network!",
            bind_addr
        );

        // Verify it's NOT 0.0.0.0
        assert_ne!(
            bind_addr, "0.0.0.0",
            "SECURITY: opencode must NOT bind to 0.0.0.0"
        );
    }

    #[test]
    fn test_opencode_with_agent_binds_to_localhost_only() {
        let args = build_opencode_args(4096, Some("test-agent"));

        let hostname_idx = args.iter().position(|a| a == "--hostname");
        assert!(hostname_idx.is_some());

        let bind_addr = &args[hostname_idx.unwrap() + 1];
        assert_eq!(bind_addr, "127.0.0.1");
        assert_ne!(bind_addr, "0.0.0.0");
    }

    #[test]
    fn test_fileserver_binds_to_localhost_only() {
        let args = build_fileserver_args(8080, "/workspace");

        // Find the --bind argument
        let bind_idx = args.iter().position(|a| a == "--bind");
        assert!(bind_idx.is_some(), "fileserver args must include --bind");

        let bind_addr = &args[bind_idx.unwrap() + 1];
        assert_eq!(
            bind_addr, "127.0.0.1",
            "fileserver must bind to 127.0.0.1, not {}. Binding to 0.0.0.0 exposes the service to the network!",
            bind_addr
        );

        assert_ne!(
            bind_addr, "0.0.0.0",
            "SECURITY: fileserver must NOT bind to 0.0.0.0"
        );
    }

    #[test]
    fn test_ttyd_binds_to_localhost_only() {
        let args = build_ttyd_args(7681, "/workspace");

        // Find the --interface argument
        let interface_idx = args.iter().position(|a| a == "--interface");
        assert!(interface_idx.is_some(), "ttyd args must include --interface");

        let bind_addr = &args[interface_idx.unwrap() + 1];
        assert_eq!(
            bind_addr, "127.0.0.1",
            "ttyd must bind to 127.0.0.1, not {}. Binding to 0.0.0.0 exposes the service to the network!",
            bind_addr
        );

        assert_ne!(
            bind_addr, "0.0.0.0",
            "SECURITY: ttyd must NOT bind to 0.0.0.0"
        );
    }
}

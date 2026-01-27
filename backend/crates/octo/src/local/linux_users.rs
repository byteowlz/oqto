//! Linux user management for multi-user isolation.
//!
//! This module provides functionality to create and manage Linux users for
//! platform users, enabling proper process isolation in multi-user deployments.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Configuration for Linux user isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LinuxUsersConfig {
    /// Enable Linux user isolation (requires root or sudo privileges).
    pub enabled: bool,
    /// Prefix for auto-created Linux usernames (e.g., "octo_" -> "octo_alice").
    pub prefix: String,
    /// Starting UID for new users. Users get sequential UIDs from this value.
    pub uid_start: u32,
    /// Shared group for all octo users. Created if it doesn't exist.
    pub group: String,
    /// Shell for new users.
    pub shell: String,
    /// Use sudo to run processes as the target user.
    /// If false, requires the main process to run as root.
    pub use_sudo: bool,
    /// Create home directories for new users.
    pub create_home: bool,
}

impl Default for LinuxUsersConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            prefix: "octo_".to_string(),
            uid_start: 2000,
            group: "octo".to_string(),
            shell: "/bin/bash".to_string(),
            use_sudo: true,
            create_home: true,
        }
    }
}

/// Prefix for project-based Linux users.
const PROJECT_PREFIX: &str = "proj_";

/// Check if a Linux user exists.
fn user_exists(username: &str) -> bool {
    Command::new("id")
        .arg("-u")
        .arg(username)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

impl LinuxUsersConfig {
    /// Get the Linux username for a platform user ID.
    ///
    /// If a Linux user already exists with the exact user_id (no prefix),
    /// that name is used. This handles admin users who have their own
    /// Linux accounts without the platform prefix.
    pub fn linux_username(&self, user_id: &str) -> String {
        // Check if user already exists without prefix (e.g., admin users)
        let sanitized = sanitize_username(user_id);
        if user_exists(&sanitized) {
            return sanitized;
        }
        // Otherwise, use the configured prefix
        format!("{}{}", self.prefix, sanitized)
    }

    /// Get the Linux username for a shared project.
    ///
    /// Projects use a different prefix to distinguish them from user accounts:
    /// - User: octo_alice
    /// - Project: octo_proj_myproject
    pub fn project_username(&self, project_id: &str) -> String {
        format!(
            "{}{}{}",
            self.prefix,
            PROJECT_PREFIX,
            sanitize_username(project_id)
        )
    }

    /// Ensure a Linux user exists for a shared project.
    ///
    /// Creates the project user if it doesn't exist and sets up the project directory.
    /// Returns the UID of the project user.
    pub fn ensure_project_user(
        &self,
        project_id: &str,
        project_path: &std::path::Path,
    ) -> Result<u32> {
        if !self.enabled {
            // Return current user's UID when not enabled
            return Ok(unsafe { libc::getuid() });
        }

        // Ensure group exists first
        self.ensure_group()?;

        let username = self.project_username(project_id);

        // Check if user already exists
        if let Some(uid) = get_user_uid(&username)? {
            debug!(
                "Project user '{}' already exists with UID {}",
                username, uid
            );
            // Ensure directory ownership is correct
            self.chown_directory_to_user(project_path, &username)?;
            return Ok(uid);
        }

        // Find next available UID
        let uid = self.find_next_uid()?;

        info!(
            "Creating Linux user '{}' with UID {} for project '{}'",
            username, uid, project_id
        );

        // Build useradd command
        let mut args = vec![
            "-u".to_string(),
            uid.to_string(),
            "-g".to_string(),
            self.group.clone(),
            "-s".to_string(),
            self.shell.clone(),
        ];

        if self.create_home {
            args.push("-m".to_string());
        } else {
            args.push("-M".to_string());
        }

        // Add comment with project ID for reference (sanitize for useradd compat)
        args.push("-c".to_string());
        args.push(sanitize_gecos(&format!(
            "Octo shared project: {}",
            project_id
        )));

        args.push(username.clone());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        run_privileged_command(self.use_sudo, "useradd", &args_refs)
            .with_context(|| format!("creating project user '{}'", username))?;

        info!("Created Linux user '{}' with UID {}", username, uid);

        // Set up project directory with correct ownership
        std::fs::create_dir_all(project_path)
            .with_context(|| format!("creating project directory: {:?}", project_path))?;
        self.chown_directory_to_user(project_path, &username)?;

        Ok(uid)
    }

    /// Set ownership of a directory to a specific Linux username.
    pub fn chown_directory_to_user(&self, path: &std::path::Path, username: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let path_str = path.to_string_lossy();

        info!("Setting ownership of '{}' to '{}'", path_str, username);

        run_privileged_command(
            self.use_sudo,
            "chown",
            &["-R", &format!("{}:{}", username, self.group), &path_str],
        )
        .with_context(|| format!("chown {} to {}", path_str, username))?;

        Ok(())
    }

    /// Get the effective Linux username for a session.
    ///
    /// This determines which Linux user should run the agent processes:
    /// - If project_id is provided, uses the project user
    /// - Otherwise, uses the platform user's Linux user
    pub fn effective_username(&self, user_id: &str, project_id: Option<&str>) -> String {
        match project_id {
            Some(pid) => self.project_username(pid),
            None => self.linux_username(user_id),
        }
    }

    /// Ensure the effective user exists for a session.
    ///
    /// This is the main entry point for automatic user creation:
    /// - If project_id is provided, ensures project user exists
    /// - Otherwise, ensures platform user's Linux user exists
    pub fn ensure_effective_user(
        &self,
        user_id: &str,
        project_id: Option<&str>,
        project_path: Option<&std::path::Path>,
    ) -> Result<u32> {
        match (project_id, project_path) {
            (Some(pid), Some(path)) => self.ensure_project_user(pid, path),
            _ => self.ensure_user(user_id),
        }
    }

    /// Check if running with sufficient privileges for user management.
    pub fn check_privileges(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let is_root = unsafe { libc::geteuid() } == 0;

        if is_root {
            debug!("Running as root, Linux user management available");
            return Ok(());
        }

        if self.use_sudo {
            // IMPORTANT: do not use `sudo -n true` as a probe.
            // Secure setups often allow NOPASSWD only for a restricted allowlist
            // (e.g. useradd/usermod/userdel), and `true` would fail.
            // Instead, probe one of the exact helpers required by setup.sh.

            let output = Command::new("sudo")
                .args(["-n", "/usr/sbin/useradd", "--help"])
                .output();

            if let Ok(out) = output {
                if out.status.success() {
                    debug!("Passwordless sudo available for user management helpers");
                    return Ok(());
                }
            }

            // If we can't verify here, rely on operation-time errors.
            debug!(
                "Could not verify sudo allowlist via /usr/sbin/useradd --help; proceeding and relying on operation-time errors"
            );
            return Ok(());
        }

        anyhow::bail!(
            "Linux user isolation requires root privileges or use_sudo=true. \
             Either run as root or enable use_sudo in config."
        );
    }

    /// Ensure the shared group exists.
    pub fn ensure_group(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if group_exists(&self.group)? {
            debug!("Group '{}' already exists", self.group);
            return Ok(());
        }

        info!("Creating group '{}'", self.group);
        run_privileged_command(self.use_sudo, "groupadd", &[&self.group])
            .context("creating group")?;

        Ok(())
    }

    /// Check if a Linux user exists for the given platform user.
    #[allow(dead_code)]
    pub fn user_exists(&self, user_id: &str) -> Result<bool> {
        let username = self.linux_username(user_id);
        linux_user_exists(&username)
    }

    /// Get the UID of a Linux user.
    #[allow(dead_code)]
    pub fn get_uid(&self, user_id: &str) -> Result<Option<u32>> {
        let username = self.linux_username(user_id);
        get_user_uid(&username)
    }

    /// Get the home directory of a Linux user.
    #[allow(dead_code)]
    pub fn get_home_dir(&self, user_id: &str) -> Result<Option<PathBuf>> {
        let username = self.linux_username(user_id);
        get_user_home(&username)
    }

    /// Create a Linux user for the given platform user.
    ///
    /// Returns the UID of the created user.
    pub fn create_user(&self, user_id: &str) -> Result<u32> {
        if !self.enabled {
            anyhow::bail!("Linux user isolation is not enabled");
        }

        let username = self.linux_username(user_id);

        // Check if user already exists
        if let Some(uid) = get_user_uid(&username)? {
            debug!("User '{}' already exists with UID {}", username, uid);
            return Ok(uid);
        }

        // Find next available UID
        let uid = self.find_next_uid()?;

        info!(
            "Creating Linux user '{}' with UID {} for platform user '{}'",
            username, uid, user_id
        );

        // Build useradd command
        let mut args = vec![
            "-u".to_string(),
            uid.to_string(),
            "-g".to_string(),
            self.group.clone(),
            "-s".to_string(),
            self.shell.clone(),
        ];

        if self.create_home {
            args.push("-m".to_string());
        } else {
            args.push("-M".to_string());
        }

        // Add comment with platform user ID for reference (sanitize for useradd compat)
        args.push("-c".to_string());
        args.push(sanitize_gecos(&format!("Octo platform user: {}", user_id)));

        args.push(username.clone());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        run_privileged_command(self.use_sudo, "useradd", &args_refs)
            .with_context(|| format!("creating user '{}'", username))?;

        info!("Created Linux user '{}' with UID {}", username, uid);
        Ok(uid)
    }

    /// Ensure a Linux user exists, creating it if necessary.
    /// Returns the UID of the user.
    pub fn ensure_user(&self, user_id: &str) -> Result<u32> {
        if !self.enabled {
            // Return current user's UID when not enabled
            return Ok(unsafe { libc::getuid() });
        }

        // Ensure group exists first
        self.ensure_group()?;

        // Create user if needed
        let uid = self.create_user(user_id)?;

        // Best-effort: ensure the per-user octo-runner daemon is enabled and started.
        // This is required for multi-user components that must run as the target Linux user
        // (e.g. per-user mmry instances, Pi runner mode).
        let username = self.linux_username(user_id);
        self.ensure_octo_runner_running(&username, uid)
            .with_context(|| format!("ensuring octo-runner for user '{}'", username))?;

        Ok(uid)
    }

    /// Ensure the per-user octo-runner daemon is enabled and started.
    fn ensure_octo_runner_running(&self, username: &str, uid: u32) -> Result<()> {
        // NOTE: do not attempt to create /run/octo/... via sudo here.
        // It must be provisioned at boot (tmpfiles) or during install.
        // Request-time privilege prompts would hang the backend.
        let base_dir = Path::new("/run/octo/runner-sockets");
        if !base_dir.exists() {
            anyhow::bail!(
                "runner socket base dir missing at {}. Install tmpfiles config (systemd/octo-runner.tmpfiles.conf) \
                 and run `sudo systemd-tmpfiles --create`, or create the directory as root with mode 2770 and group '{}'.",
                base_dir.display(),
                self.group
            );
        }

        // Fast path: if the runner socket already exists, we're good.
        // This avoids expensive privilege checks on every session creation.
        let expected_socket = base_dir.join(username).join("octo-runner.sock");
        if expected_socket.exists() {
            return Ok(());
        }

        // Ensure per-user socket directory exists.
        // If we're provisioning as root/sudo, we can set correct ownership. Otherwise,
        // we only create it when username==current user.
        let user_dir = base_dir.join(username);
        if !user_dir.exists() {
            let is_current_user = std::env::var("USER").ok().as_deref() == Some(username);
            if is_current_user {
                std::fs::create_dir_all(&user_dir)
                    .with_context(|| format!("creating {}", user_dir.display()))?;
                let _ =
                    std::fs::set_permissions(&user_dir, std::fs::Permissions::from_mode(0o2770));
            } else {
                let user_dir_str = user_dir
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("invalid user_dir path"))?;
                run_privileged_command(self.use_sudo, "mkdir", &["-p", user_dir_str])
                    .context("creating runner socket user dir")?;
                run_privileged_command(
                    self.use_sudo,
                    "chown",
                    &[&format!("{}:{}", username, self.group), user_dir_str],
                )
                .context("chown runner socket user dir")?;
                run_privileged_command(self.use_sudo, "chmod", &["2770", user_dir_str])
                    .context("chmod runner socket user dir")?;
            }
        }

        // Enable lingering so the user's systemd instance can run without login.
        // This is required for headless multi-user deployments.
        // Check if already enabled to avoid requiring sudo on every session.
        if !self.is_linger_enabled(username) {
            run_privileged_command(self.use_sudo, "loginctl", &["enable-linger", username])
                .context("enabling systemd linger")?;
        }

        // Ensure the user's systemd instance is running.
        // This is best-effort; if it fails, systemctl --user may still work depending on distro.
        let _ = run_privileged_command(
            self.use_sudo,
            "systemctl",
            &["start", &format!("user@{}.service", uid)],
        );

        // Enable + start the runner as that user.
        // We need to set the user bus environment explicitly to target the per-user manager.
        let runtime_dir = format!("/run/user/{}", uid);
        let bus = format!("unix:path={}/bus", runtime_dir);

        if let Err(e) = run_as_user(
            self.use_sudo,
            username,
            "systemctl",
            &["--user", "enable", "--now", "octo-runner"],
            &[
                ("XDG_RUNTIME_DIR", runtime_dir.as_str()),
                ("DBUS_SESSION_BUS_ADDRESS", bus.as_str()),
            ],
        ) {
            warn!(
                "Failed to enable/start octo-runner for {} via systemctl --user: {:?}",
                username, e
            );
        }

        // If the runner socket exists, we consider it good enough.
        if !expected_socket.exists() {
            anyhow::bail!(
                "octo-runner socket not found at {}",
                expected_socket.display()
            );
        }

        Ok(())
    }

    /// Check if systemd linger is already enabled for a user.
    fn is_linger_enabled(&self, username: &str) -> bool {
        // Check via loginctl show-user --value -p Linger
        // This doesn't require privileges.
        std::process::Command::new("loginctl")
            .args(["show-user", username, "-p", "Linger", "--value"])
            .output()
            .map(|out| {
                out.status.success()
                    && String::from_utf8_lossy(&out.stdout)
                        .trim()
                        .eq_ignore_ascii_case("yes")
            })
            .unwrap_or(false)
    }

    /// Find the next available UID starting from uid_start.
    fn find_next_uid(&self) -> Result<u32> {
        // Read /etc/passwd to find used UIDs
        let passwd = std::fs::read_to_string("/etc/passwd").context("reading /etc/passwd")?;

        let mut max_uid = self.uid_start - 1;

        for line in passwd.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                if let Ok(uid) = parts[2].parse::<u32>() {
                    if uid >= self.uid_start && uid > max_uid {
                        max_uid = uid;
                    }
                }
            }
        }

        Ok(max_uid + 1)
    }
}

/// Sanitize a user ID to be a valid Linux username.
/// Linux usernames must:
/// - Start with a lowercase letter or underscore
/// - Contain only lowercase letters, digits, underscores, or hyphens
/// - Be at most 32 characters
fn sanitize_username(user_id: &str) -> String {
    let mut result = String::with_capacity(32);

    for (i, c) in user_id.chars().enumerate() {
        if result.len() >= 32 {
            break;
        }

        let c = c.to_ascii_lowercase();

        if i == 0 {
            // First character must be letter or underscore
            if c.is_ascii_lowercase() || c == '_' {
                result.push(c);
            } else if c.is_ascii_digit() {
                result.push('_');
                result.push(c);
            } else {
                result.push('_');
            }
        } else {
            // Subsequent characters can be letter, digit, underscore, or hyphen
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
                result.push(c);
            } else {
                result.push('_');
            }
        }
    }

    if result.is_empty() {
        result.push_str("user");
    }

    result
}

/// Sanitize GECOS/comment field for useradd.
/// useradd (shadow) rejects ':' and control characters.
fn sanitize_gecos(input: &str) -> String {
    let mut cleaned = String::with_capacity(input.len());
    for c in input.chars() {
        if c == ':' || c == '\n' || c == '\r' || c == '\0' {
            cleaned.push(' ');
        } else {
            cleaned.push(c);
        }
    }
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Octo user".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Check if a group exists.
fn group_exists(group: &str) -> Result<bool> {
    let output = Command::new("getent")
        .args(["group", group])
        .output()
        .context("checking if group exists")?;

    Ok(output.status.success())
}

/// Check if a Linux user exists.
#[allow(dead_code)]
fn linux_user_exists(username: &str) -> Result<bool> {
    let output = Command::new("id")
        .arg(username)
        .output()
        .context("checking if user exists")?;

    Ok(output.status.success())
}

/// Get the UID of a Linux user.
fn get_user_uid(username: &str) -> Result<Option<u32>> {
    let output = Command::new("id")
        .args(["-u", username])
        .output()
        .context("getting user UID")?;

    if !output.status.success() {
        return Ok(None);
    }

    let uid_str = String::from_utf8_lossy(&output.stdout);
    let uid = uid_str.trim().parse::<u32>().context("parsing UID")?;

    Ok(Some(uid))
}

/// Get the home directory of a Linux user.
#[allow(dead_code)]
fn get_user_home(username: &str) -> Result<Option<PathBuf>> {
    let output = Command::new("getent")
        .args(["passwd", username])
        .output()
        .context("getting user home directory")?;

    if !output.status.success() {
        return Ok(None);
    }

    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = line.trim().split(':').collect();

    if parts.len() >= 6 {
        Ok(Some(PathBuf::from(parts[5])))
    } else {
        Ok(None)
    }
}

/// Run a command with optional sudo.
fn run_privileged_command(use_sudo: bool, cmd: &str, args: &[&str]) -> Result<()> {
    let is_root = unsafe { libc::geteuid() } == 0;

    let output = if use_sudo && !is_root {
        debug!("Running: sudo {} {:?}", cmd, args);
        Command::new("sudo")
            .arg("-n")
            .arg(cmd)
            .args(args)
            .output()
            .with_context(|| format!("running sudo {} {:?}", cmd, args))?
    } else {
        debug!("Running: {} {:?}", cmd, args);
        Command::new(cmd)
            .args(args)
            .output()
            .with_context(|| format!("running {} {:?}", cmd, args))?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Command failed: {} {:?}\nstderr: {}",
            cmd,
            args,
            stderr.trim()
        );
    }

    Ok(())
}

/// Run a command as a specific Linux user, with optional sudo, and environment overrides.
fn run_as_user(
    use_sudo: bool,
    username: &str,
    cmd: &str,
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<()> {
    let is_root = unsafe { libc::geteuid() } == 0;

    let mut command = if is_root {
        // Prefer runuser when root.
        let mut c = Command::new("runuser");
        c.args(["-u", username, "--", cmd]);
        c
    } else if use_sudo {
        let mut c = Command::new("sudo");
        c.args(["-n", "-u", username, cmd]);
        c
    } else {
        anyhow::bail!("must be root or have sudo enabled to run as another user");
    };

    command.args(args);
    for (k, v) in env {
        command.env(k, v);
    }

    debug!("Running as {}: {} {:?}", username, cmd, args);
    let output = command
        .output()
        .with_context(|| format!("running {} as user {}", cmd, username))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Command failed (as {}): {} {:?}\nstderr: {}",
            username,
            cmd,
            args,
            stderr.trim()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_username_simple() {
        assert_eq!(sanitize_username("alice"), "alice");
        assert_eq!(sanitize_username("bob123"), "bob123");
        assert_eq!(sanitize_username("user_name"), "user_name");
        assert_eq!(sanitize_username("user-name"), "user-name");
    }

    #[test]
    fn test_sanitize_username_uppercase() {
        assert_eq!(sanitize_username("Alice"), "alice");
        assert_eq!(sanitize_username("BOB"), "bob");
        assert_eq!(sanitize_username("MixedCase"), "mixedcase");
    }

    #[test]
    fn test_sanitize_username_starts_with_digit() {
        assert_eq!(sanitize_username("123user"), "_123user");
        assert_eq!(sanitize_username("1"), "_1");
    }

    #[test]
    fn test_sanitize_username_special_chars() {
        assert_eq!(sanitize_username("user@domain"), "user_domain");
        assert_eq!(sanitize_username("user.name"), "user_name");
        assert_eq!(sanitize_username("user name"), "user_name");
    }

    #[test]
    fn test_sanitize_username_empty() {
        assert_eq!(sanitize_username(""), "user");
    }

    #[test]
    fn test_sanitize_username_max_length() {
        let long_name = "a".repeat(50);
        let result = sanitize_username(&long_name);
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_linux_username() {
        let config = LinuxUsersConfig::default();
        assert_eq!(config.linux_username("alice"), "octo_alice");
        assert_eq!(config.linux_username("Bob"), "octo_bob");
        assert_eq!(
            config.linux_username("user@example.com"),
            "octo_user_example_com"
        );
    }

    #[test]
    fn test_linux_username_custom_prefix() {
        let config = LinuxUsersConfig {
            prefix: "workspace_".to_string(),
            ..Default::default()
        };
        assert_eq!(config.linux_username("alice"), "workspace_alice");
    }

    #[test]
    fn test_config_default() {
        let config = LinuxUsersConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.prefix, "octo_");
        assert_eq!(config.uid_start, 2000);
        assert_eq!(config.group, "octo");
        assert_eq!(config.shell, "/bin/bash");
        assert!(config.use_sudo);
        assert!(config.create_home);
    }

    #[test]
    fn test_config_serialization() {
        let config = LinuxUsersConfig {
            enabled: true,
            prefix: "test_".to_string(),
            uid_start: 3000,
            group: "testgroup".to_string(),
            shell: "/bin/zsh".to_string(),
            use_sudo: false,
            create_home: false,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: LinuxUsersConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.enabled, config.enabled);
        assert_eq!(parsed.prefix, config.prefix);
        assert_eq!(parsed.uid_start, config.uid_start);
        assert_eq!(parsed.group, config.group);
        assert_eq!(parsed.shell, config.shell);
        assert_eq!(parsed.use_sudo, config.use_sudo);
        assert_eq!(parsed.create_home, config.create_home);
    }
}

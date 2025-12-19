//! Linux user management for multi-user isolation.
//!
//! This module provides functionality to create and manage Linux users for
//! platform users, enabling proper process isolation in multi-user deployments.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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

impl LinuxUsersConfig {
    /// Get the Linux username for a platform user ID.
    pub fn linux_username(&self, user_id: &str) -> String {
        format!("{}{}", self.prefix, sanitize_username(user_id))
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
            // Check if sudo is available and we can use it without password
            let output = Command::new("sudo")
                .args(["-n", "true"])
                .output()
                .context("checking sudo availability")?;

            if output.status.success() {
                debug!("Passwordless sudo available for user management");
                return Ok(());
            }

            warn!(
                "sudo requires password. Linux user creation may prompt for password or fail. \
                 Consider adding NOPASSWD for user management commands in sudoers."
            );
            // Don't fail - we'll try anyway and fail gracefully if needed
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
    pub fn user_exists(&self, user_id: &str) -> Result<bool> {
        let username = self.linux_username(user_id);
        linux_user_exists(&username)
    }

    /// Get the UID of a Linux user.
    pub fn get_uid(&self, user_id: &str) -> Result<Option<u32>> {
        let username = self.linux_username(user_id);
        get_user_uid(&username)
    }

    /// Get the home directory of a Linux user.
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

        // Add comment with platform user ID for reference
        args.push("-c".to_string());
        args.push(format!("Octo platform user: {}", user_id));

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
        self.create_user(user_id)
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

    /// Set ownership of a directory to a Linux user.
    pub fn chown_directory(&self, path: &std::path::Path, user_id: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let username = self.linux_username(user_id);
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

/// Check if a group exists.
fn group_exists(group: &str) -> Result<bool> {
    let output = Command::new("getent")
        .args(["group", group])
        .output()
        .context("checking if group exists")?;

    Ok(output.status.success())
}

/// Check if a Linux user exists.
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
    let uid = uid_str
        .trim()
        .parse::<u32>()
        .context("parsing UID")?;

    Ok(Some(uid))
}

/// Get the home directory of a Linux user.
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
        assert_eq!(config.linux_username("user@example.com"), "octo_user_example_com");
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

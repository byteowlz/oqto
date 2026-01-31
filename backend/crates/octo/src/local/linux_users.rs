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
    /// Returns (UID, linux_username) of the project user.
    pub fn ensure_project_user(
        &self,
        project_id: &str,
        project_path: &std::path::Path,
    ) -> Result<(u32, String)> {
        if !self.enabled {
            // Return current user's UID when not enabled
            return Ok((unsafe { libc::getuid() }, project_id.to_string()));
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
            return Ok((uid, username));
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

        Ok((uid, username))
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
    ///
    /// Returns (UID, linux_username).
    pub fn ensure_effective_user(
        &self,
        user_id: &str,
        project_id: Option<&str>,
        project_path: Option<&std::path::Path>,
    ) -> Result<(u32, String)> {
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

    /// Generate a unique user ID that won't collide with existing Linux users.
    ///
    /// This should be called BEFORE creating the DB user to ensure the Linux username
    /// derived from this ID is available. Regenerates the ID if collision detected.
    ///
    /// Returns the user_id to use for both DB and Linux user creation.
    pub fn generate_unique_user_id(&self, username: &str) -> Result<String> {
        const MAX_ATTEMPTS: u32 = 10;

        for attempt in 0..MAX_ATTEMPTS {
            let user_id = crate::user::UserRepository::generate_user_id(username);
            let linux_username = self.linux_username(&user_id);

            // Check if this Linux username is available
            if let Some(_uid) = get_user_uid(&linux_username)? {
                // Linux user exists - check if it's ours (shouldn't happen for new registration)
                if let Some(gecos) = get_user_gecos(&linux_username)? {
                    if let Some(owner_id) = extract_user_id_from_gecos(&gecos) {
                        if owner_id == user_id {
                            // This is our user (idempotent retry) - ID is fine
                            debug!(
                                "Linux user '{}' already belongs to user_id '{}' (attempt {})",
                                linux_username,
                                user_id,
                                attempt + 1
                            );
                            return Ok(user_id);
                        }
                    }
                }
                // Collision with different owner - regenerate
                debug!(
                    "Linux username '{}' already exists, regenerating ID (attempt {})",
                    linux_username,
                    attempt + 1
                );
                continue;
            }

            // Username is available
            debug!(
                "Generated unique user_id '{}' -> linux username '{}' (attempt {})",
                user_id,
                linux_username,
                attempt + 1
            );
            return Ok(user_id);
        }

        anyhow::bail!(
            "Could not generate unique user_id for username '{}' after {} attempts",
            username,
            MAX_ATTEMPTS
        )
    }

    /// Verify that a Linux user matches the expected UID from the database.
    ///
    /// SECURITY: This is the primary ownership verification. UID is immutable by non-root
    /// users (unlike GECOS which can be changed via chfn), so this check cannot be bypassed.
    ///
    /// Returns Ok(()) if the UID matches, Err if mismatch or user doesn't exist.
    pub fn verify_linux_user_uid(&self, linux_username: &str, expected_uid: u32) -> Result<()> {
        if !self.enabled {
            return Ok(()); // No verification needed in single-user mode
        }

        let actual_uid = get_user_uid(linux_username)?
            .ok_or_else(|| anyhow::anyhow!("Linux user '{}' does not exist", linux_username))?;

        if actual_uid != expected_uid {
            anyhow::bail!(
                "SECURITY: Linux user '{}' UID mismatch! Expected {}, got {}. \
                 This could indicate an attack or misconfiguration.",
                linux_username,
                expected_uid,
                actual_uid
            );
        }

        Ok(())
    }

    /// Create a Linux user for the given platform user.
    ///
    /// Returns a tuple of (UID, actual_linux_username).
    ///
    /// SECURITY: Verifies ownership via GECOS field before returning an existing user's UID.
    /// If the Linux user exists but belongs to a different user_id, returns an error.
    /// Callers should use `generate_unique_user_id()` before DB user creation to avoid this.
    pub fn create_user(&self, user_id: &str) -> Result<(u32, String)> {
        if !self.enabled {
            anyhow::bail!("Linux user isolation is not enabled");
        }

        let username = self.linux_username(user_id);

        // Check if user already exists
        if let Some(uid) = get_user_uid(&username)? {
            // SECURITY: Verify this user belongs to the same platform user_id via GECOS
            if let Some(gecos) = get_user_gecos(&username)? {
                if let Some(owner_id) = extract_user_id_from_gecos(&gecos) {
                    if owner_id == user_id {
                        debug!(
                            "Linux user '{}' already exists with UID {} and belongs to user_id '{}'",
                            username, uid, user_id
                        );
                        return Ok((uid, username));
                    }
                    // SECURITY: Different owner - this should not happen if generate_unique_user_id was used
                    anyhow::bail!(
                        "Linux user '{}' belongs to different user_id '{}', expected '{}'",
                        username,
                        owner_id,
                        user_id
                    );
                }
            }
            // No GECOS or can't parse - user exists but we can't verify ownership.
            // This could be: a manually created user, a system user, or a race condition.
            // SECURITY: We cannot safely return this UID as it may belong to someone else.
            // The admin should either:
            // 1. Delete the conflicting Linux user, or
            // 2. Add proper GECOS: "Octo platform user: <user_id>"
            anyhow::bail!(
                "Linux user '{}' exists but has no valid Octo GECOS field. \
                 Cannot verify ownership for user_id '{}'. \
                 Either delete the Linux user or set GECOS to 'Octo platform user: {}'",
                username,
                user_id,
                user_id
            );
        }

        // Create the Linux user
        self.create_linux_user_internal(user_id, &username)
    }

    /// Internal helper to create a Linux user with the given username.
    /// Returns (uid, username).
    fn create_linux_user_internal(&self, user_id: &str, username: &str) -> Result<(u32, String)> {
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
        // This GECOS field is used to verify ownership on subsequent calls
        args.push("-c".to_string());
        args.push(sanitize_gecos(&format!("Octo platform user: {}", user_id)));

        args.push(username.to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        run_privileged_command(self.use_sudo, "useradd", &args_refs)
            .with_context(|| format!("creating user '{}'", username))?;

        info!("Created Linux user '{}' with UID {}", username, uid);
        Ok((uid, username.to_string()))
    }

    /// Ensure a Linux user exists, creating it if necessary.
    /// Returns (UID, actual_linux_username).
    ///
    /// The actual username may differ from `linux_username(user_id)` if a suffix was
    /// needed to avoid collision with another user. Callers should store this username
    /// in the database for future lookups.
    pub fn ensure_user(&self, user_id: &str) -> Result<(u32, String)> {
        self.ensure_user_with_verification(user_id, None, None)
    }

    /// Ensure a Linux user exists with optional UID verification.
    ///
    /// If `expected_linux_username` and `expected_uid` are provided (from DB), verifies
    /// the existing Linux user matches before returning. This prevents attacks where
    /// a user modifies their GECOS via chfn to impersonate another user.
    ///
    /// SECURITY: The UID check is the authoritative verification since UIDs cannot be
    /// changed by non-root users.
    pub fn ensure_user_with_verification(
        &self,
        user_id: &str,
        expected_linux_username: Option<&str>,
        expected_uid: Option<u32>,
    ) -> Result<(u32, String)> {
        if !self.enabled {
            // Return current user's UID and a placeholder username when not enabled
            return Ok((unsafe { libc::getuid() }, user_id.to_string()));
        }

        // If we have expected values from the DB, verify them first
        if let (Some(linux_username), Some(uid)) = (expected_linux_username, expected_uid) {
            // Verify the UID matches what's in the DB
            self.verify_linux_user_uid(linux_username, uid)?;

            // User exists and is verified - ensure runner is running
            self.ensure_group()?;
            self.ensure_octo_runner_running(linux_username, uid)
                .with_context(|| format!("ensuring octo-runner for user '{}'", linux_username))?;

            return Ok((uid, linux_username.to_string()));
        }

        // No expected values - this is a new user or legacy user without stored UID
        // Ensure group exists first
        self.ensure_group()?;

        // Create user if needed (returns actual username which may have suffix)
        let (uid, username) = self.create_user(user_id)?;

        // Best-effort: ensure the per-user octo-runner daemon is enabled and started.
        // This is required for multi-user components that must run as the target Linux user
        // (e.g. per-user mmry instances, Pi runner mode).
        self.ensure_octo_runner_running(&username, uid)
            .with_context(|| format!("ensuring octo-runner for user '{}'", username))?;

        Ok((uid, username))
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

        // Install the octo-runner systemd user service file if not present.
        // The user needs ~/.config/systemd/user/octo-runner.service for `systemctl --user enable`.
        self.install_runner_service_for_user(username, uid)
            .context("installing octo-runner.service for user")?;

        // Ensure the user's systemd instance is running.
        // This is required for systemctl --user to work.
        run_privileged_command(
            self.use_sudo,
            "systemctl",
            &["start", &format!("user@{}.service", uid)],
        )
        .context("starting user systemd instance")?;

        // Give the user's systemd instance a moment to initialize.
        // This is especially important for newly created users.
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Enable + start the runner as that user.
        // Try multiple approaches since the user may not have a D-Bus session yet.
        let runtime_dir = format!("/run/user/{}", uid);
        let bus = format!("unix:path={}/bus", runtime_dir);

        // Method 1: Use --machine=user@.host which connects via machinectl
        // This works without a local D-Bus session socket.
        let machine_arg = format!("{}@.host", username);
        let machine_result = run_privileged_command(
            self.use_sudo,
            "systemctl",
            &[
                "--machine",
                &machine_arg,
                "--user",
                "enable",
                "--now",
                "octo-runner",
            ],
        );

        if let Err(e) = &machine_result {
            debug!(
                "systemctl --machine failed for {}: {:?}, trying XDG_RUNTIME_DIR method",
                username, e
            );

            // Method 2: Set XDG_RUNTIME_DIR and DBUS_SESSION_BUS_ADDRESS explicitly
            if let Err(e2) = run_as_user(
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
                    "Failed to enable/start octo-runner for {} (both methods failed): \
                     machine method: {:?}, env method: {:?}",
                    username, e, e2
                );
            }
        }

        // Wait for the socket to appear (up to 5 seconds).
        // The service may take a moment to create its socket.
        for i in 0..10 {
            if expected_socket.exists() {
                debug!("octo-runner socket appeared after {}ms", i * 500);
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        // If the runner socket still doesn't exist, fail.
        anyhow::bail!(
            "octo-runner socket not found at {} after waiting 5s. \
             Check if octo-runner.service is properly installed for user {}.",
            expected_socket.display(),
            username
        )
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

    /// Install the octo-runner systemd user service file for a user.
    ///
    /// This creates ~/.config/systemd/user/octo-runner.service so that
    /// `systemctl --user enable octo-runner` can find the service.
    fn install_runner_service_for_user(&self, username: &str, uid: u32) -> Result<()> {
        // Get the user's home directory
        let home = get_user_home(username)?
            .ok_or_else(|| anyhow::anyhow!("could not find home directory for {}", username))?;
        let home_str = home.to_string_lossy();

        let service_dir = format!("{}/.config/systemd/user", home_str);
        let service_path = format!("{}/octo-runner.service", service_dir);

        // Check if already installed
        if Path::new(&service_path).exists() {
            debug!("octo-runner.service already installed for {}", username);
            return Ok(());
        }

        // Find the octo-runner binary
        let runner_path = find_octo_runner_binary()?;

        // Service file content
        let service_content = format!(
            r#"[Unit]
Description=Octo Runner - Process isolation daemon
After=default.target

[Service]
Type=simple
ExecStart={}
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
"#,
            runner_path.display()
        );

        // Create the directory and service file as the target user
        let is_current_user = std::env::var("USER").ok().as_deref() == Some(username);

        if is_current_user {
            std::fs::create_dir_all(&service_dir)
                .with_context(|| format!("creating {}", service_dir))?;
            std::fs::write(&service_path, &service_content)
                .with_context(|| format!("writing {}", service_path))?;
        } else {
            // Create directory as the target user
            run_as_user(self.use_sudo, username, "mkdir", &["-p", &service_dir], &[])
                .with_context(|| format!("creating {} as {}", service_dir, username))?;

            // Write the service file via a temp file and move
            // (we can't easily write file content via run_as_user)
            let temp_file = format!("/tmp/octo-runner-{}.service", uid);
            std::fs::write(&temp_file, &service_content).context("writing temp service file")?;

            // Copy and set ownership
            run_privileged_command(self.use_sudo, "cp", &[&temp_file, &service_path])
                .context("copying service file")?;
            run_privileged_command(
                self.use_sudo,
                "chown",
                &[&format!("{}:{}", username, username), &service_path],
            )
            .context("chown service file")?;

            // Clean up temp file
            let _ = std::fs::remove_file(&temp_file);

            // Reload systemd for the user to pick up the new service file
            let runtime_dir = format!("/run/user/{}", uid);
            let bus = format!("unix:path={}/bus", runtime_dir);
            let _ = run_as_user(
                self.use_sudo,
                username,
                "systemctl",
                &["--user", "daemon-reload"],
                &[
                    ("XDG_RUNTIME_DIR", runtime_dir.as_str()),
                    ("DBUS_SESSION_BUS_ADDRESS", bus.as_str()),
                ],
            );
        }

        info!("Installed octo-runner.service for user {}", username);
        Ok(())
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

/// Find the octo-runner binary.
///
/// Searches in common locations and PATH.
fn find_octo_runner_binary() -> Result<PathBuf> {
    // Try `which` first
    if let Ok(output) = Command::new("which").arg("octo-runner").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    // Check common locations
    let candidates = ["/usr/local/bin/octo-runner", "/usr/bin/octo-runner"];

    for path in candidates {
        if Path::new(path).exists() {
            return Ok(PathBuf::from(path));
        }
    }

    // Check ~/.cargo/bin for current user (development)
    if let Ok(home) = std::env::var("HOME") {
        let cargo_path = format!("{}/.cargo/bin/octo-runner", home);
        if Path::new(&cargo_path).exists() {
            return Ok(PathBuf::from(cargo_path));
        }
    }

    anyhow::bail!(
        "octo-runner binary not found. Install it with 'cargo install --path backend/crates/octo' \
         or copy to /usr/local/bin/"
    )
}

/// Get the GECOS field (comment) of a Linux user.
/// Used to verify which platform user_id owns a Linux account.
fn get_user_gecos(username: &str) -> Result<Option<String>> {
    let output = Command::new("getent")
        .args(["passwd", username])
        .output()
        .context("getting user GECOS field")?;

    if !output.status.success() {
        return Ok(None);
    }

    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = line.trim().split(':').collect();

    // passwd format: name:password:uid:gid:gecos:home:shell
    if parts.len() >= 5 {
        Ok(Some(parts[4].to_string()))
    } else {
        Ok(None)
    }
}

/// Extract the platform user_id from a GECOS field.
/// GECOS format: "Octo platform user: <user_id>"
fn extract_user_id_from_gecos(gecos: &str) -> Option<&str> {
    gecos.strip_prefix("Octo platform user: ").map(|s| s.trim())
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
///
/// Environment variables are passed to the target command using `env VAR=value cmd args...`
/// because sudo/runuser don't propagate the parent process environment by default.
fn run_as_user(
    use_sudo: bool,
    username: &str,
    cmd: &str,
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<()> {
    let is_root = unsafe { libc::geteuid() } == 0;

    // Build the actual command with environment variables using `env`.
    // Format: sudo/runuser -u user -- env VAR1=val1 VAR2=val2 cmd args...
    let mut command = if is_root {
        let mut c = Command::new("runuser");
        c.args(["-u", username, "--"]);
        c
    } else if use_sudo {
        let mut c = Command::new("sudo");
        c.args(["-n", "-u", username, "--"]);
        c
    } else {
        anyhow::bail!("must be root or have sudo enabled to run as another user");
    };

    // If we have environment variables, use `env` to set them
    if !env.is_empty() {
        command.arg("env");
        for (k, v) in env {
            command.arg(format!("{}={}", k, v));
        }
    }

    command.arg(cmd);
    command.args(args);

    debug!(
        "Running as {}: {} {:?} (env: {:?})",
        username, cmd, args, env
    );
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

//! Linux user management for multi-user isolation.
//!
//! This module provides functionality to create and manage Linux users for
//! platform users, enabling proper process isolation in multi-user deployments.

use anyhow::{Context, Result};
use log::{debug, info};
use rustix::process::{geteuid, getuid};
use serde::{Deserialize, Serialize};

use std::path::{Path, PathBuf};
use std::process::Command;
use toml::Value as TomlValue;

/// Configuration for Linux user isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LinuxUsersConfig {
    /// Enable Linux user isolation (requires root or sudo privileges).
    pub enabled: bool,
    /// Prefix for auto-created Linux usernames (e.g., "oqto_" -> "oqto_alice").
    pub prefix: String,
    /// Starting UID for new users. Users get sequential UIDs from this value.
    pub uid_start: u32,
    /// Shared group for all oqto users. Created if it doesn't exist.
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
            prefix: "oqto_".to_string(),
            uid_start: 2000,
            group: "oqto".to_string(),
            shell: "/bin/zsh".to_string(),
            use_sudo: true,
            create_home: true,
        }
    }
}

/// Prefix for project-based Linux users.
const PROJECT_PREFIX: &str = "proj_";

/// Check if a Linux user exists.
pub(crate) fn user_exists(username: &str) -> bool {
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
    /// - User: oqto_alice
    /// - Project: oqto_proj_myproject
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
            return Ok((getuid().as_raw(), project_id.to_string()));
        }

        // Note: There is intentionally no "fast path" optimization here.
        // User creation is a rare operation (happens once per user per device).
        // Always run through the full setup to ensure correctness and avoid race conditions.
        // - Always ensure group exists
        // - Always create the Linux user
        // - Always set up project directory ownership
        // - Always ensure oqto-runner is running
        // The expensive sudo operations are acceptable for this infrequent operation.

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
            "Oqto shared project: {}",
            project_id
        )));

        args.push(username.clone());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        run_privileged_command(self.use_sudo, "/usr/sbin/useradd", &args_refs)
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
            "/usr/bin/chown",
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

        let is_root = geteuid().is_root();

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

            if let Ok(out) = output
                && out.status.success()
            {
                debug!("Passwordless sudo available for user management helpers");
                return Ok(());
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
        run_privileged_command(self.use_sudo, "/usr/sbin/groupadd", &[&self.group])
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
                if let Some(gecos) = get_user_gecos(&linux_username)?
                    && let Some(owner_id) = extract_user_id_from_gecos(&gecos)
                    && owner_id == user_id
                {
                    // This is our user (idempotent retry) - ID is fine
                    debug!(
                        "Linux user '{}' already belongs to user_id '{}' (attempt {})",
                        linux_username,
                        user_id,
                        attempt + 1
                    );
                    return Ok(user_id);
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
            if let Some(gecos) = get_user_gecos(&username)?
                && let Some(owner_id) = extract_user_id_from_gecos(&gecos)
            {
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
            // No GECOS or can't parse - user exists but we can't verify ownership.
            // This could be: a manually created user, a system user, or a race condition.
            // SECURITY: We cannot safely return this UID as it may belong to someone else.
            // The admin should either:
            // 1. Delete the conflicting Linux user, or
            // 2. Add proper GECOS: "Oqto platform user <user_id>"
            anyhow::bail!(
                "Linux user '{}' exists but has no valid Oqto GECOS field. \
                 Cannot verify ownership for user_id '{}'. \
                 Either delete the Linux user or set GECOS to 'Oqto platform user {}'",
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
        args.push(sanitize_gecos(&format!("Oqto platform user {}", user_id)));

        args.push(username.to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        run_privileged_command(self.use_sudo, "/usr/sbin/useradd", &args_refs)
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
            return Ok((getuid().as_raw(), user_id.to_string()));
        }

        // If we have expected values from the DB, verify them first
        if let (Some(linux_username), Some(uid)) = (expected_linux_username, expected_uid) {
            // Verify the UID matches what's in the DB
            self.verify_linux_user_uid(linux_username, uid)?;

            // User exists and is verified - ensure runner is running
            self.ensure_group()?;
            self.ensure_oqto_runner_running(linux_username, uid)
                .with_context(|| format!("ensuring oqto-runner for user '{}'", linux_username))?;

            return Ok((uid, linux_username.to_string()));
        }

        // No expected values - this is a new user or legacy user without stored UID
        // Ensure group exists first
        self.ensure_group()?;

        // Create user if needed (returns actual username which may have suffix)
        let (uid, username) = self.create_user(user_id)?;

        // Ensure the per-user oqto-runner daemon is enabled and started.
        // This is required for multi-user components that must run as the target Linux user
        // (e.g. per-user mmry instances, Pi runner mode).
        self.ensure_oqto_runner_running(&username, uid)
            .with_context(|| {
                format!("starting oqto-runner for user '{}' (uid={})", username, uid)
            })?;

        Ok((uid, username))
    }

    /// Ensure the per-user oqto-runner daemon is enabled and started.
    fn ensure_oqto_runner_running(&self, username: &str, uid: u32) -> Result<()> {
        let base_dir = Path::new("/run/oqto/runner-sockets");
        if !base_dir.exists() {
            anyhow::bail!(
                "runner socket base dir missing at {}. Install tmpfiles config \
                 or create the directory as root with mode 2770 and group '{}'.",
                base_dir.display(),
                self.group
            );
        }

        let expected_socket = base_dir.join(username).join("oqto-runner.sock");

        // Fast path: if socket exists AND is connectable, everything is healthy.
        if expected_socket.exists() {
            if let Ok(_stream) = std::os::unix::net::UnixStream::connect(&expected_socket) {
                debug!("oqto-runner socket healthy: {}", expected_socket.display());
                return Ok(());
            }
            debug!(
                "oqto-runner socket exists but not connectable, re-running setup: {}",
                expected_socket.display()
            );
        }

        // Delegate to usermgr. It's idempotent:
        // - Always writes service files (picks up PATH/HOME/dependency changes)
        // - daemon-reloads
        // - Restarts if already running, starts if not
        // - Waits for socket to be ready
        // Only send username and uid -- the daemon constructs the service file
        // content server-side to prevent injection of arbitrary ExecStart commands.
        usermgr_request(
            "setup-user-runner",
            serde_json::json!({
                "username": username,
                "uid": uid,
            }),
        )
        .context("setup-user-runner via oqto-usermgr")?;

        // usermgr waits for the socket internally (up to 10s).
        // Quick verify that it's actually there.
        if expected_socket.exists() {
            debug!("oqto-runner socket ready: {}", expected_socket.display());
            return Ok(());
        }

        anyhow::bail!(
            "oqto-runner socket not found at {} after setup-user-runner completed. \
             Check oqto-runner.service logs for user {}.",
            expected_socket.display(),
            username
        )
    }

    /// Check if systemd linger is already enabled for a user.
    /// Ensure mmry config for a user points to the central embedding service.
    pub fn ensure_mmry_config_for_user(
        &self,
        linux_username: &str,
        uid: u32,
        host_service_url: &str,
        host_api_key: Option<&str>,
        default_model: &str,
        dimension: u16,
    ) -> Result<()> {
        if host_service_url.trim().is_empty() {
            return Ok(());
        }

        let home = get_user_home(linux_username)?.ok_or_else(|| {
            anyhow::anyhow!("could not find home directory for {}", linux_username)
        })?;
        let config_dir = home.join(".config").join("mmry");
        let config_path = config_dir.join("config.toml");

        if !config_path.exists() {
            run_as_user(self.use_sudo, linux_username, "mmry", &["init"], &[])
                .context("initializing mmry config")?;
        }

        let content = std::fs::read_to_string(&config_path).unwrap_or_default();
        let mut parsed: TomlValue = match toml::from_str(&content) {
            Ok(value) => value,
            Err(_) => {
                run_as_user(
                    self.use_sudo,
                    linux_username,
                    "mmry",
                    &["init", "--force"],
                    &[],
                )
                .context("resetting mmry config")?;
                let fresh = std::fs::read_to_string(&config_path)
                    .with_context(|| format!("reading {}", config_path.display()))?;
                toml::from_str(&fresh).context("parsing reset mmry config")?
            }
        };

        let embeddings = ensure_toml_table(&mut parsed, "embeddings");
        embeddings.insert("enabled".to_string(), TomlValue::Boolean(true));
        embeddings.insert(
            "model".to_string(),
            TomlValue::String(default_model.to_string()),
        );
        embeddings.insert(
            "dimension".to_string(),
            TomlValue::Integer(i64::from(dimension)),
        );

        let remote = ensure_toml_subtable(embeddings, "remote");
        remote.insert(
            "base_url".to_string(),
            TomlValue::String(host_service_url.to_string()),
        );
        match host_api_key {
            Some(key) => {
                remote.insert("api_key".to_string(), TomlValue::String(key.to_string()));
            }
            None => {
                remote.remove("api_key");
            }
        }
        remote.insert(
            "request_timeout_seconds".to_string(),
            TomlValue::Integer(30),
        );
        remote.insert("max_batch_size".to_string(), TomlValue::Integer(64));
        remote.insert("required".to_string(), TomlValue::Boolean(true));

        let output = format!(
            "# @schema ./config.schema.json\n{}",
            toml::to_string_pretty(&parsed).context("serializing mmry config")?
        );

        let is_current_user = std::env::var("USER").ok().as_deref() == Some(linux_username);
        if is_current_user {
            std::fs::create_dir_all(&config_dir)
                .with_context(|| format!("creating {}", config_dir.display()))?;
            std::fs::write(&config_path, output)
                .with_context(|| format!("writing {}", config_path.display()))?;
        } else {
            let temp_file = format!("/tmp/mmry-config-{}.toml", uid);
            let config_path_str = config_path.to_string_lossy().to_string();
            std::fs::write(&temp_file, output).context("writing temp mmry config")?;
            run_privileged_command(
                self.use_sudo,
                "/usr/bin/cp",
                &[&temp_file, &config_path_str],
            )
            .context("copying mmry config")?;
            run_privileged_command(
                self.use_sudo,
                "/usr/bin/chown",
                &[
                    &format!("{}:{}", linux_username, self.group),
                    &config_path_str,
                ],
            )
            .context("chown mmry config")?;
            let _ = std::fs::remove_file(&temp_file);
        }

        Ok(())
    }

    /// Find the next available UID starting from uid_start.
    fn find_next_uid(&self) -> Result<u32> {
        // UID 65534 is typically nobody, 65535 is often reserved
        const UID_MAX: u32 = 60000;

        // Read /etc/passwd to find used UIDs in our range
        let passwd = std::fs::read_to_string("/etc/passwd").context("reading /etc/passwd")?;

        let mut used_uids = std::collections::HashSet::new();
        for line in passwd.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3
                && let Ok(uid) = parts[2].parse::<u32>()
            {
                used_uids.insert(uid);
            }
        }

        // Find first available UID in range
        for uid in self.uid_start..=UID_MAX {
            if !used_uids.contains(&uid) {
                return Ok(uid);
            }
        }

        anyhow::bail!(
            "No available UIDs in range {}-{}. {} UIDs in use.",
            self.uid_start,
            UID_MAX,
            used_uids.len()
        )
    }

    /// Get the home directory for a linux username.
    /// Unlike `get_home_dir` which takes an oqto user_id, this takes the
    /// actual linux username directly.
    pub fn get_user_home(&self, linux_username: &str) -> Result<String> {
        get_user_home(linux_username)?
            .map(|p| p.to_string_lossy().to_string())
            .ok_or_else(|| anyhow::anyhow!("Home directory not found for user {}", linux_username))
    }

    /// Write a file into a user's directory, creating the directory if needed.
    /// Uses privileged commands when not root to write as the target user.
    pub fn write_file_as_user(
        &self,
        linux_username: &str,
        dir: &str,
        filename: &str,
        content: &str,
    ) -> Result<()> {
        let is_root = geteuid().is_root();
        let path = format!("{}/{}", dir, filename);

        if is_root {
            std::fs::create_dir_all(dir).with_context(|| format!("creating directory {}", dir))?;
            std::fs::write(&path, content).with_context(|| format!("writing {}", path))?;
            // chown to user
            run_privileged_command(
                false,
                "/usr/bin/chown",
                &[&format!("{}:{}", linux_username, self.group), &path],
            )?;
        } else {
            // Delegate to oqto-usermgr (runs as root) via write-file command.
            // The path must be relative to the user's home directory.
            let home = self.get_user_home(linux_username)?;
            let full_path = format!("{}/{}", dir, filename);
            let rel_path = full_path
                .strip_prefix(&format!("{}/", home))
                .with_context(|| format!("path {} is not under home {}", full_path, home))?;

            usermgr_request(
                "write-file",
                serde_json::json!({
                    "username": linux_username,
                    "path": rel_path,
                    "content": content,
                    "group": self.group,
                }),
            )
            .with_context(|| format!("write-file {} via oqto-usermgr", rel_path))?;
        }
        Ok(())
    }

    /// Set file permissions on a path owned by a user.
    pub fn chmod_file(&self, linux_username: &str, path: &str, mode: &str) -> Result<()> {
        let is_root = geteuid().is_root();
        if is_root {
            run_privileged_command(false, "/usr/bin/chmod", &[mode, path])
                .with_context(|| format!("chmod {} {}", mode, path))
        } else {
            // Delegate to usermgr
            usermgr_request(
                "chmod",
                serde_json::json!({
                    "username": linux_username,
                    "path": path,
                    "mode": mode,
                }),
            )
            .with_context(|| format!("chmod {} {} via oqto-usermgr", mode, path))
        }
    }

    /// Provision shell dotfiles (zsh + starship) for a platform user.
    ///
    /// Sends the `setup-user-shell` command to `oqto-usermgr`, which writes
    /// `.zshrc` and `~/.config/starship.toml` and changes the login shell.
    pub fn setup_user_shell(&self, linux_username: &str) -> Result<()> {
        usermgr_request(
            "setup-user-shell",
            serde_json::json!({
                "username": linux_username,
                "group": self.group,
                "shell": self.shell,
            }),
        )
        .with_context(|| format!("setup shell for {linux_username}"))
    }
}

fn ensure_toml_table<'a>(value: &'a mut TomlValue, key: &str) -> &'a mut toml::value::Table {
    if !value.is_table() {
        *value = TomlValue::Table(toml::value::Table::new());
    }

    let table = value.as_table_mut().expect("toml root table");
    table
        .entry(key.to_string())
        .or_insert_with(|| TomlValue::Table(toml::value::Table::new()));
    table
        .get_mut(key)
        .and_then(TomlValue::as_table_mut)
        .expect("toml subtable")
}

fn ensure_toml_subtable<'a>(
    parent: &'a mut toml::value::Table,
    key: &str,
) -> &'a mut toml::value::Table {
    parent
        .entry(key.to_string())
        .or_insert_with(|| TomlValue::Table(toml::value::Table::new()));
    parent
        .get_mut(key)
        .and_then(TomlValue::as_table_mut)
        .expect("toml nested subtable")
}

/// Sanitize a user ID to be a valid Linux username.
/// Linux usernames must:
/// - Start with a lowercase letter or underscore
/// - Contain only lowercase letters, digits, underscores, or hyphens
/// - Be at most 32 characters
pub(crate) fn sanitize_username(user_id: &str) -> String {
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

    while result.ends_with('-') && !result.is_empty() {
        result.pop();
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
        "Oqto user".to_string()
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
/// GECOS format: "Oqto platform user <user_id>" (colon removed by sanitize).
fn extract_user_id_from_gecos(gecos: &str) -> Option<&str> {
    let trimmed = gecos.trim();
    if let Some(rest) = trimmed.strip_prefix("Oqto platform user:") {
        return Some(rest.trim());
    }
    trimmed.strip_prefix("Oqto platform user").map(|s| s.trim())
}

/// Socket path for the oqto-usermgr daemon.
const USERMGR_SOCKET: &str = "/run/oqto/usermgr.sock";

/// Send a JSON request to the oqto-usermgr daemon and return the response.
pub fn usermgr_request(cmd: &str, args: serde_json::Value) -> Result<()> {
    usermgr_request_with_data(cmd, args).map(|_| ())
}

/// Send a request to oqto-usermgr and return the response data (if any).
pub fn usermgr_request_with_data(
    cmd: &str,
    args: serde_json::Value,
) -> Result<Option<serde_json::Value>> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(USERMGR_SOCKET)
        .with_context(|| format!("connecting to oqto-usermgr at {USERMGR_SOCKET}"))?;

    // Set timeout to avoid hanging forever
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(10)))
        .ok();

    let request = serde_json::json!({ "cmd": cmd, "args": args });
    let mut request_str = serde_json::to_string(&request)?;
    request_str.push('\n');

    stream
        .write_all(request_str.as_bytes())
        .context("writing to oqto-usermgr")?;
    stream.flush().context("flushing to oqto-usermgr")?;

    let mut reader = BufReader::new(&stream);
    let mut response_str = String::new();
    reader
        .read_line(&mut response_str)
        .context("reading from oqto-usermgr")?;

    let response: serde_json::Value =
        serde_json::from_str(&response_str).context("parsing oqto-usermgr response")?;

    if response.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        Ok(response.get("data").cloned())
    } else {
        let error = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        anyhow::bail!("oqto-usermgr {cmd}: {error}");
    }
}

/// Translate a system command into an oqto-usermgr request.
/// Returns Ok(()) if handled by usermgr, Err(None) if not handled.
fn try_usermgr(cmd: &str, args: &[&str]) -> Option<Result<()>> {
    match cmd {
        "/usr/sbin/groupadd" => {
            let group = args.first()?;
            Some(usermgr_request(
                "create-group",
                serde_json::json!({ "group": group }),
            ))
        }
        "/usr/sbin/useradd" => {
            // Parse useradd args
            let mut uid = None;
            let mut group = None;
            let mut shell = None;
            let mut gecos = None;
            let mut username = None;
            let mut create_home = true;
            let mut i = 0;
            while i < args.len() {
                match args[i] {
                    "-u" => {
                        uid = args.get(i + 1).and_then(|s| s.parse::<u32>().ok());
                        i += 2;
                    }
                    "-g" => {
                        group = args.get(i + 1).copied();
                        i += 2;
                    }
                    "-s" => {
                        shell = args.get(i + 1).copied();
                        i += 2;
                    }
                    "-c" => {
                        gecos = args.get(i + 1).copied();
                        i += 2;
                    }
                    "-m" => {
                        create_home = true;
                        i += 1;
                    }
                    "-M" => {
                        create_home = false;
                        i += 1;
                    }
                    _ => {
                        username = Some(args[i]);
                        i += 1;
                    }
                }
            }
            Some(usermgr_request(
                "create-user",
                serde_json::json!({
                    "username": username?,
                    "uid": uid?,
                    "group": group?,
                    "shell": shell?,
                    "gecos": gecos?,
                    "create_home": create_home,
                }),
            ))
        }
        "/usr/sbin/userdel" => {
            let username = args.first()?;
            Some(usermgr_request(
                "delete-user",
                serde_json::json!({ "username": username }),
            ))
        }
        "mkdir" | "/bin/mkdir" if args.first() == Some(&"-p") => {
            let path = args.get(1)?;
            Some(usermgr_request(
                "mkdir",
                serde_json::json!({ "path": path }),
            ))
        }
        "chown" | "/usr/bin/chown" => {
            if args.first() == Some(&"-R") && args.len() == 3 {
                Some(usermgr_request(
                    "chown",
                    serde_json::json!({
                        "owner": args[1],
                        "path": args[2],
                        "recursive": true,
                    }),
                ))
            } else if args.len() == 2 {
                Some(usermgr_request(
                    "chown",
                    serde_json::json!({
                        "owner": args[0],
                        "path": args[1],
                    }),
                ))
            } else {
                None
            }
        }
        "chmod" | "/usr/bin/chmod" if args.len() == 2 => Some(usermgr_request(
            "chmod",
            serde_json::json!({
                "mode": args[0],
                "path": args[1],
            }),
        )),
        "loginctl" | "/usr/bin/loginctl"
            if args.first() == Some(&"enable-linger") && args.len() == 2 =>
        {
            Some(usermgr_request(
                "enable-linger",
                serde_json::json!({ "username": args[1] }),
            ))
        }
        "/usr/bin/systemctl" if args.first() == Some(&"start") && args.len() == 2 => {
            if let Some(uid_str) = args[1]
                .strip_prefix("user@")
                .and_then(|s| s.strip_suffix(".service"))
                && let Ok(uid) = uid_str.parse::<u32>()
            {
                return Some(usermgr_request(
                    "start-user-service",
                    serde_json::json!({ "uid": uid }),
                ));
            }
            None
        }
        _ => None,
    }
}

/// Run a privileged command via oqto-usermgr daemon (preferred) or sudo fallback.
///
/// In multi-user mode, the oqto-usermgr daemon runs as root on a unix socket.
/// This provides OS-level privilege separation: even if the oqto process is
/// compromised, it cannot modify /etc/passwd or /home directly.
fn run_privileged_command(use_sudo: bool, cmd: &str, args: &[&str]) -> Result<()> {
    let is_root = geteuid().is_root();

    if is_root {
        // Running as root, execute directly
        debug!("Running (root): {} {:?}", cmd, args);
        let output = Command::new(cmd)
            .args(args)
            .output()
            .with_context(|| format!("running {} {:?}", cmd, args))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Command failed: {} {:?} -- {}", cmd, args, stderr.trim());
        }
        return Ok(());
    }

    // Try oqto-usermgr daemon first
    if let Some(result) = try_usermgr(cmd, args) {
        debug!("Via oqto-usermgr: {} {:?}", cmd, args);
        return result.with_context(|| format!("{} {:?}", cmd, args));
    }

    // Fallback to sudo for commands not handled by usermgr
    if use_sudo {
        debug!("Running: sudo {} {:?}", cmd, args);
        let output = Command::new("sudo")
            .arg("-n")
            .arg(cmd)
            .args(args)
            .output()
            .with_context(|| format!("running sudo {} {:?}", cmd, args))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output
                .status
                .code()
                .map_or("signal".to_string(), |c| c.to_string());
            tracing::error!(
                "Privileged command failed (exit {}): {} {:?}\nstderr: {}",
                exit_code,
                cmd,
                args,
                stderr.trim()
            );
            anyhow::bail!(
                "Command failed (exit {}): {} {:?} -- {}",
                exit_code,
                cmd,
                args,
                stderr.trim()
            );
        }
        return Ok(());
    }

    // No privilege mechanism available
    anyhow::bail!(
        "Cannot run privileged command {} {:?}: not root, no usermgr daemon, sudo disabled",
        cmd,
        args
    )
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
    let is_root = geteuid().is_root();

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
    fn test_sanitize_username_trailing_hyphen() {
        assert_eq!(sanitize_username("user-"), "user");
        assert_eq!(sanitize_username("tamara-WCC-"), "tamara-wcc");
        assert_eq!(sanitize_username("name-123-"), "name-123");
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
        assert_eq!(config.linux_username("alice"), "oqto_alice");
        assert_eq!(config.linux_username("Bob"), "oqto_bob");
        assert_eq!(
            config.linux_username("user@example.com"),
            "oqto_user_example_com"
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
        assert_eq!(config.prefix, "oqto_");
        assert_eq!(config.uid_start, 2000);
        assert_eq!(config.group, "oqto");
        assert_eq!(config.shell, "/bin/zsh");
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

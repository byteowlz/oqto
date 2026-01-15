//! Sandbox configuration and platform-specific wrappers.
//!
//! Provides process sandboxing using:
//! - **Linux**: bubblewrap (bwrap) for namespace isolation
//! - **macOS**: sandbox-exec with Seatbelt profiles
//!
//! ## Security Layers
//!
//! 1. **User isolation**: Process runs as a separate Linux user (Linux only)
//! 2. **Namespace/sandbox isolation**: Mount, PID, network via bwrap or sandbox-exec
//! 3. **Filesystem restrictions**: Only specified paths are accessible
//!
//! ## Configuration Hierarchy
//!
//! Sandbox config can be set at two levels:
//!
//! 1. **Global** (`config.toml`): Admin-controlled defaults and restrictions
//! 2. **Workspace** (`.octo/sandbox.toml`): Project-specific settings
//!
//! When merging configs, security restrictions are combined:
//! - `deny_read`: Union of both (global + workspace)
//! - `deny_write`: Union of both, plus `.octo/` is always denied
//! - `allow_write`: Intersection (must be allowed by both)
//! - `isolate_network`/`isolate_pid`: OR (if either enables, it's enabled)
//!
//! This ensures workspaces can only ADD restrictions, never remove them.
//!
//! ## Usage
//!
//! Global config (`config.toml`):
//! ```toml
//! [local.sandbox]
//! enabled = true
//! profile = "development"
//! ```
//!
//! Workspace config (`.octo/sandbox.toml`):
//! ```toml
//! # Request additional restrictions for this project
//! deny_read = ["~/.kube"]  # Added to global deny_read
//! isolate_network = true   # Override if global allows
//! ```

use anyhow::{Context, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[allow(unused_imports)]
use std::io::Write;

/// Sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SandboxConfig {
    /// Enable sandboxing.
    pub enabled: bool,

    /// Sandbox profile: "minimal", "development", "strict"
    pub profile: String,

    /// Paths to deny read access (always applied).
    pub deny_read: Vec<String>,

    /// Paths to allow write access (in addition to workspace).
    pub allow_write: Vec<String>,

    /// Paths to deny write access (takes precedence).
    pub deny_write: Vec<String>,

    /// Whether to isolate network (--unshare-net).
    pub isolate_network: bool,

    /// Whether to isolate PID namespace (--unshare-pid).
    pub isolate_pid: bool,

    /// Additional paths to bind read-only.
    pub extra_ro_bind: Vec<String>,

    /// Additional paths to bind read-write.
    pub extra_rw_bind: Vec<String>,
}

/// Path to global sandbox config file.
pub const GLOBAL_SANDBOX_CONFIG: &str = "~/.config/octo/sandbox.toml";

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: "development".to_string(),
            deny_read: vec![
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
                "~/.aws".to_string(),
            ],
            allow_write: vec![
                "~/.cargo".to_string(),
                "~/.rustup".to_string(),
                "~/.local/bin".to_string(),
                "~/.npm".to_string(),
                "~/.bun".to_string(),
                "~/.config/octo".to_string(), // Allow config.toml edits
                "/tmp".to_string(),
            ],
            // Always deny writes to sandbox configs - these are protected
            deny_write: vec!["~/.config/octo/sandbox.toml".to_string()],
            isolate_network: false,
            isolate_pid: true,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
        }
    }
}

impl SandboxConfig {
    /// Create a minimal sandbox config (just protect secrets).
    pub fn minimal() -> Self {
        Self {
            enabled: true,
            profile: "minimal".to_string(),
            deny_read: vec![
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
                "~/.aws".to_string(),
            ],
            allow_write: vec!["/tmp".to_string()],
            deny_write: vec![],
            isolate_network: false,
            isolate_pid: false,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
        }
    }

    /// Create a development sandbox config (protect secrets, allow tool installation).
    pub fn development() -> Self {
        Self::default()
    }

    /// Create a strict sandbox config (network isolation, limited writes).
    pub fn strict() -> Self {
        Self {
            enabled: true,
            profile: "strict".to_string(),
            deny_read: vec![
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
                "~/.aws".to_string(),
                "~/.config/octo".to_string(),
                "~/.kube".to_string(),
            ],
            allow_write: vec!["/tmp".to_string()],
            deny_write: vec![],
            isolate_network: true,
            isolate_pid: true,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
        }
    }

    /// Load config from profile name.
    pub fn from_profile(profile: &str) -> Self {
        match profile {
            "minimal" => Self::minimal(),
            "strict" => Self::strict(),
            _ => Self::development(),
        }
    }

    /// Load global sandbox config from `~/.config/octo/sandbox.toml`.
    ///
    /// Returns default config if file doesn't exist.
    /// Returns error only if file exists but can't be parsed.
    pub fn load_global() -> Result<Self> {
        let config_path = Self::expand_home(GLOBAL_SANDBOX_CONFIG);

        if !config_path.exists() {
            debug!(
                "No global sandbox config at {:?}, using defaults",
                config_path
            );
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read sandbox config from {:?}", config_path))?;

        let mut config: Self = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse sandbox config from {:?}", config_path))?;

        // Always ensure sandbox.toml itself is protected
        let sandbox_toml = "~/.config/octo/sandbox.toml".to_string();
        if !config.deny_write.contains(&sandbox_toml) {
            config.deny_write.push(sandbox_toml);
        }

        info!("Loaded global sandbox config from {:?}", config_path);
        Ok(config)
    }

    /// Load global config, then merge with workspace config.
    ///
    /// This is the main entry point for getting the effective sandbox config.
    pub fn load_for_workspace(workspace: &Path) -> Result<Self> {
        let global = Self::load_global()?;

        if !global.enabled {
            debug!("Sandbox disabled in global config");
            return Ok(global);
        }

        Ok(global.with_workspace_config(workspace))
    }

    /// Load workspace-specific sandbox config from `.octo/sandbox.toml`.
    ///
    /// Returns `None` if the file doesn't exist or can't be parsed.
    pub fn load_from_workspace(workspace: &Path) -> Option<Self> {
        let config_path = workspace.join(".octo").join("sandbox.toml");
        if !config_path.exists() {
            debug!("No workspace sandbox config at {:?}", config_path);
            return None;
        }

        match std::fs::read_to_string(&config_path) {
            Ok(contents) => match toml::from_str::<Self>(&contents) {
                Ok(config) => {
                    info!("Loaded workspace sandbox config from {:?}", config_path);
                    Some(config)
                }
                Err(e) => {
                    warn!(
                        "Failed to parse workspace sandbox config {:?}: {}",
                        config_path, e
                    );
                    None
                }
            },
            Err(e) => {
                warn!(
                    "Failed to read workspace sandbox config {:?}: {}",
                    config_path, e
                );
                None
            }
        }
    }

    /// Merge with workspace config, ensuring global restrictions are preserved.
    ///
    /// Security model:
    /// - `deny_read`: Union (workspace can add, not remove)
    /// - `deny_write`: Union (workspace can add, not remove)
    /// - `allow_write`: Intersection (must be allowed by BOTH)
    /// - `isolate_network`/`isolate_pid`: OR (if either enables, it's enabled)
    /// - `enabled`: OR (if either enables, it's enabled)
    ///
    /// This ensures workspaces can only ADD restrictions, never weaken security.
    pub fn merge_with_workspace(&self, workspace_config: &Self) -> Self {
        // Collect deny_read as union
        let mut deny_read: HashSet<String> = self.deny_read.iter().cloned().collect();
        deny_read.extend(workspace_config.deny_read.iter().cloned());

        // Collect deny_write as union
        let mut deny_write: HashSet<String> = self.deny_write.iter().cloned().collect();
        deny_write.extend(workspace_config.deny_write.iter().cloned());

        // allow_write is intersection (must be in both)
        let global_allow: HashSet<String> = self.allow_write.iter().cloned().collect();
        let workspace_allow: HashSet<String> =
            workspace_config.allow_write.iter().cloned().collect();
        let allow_write: Vec<String> = global_allow
            .intersection(&workspace_allow)
            .cloned()
            .collect();

        // extra binds are union (additive)
        let mut extra_ro_bind: HashSet<String> = self.extra_ro_bind.iter().cloned().collect();
        extra_ro_bind.extend(workspace_config.extra_ro_bind.iter().cloned());

        let mut extra_rw_bind: HashSet<String> = self.extra_rw_bind.iter().cloned().collect();
        extra_rw_bind.extend(workspace_config.extra_rw_bind.iter().cloned());

        Self {
            // Enable if either enables
            enabled: self.enabled || workspace_config.enabled,
            // Use workspace profile name if workspace is more specific
            profile: if workspace_config.profile != "development" {
                workspace_config.profile.clone()
            } else {
                self.profile.clone()
            },
            deny_read: deny_read.into_iter().collect(),
            deny_write: deny_write.into_iter().collect(),
            allow_write,
            // Isolation: OR (stricter wins)
            isolate_network: self.isolate_network || workspace_config.isolate_network,
            isolate_pid: self.isolate_pid || workspace_config.isolate_pid,
            extra_ro_bind: extra_ro_bind.into_iter().collect(),
            extra_rw_bind: extra_rw_bind.into_iter().collect(),
        }
    }

    /// Load and merge workspace config with this (global) config.
    ///
    /// Convenience method that combines `load_from_workspace` and `merge_with_workspace`.
    pub fn with_workspace_config(&self, workspace: &Path) -> Self {
        match Self::load_from_workspace(workspace) {
            Some(workspace_config) => {
                let merged = self.merge_with_workspace(&workspace_config);
                info!(
                    "Merged sandbox config: global + workspace, deny_read={}, allow_write={}, isolate_net={}, isolate_pid={}",
                    merged.deny_read.len(),
                    merged.allow_write.len(),
                    merged.isolate_network,
                    merged.isolate_pid
                );
                merged
            }
            None => self.clone(),
        }
    }

    /// Expand ~ to home directory in a path.
    fn expand_home(path: &str) -> PathBuf {
        if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(&path[2..]);
            }
        }
        PathBuf::from(path)
    }

    /// Build bwrap arguments for sandboxing a command.
    ///
    /// Returns None if bwrap is not available.
    pub fn build_bwrap_args(&self, workspace: &Path) -> Option<Vec<String>> {
        // Check if bwrap is available
        if !Self::is_bwrap_available() {
            warn!("bubblewrap (bwrap) not found, sandboxing disabled");
            return None;
        }

        let mut args = Vec::new();

        // Basic system directories (read-only)
        for dir in &["/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc"] {
            if Path::new(dir).exists() {
                args.push("--ro-bind".to_string());
                args.push(dir.to_string());
                args.push(dir.to_string());
            }
        }

        // /proc (needed for many tools)
        args.push("--proc".to_string());
        args.push("/proc".to_string());

        // /dev (minimal)
        args.push("--dev".to_string());
        args.push("/dev".to_string());

        // Home directory base (for tool directories)
        // Bind home read-only FIRST, then overlay writable paths on top
        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy().to_string();

            // Bind home read-only first
            args.push("--ro-bind".to_string());
            args.push(home_str.clone());
            args.push(home_str.clone());

            // Then bind writable directories on top
            for path in &self.allow_write {
                let expanded = Self::expand_home(path);
                if expanded.exists() || path.starts_with("~/") {
                    let expanded_str = expanded.to_string_lossy().to_string();
                    args.push("--bind".to_string());
                    args.push(expanded_str.clone());
                    args.push(expanded_str);
                }
            }

            // Block denied read paths by mounting empty tmpfs
            for path in &self.deny_read {
                let expanded = Self::expand_home(path);
                if expanded.exists() {
                    let expanded_str = expanded.to_string_lossy().to_string();
                    args.push("--tmpfs".to_string());
                    args.push(expanded_str);
                }
            }

            // Block denied write paths by binding read-only
            // Applied AFTER allow_write, so these take precedence
            for path in &self.deny_write {
                let expanded = Self::expand_home(path);
                if expanded.exists() {
                    let expanded_str = expanded.to_string_lossy().to_string();
                    args.push("--ro-bind".to_string());
                    args.push(expanded_str.clone());
                    args.push(expanded_str);
                    debug!("Deny-write path bound read-only: {}", path);
                }
            }
        }

        // Workspace directory (read-write) - MUST come after home ro-bind
        // so it takes precedence for paths under home
        let workspace_str = workspace.to_string_lossy().to_string();
        args.push("--bind".to_string());
        args.push(workspace_str.clone());
        args.push(workspace_str.clone());

        // SECURITY: Always bind .octo/ as read-only to prevent agents from
        // modifying their own sandbox configuration. This is applied AFTER
        // the workspace bind, so it takes precedence.
        let octo_dir = workspace.join(".octo");
        if octo_dir.exists() {
            let octo_dir_str = octo_dir.to_string_lossy().to_string();
            args.push("--ro-bind".to_string());
            args.push(octo_dir_str.clone());
            args.push(octo_dir_str);
            debug!("Bound .octo/ as read-only: {:?}", octo_dir);
        } else {
            // If .octo/ doesn't exist, mount an empty tmpfs to prevent creation
            // This blocks: mkdir .octo && echo "enabled=false" > .octo/sandbox.toml
            args.push("--tmpfs".to_string());
            args.push(octo_dir.to_string_lossy().to_string());
            debug!("Mounted empty tmpfs at .octo/ to prevent creation");
        }

        // /tmp (usually needed)
        args.push("--tmpfs".to_string());
        args.push("/tmp".to_string());

        // Extra read-only binds
        for path in &self.extra_ro_bind {
            let expanded = Self::expand_home(path);
            if expanded.exists() {
                let expanded_str = expanded.to_string_lossy().to_string();
                args.push("--ro-bind".to_string());
                args.push(expanded_str.clone());
                args.push(expanded_str);
            }
        }

        // Extra read-write binds
        for path in &self.extra_rw_bind {
            let expanded = Self::expand_home(path);
            if expanded.exists() {
                let expanded_str = expanded.to_string_lossy().to_string();
                args.push("--bind".to_string());
                args.push(expanded_str.clone());
                args.push(expanded_str);
            }
        }

        // Namespace isolation
        if self.isolate_pid {
            args.push("--unshare-pid".to_string());
        }

        if self.isolate_network {
            args.push("--unshare-net".to_string());
        }

        // Die with parent (important for cleanup)
        args.push("--die-with-parent".to_string());

        // Separator before command
        args.push("--".to_string());

        info!(
            "Sandbox enabled with profile '{}': {} args",
            self.profile,
            args.len()
        );
        debug!("bwrap args: {:?}", args);

        Some(args)
    }

    /// Check if bubblewrap is available.
    pub fn is_bwrap_available() -> bool {
        std::process::Command::new("bwrap")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Check if sandbox-exec is available (macOS).
    #[cfg(target_os = "macos")]
    pub fn is_sandbox_exec_available() -> bool {
        std::process::Command::new("sandbox-exec")
            .arg("-n")
            .arg("no-network") // A simple test profile
            .arg("true")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    pub fn is_sandbox_exec_available() -> bool {
        false
    }

    /// Generate a Seatbelt profile for macOS sandbox-exec.
    ///
    /// Returns the profile as a string in Seatbelt format.
    #[cfg(target_os = "macos")]
    pub fn build_seatbelt_profile(&self, workspace: &Path) -> String {
        let mut profile = String::new();

        // Header
        profile.push_str("(version 1)\n");

        // Default deny
        profile.push_str("(deny default)\n");

        // Allow basic process operations
        profile.push_str("(allow process-fork)\n");
        profile.push_str("(allow process-exec)\n");
        profile.push_str("(allow signal)\n");

        // Allow sysctl reads (needed by many tools)
        profile.push_str("(allow sysctl-read)\n");

        // Allow mach operations (needed for IPC)
        profile.push_str("(allow mach-lookup)\n");
        profile.push_str("(allow mach-priv-host-port)\n");

        // System directories (read-only)
        for dir in &["/usr", "/bin", "/sbin", "/Library", "/System"] {
            profile.push_str(&format!("(allow file-read* (subpath \"{}\"))\n", dir));
        }

        // Allow /dev access (needed for /dev/null, /dev/urandom, etc.)
        profile.push_str("(allow file-read* (subpath \"/dev\"))\n");
        profile.push_str("(allow file-write* (subpath \"/dev/null\"))\n");
        profile.push_str("(allow file-write* (subpath \"/dev/tty\"))\n");

        // Allow /private/tmp and /tmp
        profile.push_str("(allow file-read* (subpath \"/private/tmp\"))\n");
        profile.push_str("(allow file-write* (subpath \"/private/tmp\"))\n");
        profile.push_str("(allow file-read* (subpath \"/tmp\"))\n");
        profile.push_str("(allow file-write* (subpath \"/tmp\"))\n");

        // Workspace (read-write)
        let workspace_str = workspace.to_string_lossy();
        profile.push_str(&format!(
            "(allow file-read* (subpath \"{}\"))\n",
            workspace_str
        ));
        profile.push_str(&format!(
            "(allow file-write* (subpath \"{}\"))\n",
            workspace_str
        ));

        // SECURITY: Deny writes to .octo/ to prevent agents from modifying
        // their own sandbox configuration. Deny rules take precedence in Seatbelt.
        let octo_dir = workspace.join(".octo");
        let octo_dir_str = octo_dir.to_string_lossy();
        profile.push_str(&format!(
            "(deny file-write* (subpath \"{}\"))\n",
            octo_dir_str
        ));

        // Home directory (read-only by default)
        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy();
            profile.push_str(&format!("(allow file-read* (subpath \"{}\"))\n", home_str));

            // Writable directories
            for path in &self.allow_write {
                let expanded = Self::expand_home(path);
                let expanded_str = expanded.to_string_lossy();
                profile.push_str(&format!(
                    "(allow file-write* (subpath \"{}\"))\n",
                    expanded_str
                ));
            }

            // Denied directories (block even reads)
            for path in &self.deny_read {
                let expanded = Self::expand_home(path);
                let expanded_str = expanded.to_string_lossy();
                profile.push_str(&format!(
                    "(deny file-read* (subpath \"{}\"))\n",
                    expanded_str
                ));
                profile.push_str(&format!(
                    "(deny file-write* (subpath \"{}\"))\n",
                    expanded_str
                ));
            }

            // Deny-write paths (allow read, deny write)
            for path in &self.deny_write {
                let expanded = Self::expand_home(path);
                let expanded_str = expanded.to_string_lossy();
                profile.push_str(&format!(
                    "(deny file-write* (subpath \"{}\"))\n",
                    expanded_str
                ));
            }
        }

        // Network
        if self.isolate_network {
            profile.push_str("(deny network*)\n");
        } else {
            profile.push_str("(allow network*)\n");
        }

        debug!("Seatbelt profile:\n{}", profile);
        profile
    }

    /// Build sandbox-exec arguments for macOS.
    ///
    /// Creates a temporary file with the Seatbelt profile and returns
    /// the sandbox-exec arguments.
    #[cfg(target_os = "macos")]
    pub fn build_sandbox_exec_args(
        &self,
        workspace: &Path,
    ) -> Option<(Vec<String>, tempfile::NamedTempFile)> {
        if !Self::is_sandbox_exec_available() {
            warn!("sandbox-exec not available");
            return None;
        }

        let profile = self.build_seatbelt_profile(workspace);

        // Write profile to temp file
        let mut temp_file = match tempfile::NamedTempFile::new() {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to create temp file for Seatbelt profile: {}", e);
                return None;
            }
        };

        if let Err(e) = temp_file.write_all(profile.as_bytes()) {
            warn!("Failed to write Seatbelt profile: {}", e);
            return None;
        }

        let args = vec![
            "-f".to_string(),
            temp_file.path().to_string_lossy().to_string(),
        ];

        info!("macOS sandbox enabled with profile '{}'", self.profile);
        Some((args, temp_file))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn build_sandbox_exec_args(&self, _workspace: &Path) -> Option<(Vec<String>, ())> {
        None
    }

    /// Check if any sandbox is available on this platform.
    pub fn is_sandbox_available() -> bool {
        #[cfg(target_os = "linux")]
        {
            Self::is_bwrap_available()
        }
        #[cfg(target_os = "macos")]
        {
            Self::is_sandbox_exec_available()
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SandboxConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.profile, "development");
        assert!(config.deny_read.contains(&"~/.ssh".to_string()));
    }

    #[test]
    fn test_profiles() {
        let minimal = SandboxConfig::minimal();
        assert!(!minimal.isolate_network);
        assert!(!minimal.isolate_pid);

        let strict = SandboxConfig::strict();
        assert!(strict.isolate_network);
        assert!(strict.isolate_pid);
    }

    #[test]
    fn test_expand_home() {
        let expanded = SandboxConfig::expand_home("~/.ssh");
        assert!(expanded.to_string_lossy().contains(".ssh"));
        assert!(!expanded.to_string_lossy().starts_with("~"));

        let absolute = SandboxConfig::expand_home("/tmp");
        assert_eq!(absolute, PathBuf::from("/tmp"));
    }

    #[test]
    fn test_from_profile() {
        let dev = SandboxConfig::from_profile("development");
        assert_eq!(dev.profile, "development");

        let strict = SandboxConfig::from_profile("strict");
        assert_eq!(strict.profile, "strict");

        let unknown = SandboxConfig::from_profile("unknown");
        assert_eq!(unknown.profile, "development"); // Falls back to development
    }
}

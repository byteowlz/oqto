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
//! 2. **Workspace** (`.oqto/sandbox.toml`): Project-specific settings
//!
//! When merging configs, security restrictions are combined:
//! - `deny_read`: Union of both (global + workspace)
//! - `deny_write`: Union of both, plus `.oqto/` is always denied
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
//! Workspace config (`.oqto/sandbox.toml`):
//! ```toml
//! # Request additional restrictions for this project
//! deny_read = ["~/.kube"]  # Added to global deny_read
//! isolate_network = true   # Override if global allows
//! ```

use anyhow::{Context, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[allow(unused_imports)]
use std::io::Write;

// ============================================================================
// Guard (FUSE) Configuration
// ============================================================================

/// Policy for guarded path access.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum GuardPolicy {
    /// Auto-approve access, but log it.
    Auto,
    /// Prompt user for approval.
    #[default]
    Prompt,
    /// Deny access (redundant with deny_read, but explicit).
    Deny,
}

/// Configuration for oqto-guard (FUSE filesystem for runtime access control).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GuardConfig {
    /// Enable the guard FUSE filesystem.
    pub enabled: bool,

    /// Paths to expose via the guard (symlinked to FUSE mount).
    /// These paths are neither fully blocked nor fully allowed -
    /// access is controlled at runtime with user prompts.
    pub paths: Vec<String>,

    /// Per-path policy overrides.
    /// Keys are glob patterns (e.g., "~/.kube/*"), values are policies.
    #[serde(default)]
    pub policy: HashMap<String, GuardPolicy>,

    /// Timeout in seconds for user prompts.
    #[serde(default = "default_guard_timeout")]
    pub timeout_secs: u64,

    /// What to do when prompt times out.
    #[serde(default)]
    pub default_on_timeout: GuardPolicy,
}

fn default_guard_timeout() -> u64 {
    60
}

// ============================================================================
// SSH Proxy Configuration
// ============================================================================

/// Configuration for oqto-ssh-proxy (SSH agent proxy with policy).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SshProxyConfig {
    /// Enable the SSH agent proxy.
    pub enabled: bool,

    /// Allowed hosts (glob patterns).
    /// Empty means all hosts require prompting.
    pub allowed_hosts: Vec<String>,

    /// Allowed key identifiers (by comment or fingerprint).
    /// Empty means all keys are allowed.
    pub allowed_keys: Vec<String>,

    /// Prompt user for hosts not in allowed_hosts.
    #[serde(default = "default_true")]
    pub prompt_unknown: bool,

    /// Log all SSH sign requests.
    #[serde(default = "default_true")]
    pub log_connections: bool,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Network Configuration (integrates with eavs)
// ============================================================================

/// Network access mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    /// No network restrictions.
    #[default]
    Open,
    /// Full network isolation (bwrap --unshare-net).
    Isolated,
    /// Network via eavs proxy with domain filtering.
    Proxy,
}

/// Configuration for network access control.
/// When mode is "proxy", traffic goes through eavs with domain filtering.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkConfig {
    /// Network access mode.
    pub mode: NetworkMode,

    /// Allowed domains when mode is "proxy".
    /// Used to configure eavs filtering rules.
    pub allow_domains: Vec<String>,

    /// Log all network requests.
    #[serde(default)]
    pub log_requests: bool,
}

// ============================================================================
// Prompt Configuration
// ============================================================================

/// Configuration for how prompts are delivered to users.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PromptConfig {
    /// Enable desktop notifications as fallback when UI not connected.
    #[serde(default = "default_true")]
    pub desktop_notifications: bool,

    /// Auto-deny after this many seconds if no response channel available.
    #[serde(default = "default_prompt_timeout")]
    pub auto_deny_timeout_secs: u64,
}

fn default_prompt_timeout() -> u64 {
    30
}

// ============================================================================
// Sandbox Profile
// ============================================================================

/// A sandbox profile definition.
///
/// Profiles define the security settings for sandboxed processes.
/// Built-in profiles: "minimal", "development", "strict"
/// Custom profiles can be defined in `[profiles.<name>]` sections.
///
/// ## Security Layers
///
/// 1. **oqto-sandbox (bwrap)**: Hard deny/allow via namespace isolation
/// 2. **oqto-guard (FUSE)**: Runtime approval for "gray area" paths
/// 3. **oqto-ssh-proxy**: SSH access without exposing private keys
/// 4. **Network (eavs)**: Domain-level network filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SandboxProfile {
    // --- oqto-sandbox (bwrap) layer ---
    /// Paths to deny read access (always applied).
    pub deny_read: Vec<String>,

    /// Paths to allow write access (in addition to workspace).
    pub allow_write: Vec<String>,

    /// Paths to deny write access (takes precedence).
    pub deny_write: Vec<String>,

    /// Whether to isolate network (--unshare-net).
    /// Note: If network.mode is "proxy", this is handled differently.
    pub isolate_network: bool,

    /// Whether to isolate PID namespace (--unshare-pid).
    pub isolate_pid: bool,

    /// Additional paths to bind read-only.
    pub extra_ro_bind: Vec<String>,

    /// Additional paths to bind read-write.
    pub extra_rw_bind: Vec<String>,

    // --- oqto-guard (FUSE) layer ---
    /// Configuration for runtime file access control.
    #[serde(default)]
    pub guard: Option<GuardConfig>,

    // --- oqto-ssh-proxy layer ---
    /// Configuration for SSH agent proxy.
    #[serde(default)]
    pub ssh: Option<SshProxyConfig>,

    // --- Network (eavs integration) layer ---
    /// Configuration for network access control.
    #[serde(default)]
    pub network: Option<NetworkConfig>,

    // --- Prompt delivery ---
    /// Configuration for user prompts.
    #[serde(default)]
    pub prompts: Option<PromptConfig>,
}

impl Default for SandboxProfile {
    fn default() -> Self {
        Self::development()
    }
}

impl SandboxProfile {
    /// Create a minimal profile (least restrictive).
    pub fn minimal() -> Self {
        Self {
            deny_read: vec![
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
                "~/.aws".to_string(),
                "/usr/bin/systemctl".to_string(),
                "/bin/systemctl".to_string(),
                "/usr/bin/systemd-run".to_string(),
                "/bin/systemd-run".to_string(),
            ],
            allow_write: vec!["/tmp".to_string()],
            deny_write: vec![],
            isolate_network: false,
            isolate_pid: false,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
            guard: None,
            ssh: None,
            network: None,
            prompts: None,
        }
    }

    /// Create a development profile (default).
    pub fn development() -> Self {
        Self {
            deny_read: vec![
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
                "~/.aws".to_string(),
                "/usr/bin/systemctl".to_string(),
                "/bin/systemctl".to_string(),
                "/usr/bin/systemd-run".to_string(),
                "/bin/systemd-run".to_string(),
            ],
            allow_write: vec![
                // Package managers / toolchains
                "~/.cargo".to_string(),
                "~/.rustup".to_string(),
                "~/.npm".to_string(),
                "~/.bun".to_string(),
                "~/.local/bin".to_string(),
                "~/.local/share/uv".to_string(),
                "~/.cache/uv".to_string(),
                // Pi (Main Chat) - session files
                "~/.pi".to_string(),
                // Agent tools - data directories
                "~/.local/share/skdlr".to_string(),
                "~/.local/share/mmry".to_string(),
                "~/.local/share/mailz".to_string(),
                // Agent tools - config directories
                "~/.config/skdlr".to_string(),
                "~/.config/mmry".to_string(),
                "~/.config/mailz".to_string(),
                "~/.config/byt".to_string(),
                "~/.config/oqto".to_string(),
                "/tmp".to_string(),
            ],
            deny_write: vec!["~/.config/oqto/sandbox.toml".to_string()],
            isolate_network: false,
            isolate_pid: true,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
            // Development profile enables SSH proxy by default
            guard: None,
            ssh: Some(SshProxyConfig {
                enabled: true,
                allowed_hosts: vec!["github.com".to_string(), "gitlab.com".to_string()],
                allowed_keys: vec![],
                prompt_unknown: true,
                log_connections: true,
            }),
            network: Some(NetworkConfig {
                mode: NetworkMode::Open,
                allow_domains: vec![],
                log_requests: false,
            }),
            prompts: Some(PromptConfig {
                desktop_notifications: true,
                auto_deny_timeout_secs: 30,
            }),
        }
    }

    /// Create a strict profile (most restrictive).
    pub fn strict() -> Self {
        Self {
            deny_read: vec![
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
                "~/.aws".to_string(),
                "~/.config".to_string(),
                "/usr/bin/systemctl".to_string(),
                "/bin/systemctl".to_string(),
                "/usr/bin/systemd-run".to_string(),
                "/bin/systemd-run".to_string(),
            ],
            allow_write: vec!["/tmp".to_string()],
            deny_write: vec![],
            isolate_network: true,
            isolate_pid: true,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
            guard: None,
            ssh: Some(SshProxyConfig {
                enabled: false,
                ..Default::default()
            }),
            network: Some(NetworkConfig {
                mode: NetworkMode::Isolated,
                allow_domains: vec![],
                log_requests: false,
            }),
            prompts: None,
        }
    }

    /// Get a built-in profile by name.
    pub fn builtin(name: &str) -> Option<Self> {
        match name {
            "minimal" => Some(Self::minimal()),
            "development" => Some(Self::development()),
            "strict" => Some(Self::strict()),
            _ => None,
        }
    }
}

/// Sandbox configuration file structure.
///
/// This is what gets parsed from sandbox.toml. It contains:
/// - Top-level settings (enabled, profile)
/// - Optional custom profile definitions in `[profiles.<name>]`
///
/// Example:
/// ```toml
/// enabled = true
/// profile = "my-custom"
///
/// [profiles.my-custom]
/// deny_read = ["~/.ssh", "~/.gnupg", "~/.aws", "~/.kube"]
/// allow_write = ["~/.cargo", "/tmp"]
/// isolate_network = false
/// isolate_pid = true
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SandboxConfigFile {
    /// Enable sandboxing.
    pub enabled: bool,

    /// Which profile to use: "minimal", "development", "strict", or a custom name.
    pub profile: String,

    /// Custom profile definitions.
    /// Keys are profile names, values are profile settings.
    #[serde(default)]
    pub profiles: HashMap<String, SandboxProfile>,
}

/// Sandbox configuration (resolved).
///
/// This is the effective configuration after resolving the profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SandboxConfig {
    /// Enable sandboxing.
    pub enabled: bool,

    /// Sandbox profile name (for logging/debugging).
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

    /// Custom profiles loaded from config (for workspace merging).
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub profiles: HashMap<String, SandboxProfile>,
}

/// Path to user-level sandbox config file (for single-user mode).
pub const USER_SANDBOX_CONFIG: &str = "~/.config/oqto/sandbox.toml";

// Note: system-wide sandbox config loading is handled by `oqto-runner`.

impl Default for SandboxConfig {
    fn default() -> Self {
        let profile = SandboxProfile::development();
        Self {
            enabled: false,
            profile: "development".to_string(),
            deny_read: profile.deny_read,
            allow_write: profile.allow_write,
            deny_write: profile.deny_write,
            isolate_network: profile.isolate_network,
            isolate_pid: profile.isolate_pid,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            profiles: HashMap::new(),
        }
    }
}

impl From<SandboxConfigFile> for SandboxConfig {
    fn from(file: SandboxConfigFile) -> Self {
        let profile_name = if file.profile.is_empty() {
            "development"
        } else {
            &file.profile
        };

        // Resolve profile: check custom profiles first, then built-in
        let profile = file
            .profiles
            .get(profile_name)
            .cloned()
            .or_else(|| SandboxProfile::builtin(profile_name))
            .unwrap_or_else(|| {
                warn!(
                    "Unknown profile '{}', falling back to 'development'",
                    profile_name
                );
                SandboxProfile::development()
            });

        let mut config = Self {
            enabled: file.enabled,
            profile: profile_name.to_string(),
            deny_read: profile.deny_read,
            allow_write: profile.allow_write,
            deny_write: profile.deny_write,
            isolate_network: profile.isolate_network,
            isolate_pid: profile.isolate_pid,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            profiles: file.profiles,
        };

        // Always ensure sandbox.toml itself is protected
        let sandbox_toml = "~/.config/oqto/sandbox.toml".to_string();
        if !config.deny_write.contains(&sandbox_toml) {
            config.deny_write.push(sandbox_toml);
        }

        config
    }
}

impl SandboxConfig {
    /// Create a minimal sandbox config (least restrictive).
    pub fn minimal() -> Self {
        let profile = SandboxProfile::minimal();
        Self {
            enabled: true,
            profile: "minimal".to_string(),
            deny_read: profile.deny_read,
            allow_write: profile.allow_write,
            deny_write: profile.deny_write,
            isolate_network: profile.isolate_network,
            isolate_pid: profile.isolate_pid,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            profiles: HashMap::new(),
        }
    }

    /// Create a strict sandbox config (most restrictive).
    pub fn strict() -> Self {
        let profile = SandboxProfile::strict();
        Self {
            enabled: true,
            profile: "strict".to_string(),
            deny_read: profile.deny_read,
            allow_write: profile.allow_write,
            deny_write: profile.deny_write,
            isolate_network: profile.isolate_network,
            isolate_pid: profile.isolate_pid,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            profiles: HashMap::new(),
        }
    }

    /// Create a config from a named profile.
    ///
    /// Checks custom profiles in `self.profiles` first, then falls back to built-in profiles.
    pub fn from_profile(profile: &str) -> Self {
        Self::from_profile_with_custom(profile, &HashMap::new())
    }

    /// Create a config from a named profile, with custom profile definitions.
    pub fn from_profile_with_custom(
        profile_name: &str,
        custom_profiles: &HashMap<String, SandboxProfile>,
    ) -> Self {
        let profile = custom_profiles
            .get(profile_name)
            .cloned()
            .or_else(|| SandboxProfile::builtin(profile_name))
            .unwrap_or_else(|| {
                warn!(
                    "Unknown profile '{}', falling back to 'development'",
                    profile_name
                );
                SandboxProfile::development()
            });

        let mut config = Self {
            enabled: true,
            profile: profile_name.to_string(),
            deny_read: profile.deny_read,
            allow_write: profile.allow_write,
            deny_write: profile.deny_write,
            isolate_network: profile.isolate_network,
            isolate_pid: profile.isolate_pid,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            profiles: custom_profiles.clone(),
        };

        // Always ensure sandbox.toml itself is protected
        let sandbox_toml = "~/.config/oqto/sandbox.toml".to_string();
        if !config.deny_write.contains(&sandbox_toml) {
            config.deny_write.push(sandbox_toml);
        }

        config
    }

    /// Load user-level sandbox config from `~/.config/oqto/sandbox.toml`.
    ///
    /// Returns default config if file doesn't exist.
    /// Returns error only if file exists but can't be parsed.
    pub fn load_global() -> Result<Self> {
        Self::load_user_config()
    }

    /// Load user-level sandbox config from `~/.config/oqto/sandbox.toml`.
    ///
    /// Returns default config if file doesn't exist.
    /// Returns error only if file exists but can't be parsed.
    pub fn load_user_config() -> Result<Self> {
        let config_path = Self::expand_home(USER_SANDBOX_CONFIG);

        if !config_path.exists() {
            debug!(
                "No user sandbox config at {:?}, using defaults",
                config_path
            );
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read sandbox config from {:?}", config_path))?;

        let file: SandboxConfigFile = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse sandbox config from {:?}", config_path))?;

        let config: Self = file.into();

        info!(
            "Loaded user sandbox config from {:?}, profile='{}'",
            config_path, config.profile
        );
        Ok(config)
    }

    /// Load workspace-specific sandbox config from `.oqto/sandbox.toml`.
    ///
    /// The `global_profiles` parameter allows workspace configs to reference
    /// profiles defined in the global config.
    ///
    /// Returns `None` if the file doesn't exist or can't be parsed.
    pub fn load_from_workspace(
        workspace: &Path,
        global_profiles: &HashMap<String, SandboxProfile>,
    ) -> Option<Self> {
        let config_path = workspace.join(".oqto").join("sandbox.toml");
        if !config_path.exists() {
            debug!("No workspace sandbox config at {:?}", config_path);
            return None;
        }

        match std::fs::read_to_string(&config_path) {
            Ok(contents) => match toml::from_str::<SandboxConfigFile>(&contents) {
                Ok(file) => {
                    // Merge global profiles with workspace profiles (workspace takes precedence)
                    let mut merged_profiles = global_profiles.clone();
                    merged_profiles.extend(file.profiles.clone());

                    let profile_name = if file.profile.is_empty() {
                        "development"
                    } else {
                        &file.profile
                    };

                    // Resolve the profile
                    let profile = merged_profiles
                        .get(profile_name)
                        .cloned()
                        .or_else(|| SandboxProfile::builtin(profile_name))
                        .unwrap_or_else(|| {
                            warn!(
                                "Workspace references unknown profile '{}', using development",
                                profile_name
                            );
                            SandboxProfile::development()
                        });

                    let config = Self {
                        enabled: file.enabled,
                        profile: profile_name.to_string(),
                        deny_read: profile.deny_read,
                        allow_write: profile.allow_write,
                        deny_write: profile.deny_write,
                        isolate_network: profile.isolate_network,
                        isolate_pid: profile.isolate_pid,
                        extra_ro_bind: profile.extra_ro_bind,
                        extra_rw_bind: profile.extra_rw_bind,
                        profiles: merged_profiles,
                    };

                    info!(
                        "Loaded workspace sandbox config from {:?}, profile='{}'",
                        config_path, config.profile
                    );
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
    /// - `profiles`: Union (workspace can add profiles, not remove)
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

        // Merge profiles (workspace can add, global takes precedence for same name)
        let mut profiles = workspace_config.profiles.clone();
        profiles.extend(self.profiles.clone());

        Self {
            // Enable if either enables
            enabled: self.enabled || workspace_config.enabled,
            // Use workspace profile name if workspace specifies one
            profile: if !workspace_config.profile.is_empty()
                && workspace_config.profile != "development"
            {
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
            profiles,
        }
    }

    /// Load and merge workspace config with this (global) config.
    ///
    /// Convenience method that combines `load_from_workspace` and `merge_with_workspace`.
    ///
    /// Workspace configs can:
    /// - Reference profiles defined in global config
    /// - Define their own profiles (for local use only)
    /// - Add restrictions (deny_read, deny_write, isolation)
    ///
    /// Workspace configs CANNOT:
    /// - Weaken global restrictions (allow_write is intersected)
    /// - Disable isolation if global enables it
    pub fn with_workspace_config(&self, workspace: &Path) -> Self {
        match Self::load_from_workspace(workspace, &self.profiles) {
            Some(workspace_config) => {
                let merged = self.merge_with_workspace(&workspace_config);
                info!(
                    "Merged sandbox config: global + workspace, profile='{}', deny_read={}, allow_write={}, isolate_net={}, isolate_pid={}",
                    merged.profile,
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
    /// Uses the current user's home directory.
    fn expand_home(path: &str) -> PathBuf {
        Self::expand_home_for_user(path, None)
    }

    /// Expand ~ to home directory for a specific user.
    /// If username is None, uses the current user's home directory.
    fn expand_home_for_user(path: &str, username: Option<&str>) -> PathBuf {
        if let Some(rest) = path.strip_prefix("~/") {
            let home = if let Some(user) = username {
                Self::get_user_home(user)
            } else {
                dirs::home_dir()
            };

            if let Some(home) = home {
                let expanded = home.join(rest);
                debug!(
                    "Expanded path '{}' to '{}' for user {:?}",
                    path,
                    expanded.display(),
                    username.unwrap_or("(current)")
                );
                return expanded;
            } else {
                warn!(
                    "Could not determine home directory for user {:?}, using path as-is: {}",
                    username, path
                );
            }
        }
        PathBuf::from(path)
    }

    /// Get home directory for a specific user by looking up passwd.
    fn get_user_home(username: &str) -> Option<PathBuf> {
        use std::ffi::CString;

        let c_username = CString::new(username).ok()?;

        // SAFETY: getpwnam is thread-safe for reading, we only read the pw_dir field
        let passwd = unsafe { libc::getpwnam(c_username.as_ptr()) };

        if passwd.is_null() {
            warn!("User '{}' not found in passwd database", username);
            return None;
        }

        // SAFETY: passwd is valid and pw_dir is a valid C string
        let home_cstr = unsafe { std::ffi::CStr::from_ptr((*passwd).pw_dir) };
        let home_str = home_cstr.to_str().ok()?;

        debug!("Resolved home directory for '{}': {}", username, home_str);
        Some(PathBuf::from(home_str))
    }

    /// Build bwrap arguments for sandboxing a command for a specific user.
    ///
    /// If `username` is Some, paths like `~/.config` will be expanded to
    /// that user's home directory instead of the current user's.
    ///
    /// Returns None if bwrap is not available.
    pub fn build_bwrap_args_for_user(
        &self,
        workspace: &Path,
        username: Option<&str>,
    ) -> Option<Vec<String>> {
        info!(
            "Building bwrap args: workspace={}, target_user={:?}, profile={}",
            workspace.display(),
            username.unwrap_or("(current)"),
            self.profile
        );

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
        debug!("Added system directories as read-only binds");

        // /proc (needed for many tools)
        args.push("--proc".to_string());
        args.push("/proc".to_string());

        // /dev (minimal)
        args.push("--dev".to_string());
        args.push("/dev".to_string());

        // Determine target user's home directory
        let target_home = if let Some(user) = username {
            Self::get_user_home(user)
        } else {
            dirs::home_dir()
        };

        // Home directory binding strategy:
        // - Development/minimal profiles: bind home read-write, protect sensitive paths via deny_read
        // - Strict profile: bind home read-only, overlay specific allow_write paths
        //
        // The development approach is more permissive but simpler - agents can write anywhere
        // in home except explicitly denied paths. oqto-guard provides additional runtime control.
        let home_writable = self.profile == "development" || self.profile == "minimal";

        if let Some(ref home) = target_home {
            let home_str = home.to_string_lossy().to_string();
            info!(
                "Using home directory '{}' for user {:?}",
                home_str,
                username.unwrap_or("(current)")
            );

            if home_writable {
                // Development mode: bind home read-write, rely on deny_read for protection
                args.push("--bind".to_string());
                args.push(home_str.clone());
                args.push(home_str.clone());
                debug!(
                    "Bound home directory '{}' as read-write (profile={})",
                    home_str, self.profile
                );
            } else {
                // Strict mode: bind home read-only first
                args.push("--ro-bind".to_string());
                args.push(home_str.clone());
                args.push(home_str.clone());
                debug!(
                    "Bound home directory '{}' as read-only (profile={})",
                    home_str, self.profile
                );

                // Then bind writable directories on top
                for path in &self.allow_write {
                    let expanded = Self::expand_home_for_user(path, username);
                    let expanded_str = expanded.to_string_lossy().to_string();

                    // For paths under home, always add them (bwrap will create if needed)
                    // For absolute paths like /tmp, check existence
                    if path.starts_with("~/") || expanded.exists() {
                        args.push("--bind".to_string());
                        args.push(expanded_str.clone());
                        args.push(expanded_str.clone());
                        debug!(
                            "Allow-write: '{}' -> '{}' (exists: {})",
                            path,
                            expanded_str,
                            expanded.exists()
                        );
                    } else {
                        debug!(
                            "Skipping allow-write '{}' -> '{}' (path does not exist)",
                            path, expanded_str
                        );
                    }
                }
            }
        } else {
            warn!(
                "Could not determine home directory for user {:?}, home-based paths will not be bound",
                username
            );
        }

        // Workspace directory (read-write) - MUST come after home ro-bind
        // so it takes precedence for paths under home
        let workspace_str = workspace.to_string_lossy().to_string();
        args.push("--bind".to_string());
        args.push(workspace_str.clone());
        args.push(workspace_str.clone());
        debug!("Bound workspace '{}' as read-write", workspace_str);

        // Ensure sandboxed processes start in the workspace directory.
        args.push("--chdir".to_string());
        args.push(workspace_str.clone());
        debug!("Set sandbox working directory to '{}'", workspace_str);

        // SECURITY: Always bind .oqto/ as read-only to prevent agents from
        // modifying their own sandbox configuration. This is applied AFTER
        // the workspace bind, so it takes precedence.
        let oqto_dir = workspace.join(".oqto");
        if oqto_dir.exists() {
            let oqto_dir_str = oqto_dir.to_string_lossy().to_string();
            args.push("--ro-bind".to_string());
            args.push(oqto_dir_str.clone());
            args.push(oqto_dir_str);
            debug!("Bound .oqto/ as read-only: {:?}", oqto_dir);
        } else {
            // If .oqto/ doesn't exist, mount an empty tmpfs to prevent creation
            // This blocks: mkdir .oqto && echo "enabled=false" > .oqto/sandbox.toml
            args.push("--tmpfs".to_string());
            args.push(oqto_dir.to_string_lossy().to_string());
            debug!("Mounted empty tmpfs at .oqto/ to prevent creation");
        }

        // Apply deny rules AFTER workspace bind so they always take precedence,
        // even when the workspace is the user's home directory.
        if target_home.is_some() {
            // Block denied read paths by mounting empty tmpfs (dirs) or masking files.
            for path in &self.deny_read {
                let expanded = Self::expand_home_for_user(path, username);
                if expanded.exists() {
                    let expanded_str = expanded.to_string_lossy().to_string();
                    let is_dir = expanded
                        .metadata()
                        .map(|meta| meta.is_dir())
                        .unwrap_or(false);
                    if is_dir {
                        args.push("--tmpfs".to_string());
                        args.push(expanded_str.clone());
                        debug!("Deny-read (tmpfs): '{}' -> '{}'", path, expanded_str);
                    } else {
                        // Mask file paths by binding /dev/null over them.
                        args.push("--bind".to_string());
                        args.push("/dev/null".to_string());
                        args.push(expanded_str.clone());
                        debug!("Deny-read (file mask): '{}' -> '{}'", path, expanded_str);
                    }
                } else {
                    debug!(
                        "Skipping deny-read '{}' (path does not exist for user {:?})",
                        path,
                        username.unwrap_or("(current)")
                    );
                }
            }

            // Block denied write paths by binding read-only.
            // Applied AFTER allow_write/workspace bind, so these take precedence.
            for path in &self.deny_write {
                let expanded = Self::expand_home_for_user(path, username);
                if expanded.exists() {
                    let expanded_str = expanded.to_string_lossy().to_string();
                    args.push("--ro-bind".to_string());
                    args.push(expanded_str.clone());
                    args.push(expanded_str.clone());
                    debug!("Deny-write (ro-bind): '{}' -> '{}'", path, expanded_str);
                }
            }
        }

        // /tmp (usually needed)
        args.push("--tmpfs".to_string());
        args.push("/tmp".to_string());

        // Extra read-only binds
        for path in &self.extra_ro_bind {
            let expanded = Self::expand_home_for_user(path, username);
            if expanded.exists() {
                let expanded_str = expanded.to_string_lossy().to_string();
                args.push("--ro-bind".to_string());
                args.push(expanded_str.clone());
                args.push(expanded_str.clone());
                debug!("Extra ro-bind: '{}' -> '{}'", path, expanded_str);
            }
        }

        // Extra read-write binds
        for path in &self.extra_rw_bind {
            let expanded = Self::expand_home_for_user(path, username);
            if expanded.exists() {
                let expanded_str = expanded.to_string_lossy().to_string();
                args.push("--bind".to_string());
                args.push(expanded_str.clone());
                args.push(expanded_str.clone());
                debug!("Extra rw-bind: '{}' -> '{}'", path, expanded_str);
            }
        }

        // Namespace isolation
        if self.isolate_pid {
            args.push("--unshare-pid".to_string());
            debug!("PID namespace isolation enabled");
        }

        if self.isolate_network {
            args.push("--unshare-net".to_string());
            debug!("Network namespace isolation enabled");
        }

        // Die with parent (important for cleanup)
        args.push("--die-with-parent".to_string());

        // Separator before command
        args.push("--".to_string());

        info!(
            "Sandbox configured: profile='{}', user={:?}, workspace='{}', {} bwrap args",
            self.profile,
            username.unwrap_or("(current)"),
            workspace.display(),
            args.len()
        );
        debug!("Full bwrap args: {:?}", args);

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

    // macOS seatbelt/sandbox-exec helpers are implemented in the `oqto-sandbox` CLI.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::tempdir;

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

        // Unknown profiles keep their name but use development settings
        let unknown = SandboxConfig::from_profile("unknown");
        assert_eq!(unknown.profile, "unknown");
        // But should have development's settings
        assert!(unknown.isolate_pid); // development has isolate_pid=true
        assert!(!unknown.isolate_network); // development has isolate_network=false
    }

    #[test]
    fn test_custom_profile_parsing() {
        let toml_content = r#"
enabled = true
profile = "my-custom"

[profiles.my-custom]
deny_read = ["~/.ssh", "~/.kube"]
allow_write = ["/tmp"]
isolate_network = true
isolate_pid = false
"#;

        let file: SandboxConfigFile = toml::from_str(toml_content).unwrap();
        assert_eq!(file.profile, "my-custom");
        assert!(file.profiles.contains_key("my-custom"));

        let config: SandboxConfig = file.into();
        assert!(config.enabled);
        assert_eq!(config.profile, "my-custom");
        assert!(config.deny_read.contains(&"~/.ssh".to_string()));
        assert!(config.deny_read.contains(&"~/.kube".to_string()));
        assert!(config.isolate_network);
        assert!(!config.isolate_pid);
    }

    #[test]
    fn test_custom_profile_with_builtin_reference() {
        // Workspace can reference a built-in profile
        let toml_content = r#"
enabled = true
profile = "strict"
"#;

        let file: SandboxConfigFile = toml::from_str(toml_content).unwrap();
        let config: SandboxConfig = file.into();

        assert_eq!(config.profile, "strict");
        assert!(config.isolate_network);
        assert!(config.isolate_pid);
    }

    #[test]
    fn test_multiple_custom_profiles() {
        let toml_content = r#"
enabled = true
profile = "airgapped"

[profiles.airgapped]
deny_read = ["~/.ssh"]
allow_write = ["/tmp"]
isolate_network = true
isolate_pid = true

[profiles.relaxed]
deny_read = []
allow_write = ["~/.cargo", "~/.npm", "/tmp"]
isolate_network = false
isolate_pid = false
"#;

        let file: SandboxConfigFile = toml::from_str(toml_content).unwrap();
        assert_eq!(file.profiles.len(), 2);

        let config: SandboxConfig = file.into();
        assert_eq!(config.profile, "airgapped");
        assert!(config.isolate_network);

        // Can also create config from the other profile
        let relaxed = SandboxConfig::from_profile_with_custom("relaxed", &config.profiles);
        assert_eq!(relaxed.profile, "relaxed");
        assert!(!relaxed.isolate_network);
    }

    #[test]
    fn test_sandbox_toml_always_protected() {
        let toml_content = r#"
enabled = true
profile = "minimal"

[profiles.minimal]
deny_read = ["~/.ssh"]
allow_write = ["/tmp"]
deny_write = []
"#;

        let file: SandboxConfigFile = toml::from_str(toml_content).unwrap();
        let config: SandboxConfig = file.into();

        // sandbox.toml should always be in deny_write, even if not specified
        assert!(
            config
                .deny_write
                .contains(&"~/.config/oqto/sandbox.toml".to_string())
        );
    }

    #[test]
    fn test_unknown_profile_fallback() {
        let toml_content = r#"
enabled = true
profile = "nonexistent"
"#;

        let file: SandboxConfigFile = toml::from_str(toml_content).unwrap();
        let config: SandboxConfig = file.into();

        // Unknown profile keeps its name but uses development settings
        assert_eq!(config.profile, "nonexistent");
        // Verify it got development's settings
        assert!(config.isolate_pid); // development has isolate_pid=true
        assert!(!config.isolate_network); // development has isolate_network=false
    }

    #[test]
    fn test_profile_with_guard_config() {
        let toml_content = r#"
enabled = true
profile = "guarded"

[profiles.guarded]
deny_read = ["~/.ssh", "~/.gnupg"]
allow_write = ["/tmp"]
isolate_pid = true

[profiles.guarded.guard]
enabled = true
paths = ["~/.kube", "~/.docker"]
timeout_secs = 120

[profiles.guarded.guard.policy]
"~/.kube/config" = "prompt"
"~/.docker/*" = "auto"
"#;

        let file: SandboxConfigFile = toml::from_str(toml_content).unwrap();
        let config: SandboxConfig = file.into();

        assert_eq!(config.profile, "guarded");

        // Check guard config was parsed
        let profile = config.profiles.get("guarded").unwrap();
        let guard = profile.guard.as_ref().unwrap();
        assert!(guard.enabled);
        assert_eq!(guard.paths.len(), 2);
        assert!(guard.paths.contains(&"~/.kube".to_string()));
        assert_eq!(guard.timeout_secs, 120);
        assert_eq!(
            guard.policy.get("~/.kube/config"),
            Some(&GuardPolicy::Prompt)
        );
        assert_eq!(guard.policy.get("~/.docker/*"), Some(&GuardPolicy::Auto));
    }

    #[test]
    fn test_profile_with_ssh_config() {
        let toml_content = r#"
enabled = true
profile = "ssh-enabled"

[profiles.ssh-enabled]
deny_read = ["~/.ssh"]
allow_write = ["/tmp"]

[profiles.ssh-enabled.ssh]
enabled = true
allowed_hosts = ["github.com", "gitlab.com", "*.corp.internal"]
allowed_keys = ["work_key"]
prompt_unknown = true
log_connections = true
"#;

        let file: SandboxConfigFile = toml::from_str(toml_content).unwrap();
        let config: SandboxConfig = file.into();

        let profile = config.profiles.get("ssh-enabled").unwrap();
        let ssh = profile.ssh.as_ref().unwrap();
        assert!(ssh.enabled);
        assert_eq!(ssh.allowed_hosts.len(), 3);
        assert!(ssh.allowed_hosts.contains(&"github.com".to_string()));
        assert!(ssh.prompt_unknown);
    }

    #[test]
    fn test_profile_with_network_config() {
        let toml_content = r#"
enabled = true
profile = "proxied"

[profiles.proxied]
deny_read = ["~/.ssh"]
allow_write = ["/tmp"]

[profiles.proxied.network]
mode = "proxy"
allow_domains = ["crates.io", "npmjs.org", "github.com"]
log_requests = true
"#;

        let file: SandboxConfigFile = toml::from_str(toml_content).unwrap();
        let config: SandboxConfig = file.into();

        let profile = config.profiles.get("proxied").unwrap();
        let network = profile.network.as_ref().unwrap();
        assert_eq!(network.mode, NetworkMode::Proxy);
        assert_eq!(network.allow_domains.len(), 3);
        assert!(network.log_requests);
    }

    #[test]
    fn test_deny_read_after_workspace_bind_when_workspace_is_home() {
        let temp = tempdir().unwrap();
        let home = temp.path();
        std::fs::create_dir_all(home.join(".ssh")).unwrap();

        let original_home = env::var_os("HOME");
        // SAFETY: This test runs single-threaded and restores the value after
        unsafe { env::set_var("HOME", home) };

        let config = SandboxConfig::from_profile("development");
        let args = config.build_bwrap_args_for_user(home, None).unwrap();

        let home_str = home.to_string_lossy().to_string();
        let ssh_str = home.join(".ssh").to_string_lossy().to_string();

        let workspace_idx = args
            .as_slice()
            .windows(3)
            .position(|w| w[0] == "--bind" && w[1] == home_str && w[2] == home_str)
            .expect("workspace bind not found");
        let deny_idx = args
            .as_slice()
            .windows(2)
            .position(|w| w[0] == "--tmpfs" && w[1] == ssh_str)
            .expect("deny-read tmpfs not found");

        assert!(
            deny_idx > workspace_idx,
            "deny-read should be applied after workspace bind"
        );

        // SAFETY: Restoring environment after test
        match original_home {
            Some(value) => unsafe { env::set_var("HOME", value) },
            None => unsafe { env::remove_var("HOME") },
        }
    }
}

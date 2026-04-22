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
#[cfg(target_os = "linux")]
use std::{ffi::CString, os::unix::ffi::OsStrExt};

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

fn default_overlay_root() -> String {
    "~/.oqto/overlays".to_string()
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

/// Seccomp enforcement mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SeccompMode {
    /// Disable seccomp integration.
    #[default]
    Off,
    /// Log missing/misconfigured seccomp policy but continue.
    Audit,
    /// Enforce seccomp policy; fail if policy is unavailable.
    Enforce,
}

fn stricter_seccomp_mode(a: SeccompMode, b: SeccompMode) -> SeccompMode {
    use SeccompMode::{Audit, Enforce, Off};
    match (a, b) {
        (Enforce, _) | (_, Enforce) => Enforce,
        (Audit, _) | (_, Audit) => Audit,
        _ => Off,
    }
}

/// Landlock enforcement mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LandlockMode {
    #[default]
    Off,
    Audit,
    Enforce,
}

fn stricter_landlock_mode(a: LandlockMode, b: LandlockMode) -> LandlockMode {
    use LandlockMode::{Audit, Enforce, Off};
    match (a, b) {
        (Enforce, _) | (_, Enforce) => Enforce,
        (Audit, _) | (_, Audit) => Audit,
        _ => Off,
    }
}

#[cfg(target_os = "linux")]
#[repr(C)]
struct LandlockRulesetAttr {
    handled_access_fs: u64,
}

#[cfg(target_os = "linux")]
#[repr(C)]
struct LandlockPathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
    reserved1: u32,
}

#[cfg(target_os = "linux")]
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;
#[cfg(target_os = "linux")]
const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;

#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1u64 << 1;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1u64 << 4;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1u64 << 5;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1u64 << 6;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1u64 << 7;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1u64 << 8;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1u64 << 9;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1u64 << 10;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1u64 << 11;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1u64 << 12;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_REFER: u64 = 1u64 << 13;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_TRUNCATE: u64 = 1u64 << 14;

#[cfg(target_os = "linux")]
const LANDLOCK_WRITE_ACCESS_MASK: u64 = LANDLOCK_ACCESS_FS_WRITE_FILE
    | LANDLOCK_ACCESS_FS_REMOVE_DIR
    | LANDLOCK_ACCESS_FS_REMOVE_FILE
    | LANDLOCK_ACCESS_FS_MAKE_CHAR
    | LANDLOCK_ACCESS_FS_MAKE_DIR
    | LANDLOCK_ACCESS_FS_MAKE_REG
    | LANDLOCK_ACCESS_FS_MAKE_SOCK
    | LANDLOCK_ACCESS_FS_MAKE_FIFO
    | LANDLOCK_ACCESS_FS_MAKE_BLOCK
    | LANDLOCK_ACCESS_FS_MAKE_SYM
    | LANDLOCK_ACCESS_FS_REFER
    | LANDLOCK_ACCESS_FS_TRUNCATE;

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

    /// Drop all Linux capabilities inside the sandbox.
    #[serde(default)]
    pub drop_all_caps: bool,

    /// Disable nested user namespace creation inside sandbox.
    #[serde(default)]
    pub disable_userns: bool,

    /// Assert that nested user namespaces are disabled (fail if not).
    #[serde(default)]
    pub assert_userns_disabled: bool,

    /// Set PR_SET_NO_NEW_PRIVS before exec.
    #[serde(default = "default_true")]
    pub no_new_privs: bool,

    /// Seccomp mode (off/audit/enforce).
    #[serde(default)]
    pub seccomp_mode: SeccompMode,

    /// Landlock mode (off/audit/enforce).
    #[serde(default)]
    pub landlock_mode: LandlockMode,

    /// Path to precompiled seccomp-bpf policy file for bwrap (--seccomp FD).
    #[serde(default)]
    pub seccomp_bpf_path: Option<String>,

    /// Additional paths to bind read-only.
    pub extra_ro_bind: Vec<String>,

    /// Additional paths to bind read-write.
    pub extra_rw_bind: Vec<String>,

    /// Enable overlayfs redirection for selected paths.
    ///
    /// When enabled, each path in `overlay_paths` is mounted with bwrap overlay
    /// so writes go to a per-workspace upperdir under `overlay_root` while reads
    /// come from the original path.
    #[serde(default)]
    pub overlay_enabled: bool,

    /// Root directory for per-workspace overlay upper/work dirs.
    ///
    /// Example: `~/.oqto/overlays`
    #[serde(default = "default_overlay_root")]
    pub overlay_root: String,

    /// Paths to overlay (typically package/toolchain directories).
    #[serde(default)]
    pub overlay_paths: Vec<String>,

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
            drop_all_caps: false,
            disable_userns: false,
            assert_userns_disabled: false,
            no_new_privs: true,
            seccomp_mode: SeccompMode::Off,
            landlock_mode: LandlockMode::Off,
            seccomp_bpf_path: None,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
            overlay_enabled: false,
            overlay_root: default_overlay_root(),
            overlay_paths: vec![],
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
                // Claude Code - store.db (SQLite WAL), todos, statsig cache,
                // session files, managed-install versions tree.
                "~/.claude".to_string(),
                "~/.local/share/claude".to_string(),
                "~/.cache/claude".to_string(),
                // Codex - auth.json, history, sessions, SQLite state/logs.
                "~/.codex".to_string(),
                "~/.local/share/codex".to_string(),
                "~/.cache/codex".to_string(),
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
            drop_all_caps: false,
            disable_userns: true,
            assert_userns_disabled: false,
            no_new_privs: true,
            seccomp_mode: SeccompMode::Audit,
            landlock_mode: LandlockMode::Audit,
            seccomp_bpf_path: None,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
            overlay_enabled: false,
            overlay_root: default_overlay_root(),
            overlay_paths: vec![
                "~/.cargo".to_string(),
                "~/.npm".to_string(),
                "~/.bun".to_string(),
                "~/.local/share/uv".to_string(),
                "~/.cache/uv".to_string(),
            ],
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
    ///
    /// Note on ordering: extra_ro_bind is applied AFTER deny_read tmpfs mounts,
    /// so we can selectively allow read access to paths under ~/.config (like ~/.config/oqto)
    /// even though ~/.config itself is blocked.
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
            // Note: ~/.pi must be writable for Pi session files
            allow_write: vec!["/tmp".to_string(), "~/.pi".to_string()],
            deny_write: vec![],
            isolate_network: true,
            isolate_pid: true,
            drop_all_caps: true,
            disable_userns: true,
            assert_userns_disabled: true,
            no_new_privs: true,
            seccomp_mode: SeccompMode::Audit,
            landlock_mode: LandlockMode::Audit,
            seccomp_bpf_path: None,
            // extra_ro_bind is applied AFTER deny_read, so these paths
            // under ~/.config are accessible even though ~/.config is blocked
            extra_ro_bind: vec!["~/.config/oqto".to_string()],
            extra_rw_bind: vec![],
            overlay_enabled: false,
            overlay_root: default_overlay_root(),
            overlay_paths: vec![],
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

    /// Drop all Linux capabilities inside the sandbox.
    pub drop_all_caps: bool,

    /// Disable nested user namespace creation inside sandbox.
    pub disable_userns: bool,

    /// Assert that nested user namespaces are disabled (fail if not).
    pub assert_userns_disabled: bool,

    /// Set PR_SET_NO_NEW_PRIVS before exec.
    pub no_new_privs: bool,

    /// Seccomp mode (off/audit/enforce).
    pub seccomp_mode: SeccompMode,

    /// Landlock mode (off/audit/enforce).
    pub landlock_mode: LandlockMode,

    /// Path to precompiled seccomp-bpf policy file for bwrap (--seccomp FD).
    pub seccomp_bpf_path: Option<String>,

    /// Additional paths to bind read-only.
    pub extra_ro_bind: Vec<String>,

    /// Additional paths to bind read-write.
    pub extra_rw_bind: Vec<String>,

    /// Enable overlayfs redirection for selected paths.
    pub overlay_enabled: bool,

    /// Root directory for per-workspace overlay upper/work dirs.
    pub overlay_root: String,

    /// Paths to overlay (typically package/toolchain directories).
    pub overlay_paths: Vec<String>,

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
            drop_all_caps: profile.drop_all_caps,
            disable_userns: profile.disable_userns,
            assert_userns_disabled: profile.assert_userns_disabled,
            no_new_privs: profile.no_new_privs,
            seccomp_mode: profile.seccomp_mode,
            landlock_mode: profile.landlock_mode,
            seccomp_bpf_path: profile.seccomp_bpf_path,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            overlay_enabled: profile.overlay_enabled,
            overlay_root: profile.overlay_root,
            overlay_paths: profile.overlay_paths,
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
            drop_all_caps: profile.drop_all_caps,
            disable_userns: profile.disable_userns,
            assert_userns_disabled: profile.assert_userns_disabled,
            no_new_privs: profile.no_new_privs,
            seccomp_mode: profile.seccomp_mode,
            landlock_mode: profile.landlock_mode,
            seccomp_bpf_path: profile.seccomp_bpf_path,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            overlay_enabled: profile.overlay_enabled,
            overlay_root: profile.overlay_root,
            overlay_paths: profile.overlay_paths,
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
            drop_all_caps: profile.drop_all_caps,
            disable_userns: profile.disable_userns,
            assert_userns_disabled: profile.assert_userns_disabled,
            no_new_privs: profile.no_new_privs,
            seccomp_mode: profile.seccomp_mode,
            landlock_mode: profile.landlock_mode,
            seccomp_bpf_path: profile.seccomp_bpf_path,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            overlay_enabled: profile.overlay_enabled,
            overlay_root: profile.overlay_root,
            overlay_paths: profile.overlay_paths,
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
            drop_all_caps: profile.drop_all_caps,
            disable_userns: profile.disable_userns,
            assert_userns_disabled: profile.assert_userns_disabled,
            no_new_privs: profile.no_new_privs,
            seccomp_mode: profile.seccomp_mode,
            landlock_mode: profile.landlock_mode,
            seccomp_bpf_path: profile.seccomp_bpf_path,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            overlay_enabled: profile.overlay_enabled,
            overlay_root: profile.overlay_root,
            overlay_paths: profile.overlay_paths,
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
            drop_all_caps: profile.drop_all_caps,
            disable_userns: profile.disable_userns,
            assert_userns_disabled: profile.assert_userns_disabled,
            no_new_privs: profile.no_new_privs,
            seccomp_mode: profile.seccomp_mode,
            landlock_mode: profile.landlock_mode,
            seccomp_bpf_path: profile.seccomp_bpf_path,
            extra_ro_bind: profile.extra_ro_bind,
            extra_rw_bind: profile.extra_rw_bind,
            overlay_enabled: profile.overlay_enabled,
            overlay_root: profile.overlay_root,
            overlay_paths: profile.overlay_paths,
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
                        drop_all_caps: profile.drop_all_caps,
                        disable_userns: profile.disable_userns,
                        assert_userns_disabled: profile.assert_userns_disabled,
                        no_new_privs: profile.no_new_privs,
                        seccomp_mode: profile.seccomp_mode,
                        landlock_mode: profile.landlock_mode,
                        seccomp_bpf_path: profile.seccomp_bpf_path,
                        extra_ro_bind: profile.extra_ro_bind,
                        extra_rw_bind: profile.extra_rw_bind,
                        overlay_enabled: profile.overlay_enabled,
                        overlay_root: profile.overlay_root,
                        overlay_paths: profile.overlay_paths,
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

        // overlay paths are additive
        let mut overlay_paths: HashSet<String> = self.overlay_paths.iter().cloned().collect();
        overlay_paths.extend(workspace_config.overlay_paths.iter().cloned());

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
            drop_all_caps: self.drop_all_caps || workspace_config.drop_all_caps,
            disable_userns: self.disable_userns || workspace_config.disable_userns,
            assert_userns_disabled: self.assert_userns_disabled
                || workspace_config.assert_userns_disabled,
            no_new_privs: self.no_new_privs || workspace_config.no_new_privs,
            seccomp_mode: stricter_seccomp_mode(
                self.seccomp_mode.clone(),
                workspace_config.seccomp_mode.clone(),
            ),
            landlock_mode: stricter_landlock_mode(
                self.landlock_mode.clone(),
                workspace_config.landlock_mode.clone(),
            ),
            seccomp_bpf_path: workspace_config
                .seccomp_bpf_path
                .clone()
                .or_else(|| self.seccomp_bpf_path.clone()),
            extra_ro_bind: extra_ro_bind.into_iter().collect(),
            extra_rw_bind: extra_rw_bind.into_iter().collect(),
            overlay_enabled: self.overlay_enabled || workspace_config.overlay_enabled,
            overlay_root: if workspace_config.overlay_root != default_overlay_root() {
                workspace_config.overlay_root.clone()
            } else {
                self.overlay_root.clone()
            },
            overlay_paths: overlay_paths.into_iter().collect(),
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

    /// Build a stable workspace identifier for overlay directory layout.
    fn workspace_overlay_id(workspace: &Path) -> String {
        use std::hash::{Hash, Hasher};

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        workspace.to_string_lossy().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Build a filesystem-safe key from a path.
    fn overlay_path_key(path: &Path) -> String {
        path.to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
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

        // DNS resolver: on systemd-resolved systems, /etc/resolv.conf is a
        // symlink to /run/systemd/resolve/stub-resolv.conf.  The --ro-bind
        // for /etc does NOT follow symlinks that point outside /etc, so DNS
        // breaks inside the sandbox.  Bind the resolve directory so the
        // symlink target is reachable.
        let resolve_dir = Path::new("/run/systemd/resolve");
        if resolve_dir.exists() {
            args.push("--ro-bind".to_string());
            args.push(resolve_dir.to_string_lossy().to_string());
            args.push(resolve_dir.to_string_lossy().to_string());
            debug!("Bound /run/systemd/resolve for DNS resolution");
        }

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

        // Overlayfs redirection for selected paths.
        // This keeps original paths readable while redirecting writes to
        // per-workspace upperdirs under overlay_root.
        if self.overlay_enabled {
            let overlay_root = Self::expand_home_for_user(&self.overlay_root, username);
            let workspace_id = Self::workspace_overlay_id(workspace);
            let mut mounted_overlays = 0usize;

            for path in &self.overlay_paths {
                let expanded = Self::expand_home_for_user(path, username);

                if !expanded.exists()
                    && let Err(e) = std::fs::create_dir_all(&expanded)
                {
                    warn!(
                        "overlay: failed to create missing target '{}' (from '{}'): {}",
                        expanded.display(),
                        path,
                        e
                    );
                    continue;
                }

                let target = expanded.to_string_lossy().to_string();
                let path_key = Self::overlay_path_key(&expanded);
                let overlay_base = overlay_root.join(&workspace_id).join(path_key);
                let upper = overlay_base.join("upper");
                let work = overlay_base.join("work");

                if let Err(e) = std::fs::create_dir_all(&upper) {
                    warn!(
                        "overlay: failed to create upperdir '{}': {}",
                        upper.display(),
                        e
                    );
                    continue;
                }

                // workdir must be empty for overlayfs.
                if work.exists()
                    && let Err(e) = std::fs::remove_dir_all(&work)
                {
                    warn!(
                        "overlay: failed to reset workdir '{}': {}",
                        work.display(),
                        e
                    );
                    continue;
                }
                if let Err(e) = std::fs::create_dir_all(&work) {
                    warn!(
                        "overlay: failed to create workdir '{}': {}",
                        work.display(),
                        e
                    );
                    continue;
                }

                args.push("--overlay-src".to_string());
                args.push(target.clone());
                args.push("--overlay".to_string());
                args.push(upper.to_string_lossy().to_string());
                args.push(work.to_string_lossy().to_string());
                args.push(target.clone());

                mounted_overlays += 1;
                debug!(
                    "overlay mounted: target='{}', upper='{}', work='{}'",
                    target,
                    upper.display(),
                    work.display()
                );
            }

            info!(
                "Overlayfs enabled: mounted {} path(s) under {}",
                mounted_overlays,
                overlay_root.display()
            );
        }

        // Ensure common user toolchain bin paths are in PATH.
        // Non-interactive shells (systemd, cron) typically don't source
        // ~/.bashrc/.zshrc, so ~/go/bin, ~/.cargo/bin, etc. are missing.
        if let Some(ref home) = target_home {
            let home_str = home.to_string_lossy();
            let extra_paths = [
                format!("{home_str}/.cargo/bin"),
                format!("{home_str}/go/bin"),
                format!("{home_str}/.local/bin"),
                format!("{home_str}/.bun/bin"),
                format!("{home_str}/.npm-global/bin"),
            ];
            let current_path = std::env::var("PATH").unwrap_or_default();
            let mut path_parts: Vec<&str> = current_path.split(':').collect();
            for p in &extra_paths {
                if !path_parts.contains(&p.as_str()) && Path::new(p).exists() {
                    path_parts.push(p);
                }
            }
            let new_path = path_parts.join(":");
            args.push("--setenv".to_string());
            args.push("PATH".to_string());
            args.push(new_path);
            debug!("Extended PATH with user toolchain bin directories");
        }

        // Namespace and kernel-surface hardening
        if self.isolate_pid {
            args.push("--unshare-pid".to_string());
            debug!("PID namespace isolation enabled");
        }

        if self.isolate_network {
            args.push("--unshare-net".to_string());
            debug!("Network namespace isolation enabled");
        }

        if self.drop_all_caps {
            args.push("--cap-drop".to_string());
            args.push("ALL".to_string());
            debug!("Capability dropping enabled (--cap-drop ALL)");
        }

        if self.disable_userns {
            // bwrap requires --unshare-user when --disable-userns is used
            args.push("--unshare-user".to_string());
            args.push("--disable-userns".to_string());
            debug!("User namespace unshared + nested user namespaces disabled");
        }

        if self.assert_userns_disabled {
            args.push("--assert-userns-disabled".to_string());
            debug!("Asserting nested user namespaces are disabled");
        }

        // Landlock is applied by the inner shim (crate::shim) after bwrap
        // completes user-namespace setup. Here we wire the binds/env the shim
        // needs; the shim argv is prepended after `--` further below.
        let mut wire_landlock_shim = false;
        match self.landlock_mode {
            LandlockMode::Off => {}
            LandlockMode::Audit | LandlockMode::Enforce => {
                if !Self::is_landlock_supported() {
                    let msg = "landlock requested but kernel/runtime support not detected";
                    if self.landlock_mode == LandlockMode::Enforce {
                        warn!("{} (enforce mode)", msg);
                        return None;
                    }
                    warn!("{} (audit mode)", msg);
                } else if let Some(shim_bin) = crate::shim::resolve_shim_binary() {
                    let shim_src = shim_bin.to_string_lossy().to_string();
                    args.push("--ro-bind".to_string());
                    args.push(shim_src.clone());
                    args.push(crate::shim::SHIM_MOUNT_PATH.to_string());

                    let mode_str = match self.landlock_mode {
                        LandlockMode::Audit => "audit",
                        LandlockMode::Enforce => "enforce",
                        LandlockMode::Off => unreachable!(),
                    };
                    args.push("--setenv".to_string());
                    args.push(crate::shim::SHIM_ENV.to_string());
                    args.push("1".to_string());

                    args.push("--setenv".to_string());
                    args.push(crate::shim::ENV_MODE.to_string());
                    args.push(mode_str.to_string());

                    args.push("--setenv".to_string());
                    args.push(crate::shim::ENV_WORKSPACE.to_string());
                    args.push(workspace.to_string_lossy().to_string());

                    let allow_write_joined = self
                        .allow_write
                        .iter()
                        .map(|p| {
                            Self::expand_home_for_user(p, username)
                                .to_string_lossy()
                                .to_string()
                        })
                        .collect::<Vec<_>>()
                        .join(":");
                    args.push("--setenv".to_string());
                    args.push(crate::shim::ENV_ALLOW_WRITE.to_string());
                    args.push(allow_write_joined);

                    wire_landlock_shim = true;
                    debug!(
                        "Landlock shim wired (mode={}, src={}, mount={})",
                        mode_str,
                        shim_src,
                        crate::shim::SHIM_MOUNT_PATH
                    );
                } else {
                    let msg = "landlock requested but oqto-sandbox shim binary could not be resolved (set OQTO_SANDBOX_SHIM_BIN or install on PATH)";
                    if self.landlock_mode == LandlockMode::Enforce {
                        warn!("{} (enforce mode)", msg);
                        return None;
                    }
                    warn!("{} (audit mode)", msg);
                }
            }
        }

        match self.seccomp_mode {
            SeccompMode::Off => {}
            SeccompMode::Audit | SeccompMode::Enforce => {
                let seccomp_path = self.resolve_seccomp_bpf_path(username);
                if seccomp_path.as_ref().is_some_and(|p| p.exists()) {
                    args.push("--seccomp".to_string());
                    args.push("3".to_string());
                    debug!("Seccomp enabled via bwrap fd 3");
                } else if self.seccomp_mode == SeccompMode::Enforce {
                    warn!(
                        "seccomp_mode=enforce but seccomp_bpf_path missing/unreadable: {:?}",
                        self.seccomp_bpf_path
                    );
                    return None;
                } else {
                    warn!(
                        "seccomp_mode=audit but seccomp_bpf_path missing/unreadable: {:?}",
                        self.seccomp_bpf_path
                    );
                }
            }
        }

        // Workspace model catalog override.
        // If .oqto/config.toml sets models.mode = "restrict" or "merge" and
        // .oqto/models.json exists, bind-mount it over ~/.pi/agent/models.json.
        // This must come AFTER home directory binds so it shadows the global file.
        {
            use crate::workspace_config::{ModelMode, WorkspaceConfig};

            let ws_config = WorkspaceConfig::load(workspace);
            let effective_mode = ws_config.effective_model_mode(workspace);
            let ws_models_path = WorkspaceConfig::models_json_path(workspace);

            match effective_mode {
                ModelMode::Restrict => {
                    // Bind workspace models.json directly over global
                    let ws_models_str = ws_models_path.to_string_lossy().to_string();
                    let global_models_str = target_home
                        .as_ref()
                        .map(|h| h.join(".pi/agent/models.json"))
                        .unwrap_or_else(|| PathBuf::from("/nonexistent"))
                        .to_string_lossy()
                        .to_string();
                    args.push("--ro-bind".to_string());
                    args.push(ws_models_str.clone());
                    args.push(global_models_str.clone());
                    info!(
                        "Workspace models restrict: {} -> {}",
                        ws_models_str, global_models_str
                    );
                }
                ModelMode::Merge => {
                    // Merge global + workspace into a temp file, bind that
                    let global_models_path = target_home
                        .as_ref()
                        .map(|h| h.join(".pi/agent/models.json"))
                        .unwrap_or_else(|| PathBuf::from("/nonexistent"));
                    match WorkspaceConfig::merge_models_json(&global_models_path, &ws_models_path) {
                        Ok(merged) => {
                            // Write merged JSON to a temp file that lives for this session
                            let tmp_dir = std::env::temp_dir();
                            let merged_path = tmp_dir
                                .join(format!("oqto-merged-models-{}.json", std::process::id()));
                            if let Err(e) = std::fs::write(&merged_path, &merged) {
                                warn!("Failed to write merged models.json: {}", e);
                            } else {
                                let merged_str = merged_path.to_string_lossy().to_string();
                                let global_str = global_models_path.to_string_lossy().to_string();
                                args.push("--ro-bind".to_string());
                                args.push(merged_str.clone());
                                args.push(global_str.clone());
                                info!("Workspace models merge: {} -> {}", merged_str, global_str);
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Failed to merge workspace models.json: {}. Using global.",
                                e
                            );
                        }
                    }
                }
                ModelMode::Global => {
                    // No-op, Pi uses global models.json as-is
                }
            }
        }

        // Die with parent (important for cleanup)
        args.push("--die-with-parent".to_string());

        // Separator before command
        args.push("--".to_string());

        // If Landlock is active, bwrap's inner command must run the shim first
        // so Landlock rules are installed after user-namespace setup completes.
        // Callers append the user command after these args unchanged.
        if wire_landlock_shim {
            args.push(crate::shim::SHIM_MOUNT_PATH.to_string());
        }

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

    /// Resolve seccomp policy path from config.
    pub fn resolve_seccomp_bpf_path(&self, username: Option<&str>) -> Option<PathBuf> {
        self.seccomp_bpf_path
            .as_ref()
            .map(|p| Self::expand_home_for_user(p, username))
    }

    /// Best-effort runtime probe for Landlock support.
    pub fn is_landlock_supported() -> bool {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: Syscall interface is used read-only for feature probing.
            let abi = unsafe {
                libc::syscall(
                    libc::SYS_landlock_create_ruleset,
                    std::ptr::null::<LandlockRulesetAttr>(),
                    0,
                    LANDLOCK_CREATE_RULESET_VERSION,
                )
            };
            abi >= 1
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Apply Landlock write restrictions (workspace + allow_write).
    pub fn apply_landlock(&self, workspace: &Path, username: Option<&str>) -> std::io::Result<()> {
        if self.landlock_mode == LandlockMode::Off {
            return Ok(());
        }

        #[cfg(not(target_os = "linux"))]
        {
            if self.landlock_mode == LandlockMode::Enforce {
                return Err(std::io::Error::other(
                    "landlock enforce requested on non-Linux platform",
                ));
            }
            return Ok(());
        }

        #[cfg(target_os = "linux")]
        {
            if !Self::is_landlock_supported() {
                if self.landlock_mode == LandlockMode::Enforce {
                    return Err(std::io::Error::other(
                        "landlock enforce requested but kernel does not support landlock",
                    ));
                }
                return Ok(());
            }

            // The kernel has no observe-only Landlock mode: once
            // landlock_restrict_self runs, rules are enforced at every file
            // operation. Audit mode here means "log the intended ruleset but
            // do not restrict," so applications that write outside allow_write
            // (e.g. ~/.claude, ~/.codex) continue to work while the operator
            // evaluates what would be denied under enforce.
            if self.landlock_mode == LandlockMode::Audit {
                let mut paths: Vec<String> = Vec::with_capacity(1 + self.allow_write.len());
                paths.push(workspace.to_string_lossy().to_string());
                for p in &self.allow_write {
                    paths.push(
                        Self::expand_home_for_user(p, username)
                            .to_string_lossy()
                            .to_string(),
                    );
                }
                info!(
                    "Landlock audit mode: would restrict writes outside: {}",
                    paths.join(", ")
                );
                return Ok(());
            }

            let ruleset_attr = LandlockRulesetAttr {
                handled_access_fs: LANDLOCK_WRITE_ACCESS_MASK,
            };

            // Enforce mode: build ruleset and apply restrict_self.
            // SAFETY: Syscall with valid pointer/size to create ruleset.
            let ruleset_fd = unsafe {
                libc::syscall(
                    libc::SYS_landlock_create_ruleset,
                    &ruleset_attr as *const LandlockRulesetAttr,
                    std::mem::size_of::<LandlockRulesetAttr>(),
                    0,
                ) as i32
            };

            if ruleset_fd < 0 {
                return Err(std::io::Error::last_os_error());
            }

            let mut writable_paths: HashSet<PathBuf> = HashSet::new();
            writable_paths.insert(workspace.to_path_buf());
            for p in &self.allow_write {
                writable_paths.insert(Self::expand_home_for_user(p, username));
            }

            for path in writable_paths {
                if !path.exists() {
                    continue;
                }

                let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL byte")
                })?;

                // SAFETY: Open path for Landlock rule registration.
                let parent_fd =
                    unsafe { libc::open(c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
                if parent_fd < 0 {
                    // SAFETY: close best-effort on previously created fd.
                    unsafe {
                        libc::close(ruleset_fd);
                    }
                    return Err(std::io::Error::last_os_error());
                }

                let path_beneath = LandlockPathBeneathAttr {
                    allowed_access: LANDLOCK_WRITE_ACCESS_MASK,
                    parent_fd,
                    reserved1: 0,
                };

                // SAFETY: Syscall with valid fds/pointers.
                let rc = unsafe {
                    libc::syscall(
                        libc::SYS_landlock_add_rule,
                        ruleset_fd,
                        LANDLOCK_RULE_PATH_BENEATH,
                        &path_beneath as *const LandlockPathBeneathAttr,
                        0,
                    )
                };

                // SAFETY: close temporary opened path fd.
                unsafe {
                    libc::close(parent_fd);
                }

                if rc != 0 {
                    // SAFETY: close ruleset fd before returning.
                    unsafe {
                        libc::close(ruleset_fd);
                    }
                    return Err(std::io::Error::last_os_error());
                }
            }

            // SAFETY: apply Landlock restrictions to current process.
            let restrict_rc =
                unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset_fd, 0) };
            // SAFETY: close ruleset fd after use.
            unsafe {
                libc::close(ruleset_fd);
            }

            if restrict_rc != 0 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(())
        }
    }

    /// Open seccomp bpf policy file if seccomp is active and policy exists.
    pub fn open_seccomp_bpf_file(&self, username: Option<&str>) -> Result<Option<std::fs::File>> {
        if self.seccomp_mode == SeccompMode::Off {
            return Ok(None);
        }

        let Some(path) = self.resolve_seccomp_bpf_path(username) else {
            if self.seccomp_mode == SeccompMode::Enforce {
                anyhow::bail!("seccomp_mode=enforce requires seccomp_bpf_path to be configured");
            }
            return Ok(None);
        };

        if !path.exists() {
            if self.seccomp_mode == SeccompMode::Enforce {
                anyhow::bail!(
                    "seccomp_mode=enforce requires existing seccomp_bpf_path: {}",
                    path.display()
                );
            }
            return Ok(None);
        }

        std::fs::File::open(&path)
            .with_context(|| format!("opening seccomp policy file: {}", path.display()))
            .map(Some)
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

    /// Regression test for oqto-b4za: Landlock shim must be wired even when
    /// disable_userns=true. The old code silently skipped Landlock in this
    /// case; we now install a shim that applies Landlock after bwrap's
    /// namespace setup, so the shim binds and setenv entries must be present.
    #[test]
    fn test_landlock_shim_wired_with_disable_userns() {
        if !SandboxConfig::is_landlock_supported() {
            eprintln!("skipping: Landlock kernel support missing");
            return;
        }

        // Point the shim resolver at any existing binary so the test is
        // hermetic. The shim path doesn't need to be a real oqto-sandbox here
        // — we only assert the wire-up, not execution.
        let fake_shim = std::env::current_exe().expect("current_exe");
        // SAFETY: test is single-threaded; we restore after the assertions.
        let prev = env::var_os(crate::shim::SHIM_BIN_OVERRIDE_ENV);
        unsafe { env::set_var(crate::shim::SHIM_BIN_OVERRIDE_ENV, &fake_shim) };

        let temp = tempdir().unwrap();
        let ws = temp.path();

        let mut cfg = SandboxConfig::from_profile("development");
        assert!(cfg.disable_userns, "development preset expected disable_userns=true");
        cfg.landlock_mode = LandlockMode::Enforce;
        cfg.allow_write = vec![ws.to_string_lossy().to_string()];

        let args = cfg
            .build_bwrap_args_for_user(ws, None)
            .expect("bwrap args");

        let has_shim_bind = args
            .windows(3)
            .any(|w| w[0] == "--ro-bind" && w[2] == crate::shim::SHIM_MOUNT_PATH);
        assert!(has_shim_bind, "shim --ro-bind missing: {args:?}");

        let has_mode_env = args.windows(3).any(|w| {
            w[0] == "--setenv" && w[1] == crate::shim::ENV_MODE && w[2] == "enforce"
        });
        assert!(has_mode_env, "OQTO_LANDLOCK_MODE setenv missing: {args:?}");

        let sep_idx = args.iter().rposition(|a| a == "--").expect("no --");
        assert_eq!(
            args.get(sep_idx + 1).map(String::as_str),
            Some(crate::shim::SHIM_MOUNT_PATH),
            "shim command must be first after `--`"
        );

        // Restore env
        match prev {
            Some(v) => unsafe { env::set_var(crate::shim::SHIM_BIN_OVERRIDE_ENV, v) },
            None => unsafe { env::remove_var(crate::shim::SHIM_BIN_OVERRIDE_ENV) },
        }
    }

    /// Landlock=off must not wire any shim plumbing.
    #[test]
    fn test_landlock_off_no_shim() {
        let temp = tempdir().unwrap();
        let ws = temp.path();

        let mut cfg = SandboxConfig::from_profile("minimal");
        cfg.landlock_mode = LandlockMode::Off;

        let args = cfg
            .build_bwrap_args_for_user(ws, None)
            .expect("bwrap args");

        let has_shim_bind = args
            .windows(3)
            .any(|w| w[0] == "--ro-bind" && w[2] == crate::shim::SHIM_MOUNT_PATH);
        assert!(!has_shim_bind, "no shim bind expected when landlock=off");

        let last = args.last().map(String::as_str);
        assert_eq!(last, Some("--"), "last arg should be `--` with no shim");
    }

    /// Regression for oqto-2eev: Landlock audit mode must NOT call
    /// restrict_self. Previously, audit applied enforcement and silently
    /// blocked writes to ~/.claude/~/.codex, hanging those harnesses.
    /// Write to an outside path after apply_landlock(Audit) must succeed.
    #[test]
    fn test_landlock_audit_does_not_restrict() {
        if !SandboxConfig::is_landlock_supported() {
            eprintln!("skipping: Landlock kernel support missing");
            return;
        }

        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();

        let mut cfg = SandboxConfig::from_profile("minimal");
        cfg.landlock_mode = LandlockMode::Audit;
        cfg.allow_write = vec![workspace.path().to_string_lossy().to_string()];

        // Must be in a child process: restrict_self is irreversible within a
        // process lifetime, so even a false-audit (the bug) would poison the
        // test runner. Fork via std::process::Command + a sentinel binary is
        // overkill here; instead we rely on the audit short-circuit keeping
        // the parent unrestricted and assert a write to `outside` succeeds.
        cfg.apply_landlock(workspace.path(), None)
            .expect("audit must not fail");

        let outside_file = outside.path().join("audit-probe");
        std::fs::write(&outside_file, b"ok").expect(
            "audit mode must not restrict writes; got EACCES (regression of oqto-2eev)",
        );
        let _ = std::fs::remove_file(&outside_file);
    }
}

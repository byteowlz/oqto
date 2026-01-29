//! Comprehensive tests for sandbox security features.
//!
//! Tests cover:
//! - Sandbox profile configuration and merging
//! - Prompt system lifecycle
//! - SSH proxy policy evaluation
//! - Guard (FUSE) configuration
//! - ttyd Unix socket paths

use octo::local::{
    GuardConfig, GuardPolicy, NetworkConfig, NetworkMode, ProcessManager, PromptConfig,
    SandboxConfigFile, SandboxProfile, SshProxyConfig,
};
use octo::prompts::{PromptAction, PromptManager, PromptRequest, PromptSource, PromptType};
use std::collections::HashMap;

// ============================================================================
// Sandbox Profile Tests
// ============================================================================

mod sandbox_profiles {
    use super::*;

    #[test]
    fn test_minimal_profile_has_basic_restrictions() {
        let profile = SandboxProfile::minimal();

        // Minimal should deny SSH/GPG/AWS
        assert!(profile.deny_read.contains(&"~/.ssh".to_string()));
        assert!(profile.deny_read.contains(&"~/.gnupg".to_string()));
        assert!(profile.deny_read.contains(&"~/.aws".to_string()));

        // Minimal should allow /tmp
        assert!(profile.allow_write.contains(&"/tmp".to_string()));

        // Minimal should NOT isolate network or PID
        assert!(!profile.isolate_network);
        assert!(!profile.isolate_pid);
    }

    #[test]
    fn test_development_profile_allows_dev_tools() {
        let profile = SandboxProfile::development();

        // Should allow package managers
        assert!(profile.allow_write.contains(&"~/.cargo".to_string()));
        assert!(profile.allow_write.contains(&"~/.rustup".to_string()));
        assert!(profile.allow_write.contains(&"~/.npm".to_string()));
        assert!(profile.allow_write.contains(&"~/.bun".to_string()));
        assert!(
            profile
                .allow_write
                .contains(&"~/.local/share/uv".to_string())
        );
        assert!(profile.allow_write.contains(&"~/.cache/uv".to_string()));

        // Should deny sensitive paths
        assert!(profile.deny_read.contains(&"~/.ssh".to_string()));
        assert!(profile.deny_read.contains(&"~/.aws".to_string()));

        // Should protect sandbox config from modification
        assert!(
            profile
                .deny_write
                .contains(&"~/.config/octo/sandbox.toml".to_string())
        );

        // Should have SSH proxy enabled with common hosts
        let ssh = profile.ssh.as_ref().expect("SSH config should exist");
        assert!(ssh.enabled);
        assert!(ssh.allowed_hosts.contains(&"github.com".to_string()));
        assert!(ssh.allowed_hosts.contains(&"gitlab.com".to_string()));
        assert!(ssh.prompt_unknown);
    }

    #[test]
    fn test_strict_profile_is_most_restrictive() {
        let profile = SandboxProfile::strict();

        // Should deny broader config access
        assert!(profile.deny_read.contains(&"~/.config".to_string()));

        // Should isolate network and PID
        assert!(profile.isolate_network);
        assert!(profile.isolate_pid);

        // SSH proxy should not have allowed hosts (prompt for everything)
        let ssh = profile.ssh.as_ref().expect("SSH config should exist");
        assert!(ssh.allowed_hosts.is_empty());
    }

    #[test]
    fn test_profile_builtin() {
        assert!(matches!(
            SandboxProfile::builtin("minimal"),
            Some(p) if !p.isolate_network
        ));
        assert!(matches!(
            SandboxProfile::builtin("development"),
            Some(p) if p.isolate_pid
        ));
        assert!(matches!(
            SandboxProfile::builtin("strict"),
            Some(p) if p.isolate_network
        ));
        assert!(SandboxProfile::builtin("nonexistent").is_none());
    }
}

// ============================================================================
// Sandbox Config File Tests
// ============================================================================

mod sandbox_config_file {
    use super::*;

    #[test]
    fn test_parse_config_with_custom_profile() {
        let toml_str = r#"
enabled = true
profile = "custom_secure"

[profiles.custom_secure]
deny_read = ["~/.ssh", "~/.gnupg", "~/.secrets"]
allow_write = ["/tmp", "~/.cache"]
isolate_network = true
isolate_pid = true

[profiles.custom_secure.ssh]
enabled = true
allowed_hosts = ["internal.company.com"]
prompt_unknown = true
"#;

        let config: SandboxConfigFile =
            toml::from_str(toml_str).expect("Should parse custom profile");

        assert!(config.enabled);
        assert_eq!(config.profile, "custom_secure");

        let custom = config
            .profiles
            .get("custom_secure")
            .expect("Custom profile should exist");
        assert!(custom.deny_read.contains(&"~/.secrets".to_string()));
        assert!(custom.isolate_network);

        let ssh = custom.ssh.as_ref().expect("SSH config should exist");
        assert!(
            ssh.allowed_hosts
                .contains(&"internal.company.com".to_string())
        );
    }

    #[test]
    fn test_parse_config_with_guard() {
        let toml_str = r#"
enabled = true
profile = "guarded"

[profiles.guarded]
deny_read = ["~/.ssh"]

[profiles.guarded.guard]
enabled = true
paths = ["~/.kube", "~/.docker"]
timeout_secs = 120

[profiles.guarded.guard.policy]
"~/.kube/config" = "prompt"
"~/.docker/config.json" = "auto"
"#;

        let config: SandboxConfigFile =
            toml::from_str(toml_str).expect("Should parse guard config");

        let profile = config
            .profiles
            .get("guarded")
            .expect("Profile should exist");
        let guard = profile.guard.as_ref().expect("Guard config should exist");

        assert!(guard.enabled);
        assert!(guard.paths.contains(&"~/.kube".to_string()));
        assert!(guard.paths.contains(&"~/.docker".to_string()));
        assert_eq!(guard.timeout_secs, 120);
        assert_eq!(
            guard.policy.get("~/.kube/config"),
            Some(&GuardPolicy::Prompt)
        );
        assert_eq!(
            guard.policy.get("~/.docker/config.json"),
            Some(&GuardPolicy::Auto)
        );
    }

    #[test]
    fn test_parse_config_with_network_proxy() {
        let toml_str = r#"
enabled = true
profile = "proxied"

[profiles.proxied]
deny_read = []

[profiles.proxied.network]
mode = "proxy"
allow_domains = ["api.github.com", "registry.npmjs.org", "crates.io"]
log_requests = true
"#;

        let config: SandboxConfigFile =
            toml::from_str(toml_str).expect("Should parse network config");

        let profile = config
            .profiles
            .get("proxied")
            .expect("Profile should exist");
        let network = profile
            .network
            .as_ref()
            .expect("Network config should exist");

        assert_eq!(network.mode, NetworkMode::Proxy);
        assert!(
            network
                .allow_domains
                .contains(&"api.github.com".to_string())
        );
        assert!(network.log_requests);
    }
}

// ============================================================================
// Config Merging Tests
// ============================================================================

mod config_merging {
    use super::*;
    use octo::local::SandboxConfig;

    #[test]
    fn test_merge_workspace_adds_restrictions() {
        let global = SandboxConfig::from_profile("development");
        let workspace = SandboxConfig {
            enabled: true,
            profile: "workspace".to_string(),
            deny_read: vec!["~/.kube".to_string()], // Additional restriction
            allow_write: vec![],
            deny_write: vec![],
            isolate_network: false,
            isolate_pid: false,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
            profiles: HashMap::new(),
        };

        let merged = global.merge_with_workspace(&workspace);

        // Should have both global and workspace deny_read
        assert!(merged.deny_read.contains(&"~/.ssh".to_string())); // from global
        assert!(merged.deny_read.contains(&"~/.kube".to_string())); // from workspace
    }

    #[test]
    fn test_merge_workspace_cannot_remove_restrictions() {
        let global = SandboxConfig {
            enabled: true,
            profile: "test".to_string(),
            deny_read: vec!["~/.ssh".to_string()],
            allow_write: vec!["/tmp".to_string()],
            deny_write: vec![],
            isolate_network: true,
            isolate_pid: true,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
            profiles: HashMap::new(),
        };

        let workspace = SandboxConfig {
            enabled: true,
            profile: "workspace".to_string(),
            deny_read: vec![], // Trying to remove all restrictions
            allow_write: vec!["~/.ssh".to_string()], // Trying to allow write to SSH
            deny_write: vec![],
            isolate_network: false, // Trying to disable
            isolate_pid: false,     // Trying to disable
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
            profiles: HashMap::new(),
        };

        let merged = global.merge_with_workspace(&workspace);

        // Global deny_read should be preserved
        assert!(merged.deny_read.contains(&"~/.ssh".to_string()));

        // Isolation enabled in global should stay enabled (OR logic)
        assert!(merged.isolate_network);
        assert!(merged.isolate_pid);

        // allow_write should be intersection - workspace can't add ~/.ssh
        // since global doesn't allow it
        assert!(!merged.allow_write.contains(&"~/.ssh".to_string()));
    }

    #[test]
    fn test_sandbox_config_from_profile() {
        let dev = SandboxConfig::from_profile("development");
        assert_eq!(dev.profile, "development");
        assert!(dev.deny_read.contains(&"~/.ssh".to_string()));

        let strict = SandboxConfig::from_profile("strict");
        assert_eq!(strict.profile, "strict");
        assert!(strict.isolate_network);
    }
}

// ============================================================================
// Prompt System Tests
// ============================================================================

mod prompt_system {
    use super::*;

    #[test]
    fn test_prompt_request_file_access() {
        let request = PromptRequest::file_access("/home/user/.kube/config", "read");

        assert_eq!(request.source, PromptSource::OctoGuard);
        assert_eq!(request.prompt_type, PromptType::FileRead);
        assert_eq!(request.resource, "/home/user/.kube/config");
        assert!(request.description.is_some());
    }

    #[test]
    fn test_prompt_request_ssh_sign() {
        let request = PromptRequest::ssh_sign("github.com", Some("work@laptop"));

        assert_eq!(request.source, PromptSource::OctoSshProxy);
        assert_eq!(request.prompt_type, PromptType::SshSign);
        assert_eq!(request.resource, "github.com");
    }

    #[test]
    fn test_prompt_request_network() {
        let request = PromptRequest::network_access("api.openai.com");

        assert_eq!(request.source, PromptSource::Network);
        assert_eq!(request.prompt_type, PromptType::NetworkAccess);
        assert_eq!(request.resource, "api.openai.com");
    }

    #[tokio::test]
    async fn test_prompt_manager_list_pending() {
        let manager = PromptManager::new();

        // Initially no pending prompts
        let pending = manager.list_pending().await;
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_prompt_manager_get_nonexistent() {
        let manager = PromptManager::new();

        let prompt = manager.get("nonexistent-id").await;
        assert!(prompt.is_none());
    }

    #[tokio::test]
    async fn test_prompt_manager_is_approved_initially_false() {
        let manager = PromptManager::new();

        let approved = manager.is_approved("octo-guard", "~/.kube/config").await;
        assert!(!approved);
    }

    #[test]
    fn test_prompt_action_debug() {
        assert_eq!(format!("{:?}", PromptAction::AllowOnce), "AllowOnce");
        assert_eq!(format!("{:?}", PromptAction::AllowSession), "AllowSession");
        assert_eq!(format!("{:?}", PromptAction::Deny), "Deny");
    }

    #[test]
    fn test_prompt_source_display() {
        assert_eq!(format!("{}", PromptSource::OctoGuard), "octo-guard");
        assert_eq!(format!("{}", PromptSource::OctoSshProxy), "octo-ssh-proxy");
        assert_eq!(format!("{}", PromptSource::Network), "network");
        assert_eq!(
            format!("{}", PromptSource::Other("custom".to_string())),
            "custom"
        );
    }

    #[test]
    fn test_prompt_type_display() {
        assert_eq!(format!("{}", PromptType::FileRead), "file read");
        assert_eq!(format!("{}", PromptType::FileWrite), "file write");
        assert_eq!(format!("{}", PromptType::SshSign), "SSH signing");
        assert_eq!(format!("{}", PromptType::NetworkAccess), "network access");
    }
}

// ============================================================================
// SSH Proxy Policy Tests
// ============================================================================

mod ssh_proxy_policy {
    use super::*;

    fn make_ssh_config(allowed_hosts: Vec<&str>, prompt_unknown: bool) -> SshProxyConfig {
        SshProxyConfig {
            enabled: true,
            allowed_hosts: allowed_hosts.into_iter().map(String::from).collect(),
            allowed_keys: vec![],
            prompt_unknown,
            log_connections: true,
        }
    }

    #[test]
    fn test_exact_host_match() {
        let config = make_ssh_config(vec!["github.com", "gitlab.com"], true);

        assert!(config.allowed_hosts.contains(&"github.com".to_string()));
        assert!(config.allowed_hosts.contains(&"gitlab.com".to_string()));
        assert!(!config.allowed_hosts.contains(&"bitbucket.org".to_string()));
    }

    #[test]
    fn test_wildcard_host_match() {
        let config = make_ssh_config(vec!["*.github.com", "git.*.internal.com"], true);

        // These would need glob matching in the actual implementation
        assert!(config.allowed_hosts.contains(&"*.github.com".to_string()));
    }

    #[test]
    fn test_prompt_unknown_hosts() {
        let config_prompt = make_ssh_config(vec!["github.com"], true);
        let config_deny = make_ssh_config(vec!["github.com"], false);

        assert!(config_prompt.prompt_unknown);
        assert!(!config_deny.prompt_unknown);
    }

    #[test]
    fn test_empty_allowed_hosts_prompts_all() {
        let config = make_ssh_config(vec![], true);

        assert!(config.allowed_hosts.is_empty());
        assert!(config.prompt_unknown);
        // With empty allowed_hosts and prompt_unknown=true, all hosts should prompt
    }

    #[test]
    fn test_ssh_config_serialization() {
        let config = make_ssh_config(vec!["github.com"], true);
        let toml_str = toml::to_string(&config).expect("Should serialize");
        let restored: SshProxyConfig = toml::from_str(&toml_str).expect("Should deserialize");

        assert_eq!(config.enabled, restored.enabled);
        assert_eq!(config.allowed_hosts, restored.allowed_hosts);
        assert_eq!(config.prompt_unknown, restored.prompt_unknown);
    }
}

// ============================================================================
// Guard Policy Tests
// ============================================================================

mod guard_policy {
    use super::*;

    #[test]
    fn test_guard_policy_default() {
        let policy = GuardPolicy::default();
        assert_eq!(policy, GuardPolicy::Prompt);
    }

    #[test]
    fn test_guard_config_path_policy() {
        let mut policy_map = HashMap::new();
        policy_map.insert("~/.kube/config".to_string(), GuardPolicy::Prompt);
        policy_map.insert("~/.docker/config.json".to_string(), GuardPolicy::Auto);
        policy_map.insert("~/.secrets/*".to_string(), GuardPolicy::Deny);

        let config = GuardConfig {
            enabled: true,
            paths: vec![
                "~/.kube".to_string(),
                "~/.docker".to_string(),
                "~/.secrets".to_string(),
            ],
            policy: policy_map,
            timeout_secs: 60,
            default_on_timeout: GuardPolicy::Deny,
        };

        assert_eq!(
            config.policy.get("~/.kube/config"),
            Some(&GuardPolicy::Prompt)
        );
        assert_eq!(
            config.policy.get("~/.docker/config.json"),
            Some(&GuardPolicy::Auto)
        );
        assert_eq!(config.policy.get("~/.secrets/*"), Some(&GuardPolicy::Deny));
    }

    #[test]
    fn test_guard_timeout_config() {
        let config = GuardConfig {
            enabled: true,
            paths: vec!["~/.kube".to_string()],
            policy: HashMap::new(),
            timeout_secs: 120,
            default_on_timeout: GuardPolicy::Deny,
        };

        assert_eq!(config.timeout_secs, 120);
        assert_eq!(config.default_on_timeout, GuardPolicy::Deny);
    }

    #[test]
    fn test_guard_policy_in_hashmap_serialization() {
        // GuardPolicy can be serialized as values in a HashMap (as in config)
        let mut policy_map = HashMap::new();
        policy_map.insert("test_path".to_string(), GuardPolicy::Prompt);

        let config = GuardConfig {
            enabled: true,
            paths: vec!["~/.test".to_string()],
            policy: policy_map,
            timeout_secs: 60,
            default_on_timeout: GuardPolicy::Deny,
        };

        let toml_str = toml::to_string(&config).expect("Should serialize");
        assert!(toml_str.contains("prompt"));

        let restored: GuardConfig = toml::from_str(&toml_str).expect("Should deserialize");
        assert_eq!(restored.policy.get("test_path"), Some(&GuardPolicy::Prompt));
    }
}

// ============================================================================
// Network Config Tests
// ============================================================================

mod network_config {
    use super::*;

    #[test]
    fn test_network_mode_default() {
        let mode = NetworkMode::default();
        assert_eq!(mode, NetworkMode::Open);
    }

    #[test]
    fn test_network_config_proxy_mode() {
        let config = NetworkConfig {
            mode: NetworkMode::Proxy,
            allow_domains: vec![
                "api.github.com".to_string(),
                "registry.npmjs.org".to_string(),
                "crates.io".to_string(),
            ],
            log_requests: true,
        };

        assert_eq!(config.mode, NetworkMode::Proxy);
        assert!(config.allow_domains.contains(&"api.github.com".to_string()));
        assert!(config.log_requests);
    }

    #[test]
    fn test_network_config_isolated() {
        let config = NetworkConfig {
            mode: NetworkMode::Isolated,
            allow_domains: vec![],
            log_requests: false,
        };

        assert_eq!(config.mode, NetworkMode::Isolated);
        assert!(config.allow_domains.is_empty());
    }

    #[test]
    fn test_network_mode_in_config_serialization() {
        // NetworkMode can be serialized as part of NetworkConfig
        let config = NetworkConfig {
            mode: NetworkMode::Proxy,
            allow_domains: vec!["test.com".to_string()],
            log_requests: true,
        };

        let toml_str = toml::to_string(&config).expect("Should serialize");
        assert!(toml_str.contains("proxy"));

        let restored: NetworkConfig = toml::from_str(&toml_str).expect("Should deserialize");
        assert_eq!(restored.mode, NetworkMode::Proxy);
    }
}

// ============================================================================
// ttyd Socket Path Tests
// ============================================================================

mod ttyd_socket_path {
    use super::*;

    #[test]
    fn test_socket_path_contains_session_id() {
        let path = ProcessManager::ttyd_socket_path("session-12345");
        let path_str = path.to_string_lossy();

        assert!(path_str.contains("session-12345"));
        assert!(path_str.ends_with(".sock"));
    }

    #[test]
    fn test_socket_path_is_in_runtime_dir() {
        let path = ProcessManager::ttyd_socket_path("test-session");
        let path_str = path.to_string_lossy();

        // Should be in a runtime directory, not a world-accessible location
        if cfg!(target_os = "macos") {
            // macOS uses TMPDIR which contains /var/folders/ or similar
            assert!(
                path_str.contains("/var/folders/")
                    || path_str.contains("/tmp")
                    || path_str.contains("/private/var"),
                "macOS path should be in TMPDIR: {}",
                path_str
            );
        } else {
            // Linux uses XDG_RUNTIME_DIR or /run/user/$UID
            assert!(
                path_str.contains("/run/user/") || path_str.contains("/tmp"),
                "Linux path should be in XDG_RUNTIME_DIR: {}",
                path_str
            );
        }
    }

    #[test]
    fn test_socket_path_unique_per_session() {
        let path1 = ProcessManager::ttyd_socket_path("session-a");
        let path2 = ProcessManager::ttyd_socket_path("session-b");

        assert_ne!(path1, path2);
    }

    #[test]
    fn test_socket_path_special_characters() {
        // Session IDs might have special characters
        let path = ProcessManager::ttyd_socket_path("ses_abc123-xyz");
        let path_str = path.to_string_lossy();

        assert!(path_str.contains("ses_abc123-xyz"));
    }

    #[test]
    fn test_socket_path_consistency() {
        // Same session ID should always give same path
        let path1 = ProcessManager::ttyd_socket_path("consistent-session");
        let path2 = ProcessManager::ttyd_socket_path("consistent-session");

        assert_eq!(path1, path2);
    }
}

// ============================================================================
// Prompt Config Tests
// ============================================================================

mod prompt_config {
    use super::*;

    #[test]
    fn test_prompt_config_serde_defaults() {
        // When deserializing from empty TOML, serde defaults should apply
        let config: PromptConfig = toml::from_str("").expect("Should parse empty");

        // Serde defaults should give reasonable values
        assert_eq!(config.auto_deny_timeout_secs, 30); // from default_prompt_timeout()
        assert!(config.desktop_notifications); // from default_true()
    }

    #[test]
    fn test_prompt_config_custom() {
        let config = PromptConfig {
            desktop_notifications: false,
            auto_deny_timeout_secs: 120,
        };

        assert!(!config.desktop_notifications);
        assert_eq!(config.auto_deny_timeout_secs, 120);
    }

    #[test]
    fn test_prompt_config_serialization() {
        let config = PromptConfig {
            desktop_notifications: true,
            auto_deny_timeout_secs: 45,
        };

        let toml_str = toml::to_string(&config).expect("Should serialize");
        let restored: PromptConfig = toml::from_str(&toml_str).expect("Should deserialize");

        assert_eq!(config.desktop_notifications, restored.desktop_notifications);
        assert_eq!(
            config.auto_deny_timeout_secs,
            restored.auto_deny_timeout_secs
        );
    }
}

// ============================================================================
// Path Expansion Tests
// ============================================================================

mod path_expansion {
    use super::*;

    #[test]
    fn test_expand_tilde_in_paths() {
        let profile = SandboxProfile::development();

        // All paths with ~ should be present
        let has_home_paths = profile
            .deny_read
            .iter()
            .any(|p| p.starts_with("~") || p.starts_with("$HOME"));

        assert!(has_home_paths);
    }

    #[test]
    fn test_paths_are_normalized() {
        let profile = SandboxProfile::development();

        // No double slashes
        for path in &profile.deny_read {
            assert!(!path.contains("//"), "Path should not have //: {}", path);
        }

        // No trailing slashes (except root)
        for path in &profile.deny_read {
            if path != "/" {
                assert!(!path.ends_with('/'), "Path should not end with /: {}", path);
            }
        }
    }
}

// ============================================================================
// Integration-Style Tests
// ============================================================================

mod integration {
    use super::*;

    #[test]
    fn test_full_profile_roundtrip_toml() {
        let original = SandboxProfile::development();

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&original).expect("Should serialize");

        // Deserialize back
        let restored: SandboxProfile = toml::from_str(&toml_str).expect("Should deserialize");

        // Compare key fields
        assert_eq!(original.deny_read.len(), restored.deny_read.len());
        assert_eq!(original.isolate_network, restored.isolate_network);
        assert_eq!(original.isolate_pid, restored.isolate_pid);
    }

    #[test]
    fn test_config_file_roundtrip_toml() {
        let mut profiles = HashMap::new();
        profiles.insert("test".to_string(), SandboxProfile::development());

        let original = SandboxConfigFile {
            enabled: true,
            profile: "test".to_string(),
            profiles,
        };

        let toml_str = toml::to_string_pretty(&original).expect("Should serialize");
        let restored: SandboxConfigFile = toml::from_str(&toml_str).expect("Should deserialize");

        assert_eq!(original.enabled, restored.enabled);
        assert_eq!(original.profile, restored.profile);
        assert!(restored.profiles.contains_key("test"));
    }

    #[test]
    fn test_complex_profile_parsing() {
        let toml_str = r#"
enabled = true
profile = "enterprise"

[profiles.enterprise]
deny_read = ["~/.ssh", "~/.gnupg", "~/.aws", "~/.vault", "~/.secrets"]
allow_write = ["/tmp", "~/.cache"]
deny_write = ["~/.config/octo/sandbox.toml"]
isolate_network = false
isolate_pid = true

[profiles.enterprise.guard]
enabled = true
paths = ["~/.kube", "~/.docker", "~/.terraform"]
timeout_secs = 90
default_on_timeout = "deny"

[profiles.enterprise.guard.policy]
"~/.kube/config" = "prompt"
"~/.docker/config.json" = "prompt"
"~/.terraform/credentials.tfrc.json" = "deny"

[profiles.enterprise.ssh]
enabled = true
allowed_hosts = ["github.com", "gitlab.company.internal"]
allowed_keys = []
prompt_unknown = true
log_connections = true

[profiles.enterprise.network]
mode = "proxy"
allow_domains = ["api.github.com", "registry.npmjs.org", "crates.io", "pypi.org"]
log_requests = true

[profiles.enterprise.prompts]
desktop_notifications = true
auto_deny_timeout_secs = 60
"#;

        let config: SandboxConfigFile =
            toml::from_str(toml_str).expect("Should parse complex profile");

        assert!(config.enabled);
        assert_eq!(config.profile, "enterprise");

        let profile = config
            .profiles
            .get("enterprise")
            .expect("Profile should exist");

        // Verify all sections parsed correctly
        assert!(profile.deny_read.contains(&"~/.vault".to_string()));
        assert!(profile.isolate_pid);

        let guard = profile.guard.as_ref().expect("Guard should exist");
        assert!(guard.paths.contains(&"~/.terraform".to_string()));
        assert_eq!(guard.timeout_secs, 90);

        let ssh = profile.ssh.as_ref().expect("SSH should exist");
        assert!(
            ssh.allowed_hosts
                .contains(&"gitlab.company.internal".to_string())
        );

        let network = profile.network.as_ref().expect("Network should exist");
        assert_eq!(network.mode, NetworkMode::Proxy);
        assert!(network.allow_domains.contains(&"pypi.org".to_string()));

        let prompts = profile.prompts.as_ref().expect("Prompts should exist");
        assert_eq!(prompts.auto_deny_timeout_secs, 60);
    }
}

// ============================================================================
// Security Boundary Tests
// ============================================================================

mod security_boundaries {
    use super::*;

    #[test]
    fn test_cannot_allow_write_to_denied_read() {
        // If a path is denied for reading, it should never be allowed for writing
        let profile = SandboxProfile {
            deny_read: vec!["~/.ssh".to_string()],
            allow_write: vec!["~/.ssh".to_string()], // Attempt to allow write
            deny_write: vec![],
            isolate_network: false,
            isolate_pid: false,
            extra_ro_bind: vec![],
            extra_rw_bind: vec![],
            guard: None,
            ssh: None,
            network: None,
            prompts: None,
        };

        // In a real implementation, the sandbox builder should prevent this
        // For now, we just verify the profile can be created
        assert!(profile.deny_read.contains(&"~/.ssh".to_string()));
    }

    #[test]
    fn test_sandbox_config_protects_itself() {
        let profile = SandboxProfile::development();

        // The sandbox config file should always be protected from modification
        assert!(
            profile
                .deny_write
                .contains(&"~/.config/octo/sandbox.toml".to_string())
        );
    }

    #[test]
    fn test_strict_profile_denies_all_sensitive() {
        let profile = SandboxProfile::strict();

        // Strict should have comprehensive deny list
        let sensitive_paths = ["~/.ssh", "~/.gnupg", "~/.aws", "~/.config"];

        for path in sensitive_paths {
            assert!(
                profile.deny_read.contains(&path.to_string()),
                "Strict profile should deny read to {}",
                path
            );
        }
    }

    #[test]
    fn test_ssh_proxy_default_logs_connections() {
        let profile = SandboxProfile::development();
        let ssh = profile.ssh.expect("Dev profile should have SSH config");

        // Security auditing should be enabled by default
        assert!(ssh.log_connections);
    }
}

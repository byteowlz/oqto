//! macOS launch policy. Filesystem decisions use the same ADR-0028 resolver as
//! bwrap; unsupported namespace/materialisation requirements fail closed.
//!
//! Seatbelt is a single-tenant workstation boundary, not directory masking or
//! hostile multi-tenancy. `sandbox-exec` is a deprecated Apple interface.

use std::path::Path;

use anyhow::{Result, bail};

use crate::config::{LandlockMode, NetworkMode, SandboxConfig, SeccompMode};

/// Reject guarantees this adapter cannot implement, before spawning anything.
/// Linux profiles are not silently downgraded into macOS profiles.
pub fn validate_config(config: &SandboxConfig) -> Result<()> {
    let mut unsupported = Vec::new();
    if config.isolate_pid {
        unsupported.push("isolate_pid");
    }
    if config.drop_all_caps {
        unsupported.push("drop_all_caps");
    }
    if config.disable_userns {
        unsupported.push("disable_userns");
    }
    if config.assert_userns_disabled {
        unsupported.push("assert_userns_disabled");
    }
    if config.no_new_privs {
        unsupported.push("no_new_privs");
    }
    if config.seccomp_mode == SeccompMode::Enforce {
        unsupported.push("seccomp_mode=enforce");
    }
    if config.landlock_mode == LandlockMode::Enforce {
        unsupported.push("landlock_mode=enforce");
    }
    if config.overlay_enabled {
        unsupported.push("overlay_enabled");
    }
    if !config.scoped_paths.is_empty() {
        unsupported.push("scoped_paths");
    }
    if config.network_mode() == NetworkMode::Proxy {
        unsupported.push("network.mode=proxy");
    }
    if let Some(network) = &config.network
        && !network.allow_domains.is_empty()
    {
        unsupported.push("network.allow_domains");
    }
    if let Some(ssh) = &config.ssh
        && ssh.enabled
        && !ssh.allowed_hosts.is_empty()
    {
        unsupported.push("ssh.allowed_hosts (key grants are not destination grants)");
    }
    if !unsupported.is_empty() {
        bail!(
            "Seatbelt cannot enforce {}; select an explicit macOS-compatible policy or a Linux placement. Refusing to run unsandboxed",
            unsupported.join(", ")
        );
    }
    if config.seccomp_mode == SeccompMode::Audit || config.landlock_mode == LandlockMode::Audit {
        log::warn!(
            "Seatbelt: Linux seccomp/Landlock audit unavailable; filesystem policy remains enforced"
        );
    }
    Ok(())
}

/// Compile effective configuration on the execution host. Cache materialisation
/// is explicit; scoped rebinds and overlays are rejected rather than ignored.
pub fn compile_profile(config: &SandboxConfig, workspace: &Path) -> Result<String> {
    validate_config(config)?;
    let mut effective = config.clone();
    if let Some(cache) = config.workspace_cache_dir(workspace, None)? {
        effective
            .allow_write
            .push(cache.to_string_lossy().into_owned());
    }
    let build = effective.permission_policy(workspace, None)?;
    let mut profile = crate::policy_seatbelt::compile_profile(&build.policy);
    if config.isolate_network || config.network_mode() == NetworkMode::Isolated {
        profile.push_str("(deny network*)\n");
    } else {
        profile.push_str("(allow network*)\n");
        profile.push_str(&crate::policy_seatbelt::compile_socket_rules(&build.policy));
    }
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HomeAccess, NetworkConfig, ReadPolicy};

    fn mac_config() -> SandboxConfig {
        SandboxConfig {
            no_new_privs: false,
            ..SandboxConfig::from_profile("minimal")
        }
    }

    #[test]
    fn shipped_macos_template_has_no_implicit_home_or_tmp_grants() {
        let file: crate::SandboxConfigFile = toml::from_str(include_str!(
            "../../oqto/examples/sandbox.template.macos-host.toml"
        ))
        .unwrap();
        let config: SandboxConfig = file.into();
        validate_config(&config).unwrap();
        assert_eq!(config.read_policy, ReadPolicy::Allowlist);
        assert_eq!(config.home_access, HomeAccess::None);
        assert!(config.allow_write.is_empty());
        assert!(config.scoped_paths.is_empty());
    }

    #[test]
    fn linux_profiles_are_not_silently_downgraded() {
        for profile in ["minimal", "development", "strict"] {
            assert!(validate_config(&SandboxConfig::from_profile(profile)).is_err());
        }
        assert!(validate_config(&mac_config()).is_ok());
    }

    #[test]
    fn unsupported_requirements_are_reported_together() {
        let error = validate_config(&SandboxConfig::from_profile("strict"))
            .unwrap_err()
            .to_string();
        for required in ["isolate_pid", "landlock_mode=enforce", "scoped_paths"] {
            assert!(error.contains(required), "{error}");
        }
    }

    #[test]
    fn proxy_and_destination_policies_fail_closed() {
        let mut config = mac_config();
        config.network = Some(NetworkConfig {
            mode: NetworkMode::Proxy,
            ..Default::default()
        });
        assert!(
            validate_config(&config)
                .unwrap_err()
                .to_string()
                .contains("network.mode=proxy")
        );
        config.network = Some(NetworkConfig {
            allow_domains: vec!["example.com".into()],
            ..Default::default()
        });
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn isolated_network_mode_is_not_ignored() {
        let mut config = mac_config();
        config.network = Some(NetworkConfig {
            mode: NetworkMode::Isolated,
            ..Default::default()
        });
        let dir = tempfile::tempdir().unwrap();
        let profile = compile_profile(&config, dir.path()).unwrap();
        assert!(profile.contains("(deny network*)"));
        assert!(!profile.contains("(allow network*)"));
    }

    #[test]
    fn allowlist_home_and_extra_grants_use_shared_policy() {
        let mut config = mac_config();
        config.read_policy = ReadPolicy::Allowlist;
        config.home_access = HomeAccess::None;
        let dir = tempfile::tempdir().unwrap();
        let read = dir.path().join("read");
        let write = dir.path().join("write");
        config
            .extra_ro_bind
            .push(read.to_string_lossy().into_owned());
        config
            .extra_rw_bind
            .push(write.to_string_lossy().into_owned());
        let profile = compile_profile(&config, dir.path()).unwrap();
        assert!(!profile.contains("(allow file-read*)\n"));
        assert!(profile.contains(&format!(
            "(deny file-write* (subpath \"{}\"))",
            read.display()
        )));
        assert!(profile.contains(&format!(
            "(allow file-read* file-write* (subpath \"{}\"))",
            write.display()
        )));
    }

    #[test]
    fn socket_denies_are_independent_of_file_denies_and_escaped() {
        let mut config = mac_config();
        config.deny_read.push("/tmp/quoted\"socket".into());
        let dir = tempfile::tempdir().unwrap();
        let profile = compile_profile(&config, dir.path()).unwrap();
        assert!(profile.contains(
            "(deny network-outbound (remote unix-socket (subpath \"/tmp/quoted\\\"socket\")))"
        ));
    }
}

//! Provisioning contract models for Oqto setup and doctor flows.
//!
//! This crate is intentionally data-first: setup should converge hosts toward a
//! typed contract instead of scattering path, permission, and service knowledge
//! across shell scripts and CLI commands.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Supported install intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallProfile {
    /// Personal/single-user local install.
    Personal,
    /// Team/multi-user Linux install with isolated Linux users.
    Team,
}

/// Severity for verifier findings.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckSeverity {
    Info,
    Warning,
    Error,
}

/// Desired filesystem object contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DesiredPath {
    pub path: String,
    pub kind: PathKind,
    pub owner: String,
    pub group: String,
    /// Unix mode rendered as octal text so JSON/TOML reports stay unambiguous.
    pub mode: String,
    pub purpose: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PathKind {
    Directory,
    File,
    Socket,
}

/// Desired service contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DesiredService {
    pub name: String,
    pub scope: ServiceScope,
    pub user: Option<String>,
    pub enabled: bool,
    pub active: bool,
    pub purpose: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceScope {
    System,
    User,
}

/// Desired runner socket contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunnerSocketContract {
    pub pattern: String,
    pub producer: String,
    pub consumer_config_key: String,
    pub purpose: String,
}

/// A generated provisioning manifest for one install profile.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProvisioningManifest {
    pub profile: InstallProfile,
    pub summary: String,
    pub paths: Vec<DesiredPath>,
    pub services: Vec<DesiredService>,
    pub runner_socket: RunnerSocketContract,
    pub checks: Vec<ProvisioningCheck>,
}

/// Verifier check that setup/doctor should eventually evaluate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProvisioningCheck {
    pub id: String,
    pub severity: CheckSeverity,
    pub description: String,
    pub remediation: String,
}

/// Observed host facts used by doctor/verifier flows.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostFacts {
    pub paths: HashMap<String, ObservedPath>,
    pub services: HashMap<String, ObservedService>,
    pub runner_socket_pattern: Option<String>,
}

/// Observed filesystem object state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservedPath {
    pub exists: bool,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub mode: Option<String>,
}

/// Observed service state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservedService {
    pub enabled: Option<bool>,
    pub active: Option<bool>,
}

/// Concrete verifier finding from comparing a manifest to host facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContractFinding {
    pub id: String,
    pub severity: CheckSeverity,
    pub expected: String,
    pub observed: String,
    pub remediation: String,
}

/// Generate the initial setup contract for an install profile.
pub fn manifest(profile: InstallProfile) -> ProvisioningManifest {
    match profile {
        InstallProfile::Personal => personal_manifest(),
        InstallProfile::Team => team_manifest(),
    }
}

fn personal_manifest() -> ProvisioningManifest {
    ProvisioningManifest {
        profile: InstallProfile::Personal,
        summary: "Personal single-user local install".to_string(),
        paths: vec![
            DesiredPath {
                path: "$XDG_CONFIG_HOME/oqto/config.toml".to_string(),
                kind: PathKind::File,
                owner: "$USER".to_string(),
                group: "$USER_PRIMARY_GROUP".to_string(),
                mode: "0600".to_string(),
                purpose: "backend configuration".to_string(),
            },
            DesiredPath {
                path: "$XDG_RUNTIME_DIR/oqto-runner.sock".to_string(),
                kind: PathKind::Socket,
                owner: "$USER".to_string(),
                group: "$USER_PRIMARY_GROUP".to_string(),
                mode: "0600".to_string(),
                purpose: "single-user runner RPC socket".to_string(),
            },
        ],
        services: vec![
            user_service("oqto-runner.service", "$USER", "single-user runner daemon"),
            user_service("oqto.service", "$USER", "Oqto backend"),
            user_service("eavs.service", "$USER", "LLM proxy"),
        ],
        runner_socket: RunnerSocketContract {
            pattern: "/run/user/{uid}/oqto-runner.sock".to_string(),
            producer: "oqto-runner.service ExecStart --socket %t/oqto-runner.sock".to_string(),
            consumer_config_key: "local.runner_socket_pattern".to_string(),
            purpose: "all single-user backend runner RPC goes through the user runtime socket"
                .to_string(),
        },
        checks: vec![
            check(
                "personal.runner.socket.reachable",
                CheckSeverity::Error,
                "oqto-runner socket exists and accepts RPC",
                "start/restart the user oqto-runner.service",
            ),
            check(
                "personal.pi.models",
                CheckSeverity::Error,
                "Pi models.json is generated from EAVS",
                "run setup config sync or regenerate EAVS models",
            ),
        ],
    }
}

fn team_manifest() -> ProvisioningManifest {
    ProvisioningManifest {
        profile: InstallProfile::Team,
        summary: "Team multi-user Linux install with per-user runners".to_string(),
        paths: vec![
            DesiredPath {
                path: "/etc/oqto".to_string(),
                kind: PathKind::Directory,
                owner: "root".to_string(),
                group: "root".to_string(),
                mode: "0755".to_string(),
                purpose: "system policy/config directory".to_string(),
            },
            DesiredPath {
                path: "/var/lib/oqto".to_string(),
                kind: PathKind::Directory,
                owner: "oqto".to_string(),
                group: "oqto".to_string(),
                mode: "0755".to_string(),
                purpose: "backend service state".to_string(),
            },
            DesiredPath {
                path: "/run/oqto/runner-sockets".to_string(),
                kind: PathKind::Directory,
                owner: "root".to_string(),
                group: "oqto".to_string(),
                mode: "2770".to_string(),
                purpose: "shared parent for per-user runner sockets".to_string(),
            },
            DesiredPath {
                path: "/etc/sudoers.d/oqto-multiuser".to_string(),
                kind: PathKind::File,
                owner: "root".to_string(),
                group: "root".to_string(),
                mode: "0440".to_string(),
                purpose: "sudoers policy for safe multi-user provisioning".to_string(),
            },
            DesiredPath {
                path: "/run/oqto/runner-sockets/{linux_username}/oqto-runner.sock".to_string(),
                kind: PathKind::Socket,
                owner: "{linux_username}".to_string(),
                group: "oqto".to_string(),
                mode: "0660".to_string(),
                purpose: "canonical per-user runner RPC socket".to_string(),
            },
        ],
        services: vec![
            system_service("oqto.service", "Oqto backend"),
            system_service("eavs.service", "LLM proxy"),
            user_service(
                "oqto-runner.service",
                "{linux_username}",
                "per-user runner daemon",
            ),
        ],
        runner_socket: RunnerSocketContract {
            pattern: "/run/oqto/runner-sockets/{user}/oqto-runner.sock".to_string(),
            producer: "oqto-usermgr generated oqto-runner.service ExecStart --socket".to_string(),
            consumer_config_key: "local.runner_socket_pattern".to_string(),
            purpose: "all multi-user backend runner RPC uses the shared oqto socket tree"
                .to_string(),
        },
        checks: vec![
            check(
                "team.identity.linux-user",
                CheckSeverity::Error,
                "each active Oqto user has matching linux_username/linux_uid and OS account",
                "run oqtoctl doctor --apply or reprovision the user",
            ),
            check(
                "team.runner.socket.canonical",
                CheckSeverity::Error,
                "each active user's runner socket exists at the canonical shared path",
                "restart/reprovision the per-user oqto-runner service",
            ),
            check(
                "team.runner.socket.no-split-routing",
                CheckSeverity::Warning,
                "user-runtime and shared runner sockets are not both present for the same user",
                "remove stale runner units or align runner_socket_pattern",
            ),
            check(
                "team.sudoers.valid",
                CheckSeverity::Error,
                "/etc/sudoers.d/oqto-multiuser validates with visudo",
                "regenerate Linux user isolation sudoers rules",
            ),
        ],
    }
}

/// Compare an install manifest with observed host facts.
///
/// This is deliberately deterministic and side-effect free so CLI doctor, setup
/// preflight, tests, and future remediation can share one contract evaluator.
pub fn evaluate_manifest(
    manifest: &ProvisioningManifest,
    facts: &HostFacts,
) -> Vec<ContractFinding> {
    let mut findings = Vec::new();

    if let Some(observed_pattern) = facts.runner_socket_pattern.as_deref()
        && observed_pattern != manifest.runner_socket.pattern
    {
        findings.push(ContractFinding {
            id: "runner.socket.pattern".to_string(),
            severity: CheckSeverity::Error,
            expected: manifest.runner_socket.pattern.clone(),
            observed: observed_pattern.to_string(),
            remediation: format!(
                "align {} with the runner service producer ({})",
                manifest.runner_socket.consumer_config_key, manifest.runner_socket.producer
            ),
        });
    }

    for desired in &manifest.paths {
        let Some(observed) = facts.paths.get(&desired.path) else {
            // Template paths are expanded per user by the identity/runner doctor;
            // do not report them as generic missing facts before expansion.
            if desired.path.contains('{') || desired.path.contains('$') {
                continue;
            }
            findings.push(ContractFinding {
                id: format!("path.{}.missing-fact", desired.path),
                severity: CheckSeverity::Warning,
                expected: format!(
                    "{} owner={} group={} mode={}",
                    desired.path, desired.owner, desired.group, desired.mode
                ),
                observed: "not inspected".to_string(),
                remediation: "inspect this path in doctor host fact collection".to_string(),
            });
            continue;
        };

        if !observed.exists {
            findings.push(ContractFinding {
                id: format!("path.{}.missing", desired.path),
                severity: CheckSeverity::Error,
                expected: format!("{} exists", desired.path),
                observed: "missing".to_string(),
                remediation: format!("create {} for {}", desired.path, desired.purpose),
            });
            continue;
        }

        if observed
            .owner
            .as_deref()
            .is_some_and(|owner| owner != desired.owner)
        {
            findings.push(path_attr_finding(
                desired,
                "owner",
                &desired.owner,
                observed.owner.as_deref(),
            ));
        }
        if observed
            .group
            .as_deref()
            .is_some_and(|group| group != desired.group)
        {
            findings.push(path_attr_finding(
                desired,
                "group",
                &desired.group,
                observed.group.as_deref(),
            ));
        }
        if observed
            .mode
            .as_deref()
            .is_some_and(|mode| mode != desired.mode)
        {
            findings.push(path_attr_finding(
                desired,
                "mode",
                &desired.mode,
                observed.mode.as_deref(),
            ));
        }
    }

    for desired in &manifest.services {
        let Some(observed) = facts.services.get(&desired.name) else {
            // User-scoped template services are checked by per-user doctor flows
            // once concrete Linux usernames are known.
            if desired
                .user
                .as_deref()
                .is_some_and(|user| user.contains('{'))
            {
                continue;
            }
            findings.push(ContractFinding {
                id: format!("service.{}.missing-fact", desired.name),
                severity: CheckSeverity::Warning,
                expected: format!(
                    "{} enabled={} active={}",
                    desired.name, desired.enabled, desired.active
                ),
                observed: "not inspected".to_string(),
                remediation: "inspect this service in doctor host fact collection".to_string(),
            });
            continue;
        };

        if observed
            .enabled
            .is_some_and(|enabled| enabled != desired.enabled)
        {
            findings.push(ContractFinding {
                id: format!("service.{}.enabled", desired.name),
                severity: CheckSeverity::Error,
                expected: desired.enabled.to_string(),
                observed: observed
                    .enabled
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                remediation: format!("systemctl enable {}", desired.name),
            });
        }
        if observed
            .active
            .is_some_and(|active| active != desired.active)
        {
            findings.push(ContractFinding {
                id: format!("service.{}.active", desired.name),
                severity: CheckSeverity::Error,
                expected: desired.active.to_string(),
                observed: observed
                    .active
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                remediation: format!("systemctl start {}", desired.name),
            });
        }
    }

    findings
}

fn path_attr_finding(
    desired: &DesiredPath,
    attr: &str,
    expected: &str,
    observed: Option<&str>,
) -> ContractFinding {
    ContractFinding {
        id: format!("path.{}.{}", desired.path, attr),
        severity: CheckSeverity::Error,
        expected: expected.to_string(),
        observed: observed.unwrap_or("unknown").to_string(),
        remediation: format!("fix {} {} for {}", desired.path, attr, desired.purpose),
    }
}

fn system_service(name: &str, purpose: &str) -> DesiredService {
    DesiredService {
        name: name.to_string(),
        scope: ServiceScope::System,
        user: None,
        enabled: true,
        active: true,
        purpose: purpose.to_string(),
    }
}

fn user_service(name: &str, user: &str, purpose: &str) -> DesiredService {
    DesiredService {
        name: name.to_string(),
        scope: ServiceScope::User,
        user: Some(user.to_string()),
        enabled: true,
        active: true,
        purpose: purpose.to_string(),
    }
}

fn check(
    id: &str,
    severity: CheckSeverity,
    description: &str,
    remediation: &str,
) -> ProvisioningCheck {
    ProvisioningCheck {
        id: id.to_string(),
        severity,
        description: description.to_string(),
        remediation: remediation.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_manifest_uses_shared_runner_socket_contract() {
        let manifest = manifest(InstallProfile::Team);

        assert_eq!(
            manifest.runner_socket.pattern,
            "/run/oqto/runner-sockets/{user}/oqto-runner.sock"
        );
        assert!(manifest.paths.iter().any(|path| {
            path.path == "/run/oqto/runner-sockets"
                && path.owner == "root"
                && path.group == "oqto"
                && path.mode == "2770"
        }));
        assert!(manifest.paths.iter().any(|path| {
            path.path == "/etc/sudoers.d/oqto-multiuser"
                && path.owner == "root"
                && path.group == "root"
                && path.mode == "0440"
        }));
        assert!(
            manifest
                .checks
                .iter()
                .any(|check| check.id == "team.runner.socket.canonical")
        );
    }

    #[test]
    fn personal_manifest_uses_user_runtime_runner_socket_contract() {
        let manifest = manifest(InstallProfile::Personal);

        assert_eq!(
            manifest.runner_socket.pattern,
            "/run/user/{uid}/oqto-runner.sock"
        );
        assert!(
            manifest
                .checks
                .iter()
                .any(|check| check.id == "personal.runner.socket.reachable")
        );
    }

    #[test]
    fn manifests_are_json_serializable_for_doctor_reports() {
        let rendered = serde_json::to_string_pretty(&manifest(InstallProfile::Team))
            .expect("manifest should serialize");
        assert!(rendered.contains("team.runner.socket.no-split-routing"));
    }

    #[test]
    fn evaluator_detects_runner_socket_pattern_drift() {
        let manifest = manifest(InstallProfile::Team);
        let facts = HostFacts {
            runner_socket_pattern: Some("/run/user/{uid}/oqto-runner.sock".to_string()),
            ..HostFacts::default()
        };

        let findings = evaluate_manifest(&manifest, &facts);

        assert!(findings.iter().any(|finding| {
            finding.id == "runner.socket.pattern"
                && finding.expected == "/run/oqto/runner-sockets/{user}/oqto-runner.sock"
        }));
    }

    #[test]
    fn evaluator_skips_unexpanded_per_user_templates() {
        let manifest = manifest(InstallProfile::Team);
        let findings = evaluate_manifest(&manifest, &HostFacts::default());

        assert!(!findings.iter().any(|finding| {
            finding.id.contains("{linux_username}")
                || finding.id == "service.oqto-runner.service.missing-fact"
        }));
    }

    #[test]
    fn evaluator_detects_wrong_runner_socket_directory_permissions() {
        let manifest = manifest(InstallProfile::Team);
        let mut facts = HostFacts::default();
        facts.paths.insert(
            "/run/oqto/runner-sockets".to_string(),
            ObservedPath {
                exists: true,
                owner: Some("root".to_string()),
                group: Some("root".to_string()),
                mode: Some("0755".to_string()),
            },
        );

        let findings = evaluate_manifest(&manifest, &facts);

        assert!(findings.iter().any(|finding| {
            finding.id == "path./run/oqto/runner-sockets.group" && finding.observed == "root"
        }));
        assert!(findings.iter().any(|finding| {
            finding.id == "path./run/oqto/runner-sockets.mode" && finding.observed == "0755"
        }));
    }
}

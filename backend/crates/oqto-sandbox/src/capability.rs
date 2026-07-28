//! What a sandbox backend can enforce, and what to do when it cannot.
//!
//! Applying the guarantees a backend *does* provide is unconditional. This
//! module only decides the reaction to the ones it cannot, so a backend can
//! never quietly enforce less than the policy asked for.
//!
//! Deliberately not named `audit`: in [`crate::LandlockMode`] and
//! [`crate::SeccompMode`] that word means "do not restrict, only log the
//! intended ruleset". Reusing it here would invite an implementation that skips
//! enforcement, which is the opposite of the requirement.

use crate::path_policy::{Access, ResolvedPolicy};
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

/// How to react to a guarantee the selected backend cannot provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnMissingCapability {
    /// Refuse to start. The default for profiles intended to contain untrusted
    /// work, where running with an unknown weakening is worse than not running.
    #[default]
    Fail,
    /// Start with every available guarantee still enforced, and report the gap.
    ///
    /// Intended for standalone macOS and development, where refusing outright
    /// pushes people to disable the sandbox altogether, which is strictly worse
    /// than running with two known and reported gaps.
    Warn,
    /// Start and say nothing. An escape hatch, never a default.
    Ignore,
}

/// What one backend can express.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendCapabilities {
    pub backend: String,
    /// The strongest access the backend can grant.
    pub max_access: Access,
    /// Whether a denied directory can be made to appear *empty* rather than
    /// merely inaccessible. bwrap mounts a tmpfs; Seatbelt can only deny.
    pub can_mask_directory: bool,
    /// Whether the backend can materialise a missing target before granting it.
    /// Without this a first session writes into a mask and loses its state.
    pub can_create_missing_targets: bool,
    pub can_isolate_network: bool,
    pub can_isolate_pid: bool,
}

impl BackendCapabilities {
    /// Linux bwrap: mount namespaces give every guarantee in this contract.
    #[must_use]
    pub fn bwrap() -> Self {
        Self {
            backend: "bwrap".to_string(),
            max_access: Access::Write,
            can_mask_directory: true,
            can_create_missing_targets: true,
            can_isolate_network: true,
            can_isolate_pid: true,
        }
    }

    /// macOS Seatbelt: a path filter, not a namespace.
    ///
    /// It decides access per path, so the policy model translates, but it
    /// cannot replace a directory with an empty one, cannot create a missing
    /// target, and does not isolate the network or process namespace.
    #[must_use]
    pub fn seatbelt() -> Self {
        Self {
            backend: "seatbelt".to_string(),
            max_access: Access::Write,
            can_mask_directory: false,
            can_create_missing_targets: false,
            can_isolate_network: false,
            can_isolate_pid: false,
        }
    }
}

/// A guarantee the policy asked for that the backend cannot provide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityGap {
    pub guarantee: Guarantee,
    /// The paths that wanted it, for the operator rather than the log.
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Guarantee {
    /// A denied directory is inaccessible but its existence and name remain
    /// observable.
    DirectoryMasking,
    MissingTargetCreation,
    NetworkIsolation,
    PidIsolation,
}

impl fmt::Display for Guarantee {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::DirectoryMasking => "a denied directory is inaccessible but still observable",
            Self::MissingTargetCreation => {
                "a missing target cannot be created before it is granted"
            }
            Self::NetworkIsolation => "the network is not isolated",
            Self::PidIsolation => "the process namespace is not isolated",
        };
        formatter.write_str(text)
    }
}

/// The result of checking a policy against a backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityReport {
    pub backend: String,
    pub gaps: Vec<CapabilityGap>,
}

impl CapabilityReport {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.gaps.is_empty()
    }

    /// One line per gap, suitable for a session banner rather than a log file.
    #[must_use]
    pub fn describe(&self) -> Vec<String> {
        self.gaps
            .iter()
            .map(|gap| {
                format!(
                    "{} cannot enforce: {} ({} path(s))",
                    self.backend,
                    gap.guarantee,
                    gap.paths.len()
                )
            })
            .collect()
    }
}

/// What the policy requires beyond raw path access.
#[derive(Debug, Clone, Copy, Default)]
pub struct RequiredGuarantees {
    pub network_isolation: bool,
    pub pid_isolation: bool,
    pub create_missing_targets: bool,
}

/// Compare a resolved policy against a backend.
///
/// Returns every guarantee the policy wants and the backend lacks. The caller
/// decides what that means via [`OnMissingCapability`]; this function never
/// weakens the policy.
#[must_use]
pub fn check_policy(
    policy: &ResolvedPolicy,
    required: RequiredGuarantees,
    capabilities: &BackendCapabilities,
) -> CapabilityReport {
    let mut gaps = Vec::new();

    if !capabilities.can_mask_directory {
        let masked: Vec<PathBuf> = policy
            .rules()
            .iter()
            .filter(|rule| rule.access == Access::None)
            .map(|rule| rule.path().to_path_buf())
            .collect();
        if !masked.is_empty() {
            gaps.push(CapabilityGap {
                guarantee: Guarantee::DirectoryMasking,
                paths: masked,
            });
        }
    }

    if required.create_missing_targets && !capabilities.can_create_missing_targets {
        gaps.push(CapabilityGap {
            guarantee: Guarantee::MissingTargetCreation,
            paths: Vec::new(),
        });
    }
    if required.network_isolation && !capabilities.can_isolate_network {
        gaps.push(CapabilityGap {
            guarantee: Guarantee::NetworkIsolation,
            paths: Vec::new(),
        });
    }
    if required.pid_isolation && !capabilities.can_isolate_pid {
        gaps.push(CapabilityGap {
            guarantee: Guarantee::PidIsolation,
            paths: Vec::new(),
        });
    }

    CapabilityReport {
        backend: capabilities.backend.clone(),
        gaps,
    }
}

/// Decide whether a spawn may proceed.
///
/// `Err` means refuse. `Ok` carries the gaps that were accepted, so the caller
/// can surface them where a human will see them rather than only in a log.
pub fn authorise(
    report: CapabilityReport,
    mode: OnMissingCapability,
) -> Result<Vec<CapabilityGap>, CapabilityReport> {
    if report.is_complete() {
        return Ok(Vec::new());
    }
    match mode {
        OnMissingCapability::Fail => Err(report),
        OnMissingCapability::Warn | OnMissingCapability::Ignore => Ok(report.gaps),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_policy::{PolicyLayer, ResolvedRule, RuleOrigin};

    fn origin() -> RuleOrigin {
        RuleOrigin {
            layer: PolicyLayer::Admin,
            source: "test".to_string(),
        }
    }

    fn policy_with_mask() -> ResolvedPolicy {
        let mut policy = ResolvedPolicy::new(Access::Read, origin());
        policy.add_rule(ResolvedRule::new("/home/agent/.ssh", Access::None, origin()).unwrap());
        policy
    }

    #[test]
    fn bwrap_provides_every_guarantee_in_the_contract() {
        let report = check_policy(
            &policy_with_mask(),
            RequiredGuarantees {
                network_isolation: true,
                pid_isolation: true,
                create_missing_targets: true,
            },
            &BackendCapabilities::bwrap(),
        );
        assert!(report.is_complete(), "unexpected gaps: {:?}", report.gaps);
    }

    #[test]
    fn seatbelt_reports_what_it_cannot_do() {
        let report = check_policy(
            &policy_with_mask(),
            RequiredGuarantees {
                network_isolation: true,
                pid_isolation: true,
                create_missing_targets: true,
            },
            &BackendCapabilities::seatbelt(),
        );
        let guarantees: Vec<_> = report.gaps.iter().map(|gap| gap.guarantee).collect();
        assert_eq!(
            guarantees,
            vec![
                Guarantee::DirectoryMasking,
                Guarantee::MissingTargetCreation,
                Guarantee::NetworkIsolation,
                Guarantee::PidIsolation,
            ]
        );
        // The gap names the paths that wanted it, not just the guarantee.
        assert_eq!(report.gaps[0].paths.len(), 1);
    }

    #[test]
    fn fail_refuses_and_warn_proceeds_with_the_gaps() {
        let report = check_policy(
            &policy_with_mask(),
            RequiredGuarantees::default(),
            &BackendCapabilities::seatbelt(),
        );
        assert!(authorise(report.clone(), OnMissingCapability::Fail).is_err());

        let accepted = authorise(report, OnMissingCapability::Warn).expect("warn proceeds");
        assert_eq!(accepted.len(), 1);
    }

    #[test]
    fn a_complete_backend_is_authorised_under_every_mode() {
        for mode in [
            OnMissingCapability::Fail,
            OnMissingCapability::Warn,
            OnMissingCapability::Ignore,
        ] {
            let report = check_policy(
                &policy_with_mask(),
                RequiredGuarantees::default(),
                &BackendCapabilities::bwrap(),
            );
            assert_eq!(authorise(report, mode).expect("authorised"), Vec::new());
        }
    }

    #[test]
    fn fail_is_the_default_reaction() {
        assert_eq!(OnMissingCapability::default(), OnMissingCapability::Fail);
    }

    #[test]
    fn the_report_describes_gaps_for_a_person() {
        let report = check_policy(
            &policy_with_mask(),
            RequiredGuarantees::default(),
            &BackendCapabilities::seatbelt(),
        );
        let described = report.describe();
        assert_eq!(described.len(), 1);
        assert!(described[0].contains("seatbelt"), "{described:?}");
        assert!(described[0].contains("observable"), "{described:?}");
    }
}

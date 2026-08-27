use crate::policy::{Protocol, WorkspaceEgressPolicy};
use crate::tier::{EgressTier, TierClaim};
use serde::{Deserialize, Serialize};

/// What a probe attempted and what the workload observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeAttempt {
    pub host: String,
    pub protocol: Protocol,
    pub port: u16,
    /// Whether the workload actually reached the destination.
    pub reached: bool,
    /// Whether the enforcer recorded the attempt.
    pub audited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeFinding {
    /// A destination the policy denies was refused, as required.
    DenialDemonstrated { host: String },
    /// A destination the policy denies was reachable. The tier claim is false.
    DenialFailed { host: String },
    /// A destination the policy permits was refused. Enforcement is too tight.
    AllowedDestinationBlocked { host: String },
    /// Reached and recorded, which is what `open_attributable` promises.
    CaptureDemonstrated { host: String },
    /// Reached without a record, so attribution cannot be claimed.
    CaptureFailed { host: String },
}

impl ProbeFinding {
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::DenialFailed { .. }
                | Self::AllowedDestinationBlocked { .. }
                | Self::CaptureFailed { .. }
        )
    }
}

/// The verdict of one probe run against one placement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeReport {
    pub workspace_id: String,
    pub findings: Vec<ProbeFinding>,
}

impl ProbeReport {
    pub fn failures(&self) -> impl Iterator<Item = &ProbeFinding> {
        self.findings.iter().filter(|f| f.is_failure())
    }

    pub fn passed(&self) -> bool {
        self.failures().next().is_none()
    }

    fn demonstrated_denial(&self) -> bool {
        self.passed()
            && self
                .findings
                .iter()
                .any(|f| matches!(f, ProbeFinding::DenialDemonstrated { .. }))
    }

    /// The claim these observations support. A tier is never taken from
    /// configuration: without an observed denial the claim degrades to `None`.
    pub fn claim(&self, configured_tier: EgressTier) -> TierClaim {
        TierClaim::measured(configured_tier, self.demonstrated_denial())
    }
}

/// Grades probe attempts against the policy that was supposed to bind them.
///
/// This is deliberately a pure function over observations: whoever ran the
/// attempts (doctor, integration test, live placement) grades them the same way.
pub fn evaluate(policy: &WorkspaceEgressPolicy, attempts: &[ProbeAttempt]) -> ProbeReport {
    let findings = attempts
        .iter()
        .map(|attempt| {
            let permitted = policy.permits(&attempt.host, attempt.protocol, attempt.port);
            match (permitted, attempt.reached) {
                (false, true) => ProbeFinding::DenialFailed {
                    host: attempt.host.clone(),
                },
                (false, false) => ProbeFinding::DenialDemonstrated {
                    host: attempt.host.clone(),
                },
                (true, false) => ProbeFinding::AllowedDestinationBlocked {
                    host: attempt.host.clone(),
                },
                (true, true) if attempt.audited => ProbeFinding::CaptureDemonstrated {
                    host: attempt.host.clone(),
                },
                (true, true) => ProbeFinding::CaptureFailed {
                    host: attempt.host.clone(),
                },
            }
        })
        .collect();

    ProbeReport {
        workspace_id: policy.workspace_id.clone(),
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::DestinationRule;

    fn attempt(host: &str, reached: bool, audited: bool) -> ProbeAttempt {
        ProbeAttempt {
            host: host.to_string(),
            protocol: Protocol::Https,
            port: 443,
            reached,
            audited,
        }
    }

    fn policy() -> WorkspaceEgressPolicy {
        WorkspaceEgressPolicy::denied("ws-1")
            .with_destinations(vec![DestinationRule::https("github.com")])
    }

    #[test]
    fn reaching_a_denied_host_fails_the_run_and_the_claim() {
        let report = evaluate(&policy(), &[attempt("evil.example", true, true)]);
        assert!(!report.passed());
        assert_eq!(
            report.claim(EgressTier::Isolated).advertisable_tier(),
            EgressTier::None
        );
    }

    #[test]
    fn refusing_a_denied_host_demonstrates_the_tier() {
        let report = evaluate(&policy(), &[attempt("evil.example", false, true)]);
        assert!(report.passed());
        assert_eq!(
            report.claim(EgressTier::Isolated).advertisable_tier(),
            EgressTier::Isolated
        );
    }

    #[test]
    fn a_run_without_any_denial_attempt_proves_nothing() {
        let report = evaluate(&policy(), &[attempt("github.com", true, true)]);
        assert!(report.passed());
        assert_eq!(
            report.claim(EgressTier::Isolated).advertisable_tier(),
            EgressTier::None
        );
    }

    #[test]
    fn blocking_an_allowed_destination_is_a_failure() {
        let report = evaluate(&policy(), &[attempt("github.com", false, false)]);
        assert!(!report.passed());
    }

    #[test]
    fn reaching_without_a_record_fails_capture() {
        let report = evaluate(&policy(), &[attempt("github.com", true, false)]);
        assert!(matches!(
            report.findings.as_slice(),
            [ProbeFinding::CaptureFailed { .. }]
        ));
        assert!(!report.passed());
    }
}

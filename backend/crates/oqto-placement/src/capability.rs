//! Evidence for a placement, not a grant to use it. Read-only host discovery
//! must never be confused with a successful launch or operator authorization.

use crate::PlacementKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "status", content = "reason")]
pub enum ProbeEvidence {
    Verified,
    Denied(String),
    Unverified(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlacementCapabilityReport {
    pub backend: PlacementKind,
    /// Names the authority that observed the runtime, not an authorization
    /// credential. Only the host-side supervisor may produce this report.
    pub source: String,
    pub observed_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub effective_rootless: ProbeEvidence,
    /// A read-only `podman info` cannot fill this field: a launch canary or
    /// successful actual placement must supply independent evidence.
    pub container_launch: ProbeEvidence,
    pub runner_sandbox: ProbeEvidence,
    pub operator_policy: ProbeEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementAvailability {
    pub available: bool,
    pub denial_reasons: Vec<String>,
}

impl PlacementCapabilityReport {
    pub fn evaluate_at(&self, now_unix_ms: u64) -> PlacementAvailability {
        let mut denial_reasons = Vec::new();
        if now_unix_ms < self.observed_at_unix_ms || now_unix_ms >= self.expires_at_unix_ms {
            denial_reasons.push("capability evidence is stale or not yet valid".to_string());
        }
        for (name, evidence) in [
            ("effective rootless runtime", &self.effective_rootless),
            ("container launch", &self.container_launch),
            ("runner sandbox", &self.runner_sandbox),
            ("operator policy", &self.operator_policy),
        ] {
            match evidence {
                ProbeEvidence::Verified => {}
                ProbeEvidence::Denied(reason) | ProbeEvidence::Unverified(reason) => {
                    denial_reasons.push(format!("{name}: {reason}"));
                }
            }
        }
        PlacementAvailability {
            available: denial_reasons.is_empty(),
            denial_reasons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> PlacementCapabilityReport {
        PlacementCapabilityReport {
            backend: PlacementKind::RootlessPodman,
            source: "host-side-podman-supervisor".into(),
            observed_at_unix_ms: 100,
            expires_at_unix_ms: 200,
            effective_rootless: ProbeEvidence::Verified,
            container_launch: ProbeEvidence::Unverified("launch has not been tested".into()),
            runner_sandbox: ProbeEvidence::Unverified("runner has not attested sandbox".into()),
            operator_policy: ProbeEvidence::Unverified("no operator grant".into()),
        }
    }

    #[test]
    fn discovery_alone_never_makes_a_placement_available() {
        let result = report().evaluate_at(150);
        assert!(!result.available);
        assert_eq!(result.denial_reasons.len(), 3);
    }

    #[test]
    fn policy_and_sandbox_are_independent_hard_gates() {
        let mut evidence = report();
        evidence.container_launch = ProbeEvidence::Verified;
        evidence.runner_sandbox = ProbeEvidence::Verified;
        evidence.operator_policy = ProbeEvidence::Denied("multi-user tier prohibited".into());
        assert!(!evidence.evaluate_at(150).available);
        evidence.operator_policy = ProbeEvidence::Verified;
        evidence.runner_sandbox = ProbeEvidence::Denied("sandbox unavailable".into());
        assert!(!evidence.evaluate_at(150).available);
        evidence.runner_sandbox = ProbeEvidence::Verified;
        assert!(evidence.evaluate_at(150).available);
        assert!(!evidence.evaluate_at(200).available);
        assert!(!evidence.evaluate_at(99).available);
    }

    #[test]
    fn unavailable_rootless_runtime_cannot_be_promoted_by_other_evidence() {
        let mut evidence = report();
        evidence.effective_rootless = ProbeEvidence::Denied("engine is rootful".into());
        evidence.container_launch = ProbeEvidence::Verified;
        evidence.runner_sandbox = ProbeEvidence::Verified;
        evidence.operator_policy = ProbeEvidence::Verified;
        let result = evidence.evaluate_at(150);
        assert!(!result.available);
        assert!(
            result
                .denial_reasons
                .iter()
                .any(|reason| reason.contains("rootful"))
        );
    }
}

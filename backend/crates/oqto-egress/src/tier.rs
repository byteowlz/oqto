use serde::{Deserialize, Serialize};

/// How strongly a policy is bound to the running workload.
///
/// A tier states what a malicious agent cannot do. Controls that only bind a
/// cooperative client are advisory and must not be advertised as containment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EgressTier {
    /// No enforcement. The workload may reach whatever the host can reach.
    #[default]
    None,
    /// Enforcement a cooperative client honours (proxy environment variables).
    /// A workload that ignores the configuration still reaches the network.
    Transparent,
    /// The workload has no route out except the enforcer, because it has no
    /// network of its own. Bypass requires escaping the placement itself.
    Isolated,
}

impl EgressTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Transparent => "transparent",
            Self::Isolated => "isolated",
        }
    }

    /// Whether a workload that ignores its configuration is still bound.
    pub fn binds_uncooperative_workload(self) -> bool {
        matches!(self, Self::Isolated)
    }
}

/// Who vouches for a tier claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attestation {
    /// Oqto measured the denial itself on this host.
    SelfMeasured,
    /// An operator asserts the property; Oqto cannot prove it locally.
    Operator,
}

/// A tier claim that has been demonstrated rather than configured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierClaim {
    pub tier: EgressTier,
    pub attestation: Attestation,
    /// Denial actually observed by a probe. A claim above `None` without this
    /// is not advertisable.
    pub demonstrated_denial: bool,
}

impl TierClaim {
    pub fn measured(tier: EgressTier, demonstrated_denial: bool) -> Self {
        Self {
            tier,
            attestation: Attestation::SelfMeasured,
            demonstrated_denial,
        }
    }

    /// The tier that may be advertised. An undemonstrated claim degrades to
    /// `None` instead of being taken on trust.
    pub fn advertisable_tier(&self) -> EgressTier {
        if self.tier == EgressTier::None || self.demonstrated_denial {
            self.tier
        } else {
            EgressTier::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_isolated_binds_an_uncooperative_workload() {
        assert!(!EgressTier::None.binds_uncooperative_workload());
        assert!(!EgressTier::Transparent.binds_uncooperative_workload());
        assert!(EgressTier::Isolated.binds_uncooperative_workload());
    }

    #[test]
    fn undemonstrated_claim_degrades_to_none() {
        let claim = TierClaim::measured(EgressTier::Isolated, false);
        assert_eq!(claim.advertisable_tier(), EgressTier::None);
    }

    #[test]
    fn demonstrated_claim_is_advertisable() {
        let claim = TierClaim::measured(EgressTier::Isolated, true);
        assert_eq!(claim.advertisable_tier(), EgressTier::Isolated);
    }

    #[test]
    fn tiers_order_by_strength() {
        assert!(EgressTier::Isolated > EgressTier::Transparent);
        assert!(EgressTier::Transparent > EgressTier::None);
    }
}

use crate::policy::WorkspaceEgressPolicy;
use crate::tier::EgressTier;
use anyhow::Result;

/// Configuration text for one enforcer, plus how the placement reaches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPolicy {
    pub enforcer: &'static str,
    pub config: String,
    /// Environment a cooperative client needs. Under
    /// [`EgressTier::Isolated`] this is a convenience, not the control.
    pub client_env: Vec<(String, String)>,
}

/// An egress enforcer Oqto can compile policy for.
///
/// Oqto owns the policy, the lifecycle, and the audit; the enforcer is
/// replaceable. Keeping this trait narrow is what makes a bought enforcer safe
/// to adopt and safe to drop.
pub trait EgressEnforcer {
    fn name(&self) -> &'static str;

    /// The strongest tier this enforcer can reach in the given placement. The
    /// actual advertised tier still requires a demonstrated denial.
    fn max_tier(&self) -> EgressTier;

    fn compile(&self, policy: &WorkspaceEgressPolicy) -> Result<CompiledPolicy>;
}

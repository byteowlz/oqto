//! Canonical workspace egress policy and the seam enforcers plug into.
//!
//! ADR-0035. Oqto owns the policy, the lifecycle, and the audit; the enforcer
//! that carries traffic is replaceable. Three rules hold the model together:
//! absent or invalid configuration permits nothing, a tier claim requires an
//! observed denial, and provider-side fetching is a separate permission because
//! no network enforcer can see that second hop.

pub mod config;
pub mod enforcer;
pub mod iron_proxy;
pub mod policy;
pub mod probe;
pub mod tier;

pub use config::{EgressConfig, load_policy};
pub use enforcer::{CompiledPolicy, EgressEnforcer};
pub use iron_proxy::{IronProxyEnforcer, ProxyEndpoint};
pub use policy::{DestinationRule, MANDATORY_DENY_CIDRS, Protocol, Verdict, WorkspaceEgressPolicy};
pub use probe::{ProbeAttempt, ProbeFinding, ProbeReport, evaluate};
pub use tier::{Attestation, EgressTier, TierClaim};

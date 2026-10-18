//! Permission authority, not authentication or an agent-facing approval API.
//!
//! The owning runner must derive Binding and operator identity from verified
//! authority, keep this ledger outside executor mounts, and gate *all* provider
//! invocations and result deliveries. Never derive either identity from model
//! arguments. OS confinement and domain stores retain their existing owners.

mod ceiling;
mod ledger;
mod policy;

pub use ceiling::{
    CeilingInput, NETWORK_EGRESS, PATH_PREFIX, Remembered, SHELL_PREFIX, WORKSPACE_READ,
    WORKSPACE_SHELL, WORKSPACE_WRITE, compile_ceiling, normalize_lexical, valid_shell_prefix,
};
pub use ledger::{Admission, Authority, GrantLifetime, Interrupted, Permit, Request, RequestState};
pub use policy::{Binding, Capability, Operation, PathLayer, Policy, Rule, RuleMode, Target};

use anyhow::{Result, ensure};

/// Admission fence for a separately supervised executor. Advancing it does
/// not kill processes: the runner must confirm stop before installing a new
/// executor. Failed preparation leaves this fence closed, never falls back.
#[derive(Debug, Default)]
pub struct ExecutorFence {
    generation: u64,
    accepting: bool,
}

impl ExecutorFence {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn freeze(&mut self) {
        self.accepting = false;
    }

    pub fn install_after_confirmed_stop(&mut self, previous: u64) -> Result<u64> {
        ensure!(
            !self.accepting,
            "executor must be frozen before replacement"
        );
        ensure!(previous == self.generation, "stale executor replacement");
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("executor generation exhausted"))?;
        self.accepting = true;
        Ok(self.generation)
    }

    pub fn admit(&self, generation: u64) -> Result<()> {
        ensure!(
            self.accepting && generation == self.generation,
            "executor fenced"
        );
        Ok(())
    }
}

//! Audit composition uses the execution Principal's runner, never backend-local
//! files or a fabricated chat Session. Audit failure prevents authorization.
use super::{ConnectionAudit, ConnectionAuditEvent};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use oqto_history::oqto_log::host_actions::HostActionRecord;
use oqto_runner::client::RunnerClient;

pub struct RunnerConnectionAudit {
    runner: RunnerClient,
    account_id: String,
    workspace_id: String,
}
impl RunnerConnectionAudit {
    /// All arguments come from authenticated host routing/authoritative binding.
    pub fn new(runner: RunnerClient, account_id: String, workspace_id: String) -> Self {
        Self {
            runner,
            account_id,
            workspace_id,
        }
    }
}
#[async_trait]
impl ConnectionAudit for RunnerConnectionAudit {
    async fn record(&self, event: &ConnectionAuditEvent) -> Result<()> {
        ensure!(
            event.account_id == self.account_id,
            "audit account mismatch"
        );
        self.runner
            .record_host_action(
                self.workspace_id.clone(),
                HostActionRecord {
                    event_id: event.event_id.clone(),
                    account_id: event.account_id.clone(),
                    action: event.action.to_owned(),
                    connection_id: event.connection_id.clone(),
                    connection_revision: event.revision,
                    instance_id: event.instance_id.clone(),
                    slot: event.slot.clone(),
                    content_digest: event.content_digest.clone(),
                },
            )
            .await
    }
}

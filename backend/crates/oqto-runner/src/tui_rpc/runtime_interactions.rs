//! Host-side binding projection; never accepts a binding from the native socket.
//! A RuntimeBinding is response metadata, not a transferable authorization token.
use super::*;
use oqto_protocol::runtime_interactions::{
    RuntimeBinding, RuntimeGeneration, RuntimeId, RuntimeOperation,
};

fn requires_session_control(operation: &RuntimeOperation) -> Result<bool> {
    match operation {
        RuntimeOperation::CommandDispatch { .. } | RuntimeOperation::QueueApply { .. } => Ok(true),
        RuntimeOperation::Capabilities
        | RuntimeOperation::CommandCatalog
        | RuntimeOperation::QueueSnapshot
        | RuntimeOperation::ErrorList { .. }
        | RuntimeOperation::ReceiptGet { .. } => Ok(false),
        RuntimeOperation::PermissionSnapshot
        | RuntimeOperation::PermissionDecide { .. }
        | RuntimeOperation::PolicyRead
        | RuntimeOperation::PolicyApply { .. } => {
            bail!("Semantic permission/policy authority is not Session control")
        }
    }
}

impl TuiObserver {
    /// Project CURRENT host admission and verified native continuity into response metadata.
    /// Does not dispatch, approve a tool, authorize policy writes or register a binding in Pi.
    /// Session control is a prerequisite here, not command/effect admission. Effect callers
    /// must separately authorize the specific operation and revalidate after every await.
    /// Legacy sources without an issued interactionRuntimeGeneration remain unavailable.
    pub async fn runtime_binding_for(
        &mut self,
        operation: &RuntimeOperation,
    ) -> Result<RuntimeBinding> {
        let control = requires_session_control(operation)?;
        self.authority.revalidate(&self.binding)?;
        // Reject missing/revoked control before any native read.
        let before = self.runtime_controller(control)?;
        self.snapshot_state().await?;
        self.authority.revalidate(&self.binding)?;
        let after = self.runtime_controller(control)?;
        ensure!(before == after, "Runtime controller changed");
        let runtime_generation = self
            .details
            .interaction_runtime_generation
            .clone()
            .context("Native runtime continuity unavailable")?;
        let (controller_id, controller_generation) = match after {
            Some((id, generation)) => (
                Some(RuntimeId::try_from(id).map_err(anyhow::Error::msg)?),
                Some(RuntimeGeneration::Id(
                    RuntimeId::try_from(generation).map_err(anyhow::Error::msg)?,
                )),
            ),
            None => (None, None),
        };
        Ok(RuntimeBinding {
            public_session_id: RuntimeId::try_from(self.binding.public_session_id.clone())
                .map_err(anyhow::Error::msg)?,
            native_session_id: RuntimeId::try_from(self.binding.external_session_id.clone())
                .map_err(anyhow::Error::msg)?,
            runtime_generation,
            controller_id,
            controller_generation,
        })
    }

    fn runtime_controller(&self, required: bool) -> Result<Option<(String, String)>> {
        if !required {
            return Ok(None);
        }
        self.input_lease
            .as_ref()
            .context("Session control not acquired")?
            .runtime_controller(&self.binding)
            .map(Some)
    }
}

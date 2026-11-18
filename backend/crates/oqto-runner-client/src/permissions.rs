//! Permission presentation state. Transport only: a consumer can show options
//! and build an explicit decision, never decide on the producer's behalf.

use crate::contract::{
    RuntimeId, RuntimePermissionPresentation, RuntimePermissionReceipt, RuntimePermissionSnapshot,
    RuntimeRevision,
};

/// Active requests and retained receipts as last reported by the producer.
#[derive(Debug, Default, Clone)]
pub struct PermissionState {
    active: Vec<RuntimePermissionPresentation>,
    receipts: Vec<RuntimePermissionReceipt>,
}

impl PermissionState {
    #[must_use]
    pub fn active(&self) -> &[RuntimePermissionPresentation] {
        &self.active
    }

    #[must_use]
    pub fn receipts(&self) -> &[RuntimePermissionReceipt] {
        &self.receipts
    }

    pub fn apply_snapshot(&mut self, snapshot: RuntimePermissionSnapshot) {
        self.active = snapshot.active;
        self.receipts = snapshot.receipts;
    }

    #[must_use]
    pub fn find(
        &self,
        handle: &RuntimeId,
        revision: RuntimeRevision,
    ) -> Option<&RuntimePermissionPresentation> {
        self.active
            .iter()
            .find(|entry| &entry.handle_id == handle && entry.request_revision == revision)
    }

    /// Producer deadline wins over any local countdown; a phone clock cannot extend it.
    #[must_use]
    pub fn is_expired(presentation: &RuntimePermissionPresentation, now_ms: u64) -> bool {
        now_ms >= u64::from(presentation.expires_at_ms)
    }

    #[must_use]
    pub fn issued_option(
        presentation: &RuntimePermissionPresentation,
        option_id: &RuntimeId,
    ) -> bool {
        presentation
            .options
            .iter()
            .any(|option| &option.option_id == option_id)
    }
}

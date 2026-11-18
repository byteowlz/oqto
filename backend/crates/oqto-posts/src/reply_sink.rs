//! Durable reply outbox delivery seam, deliberately not an approval interface.
//! A concrete Session sink MUST deduplicate `request_id` durably in the target
//! Session. It may be called again after any crash, including after delivery
//! but before the outbox acknowledgement was committed. No text/order matching.
use crate::wire::{Mode, Reply, Stamp};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct DeliveryRequest {
    pub owner_identity: String,
    /// The original wire request id, used unchanged as the Session input key.
    pub request_id: String,
    /// Durable public Session id comes from the post's stored stamp only.
    pub target: Stamp,
    pub reply: Reply,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryStatus {
    Queued,
    Delivered,
}
#[derive(Debug, Clone, Copy)]
pub struct DeliveryReceipt {
    pub status: DeliveryStatus,
    /// Requested `steer` on an idle Session becomes follow-up. Only the sink,
    /// reading durable/live Session state, can make this determination.
    pub effective_mode: Mode,
}
impl DeliveryReceipt {
    pub fn validate(&self, requested: Mode) -> Result<()> {
        ensure!(
            self.effective_mode == requested
                || (requested == Mode::Steer && self.effective_mode == Mode::FollowUp),
            "sink changed reply mode unexpectedly"
        );
        Ok(())
    }
}
#[async_trait]
pub trait ReplySink: Send + Sync {
    /// Acceptance must be durable and idempotent on request_id. On an unknown
    /// outcome, return Err: the outbox stays pending for an explicit replay.
    /// `Delivered` is input-placement evidence, not a model answer/approval.
    async fn deliver(&self, request: &DeliveryRequest) -> Result<DeliveryReceipt>;
}

#[derive(Debug, Default)]
pub struct DispatchReport {
    pub accepted: usize,
    pub pending: usize,
}

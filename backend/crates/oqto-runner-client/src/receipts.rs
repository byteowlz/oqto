//! Request outcomes. A receipt records what happened to one request and is never
//! rewritten into success by a later poll.

use crate::contract::{
    RuntimeCommandDisposition, RuntimeEffectState, RuntimeFamily, RuntimeId,
    RuntimePermissionOutcome, RuntimeRejectCode, RuntimeUnavailableReason,
};

/// What a producer reported for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedReceipt {
    Command {
        disposition: RuntimeCommandDisposition,
        rejection: Option<RuntimeRejectCode>,
        effect: Option<RuntimeEffectState>,
        error_id: Option<RuntimeId>,
    },
    Permission {
        outcome: RuntimePermissionOutcome,
    },
    Unavailable {
        family: RuntimeFamily,
        reason: RuntimeUnavailableReason,
    },
    Rejected {
        code: RuntimeRejectCode,
    },
}

/// One recorded outcome, keyed by the request that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptEntry {
    pub request_id: RuntimeId,
    pub operation_id: Option<RuntimeId>,
    pub receipt: RecordedReceipt,
}

/// Append-only record of this consumer's requests.
#[derive(Debug, Default, Clone)]
pub struct ReceiptLog {
    entries: Vec<ReceiptEntry>,
}

impl ReceiptLog {
    #[must_use]
    pub fn entries(&self) -> &[ReceiptEntry] {
        &self.entries
    }

    #[must_use]
    pub fn last(&self) -> Option<&ReceiptEntry> {
        self.entries.last()
    }

    /// Latest receipt for a producer-issued operation identity.
    #[must_use]
    pub fn by_operation(&self, operation_id: &RuntimeId) -> Option<&ReceiptEntry> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.operation_id.as_ref() == Some(operation_id))
    }

    pub fn record(&mut self, entry: ReceiptEntry) {
        self.entries.push(entry);
    }

    /// Apply a later source outcome. An accepted command stays accepted until the
    /// source says otherwise; nothing is erased and unknown operations are ignored.
    pub fn apply_outcome(
        &mut self,
        operation_id: &RuntimeId,
        state: RuntimeEffectState,
        error_id: Option<RuntimeId>,
    ) -> bool {
        for entry in self.entries.iter_mut().rev() {
            if entry.operation_id.as_ref() != Some(operation_id) {
                continue;
            }
            if let RecordedReceipt::Command {
                effect,
                error_id: slot,
                ..
            } = &mut entry.receipt
            {
                *effect = Some(state);
                if error_id.is_some() {
                    *slot = error_id;
                }
                return true;
            }
        }
        false
    }
}

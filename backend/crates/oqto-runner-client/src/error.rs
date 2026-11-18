//! Typed consumer failures. No error carries private task text or secrets.

use crate::contract::{RuntimeFamily, RuntimeRejectCode, RuntimeUnavailableReason};

/// Everything a consumer can refuse locally, before or after contacting a producer.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ClientError {
    /// Response/event version is not the one supported by this build.
    #[error("unsupported contract version")]
    VersionMismatch,
    /// A response arrived that does not answer the outstanding request.
    #[error("response does not answer the outstanding request")]
    UncorrelatedResponse,
    /// Operation/result families disagree, or issued identities do not match.
    #[error("response result does not match the issued operation")]
    ResultMismatch,
    /// Another request is already outstanding on this session.
    #[error("another request is already outstanding")]
    RequestOutstanding,
    /// Nothing is outstanding, so a response cannot be attributed.
    #[error("no request is outstanding")]
    NoOutstandingRequest,
    /// The catalog moved since the descriptor was chosen.
    #[error("catalog generation is stale")]
    StaleGeneration,
    /// No catalog has been applied yet, so nothing can be fenced.
    #[error("catalog is unknown; request it first")]
    CatalogUnknown,
    /// The command is not in the current catalog.
    #[error("unknown command")]
    UnknownCommand,
    /// The command exists but the producer reports it unavailable.
    #[error("command is unavailable")]
    CommandUnavailable(RuntimeUnavailableReason),
    /// No complete/partial queue snapshot with a cursor is known yet.
    #[error("queue cursor is unknown")]
    UnknownQueueCursor,
    /// The source does not declare this action for its queue.
    #[error("queue action is not supported by the source")]
    UnsupportedAction,
    /// The queue generation differs from the bound runtime generation.
    #[error("queue snapshot belongs to another runtime generation")]
    ForeignQueueGeneration,
    /// The queue snapshot's revision went backwards.
    #[error("queue revision is stale")]
    StaleQueueRevision,
    /// The permission request is not in the current snapshot.
    #[error("permission request is not active")]
    NotActive,
    /// The permission request expired according to the producer deadline.
    #[error("permission request expired")]
    Expired,
    /// The option was not issued for this request.
    #[error("option was not issued for this request")]
    UnknownOption,
    /// Controlled decisions need an authenticated controller binding.
    #[error("no controller binding for a controlled decision")]
    MissingController,
    /// The response binding no longer matches the session binding.
    #[error("runtime binding changed; re-admit the session")]
    BindingChanged,
    /// The producer reported a concrete unavailability.
    #[error("producer reports unavailable")]
    Unavailable {
        family: RuntimeFamily,
        reason: RuntimeUnavailableReason,
    },
    /// The producer rejected the operation.
    #[error("producer rejected the operation")]
    Rejected(RuntimeRejectCode),
    /// A frame could not be decoded within the contract bounds.
    #[error("contract frame could not be decoded")]
    Decode,
}

impl ClientError {
    /// Convenience for mapping a decoded rejection payload.
    #[must_use]
    pub fn from_reject(code: RuntimeRejectCode) -> Self {
        Self::Rejected(code)
    }
}

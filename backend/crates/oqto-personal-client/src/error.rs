/// Sanitized failures: never include a URL, credential, QR or response body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientError {
    #[error("invalid personal-host input")]
    InvalidInput,
    #[error("invalid personal-host response")]
    InvalidResponse,
    #[error("personal Device or action denied")]
    Denied,
    #[error("personal host is offline or transport unavailable")]
    Offline,
    #[error("paired host certificate pin mismatch; no fallback")]
    PinMismatch,
    #[error("personal host is synchronizing; request not ready")]
    NotReady,
    #[error("outcome unknown; nothing was automatically retried")]
    OutcomeUnknown,
    #[error("platform storage failed; enrollment was not replayed")]
    Storage,
    #[error("host is already recorded; inspect or forget explicitly")]
    AlreadyRecorded,
    #[error("client generation changed; stale result discarded")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, ClientError>;

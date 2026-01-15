use serde::{Deserialize, Serialize};

/// Auto-attach behavior when opening chat history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionAutoAttachMode {
    Off,
    Attach,
    Resume,
}

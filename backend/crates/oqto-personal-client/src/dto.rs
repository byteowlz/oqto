use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Nonsecret platform-persisted record, never an authorization grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    pub id: String,
    pub label: String,
    pub origin: String,
    pub certificate_sha256: String,
    /// Unique local registration generation, retained across process restarts.
    /// Forget/re-pair MUST use a new value, even for the same host and pin.
    pub registration_id: String,
    /// False includes an uncertain enrollment and is never automatically retried.
    pub paired: bool,
}

impl Host {
    /// Opaque Keychain/Keystore entry name, not credential material. Use for
    /// explicit local Forget after invalidating client admission and CAS-removing
    /// this exact host generation from the platform registry.
    pub fn credential_storage_key(&self) -> String {
        format!("oqto.personal.device.{}.{}", self.id, self.registration_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PairingPreview {
    pub host_id: String,
    pub origin: String,
    pub certificate_sha256: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub workspace: String,
    pub title: Option<String>,
    pub display_title: Option<String>,
    pub activity_at_ms: Option<i64>,
    pub workspace_root: Option<String>,
    #[serde(default)]
    pub live: bool,
    #[serde(default)]
    pub streaming: bool,
    /// Optional facts bound to this Host's fresh catalog, never new admission.
    #[serde(
        default,
        deserialize_with = "oqto_session_relations::deserialize_optional",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub relations: Vec<oqto_session_relations::Relation>,
}

/// A lossless transport projection of canonical history, not a UI timeline or
/// a new history authority. Opaque extensions retain newer canonical fields.
#[derive(Clone, Serialize, Deserialize)]
pub struct CanonicalMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub parts: Vec<CanonicalPart>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CanonicalPart {
    pub id: String,
    pub part_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Includes canonical tool/media fields without interpreting their payloads.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeHead {
    pub source: String,
    pub branch_id: String,
    pub revision: u64,
    /// Explicit null is known-empty. A missing field is refused by page parsing.
    pub native_leaf_id: Option<String>,
    #[serde(default)]
    pub live: bool,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// One server page, oldest-first. Never merge by text, index, count or position.
/// Pass `next_before` back unchanged; a fresh page resets the paging fence.
#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryPage {
    pub messages: Vec<CanonicalMessage>,
    pub has_more: bool,
    pub next_before: Option<String>,
    pub native_graph: NativeHead,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub provider: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ContextUsage {
    pub tokens: Option<u64>,
    pub context_window: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Attachment {
    pub host_id: String,
    pub session_id: String,
    pub id: String,
    pub streaming: bool,
    pub compacting: bool,
    pub control_supported: bool,
    /// A read may clear this latch, but only request_control may establish it.
    pub input_held: bool,
    pub native_leaf_id: Option<String>,
    pub preview_text: Option<String>,
    pub preview_tool: Option<String>,
    pub model: Option<Model>,
    pub thinking_level: Option<String>,
    pub context_usage: Option<ContextUsage>,
    pub queue_pending: Option<bool>,
    pub queue_count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InputReceipt {
    pub session_id: String,
    pub request_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationState {
    Connecting,
    Connected,
    Disconnected,
    Synchronizing,
}

#[derive(Clone, Serialize)]
pub struct Observation {
    pub state: ObservationState,
    pub attachment: Option<Attachment>,
    pub retry_after_ms: u64,
    pub reconnected: bool,
    pub history: Option<HistoryPage>,
    pub history_pending: bool,
}

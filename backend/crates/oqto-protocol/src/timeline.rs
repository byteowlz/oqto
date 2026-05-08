//! Canonical timeline v1 types.
//!
//! The timeline is Oqto's durable, lossless conversation graph. It is intentionally
//! richer than the chat message DTO used by the UI or the compatibility projection
//! written to hstry: turns and branches are first-class, tool calls/results remain
//! distinct parts, harness-native events can be retained byte-for-byte, and context
//! snapshots capture what an agent actually saw before a turn was generated.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use hstry_core::parts::Part;

use crate::messages::{Role, Usage};

/// Current canonical timeline schema version.
pub const TIMELINE_SCHEMA_VERSION: u32 = 1;

/// Top-level lossless timeline document for a single Oqto session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineDocument {
    pub schema_version: u32,
    pub session: TimelineSession,
    pub branches: Vec<TimelineBranch>,
    pub turns: Vec<TimelineTurn>,
}

/// Stable session identities. Oqto control paths use `platform_id`; imports and
/// harness interop use `external_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSession {
    pub session_id: String,
    pub platform_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// A branch in the conversation DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineBranch {
    pub branch_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_branch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_turn_id: Option<String>,
    pub created_at: i64,
}

/// A durable turn node in the timeline DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineTurn {
    pub turn_id: String,
    pub session_id: String,
    pub branch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_turn_id: Option<String>,
    pub turn_version: u64,
    pub role: Role,
    pub status: TurnStatus,
    pub parts: Vec<TimelinePart>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_events: Vec<RawNativeEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<AgentContextSnapshot>,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TimelineSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Streaming,
    Committed,
    Failed,
    Aborted,
}

/// Atomic timeline content. Tool calls and tool results are represented as
/// distinct lifecycle entries, not collapsed into a single assistant text blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimelinePart {
    MessagePart {
        part_id: String,
        seq: u32,
        part: Part,
    },
    ToolCall {
        part_id: String,
        seq: u32,
        tool_call_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        arguments: Option<Value>,
        status: ToolLifecycleStatus,
    },
    ToolResult {
        part_id: String,
        seq: u32,
        tool_call_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        is_error: bool,
        status: ToolLifecycleStatus,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolLifecycleStatus {
    Started,
    Delta,
    Completed,
    Failed,
}

/// Byte-preserving native event retention for deterministic re-projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawNativeEvent {
    pub event_id: String,
    pub seq: u64,
    pub harness: String,
    pub event_type: String,
    pub received_at: i64,
    /// Exact native JSON/event payload when available.
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_sha256: Option<String>,
}

/// Snapshot of the inputs/context an agent had when generating a turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContextSnapshot {
    pub snapshot_id: String,
    pub captured_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_turn_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_file_refs: Vec<ContextFileRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFileRef {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSource {
    pub source_kind: String,
    pub source_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_entry_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_timestamp: Option<i64>,
}

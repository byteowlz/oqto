//! Canonical timeline v1 types.
//!
//! The timeline is Oqto's durable, lossless conversation graph. It is intentionally
//! richer than the chat message DTO used by the UI or the compatibility projection
//! written to hstry: turns and branches are first-class, tool calls/results remain
//! distinct parts, harness-native envelopes can be retained byte-for-byte, and
//! context snapshots capture what an agent actually saw before a turn was generated.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use hstry_core::parts::Part;

use crate::messages::{Role, StopReason, Usage};

/// Current canonical timeline schema version.
pub const TIMELINE_SCHEMA_VERSION: u32 = 1;

/// Extension namespace prefixes reserved by timeline v1.
pub const EXTENSION_NAMESPACES: &[&str] = &["oqto", "pi", "acp", "provider", "hstry"];

/// Top-level lossless timeline document for a single Oqto session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineDocument {
    pub schema_version: u32,
    pub session: TimelineSession,
    pub branches: Vec<TimelineBranch>,
    pub turns: Vec<TimelineTurn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_envelopes: Vec<RawEnvelope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<TimelineArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// Stable session identities. Oqto control paths use `platform_id`; imports and
/// harness interop use `external_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSession {
    pub session_id: String,
    pub platform_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    pub user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    pub messages: Vec<TimelineMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<AgentContextSnapshot>,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TimelineSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Streaming,
    Committed,
    Failed,
    Aborted,
}

/// Content-bearing message within a turn. A turn may contain multiple messages
/// when a native source emits assistant text, tool calls, and tool results as a
/// single logical turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineMessage {
    pub message_id: String,
    pub seq: u32,
    pub role: Role,
    pub parts: Vec<TimelinePart>,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// Atomic timeline content. Tool calls and tool results are represented as
/// distinct lifecycle entries, not collapsed into a single assistant text blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimelinePart {
    Content {
        part_id: String,
        seq: u32,
        part: Part,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        raw_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        extensions: Option<Value>,
    },
    ToolCall {
        part_id: String,
        seq: u32,
        tool_call_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        arguments: Option<Value>,
        status: ToolLifecycleStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        started_at: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        completed_at: Option<i64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        raw_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        extensions: Option<Value>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<ToolError>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        started_at: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        completed_at: Option<i64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        raw_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        extensions: Option<Value>,
    },
    Delegation {
        part_id: String,
        seq: u32,
        target_session_id: String,
        target_turn_id: Option<String>,
        status: ToolLifecycleStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        extensions: Option<Value>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolLifecycleStatus {
    Pending,
    Started,
    Running,
    Delta,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolError {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineArtifact {
    pub artifact_id: String,
    pub kind: String,
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// Byte-preserving native envelope retention for deterministic re-projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEnvelope {
    pub raw_id: String,
    pub source: String,
    pub harness: String,
    pub native_type: String,
    pub source_sequence: u64,
    pub received_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_schema_version: Option<String>,
    /// Exact native JSON/event payload when available.
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// Backward-compatible alias while older code and docs transition from event to
/// envelope terminology.
pub type RawNativeEvent = RawEnvelope;

/// Snapshot of the inputs/context an agent had when generating a turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContextSnapshot {
    pub snapshot_id: String,
    pub captured_at: i64,
    pub platform: String,
    pub harness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readable_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_turn_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_file_refs: Vec<ContextFileRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_v1_round_trips_unknown_extensions() {
        let document = TimelineDocument {
            schema_version: TIMELINE_SCHEMA_VERSION,
            session: TimelineSession {
                session_id: "ses-1".to_string(),
                platform_id: "oqto-1".to_string(),
                external_id: Some("pi-1".to_string()),
                user_id: "user-1".to_string(),
                workspace_id: Some("workspace-1".to_string()),
                created_at: 1,
                updated_at: 2,
                extensions: Some(serde_json::json!({"pi.session_name":"demo"})),
            },
            branches: vec![TimelineBranch {
                branch_id: "branch-main".to_string(),
                session_id: "ses-1".to_string(),
                parent_branch_id: None,
                forked_from_turn_id: None,
                head_turn_id: Some("turn-1".to_string()),
                created_at: 1,
                extensions: None,
            }],
            turns: vec![TimelineTurn {
                turn_id: "turn-1".to_string(),
                session_id: "ses-1".to_string(),
                branch_id: "branch-main".to_string(),
                parent_turn_id: None,
                turn_version: 1,
                role: Role::Assistant,
                status: TurnStatus::Committed,
                stop_reason: Some(StopReason::ToolUse),
                usage: None,
                messages: vec![TimelineMessage {
                    message_id: "msg-1".to_string(),
                    seq: 0,
                    role: Role::Assistant,
                    parts: vec![
                        TimelinePart::ToolCall {
                            part_id: "part-1".to_string(),
                            seq: 0,
                            tool_call_id: "call-1".to_string(),
                            name: "bash".to_string(),
                            title: Some("Run command".to_string()),
                            arguments: Some(serde_json::json!({"command":"true"})),
                            status: ToolLifecycleStatus::Completed,
                            started_at: Some(1),
                            completed_at: Some(2),
                            raw_refs: vec!["raw-1".to_string()],
                            extensions: None,
                        },
                        TimelinePart::ToolResult {
                            part_id: "part-2".to_string(),
                            seq: 1,
                            tool_call_id: "call-1".to_string(),
                            name: Some("bash".to_string()),
                            result: Some(serde_json::json!({"stdout":""})),
                            is_error: false,
                            status: ToolLifecycleStatus::Completed,
                            error: None,
                            started_at: None,
                            completed_at: Some(3),
                            raw_refs: vec!["raw-2".to_string()],
                            extensions: None,
                        },
                    ],
                    created_at: 1,
                    completed_at: Some(3),
                    source_message_id: Some("native-msg-1".to_string()),
                    extensions: None,
                }],
                raw_refs: vec!["raw-1".to_string(), "raw-2".to_string()],
                context_snapshot: Some(AgentContextSnapshot {
                    snapshot_id: "ctx-1".to_string(),
                    captured_at: 1,
                    platform: "oqto".to_string(),
                    harness: "pi".to_string(),
                    workspace_id: Some("workspace-1".to_string()),
                    user_id: "user-1".to_string(),
                    request_id: Some("req-1".to_string()),
                    correlation_id: Some("corr-1".to_string()),
                    sandbox: Some(serde_json::json!({"profile":"development"})),
                    model: Some("model".to_string()),
                    provider: Some("provider".to_string()),
                    readable_id: Some("abc-def".to_string()),
                    context_source: Some("agent-context-env".to_string()),
                    system_prompt_hash: Some("sha256:abc".to_string()),
                    included_turn_ids: vec![],
                    included_file_refs: vec![],
                    usage: None,
                    extensions: None,
                }),
                created_at: 1,
                committed_at: Some(3),
                source: None,
                extensions: None,
            }],
            raw_envelopes: vec![RawEnvelope {
                raw_id: "raw-1".to_string(),
                source: "rpc".to_string(),
                harness: "pi".to_string(),
                native_type: "tool_call".to_string(),
                source_sequence: 1,
                received_at: 1,
                native_schema_version: None,
                payload: serde_json::json!({"future_field":true}),
                payload_sha256: Some("sha256:abc".to_string()),
                extensions: Some(serde_json::json!({"provider.openai":"kept"})),
            }],
            artifacts: vec![],
            extensions: Some(serde_json::json!({"oqto.schema":"timeline-v1"})),
        };

        let json = serde_json::to_string(&document).expect("serialize timeline");
        let parsed: TimelineDocument = serde_json::from_str(&json).expect("deserialize timeline");
        assert_eq!(parsed.schema_version, TIMELINE_SCHEMA_VERSION);
        assert_eq!(parsed.turns[0].messages[0].parts.len(), 2);
        assert_eq!(parsed.raw_envelopes[0].payload["future_field"], true);
        assert_eq!(
            parsed.session.extensions.unwrap()["pi.session_name"],
            "demo"
        );
    }
}

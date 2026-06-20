//! Canonical event stream to timeline assembly.
//!
//! Events are ephemeral UI/state signals. This assembler is the deterministic
//! boundary that turns a replayed stream of canonical events into durable timeline
//! records. A finalized `stream.message_end` remains authoritative for message
//! content; earlier deltas only provide an in-flight preview when no final snapshot
//! arrived.

use anyhow::Result;
use oqto_protocol::events::{Event, EventPayload};
use oqto_protocol::messages::{Message, Role, StopReason};
use oqto_protocol::timeline::{
    RawEnvelope, TimelineMessage, TimelinePart, TimelineSource, TimelineTurn, ToolError,
    ToolLifecycleStatus, TurnStatus,
};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::oqto_log::ids::{MessageIdInput, TurnIdInput, derive_message_id, derive_turn_id};

#[derive(Debug, Clone)]
pub struct EventAssemblyInput<'a> {
    pub session_id: &'a str,
    pub branch_id: &'a str,
    pub parent_turn_id: Option<&'a str>,
    pub next_turn_version: u64,
    pub source_kind: &'a str,
    pub source_session_id: &'a str,
    pub events: &'a [Event],
}

#[derive(Debug, Clone)]
pub struct EventAssemblyOutput {
    pub turn: Option<TimelineTurn>,
    pub raw_envelopes: Vec<RawEnvelope>,
}

#[derive(Debug, Clone)]
struct MessageDraft {
    message_id: String,
    role: Role,
    text: BTreeMap<usize, String>,
    thinking: BTreeMap<usize, String>,
    tool_calls: BTreeMap<usize, ToolCallDraft>,
    final_message: Option<Message>,
    created_at: i64,
    completed_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct ToolCallDraft {
    tool_call_id: String,
    name: String,
    input_delta: String,
    final_input: Option<Value>,
    status: ToolLifecycleStatus,
    started_at: Option<i64>,
    completed_at: Option<i64>,
}

pub fn assemble_events_to_timeline_turn(
    input: &EventAssemblyInput<'_>,
) -> Result<EventAssemblyOutput> {
    let mut raw_envelopes = Vec::new();
    let mut drafts: BTreeMap<String, MessageDraft> = BTreeMap::new();
    let mut tool_results: Vec<(i64, String, String, Value, bool)> = Vec::new();
    let mut status = TurnStatus::Streaming;
    let mut stop_reason: Option<StopReason> = None;
    let mut role = Role::Assistant;

    for (seq, event) in input.events.iter().enumerate() {
        let payload = serde_json::to_value(event)?;
        let raw_id = derive_raw_id(input.session_id, input.source_kind, seq as u64, &payload);
        raw_envelopes.push(RawEnvelope {
            raw_id,
            source: input.source_kind.to_string(),
            harness: "canonical".to_string(),
            native_type: event_type_name(&event.payload).to_string(),
            source_sequence: seq as u64,
            received_at: event.ts,
            native_schema_version: Some("canonical-event-v1".to_string()),
            payload,
            payload_sha256: Some(stable_json_sha256(&serde_json::to_value(event)?)),
            extensions: None,
        });

        match &event.payload {
            EventPayload::StreamMessageStart {
                message_id,
                role: r,
            } => {
                role = parse_role(r);
                drafts.entry(message_id.clone()).or_insert(MessageDraft {
                    message_id: message_id.clone(),
                    role,
                    text: BTreeMap::new(),
                    thinking: BTreeMap::new(),
                    tool_calls: BTreeMap::new(),
                    final_message: None,
                    created_at: event.ts,
                    completed_at: None,
                });
            }
            EventPayload::StreamTextDelta {
                message_id,
                delta,
                content_index,
            } => {
                let draft = ensure_draft(&mut drafts, message_id, Role::Assistant, event.ts);
                draft
                    .text
                    .entry(*content_index)
                    .or_default()
                    .push_str(delta);
            }
            EventPayload::StreamThinkingDelta {
                message_id,
                delta,
                content_index,
            } => {
                let draft = ensure_draft(&mut drafts, message_id, Role::Assistant, event.ts);
                draft
                    .thinking
                    .entry(*content_index)
                    .or_default()
                    .push_str(delta);
            }
            EventPayload::StreamToolCallStart {
                message_id,
                tool_call_id,
                name,
                content_index,
            } => {
                let draft = ensure_draft(&mut drafts, message_id, Role::Assistant, event.ts);
                draft.tool_calls.insert(
                    *content_index,
                    ToolCallDraft {
                        tool_call_id: tool_call_id.clone(),
                        name: name.clone(),
                        input_delta: String::new(),
                        final_input: None,
                        status: ToolLifecycleStatus::Started,
                        started_at: Some(event.ts),
                        completed_at: None,
                    },
                );
            }
            EventPayload::StreamToolCallDelta {
                message_id,
                tool_call_id,
                delta,
                content_index,
            } => {
                let draft = ensure_draft(&mut drafts, message_id, Role::Assistant, event.ts);
                let call = draft
                    .tool_calls
                    .entry(*content_index)
                    .or_insert(ToolCallDraft {
                        tool_call_id: tool_call_id.clone(),
                        name: "tool".to_string(),
                        input_delta: String::new(),
                        final_input: None,
                        status: ToolLifecycleStatus::Running,
                        started_at: None,
                        completed_at: None,
                    });
                call.input_delta.push_str(delta);
                call.status = ToolLifecycleStatus::Running;
            }
            EventPayload::StreamToolCallEnd {
                message_id,
                tool_call,
                content_index,
                ..
            } => {
                let draft = ensure_draft(&mut drafts, message_id, Role::Assistant, event.ts);
                let call = draft
                    .tool_calls
                    .entry(*content_index)
                    .or_insert(ToolCallDraft {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        input_delta: String::new(),
                        final_input: None,
                        status: ToolLifecycleStatus::Completed,
                        started_at: None,
                        completed_at: Some(event.ts),
                    });
                call.tool_call_id = tool_call.id.clone();
                call.name = tool_call.name.clone();
                call.final_input = Some(tool_call.input.clone());
                call.status = ToolLifecycleStatus::Completed;
                call.completed_at = Some(event.ts);
            }
            EventPayload::StreamMessageEnd { message } => {
                role = message.role;
                let draft = ensure_draft(&mut drafts, &message.id, message.role, event.ts);
                draft.final_message = Some(message.clone());
                draft.completed_at = Some(event.ts);
            }
            EventPayload::StreamDone { reason } => {
                stop_reason = Some(*reason);
                status = TurnStatus::Committed;
            }
            EventPayload::ToolEnd {
                tool_call_id,
                name,
                output,
                is_error,
                ..
            } => {
                tool_results.push((
                    event.ts,
                    tool_call_id.clone(),
                    name.clone(),
                    output.clone(),
                    *is_error,
                ));
                if *is_error {
                    status = TurnStatus::Failed;
                    stop_reason = Some(StopReason::Error);
                }
            }
            EventPayload::AgentError {
                error: _,
                recoverable: false,
                ..
            } => {
                status = TurnStatus::Failed;
                stop_reason = Some(StopReason::Error);
            }
            EventPayload::AgentError { .. } => {}
            _ => {}
        }
    }

    if drafts.is_empty() && tool_results.is_empty() {
        return Ok(EventAssemblyOutput {
            turn: None,
            raw_envelopes,
        });
    }
    if matches!(status, TurnStatus::Streaming) {
        status = TurnStatus::Committed;
    }

    let source_hash = stable_json_sha256(&serde_json::to_value(input.events)?);
    let turn_id = derive_turn_id(&TurnIdInput {
        session_id: input.session_id,
        branch_id: input.branch_id,
        parent_turn_id: input.parent_turn_id,
        turn_version: input.next_turn_version as i64,
        role: role_to_str(role),
        source_kind: Some(input.source_kind),
        source_session_id: Some(input.source_session_id),
        source_entry_id: Some("canonical-event-batch"),
        source_hash: Some(&source_hash),
    });

    let mut messages = Vec::new();
    for (seq, draft) in drafts.values().enumerate() {
        messages.push(draft_to_timeline_message(&turn_id, seq as u32, draft)?);
    }
    if !tool_results.is_empty() {
        let seq = messages.len() as u32;
        messages.push(tool_results_message(&turn_id, seq, &tool_results)?);
    }

    let created_at = input.events.first().map(|event| event.ts).unwrap_or(0);
    let committed_at = input.events.last().map(|event| event.ts);
    let turn = TimelineTurn {
        turn_id: turn_id.clone(),
        session_id: input.session_id.to_string(),
        branch_id: input.branch_id.to_string(),
        parent_turn_id: input.parent_turn_id.map(ToString::to_string),
        turn_version: input.next_turn_version,
        role,
        status,
        stop_reason,
        usage: None,
        messages,
        raw_refs: raw_envelopes.iter().map(|raw| raw.raw_id.clone()).collect(),
        context_snapshot: None,
        created_at,
        committed_at,
        source: Some(TimelineSource {
            source_kind: input.source_kind.to_string(),
            source_session_id: input.source_session_id.to_string(),
            source_entry_id: Some("canonical-event-batch".to_string()),
            source_hash: Some(source_hash),
            source_timestamp: committed_at,
        }),
        extensions: None,
    };
    Ok(EventAssemblyOutput {
        turn: Some(turn),
        raw_envelopes,
    })
}

fn ensure_draft<'a>(
    drafts: &'a mut BTreeMap<String, MessageDraft>,
    message_id: &str,
    role: Role,
    ts: i64,
) -> &'a mut MessageDraft {
    drafts
        .entry(message_id.to_string())
        .or_insert(MessageDraft {
            message_id: message_id.to_string(),
            role,
            text: BTreeMap::new(),
            thinking: BTreeMap::new(),
            tool_calls: BTreeMap::new(),
            final_message: None,
            created_at: ts,
            completed_at: None,
        })
}

fn draft_to_timeline_message(
    turn_id: &str,
    seq: u32,
    draft: &MessageDraft,
) -> Result<TimelineMessage> {
    if let Some(message) = &draft.final_message {
        return finalized_message_to_timeline(turn_id, seq, message, draft.completed_at);
    }

    let content = draft.text.values().cloned().collect::<Vec<_>>().join("");
    let message_id = derive_message_id(&MessageIdInput {
        turn_id,
        seq: i64::from(seq),
        kind: "assembled_message",
        role: Some(role_to_str(draft.role)),
        source_message_id: Some(&draft.message_id),
        content: Some(&content),
    });
    let mut parts = Vec::new();
    let mut part_seq = 0u32;
    for text in draft.thinking.values() {
        parts.push(TimelinePart::Content {
            part_id: format!("{message_id}:part:{part_seq}"),
            seq: part_seq,
            part: oqto_protocol::Part::Thinking {
                id: format!("{message_id}:part:{part_seq}"),
                text: text.clone(),
            },
            raw_refs: vec![],
            extensions: None,
        });
        part_seq += 1;
    }
    for text in draft.text.values() {
        parts.push(TimelinePart::Content {
            part_id: format!("{message_id}:part:{part_seq}"),
            seq: part_seq,
            part: oqto_protocol::Part::Text {
                id: format!("{message_id}:part:{part_seq}"),
                text: text.clone(),
                format: None,
            },
            raw_refs: vec![],
            extensions: None,
        });
        part_seq += 1;
    }
    for call in draft.tool_calls.values() {
        parts.push(TimelinePart::ToolCall {
            part_id: format!("{message_id}:part:{part_seq}"),
            seq: part_seq,
            tool_call_id: call.tool_call_id.clone(),
            name: call.name.clone(),
            title: None,
            arguments: call
                .final_input
                .clone()
                .or_else(|| parse_json_maybe(&call.input_delta)),
            status: call.status,
            started_at: call.started_at,
            completed_at: call.completed_at,
            raw_refs: vec![],
            extensions: None,
        });
        part_seq += 1;
    }
    Ok(TimelineMessage {
        message_id,
        seq,
        role: draft.role,
        parts,
        created_at: draft.created_at,
        completed_at: draft.completed_at,
        source_message_id: Some(draft.message_id.clone()),
        extensions: None,
    })
}

fn finalized_message_to_timeline(
    turn_id: &str,
    seq: u32,
    message: &Message,
    completed_at: Option<i64>,
) -> Result<TimelineMessage> {
    let text = message
        .parts
        .iter()
        .filter_map(|part| match part {
            oqto_protocol::Part::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let message_id = derive_message_id(&MessageIdInput {
        turn_id,
        seq: i64::from(seq),
        kind: "final_message",
        role: Some(role_to_str(message.role)),
        source_message_id: Some(&message.id),
        content: Some(&text),
    });
    let parts = message
        .parts
        .iter()
        .enumerate()
        .map(|(idx, part)| TimelinePart::Content {
            part_id: format!("{message_id}:part:{idx}"),
            seq: idx as u32,
            part: part.clone(),
            raw_refs: vec![],
            extensions: None,
        })
        .collect();
    Ok(TimelineMessage {
        message_id,
        seq,
        role: message.role,
        parts,
        created_at: message.created_at,
        completed_at,
        source_message_id: Some(message.id.clone()),
        extensions: message.metadata.clone(),
    })
}

fn tool_results_message(
    turn_id: &str,
    seq: u32,
    results: &[(i64, String, String, Value, bool)],
) -> Result<TimelineMessage> {
    let content = serde_json::to_string(results)?;
    let message_id = derive_message_id(&MessageIdInput {
        turn_id,
        seq: i64::from(seq),
        kind: "tool_results",
        role: Some("tool"),
        source_message_id: Some("tool-events"),
        content: Some(&content),
    });
    let parts = results
        .iter()
        .enumerate()
        .map(
            |(idx, (ts, tool_call_id, name, output, is_error))| TimelinePart::ToolResult {
                part_id: format!("{message_id}:part:{idx}"),
                seq: idx as u32,
                tool_call_id: tool_call_id.clone(),
                name: Some(name.clone()),
                result: Some(output.clone()),
                is_error: *is_error,
                status: if *is_error {
                    ToolLifecycleStatus::Failed
                } else {
                    ToolLifecycleStatus::Completed
                },
                error: is_error.then(|| ToolError {
                    message: output.to_string(),
                    code: None,
                    details: Some(output.clone()),
                }),
                started_at: None,
                completed_at: Some(*ts),
                raw_refs: vec![],
                extensions: None,
            },
        )
        .collect();
    Ok(TimelineMessage {
        message_id,
        seq,
        role: Role::Tool,
        parts,
        created_at: results.first().map(|v| v.0).unwrap_or(0),
        completed_at: results.last().map(|v| v.0),
        source_message_id: Some("tool-events".to_string()),
        extensions: None,
    })
}

fn parse_json_maybe(s: &str) -> Option<Value> {
    if s.trim().is_empty() {
        None
    } else {
        serde_json::from_str(s)
            .ok()
            .or_else(|| Some(Value::String(s.to_string())))
    }
}

fn parse_role(role: &str) -> Role {
    match role {
        "user" => Role::User,
        "system" => Role::System,
        "tool" => Role::Tool,
        _ => Role::Assistant,
    }
}

fn role_to_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

fn event_type_name(payload: &EventPayload) -> &'static str {
    match payload {
        EventPayload::StreamMessageStart { .. } => "stream.message_start",
        EventPayload::StreamTextDelta { .. } => "stream.text_delta",
        EventPayload::StreamThinkingDelta { .. } => "stream.thinking_delta",
        EventPayload::StreamToolCallStart { .. } => "stream.tool_call_start",
        EventPayload::StreamToolCallDelta { .. } => "stream.tool_call_delta",
        EventPayload::StreamToolCallEnd { .. } => "stream.tool_call_end",
        EventPayload::StreamMessageEnd { .. } => "stream.message_end",
        EventPayload::StreamDone { .. } => "stream.done",
        EventPayload::ToolEnd { .. } => "tool.end",
        EventPayload::AgentError { .. } => "agent.error",
        _ => "canonical.event",
    }
}

fn stable_json_sha256(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_string(value).unwrap_or_default().as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn derive_raw_id(session_id: &str, source_kind: &str, seq: u64, payload: &Value) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(
        format!(
            "v1|{session_id}|{source_kind}|{seq}|{}",
            stable_json_sha256(payload)
        )
        .as_bytes(),
    );
    format!("raw:{}", hex::encode(&hasher.finalize()[..16]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oqto_protocol::events::ToolCallInfo;

    fn event(ts: i64, payload: EventPayload) -> Event {
        Event {
            session_id: "ses".to_string(),
            runner_id: "runner".to_string(),
            ts,
            payload,
        }
    }

    fn input(events: &[Event]) -> EventAssemblyInput<'_> {
        EventAssemblyInput {
            session_id: "ses",
            branch_id: "branch:ses:main",
            parent_turn_id: None,
            next_turn_version: 1,
            source_kind: "canonical_event_stream",
            source_session_id: "ses",
            events,
        }
    }

    #[test]
    fn assembles_text_deltas_deterministically() {
        let events = vec![
            event(
                1,
                EventPayload::StreamMessageStart {
                    message_id: "m1".to_string(),
                    role: "assistant".to_string(),
                },
            ),
            event(
                2,
                EventPayload::StreamTextDelta {
                    message_id: "m1".to_string(),
                    delta: "hel".to_string(),
                    content_index: 0,
                },
            ),
            event(
                3,
                EventPayload::StreamTextDelta {
                    message_id: "m1".to_string(),
                    delta: "lo".to_string(),
                    content_index: 0,
                },
            ),
            event(
                4,
                EventPayload::StreamDone {
                    reason: StopReason::Stop,
                },
            ),
        ];
        let a = assemble_events_to_timeline_turn(&input(&events)).unwrap();
        let b = assemble_events_to_timeline_turn(&input(&events)).unwrap();
        assert_eq!(
            serde_json::to_value(&a.turn).unwrap(),
            serde_json::to_value(&b.turn).unwrap()
        );
        let turn = a.turn.unwrap();
        assert_eq!(turn.status, TurnStatus::Committed);
        assert_eq!(turn.raw_refs.len(), 4);
    }

    #[test]
    fn final_message_snapshot_overrides_delta_preview() {
        let message = Message {
            id: "m1".to_string(),
            idx: 0,
            role: Role::Assistant,
            client_id: None,
            sender: None,
            parts: vec![oqto_protocol::Part::Text {
                id: "p-final".to_string(),
                text: "final".to_string(),
                format: None,
            }],
            created_at: 10,
            model: None,
            provider: None,
            stop_reason: None,
            usage: None,
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            metadata: None,
        };
        let events = vec![
            event(
                1,
                EventPayload::StreamMessageStart {
                    message_id: "m1".to_string(),
                    role: "assistant".to_string(),
                },
            ),
            event(
                2,
                EventPayload::StreamTextDelta {
                    message_id: "m1".to_string(),
                    delta: "preview".to_string(),
                    content_index: 0,
                },
            ),
            event(3, EventPayload::StreamMessageEnd { message }),
        ];
        let turn = assemble_events_to_timeline_turn(&input(&events))
            .unwrap()
            .turn
            .unwrap();
        match &turn.messages[0].parts[0] {
            TimelinePart::Content {
                part: oqto_protocol::Part::Text { text, .. },
                ..
            } => assert_eq!(text, "final"),
            _ => panic!("expected final text"),
        }
    }

    #[test]
    fn assembles_tool_lifecycle_and_error_status() {
        let events = vec![
            event(
                1,
                EventPayload::StreamMessageStart {
                    message_id: "m1".to_string(),
                    role: "assistant".to_string(),
                },
            ),
            event(
                2,
                EventPayload::StreamToolCallStart {
                    message_id: "m1".to_string(),
                    tool_call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    content_index: 0,
                },
            ),
            event(
                3,
                EventPayload::StreamToolCallEnd {
                    message_id: "m1".to_string(),
                    tool_call_id: "call-1".to_string(),
                    tool_call: ToolCallInfo {
                        id: "call-1".to_string(),
                        name: "bash".to_string(),
                        input: serde_json::json!({"command":"false"}),
                    },
                    content_index: 0,
                },
            ),
            event(
                4,
                EventPayload::ToolEnd {
                    tool_call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    output: serde_json::json!({"stderr":"no"}),
                    is_error: true,
                    duration_ms: Some(1),
                },
            ),
        ];
        let turn = assemble_events_to_timeline_turn(&input(&events))
            .unwrap()
            .turn
            .unwrap();
        assert_eq!(turn.status, TurnStatus::Failed);
        assert!(matches!(
            turn.messages[0].parts[0],
            TimelinePart::ToolCall { .. }
        ));
        assert!(matches!(
            turn.messages[1].parts[0],
            TimelinePart::ToolResult { is_error: true, .. }
        ));
    }
}

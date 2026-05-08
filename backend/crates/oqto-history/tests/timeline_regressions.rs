use std::collections::HashMap;

use oqto_history::oqto_log::event_assembler::{
    EventAssemblyInput, assemble_events_to_timeline_turn,
};
use oqto_history::oqto_log::native_projector::{
    PiTimelineProjectionInput, project_pi_messages_to_timeline,
};
use oqto_history::oqto_log::projection::{project_active_branch_to_hstry, validate_timeline};
use oqto_pi::AgentMessage;
use oqto_protocol::events::{Event, EventPayload, ToolCallInfo};
use oqto_protocol::messages::{Message, Role, StopReason};
use oqto_protocol::timeline::{TimelineDocument, TimelinePart, TurnStatus};
use serde_json::Value;

fn agent_msg(role: &str, content: Value, ts: u64) -> AgentMessage {
    AgentMessage {
        role: role.to_string(),
        content,
        timestamp: Some(ts),
        tool_call_id: None,
        tool_name: None,
        is_error: None,
        api: None,
        provider: None,
        model: None,
        usage: None,
        stop_reason: None,
        extra: HashMap::new(),
    }
}

fn project(messages: &[AgentMessage]) -> TimelineDocument {
    project_pi_messages_to_timeline(&PiTimelineProjectionInput {
        session_id: "oqto-session",
        platform_id: "platform-session",
        external_id: Some("pi-session"),
        user_id: "user",
        workspace_id: Some("workspace"),
        source_session_id: "pi-session",
        source_kind: "pi_jsonl_golden",
        messages,
    })
    .expect("project pi messages")
}

fn event(ts: i64, payload: EventPayload) -> Event {
    Event {
        session_id: "oqto-session".to_string(),
        runner_id: "runner".to_string(),
        ts,
        payload,
    }
}

#[test]
fn golden_pi_projection_is_replay_stable_and_preserves_raw_refs() {
    let messages = vec![
        agent_msg("user", Value::String("build the tree".to_string()), 1),
        agent_msg(
            "assistant",
            serde_json::json!([
                {"type":"text","text":"I'll inspect it."},
                {"type":"tool_use","id":"call-read","name":"read","input":{"path":"src/lib.rs"}}
            ]),
            2,
        ),
    ];

    let first = project(&messages);
    let second = project(&messages);

    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap()
    );
    validate_timeline(&first).expect("valid timeline");
    assert_eq!(first.turns.len(), 2);
    assert_eq!(first.raw_envelopes.len(), 2);
    assert!(first.turns.iter().all(|turn| !turn.raw_refs.is_empty()));
    assert_eq!(
        first.branches[0].head_turn_id.as_deref(),
        Some(first.turns[1].turn_id.as_str())
    );
}

#[test]
fn golden_tool_call_result_lifecycle_projects_and_links_across_turns() {
    let mut tool_result = agent_msg("tool", serde_json::json!({"stdout":"ok"}), 3);
    tool_result.tool_call_id = Some("call-bash".to_string());
    tool_result.tool_name = Some("bash".to_string());

    let messages = vec![
        agent_msg(
            "assistant",
            serde_json::json!([{ "type":"tool_use", "id":"call-bash", "name":"bash", "input":{"command":"true"} }]),
            2,
        ),
        tool_result,
    ];

    let doc = project(&messages);
    validate_timeline(&doc).expect("tool result links to previous call");
    assert!(matches!(
        doc.turns[0].messages[0].parts[0],
        TimelinePart::ToolCall { ref tool_call_id, .. } if tool_call_id == "call-bash"
    ));
    assert!(matches!(
        doc.turns[1].messages[0].parts[0],
        TimelinePart::ToolResult { ref tool_call_id, is_error: false, .. } if tool_call_id == "call-bash"
    ));
}

#[test]
fn golden_terminal_error_turn_is_explicitly_failed() {
    let mut error = agent_msg("assistant", Value::String("model failed".to_string()), 4);
    error.is_error = Some(true);
    let doc = project(&[error]);
    validate_timeline(&doc).expect("valid error timeline");
    assert_eq!(doc.turns[0].status, TurnStatus::Failed);
}

#[test]
fn invariant_rejects_missing_raw_ref() {
    let messages = vec![agent_msg("user", Value::String("hello".to_string()), 1)];
    let mut doc = project(&messages);
    doc.raw_envelopes.clear();
    let err = validate_timeline(&doc).expect_err("missing raw ref must fail");
    assert!(err.to_string().contains("missing raw envelope"));
}

#[test]
fn golden_hstry_projection_has_deep_links_for_active_branch() {
    let messages = vec![
        agent_msg("user", Value::String("searchable thing".to_string()), 1),
        agent_msg(
            "assistant",
            Value::String("found searchable thing".to_string()),
            2,
        ),
    ];
    let doc = project(&messages);
    let records = project_active_branch_to_hstry(&doc, &doc.branches[0].branch_id)
        .expect("project active branch");
    assert_eq!(records.len(), 2);
    for record in records {
        assert_eq!(record.oqto_session_id, "oqto-session");
        assert!(!record.branch_id.is_empty());
        assert!(!record.turn_id.is_empty());
        assert!(!record.message_id.is_empty());
        assert!(record.metadata["oqto"]["turn_id"].is_string());
    }
}

#[test]
fn golden_event_final_snapshot_wins_over_stream_deltas() {
    let final_message = Message {
        id: "m-final".to_string(),
        idx: 0,
        role: Role::Assistant,
        client_id: None,
        sender: None,
        parts: vec![hstry_core::parts::Part::Text {
            id: "p-final".to_string(),
            text: "authoritative final".to_string(),
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
                message_id: "m-final".to_string(),
                role: "assistant".to_string(),
            },
        ),
        event(
            2,
            EventPayload::StreamTextDelta {
                message_id: "m-final".to_string(),
                delta: "preview".to_string(),
                content_index: 0,
            },
        ),
        event(
            3,
            EventPayload::StreamMessageEnd {
                message: final_message,
            },
        ),
        event(
            4,
            EventPayload::StreamDone {
                reason: StopReason::Stop,
            },
        ),
    ];
    let output = assemble_events_to_timeline_turn(&EventAssemblyInput {
        session_id: "oqto-session",
        branch_id: "branch:oqto-session:main",
        parent_turn_id: None,
        next_turn_version: 1,
        source_kind: "canonical_events",
        source_session_id: "oqto-session",
        events: &events,
    })
    .expect("assemble events");
    let turn = output.turn.expect("turn");
    assert_eq!(turn.status, TurnStatus::Committed);
    match &turn.messages[0].parts[0] {
        TimelinePart::Content {
            part: hstry_core::parts::Part::Text { text, .. },
            ..
        } => {
            assert_eq!(text, "authoritative final");
        }
        other => panic!("unexpected part: {other:?}"),
    }
}

#[test]
fn golden_event_tool_error_marks_turn_failed() {
    let events = vec![
        event(
            1,
            EventPayload::StreamMessageStart {
                message_id: "m-tool".to_string(),
                role: "assistant".to_string(),
            },
        ),
        event(
            2,
            EventPayload::StreamToolCallStart {
                message_id: "m-tool".to_string(),
                tool_call_id: "call-1".to_string(),
                name: "bash".to_string(),
                content_index: 0,
            },
        ),
        event(
            3,
            EventPayload::StreamToolCallEnd {
                message_id: "m-tool".to_string(),
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
                duration_ms: Some(12),
            },
        ),
    ];
    let output = assemble_events_to_timeline_turn(&EventAssemblyInput {
        session_id: "oqto-session",
        branch_id: "branch:oqto-session:main",
        parent_turn_id: None,
        next_turn_version: 1,
        source_kind: "canonical_events",
        source_session_id: "oqto-session",
        events: &events,
    })
    .expect("assemble events");
    let turn = output.turn.expect("turn");
    assert_eq!(turn.status, TurnStatus::Failed);
    assert_eq!(output.raw_envelopes.len(), events.len());
    assert!(matches!(
        turn.messages[1].parts[0],
        TimelinePart::ToolResult { is_error: true, .. }
    ));
}

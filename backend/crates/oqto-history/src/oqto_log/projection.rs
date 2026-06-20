//! Pure projection helpers for canonical timeline views.
//!
//! Storage code persists the lossless graph. These helpers derive narrower views
//! such as the active branch chat timeline and search projection documents.

use anyhow::{Context, Result, bail};
use oqto_protocol::Part;
use oqto_protocol::messages::Role;
use oqto_protocol::timeline::{
    TimelineDocument, TimelineMessage, TimelinePart, TimelineTurn, ToolLifecycleStatus, TurnStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Flattened search projection with pointers back to canonical timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HstryProjectionRecord {
    pub oqto_session_id: String,
    pub platform_id: String,
    pub harness_session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub branch_id: String,
    pub turn_id: String,
    pub message_id: String,
    pub role: Role,
    pub content: String,
    pub parts: Vec<Part>,
    pub raw_refs: Vec<String>,
    pub metadata: Value,
}

/// Return turns on the active branch by walking `head_turn_id -> parent_turn_id`.
pub fn active_branch_turns(
    document: &TimelineDocument,
    branch_id: &str,
) -> Result<Vec<TimelineTurn>> {
    let branch = document
        .branches
        .iter()
        .find(|branch| branch.branch_id == branch_id)
        .with_context(|| format!("timeline branch not found: {branch_id}"))?;

    let turns_by_id: HashMap<&str, &TimelineTurn> = document
        .turns
        .iter()
        .map(|turn| (turn.turn_id.as_str(), turn))
        .collect();

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = branch.head_turn_id.as_deref();
    while let Some(turn_id) = cursor {
        if !seen.insert(turn_id.to_string()) {
            bail!("cycle detected in timeline branch {branch_id} at turn {turn_id}");
        }
        let turn = turns_by_id
            .get(turn_id)
            .with_context(|| format!("branch {branch_id} references missing turn {turn_id}"))?;
        out.push((*turn).clone());
        cursor = turn.parent_turn_id.as_deref();
    }
    out.reverse();
    Ok(out)
}

/// Validate structural invariants required before projection.
pub fn validate_timeline(document: &TimelineDocument) -> Result<()> {
    let mut turn_versions = HashSet::new();
    let turn_ids: HashSet<&str> = document
        .turns
        .iter()
        .map(|turn| turn.turn_id.as_str())
        .collect();
    let branch_ids: HashSet<&str> = document
        .branches
        .iter()
        .map(|branch| branch.branch_id.as_str())
        .collect();
    let raw_ids: HashSet<&str> = document
        .raw_envelopes
        .iter()
        .map(|raw| raw.raw_id.as_str())
        .collect();

    for turn in &document.turns {
        reject_temporary_id(&turn.turn_id)?;
        if !branch_ids.contains(turn.branch_id.as_str()) {
            bail!(
                "turn {} references missing branch {}",
                turn.turn_id,
                turn.branch_id
            );
        }
        if let Some(parent_turn_id) = &turn.parent_turn_id
            && !turn_ids.contains(parent_turn_id.as_str())
        {
            bail!(
                "turn {} references missing parent {}",
                turn.turn_id,
                parent_turn_id
            );
        }
        if !turn_versions.insert((turn.session_id.as_str(), turn.turn_version)) {
            bail!(
                "duplicate turn_version {} in session {}",
                turn.turn_version,
                turn.session_id
            );
        }
        for raw_ref in &turn.raw_refs {
            if !raw_ids.contains(raw_ref.as_str()) {
                bail!(
                    "turn {} references missing raw envelope {}",
                    turn.turn_id,
                    raw_ref
                );
            }
        }
    }
    validate_tool_links(document)?;
    Ok(())
}

/// Project the active branch to flattened search records.
pub fn project_active_branch_to_search(
    document: &TimelineDocument,
    branch_id: &str,
) -> Result<Vec<HstryProjectionRecord>> {
    validate_timeline(document)?;
    let turns = active_branch_turns(document, branch_id)?;
    let mut records = Vec::new();
    for turn in turns.into_iter().filter(|turn| {
        matches!(
            turn.status,
            TurnStatus::Committed | TurnStatus::Failed | TurnStatus::Aborted
        )
    }) {
        for message in &turn.messages {
            let parts = message_parts_for_search(message);
            let content = parts
                .iter()
                .filter_map(part_text_for_search)
                .collect::<Vec<_>>()
                .join("\n");
            records.push(HstryProjectionRecord {
                oqto_session_id: document.session.session_id.clone(),
                platform_id: document.session.platform_id.clone(),
                harness_session_id: document.session.external_id.clone(),
                workspace_id: document.session.workspace_id.clone(),
                branch_id: turn.branch_id.clone(),
                turn_id: turn.turn_id.clone(),
                message_id: message.message_id.clone(),
                role: message.role,
                content,
                parts,
                raw_refs: collect_raw_refs(message),
                metadata: serde_json::json!({
                    "oqto": {
                        "session_id": document.session.session_id,
                        "platform_id": document.session.platform_id,
                        "branch_id": turn.branch_id,
                        "turn_id": turn.turn_id,
                        "turn_version": turn.turn_version,
                        "message_id": message.message_id,
                    },
                    "harness_session_id": document.session.external_id,
                }),
            });
        }
    }
    Ok(records)
}

fn validate_tool_links(document: &TimelineDocument) -> Result<()> {
    let mut calls = HashSet::new();
    for turn in &document.turns {
        for message in &turn.messages {
            for part in &message.parts {
                if let TimelinePart::ToolCall { tool_call_id, .. } = part {
                    calls.insert(tool_call_id.as_str());
                }
            }
        }
    }
    for turn in &document.turns {
        for message in &turn.messages {
            for part in &message.parts {
                if let TimelinePart::ToolResult { tool_call_id, .. } = part
                    && !calls.contains(tool_call_id.as_str())
                {
                    bail!(
                        "tool_result {} in turn {} has no matching tool_call",
                        tool_call_id,
                        turn.turn_id
                    );
                }
            }
        }
    }
    Ok(())
}

fn reject_temporary_id(id: &str) -> Result<()> {
    if id.starts_with("pending-") || id.starts_with("tmp:") {
        bail!("temporary id must not be persisted to timeline: {id}");
    }
    Ok(())
}

fn message_parts_for_search(message: &TimelineMessage) -> Vec<Part> {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            TimelinePart::Content { part, .. } => Some(part.clone()),
            TimelinePart::ToolCall {
                tool_call_id,
                name,
                arguments,
                status,
                ..
            } => {
                let mut part =
                    Part::tool_call(tool_call_id.clone(), name.clone(), arguments.clone());
                if let Part::ToolCall {
                    status: part_status,
                    ..
                } = &mut part
                {
                    *part_status = timeline_tool_status_to_part(*status);
                }
                Some(part)
            }
            TimelinePart::ToolResult {
                tool_call_id,
                result,
                is_error,
                ..
            } => Some(Part::tool_result(
                tool_call_id.clone(),
                result.clone(),
                *is_error,
            )),
            TimelinePart::Delegation { .. } => None,
        })
        .collect()
}

fn timeline_tool_status_to_part(status: ToolLifecycleStatus) -> oqto_protocol::ToolStatus {
    match status {
        ToolLifecycleStatus::Pending => oqto_protocol::ToolStatus::Pending,
        ToolLifecycleStatus::Started
        | ToolLifecycleStatus::Running
        | ToolLifecycleStatus::Delta => oqto_protocol::ToolStatus::Running,
        ToolLifecycleStatus::Completed => oqto_protocol::ToolStatus::Success,
        ToolLifecycleStatus::Failed | ToolLifecycleStatus::Cancelled => {
            oqto_protocol::ToolStatus::Error
        }
    }
}

fn part_text_for_search(part: &Part) -> Option<String> {
    match part {
        Part::Text { text, .. } => Some(text.clone()),
        Part::Thinking { text, .. } => Some(text.clone()),
        Part::ToolResult { output, .. } => output.as_ref().map(ToString::to_string),
        _ => None,
    }
}

fn collect_raw_refs(message: &TimelineMessage) -> Vec<String> {
    let mut refs = Vec::new();
    for part in &message.parts {
        match part {
            TimelinePart::Content { raw_refs, .. }
            | TimelinePart::ToolCall { raw_refs, .. }
            | TimelinePart::ToolResult { raw_refs, .. } => refs.extend(raw_refs.clone()),
            TimelinePart::Delegation { .. } => {}
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use oqto_protocol::timeline::{
        RawEnvelope, TimelineBranch, TimelineDocument, TimelineMessage, TimelineSession,
    };

    fn base_document(turns: Vec<TimelineTurn>, head_turn_id: &str) -> TimelineDocument {
        TimelineDocument {
            schema_version: 1,
            session: TimelineSession {
                session_id: "ses".to_string(),
                platform_id: "platform".to_string(),
                external_id: Some("pi".to_string()),
                user_id: "user".to_string(),
                workspace_id: Some("workspace".to_string()),
                created_at: 1,
                updated_at: 1,
                extensions: None,
            },
            branches: vec![TimelineBranch {
                branch_id: "main".to_string(),
                session_id: "ses".to_string(),
                parent_branch_id: None,
                forked_from_turn_id: None,
                head_turn_id: Some(head_turn_id.to_string()),
                created_at: 1,
                extensions: None,
            }],
            turns,
            raw_envelopes: vec![RawEnvelope {
                raw_id: "raw-1".to_string(),
                source: "jsonl".to_string(),
                harness: "pi".to_string(),
                native_type: "message".to_string(),
                source_sequence: 1,
                received_at: 1,
                native_schema_version: None,
                payload: serde_json::json!({"role":"assistant"}),
                payload_sha256: Some("sha256:1".to_string()),
                extensions: None,
            }],
            artifacts: vec![],
            extensions: None,
        }
    }

    fn text_turn(id: &str, parent: Option<&str>, version: u64, text: &str) -> TimelineTurn {
        TimelineTurn {
            turn_id: id.to_string(),
            session_id: "ses".to_string(),
            branch_id: "main".to_string(),
            parent_turn_id: parent.map(str::to_string),
            turn_version: version,
            role: Role::Assistant,
            status: TurnStatus::Committed,
            stop_reason: None,
            usage: None,
            messages: vec![TimelineMessage {
                message_id: format!("msg-{id}"),
                seq: 0,
                role: Role::Assistant,
                parts: vec![TimelinePart::Content {
                    part_id: format!("part-{id}"),
                    seq: 0,
                    part: Part::text(text),
                    raw_refs: vec!["raw-1".to_string()],
                    extensions: None,
                }],
                created_at: 1,
                completed_at: Some(1),
                source_message_id: None,
                extensions: None,
            }],
            raw_refs: vec!["raw-1".to_string()],
            context_snapshot: None,
            created_at: 1,
            committed_at: Some(1),
            source: None,
            extensions: None,
        }
    }

    #[test]
    fn active_branch_walks_head_to_root_in_order() {
        let document = base_document(
            vec![
                text_turn("turn-1", None, 1, "one"),
                text_turn("turn-2", Some("turn-1"), 2, "two"),
            ],
            "turn-2",
        );
        let turns = active_branch_turns(&document, "main").expect("active branch");
        assert_eq!(
            turns
                .iter()
                .map(|turn| turn.turn_id.as_str())
                .collect::<Vec<_>>(),
            vec!["turn-1", "turn-2"]
        );
    }

    #[test]
    fn search_projection_carries_deep_links() {
        let document = base_document(vec![text_turn("turn-1", None, 1, "hello")], "turn-1");
        let records = project_active_branch_to_search(&document, "main").expect("project");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].content, "hello");
        assert_eq!(records[0].metadata["oqto"]["turn_id"], "turn-1");
        assert_eq!(records[0].raw_refs, vec!["raw-1"]);
    }

    #[test]
    fn invariant_rejects_temporary_ids() {
        let mut turn = text_turn("pending-1", None, 1, "bad");
        turn.turn_id = "pending-1".to_string();
        let document = base_document(vec![turn], "pending-1");
        let err = validate_timeline(&document).expect_err("temporary id rejected");
        assert!(err.to_string().contains("temporary id"));
    }
}

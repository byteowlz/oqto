use sha2::{Digest, Sha256};

/// Input used to derive stable canonical turn IDs.
#[derive(Debug, Clone)]
pub struct TurnIdInput<'a> {
    pub session_id: &'a str,
    pub branch_id: &'a str,
    pub parent_turn_id: Option<&'a str>,
    pub turn_version: i64,
    pub role: &'a str,
    pub source_kind: Option<&'a str>,
    pub source_session_id: Option<&'a str>,
    pub source_entry_id: Option<&'a str>,
    pub source_hash: Option<&'a str>,
}

/// Input used to derive stable canonical message IDs.
#[derive(Debug, Clone)]
pub struct MessageIdInput<'a> {
    pub turn_id: &'a str,
    pub seq: i64,
    pub kind: &'a str,
    pub role: Option<&'a str>,
    pub source_message_id: Option<&'a str>,
    pub content: Option<&'a str>,
}

fn normalize(opt: Option<&str>) -> &str {
    opt.unwrap_or("")
}

fn id(prefix: &str, material: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(material.as_bytes());
    let digest = hasher.finalize();
    format!("{}:{}", prefix, hex::encode(&digest[..16]))
}

/// Derive a replay-stable canonical turn ID.
pub fn derive_turn_id(input: &TurnIdInput<'_>) -> String {
    let material = format!(
        "v1|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        input.session_id,
        input.branch_id,
        normalize(input.parent_turn_id),
        input.turn_version,
        input.role,
        normalize(input.source_kind),
        normalize(input.source_session_id),
        normalize(input.source_entry_id),
        normalize(input.source_hash),
    );
    id("turn", &material)
}

/// Derive a replay-stable canonical message ID.
pub fn derive_message_id(input: &MessageIdInput<'_>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalize(input.content).as_bytes());
    let content_hash = hex::encode(&hasher.finalize()[..8]);

    let material = format!(
        "v1|{}|{}|{}|{}|{}|{}",
        input.turn_id,
        input.seq,
        input.kind,
        normalize(input.role),
        normalize(input.source_message_id),
        content_hash,
    );
    id("msg", &material)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_id_is_deterministic() {
        let input = TurnIdInput {
            session_id: "sess-1",
            branch_id: "main",
            parent_turn_id: Some("turn:abc"),
            turn_version: 42,
            role: "assistant",
            source_kind: Some("pi_jsonl"),
            source_session_id: Some("pi-session-1"),
            source_entry_id: Some("entry-42"),
            source_hash: Some("sha256:deadbeef"),
        };

        let a = derive_turn_id(&input);
        let b = derive_turn_id(&input);
        assert_eq!(a, b);
        assert!(a.starts_with("turn:"));
    }

    #[test]
    fn message_id_changes_with_sequence() {
        let base = MessageIdInput {
            turn_id: "turn:x",
            seq: 1,
            kind: "text",
            role: Some("assistant"),
            source_message_id: Some("m1"),
            content: Some("hello"),
        };

        let id1 = derive_message_id(&base);
        let id2 = derive_message_id(&MessageIdInput { seq: 2, ..base });

        assert_ne!(id1, id2);
        assert!(id1.starts_with("msg:"));
    }
}

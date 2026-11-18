//! Generative UI parts (revision 2.2): `oqto.ui` entries carrying one state of an OpenUI part
//! in the Oqto profile (ADR-0074). Written by the host only; rendering one grants nothing.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::frames::FrameError;

/// The entry kind of a part.
pub const UI_KIND: &str = "oqto.ui";
/// The envelope schema id.
pub const ENVELOPE: &str = "oqto.openui.part.v1";
/// The profile id.
pub const PROFILE: &str = "oqto";
/// Largest body in UTF-8 bytes (`operations.json` `limits.max_ui_body_bytes`).
pub const MAX_BODY_BYTES: usize = 32_768;
/// Largest fallback in characters (`limits.max_ui_fallback_chars`).
pub const MAX_FALLBACK_CHARS: usize = 4000;

/// The profile a part was written for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Always `oqto`.
    pub id: String,
    /// `major.minor.patch`.
    pub version: String,
}

/// What an answer must look like, when the part waits for one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Answer {
    /// JSON Schema the host validates answers against.
    pub schema: serde_json::Map<String, Value>,
    /// Seconds until the part expires.
    pub timeout_seconds: u32,
}

/// The `oqto.openui.part.v1` envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Envelope {
    /// `oqto.openui.part.v1`.
    pub schema: String,
    /// Stable id; states reconcile by it.
    pub part_id: String,
    /// Profile id and version.
    pub profile: Profile,
    /// OpenUI Lang program.
    pub body: String,
    /// Required plain text.
    pub fallback: String,
    /// Present when the part waits for an answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<Answer>,
}

/// One state of a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartState {
    /// On screen; answerable when `answer` is present.
    Shown,
    /// Final: answered.
    Answered,
    /// Final: timed out.
    Expired,
    /// Final: taken back by the host.
    Withdrawn,
}

/// Where a part came from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    /// Tool callId.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    /// Entry id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<u64>,
}

/// `data` of an `oqto.ui` entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiPart {
    /// The envelope.
    pub part: Envelope,
    /// This state.
    pub state: PartState,
    /// Origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}

fn invalid(why: &str) -> FrameError {
    FrameError(format!("ui part: {why}"))
}

fn version_ok(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

impl UiPart {
    /// Parse and check `data` against `frames.schema.json` `definitions.ui_part`.
    pub fn parse(data: &Value) -> Result<Self, FrameError> {
        let part: Self =
            serde_json::from_value(data.clone()).map_err(|e| invalid(&e.to_string()))?;
        let envelope = &part.part;
        let id_ok = (8..=64).contains(&envelope.part_id.len())
            && envelope
                .part_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        let fallback_chars = envelope.fallback.chars().count();
        let checks = [
            (envelope.schema == ENVELOPE, "schema"),
            (id_ok, "partId"),
            (envelope.profile.id == PROFILE, "profile.id"),
            (version_ok(&envelope.profile.version), "profile.version"),
            (
                !envelope.body.is_empty() && envelope.body.chars().count() <= MAX_BODY_BYTES,
                "body",
            ),
            (
                (1..=MAX_FALLBACK_CHARS).contains(&fallback_chars),
                "fallback",
            ),
            (
                envelope
                    .answer
                    .as_ref()
                    .is_none_or(|a| (1..=86_400).contains(&a.timeout_seconds)),
                "answer.timeoutSeconds",
            ),
            (
                part.origin
                    .as_ref()
                    .and_then(|o| o.call.as_ref())
                    .is_none_or(|c| (1..=128).contains(&c.chars().count())),
                "origin.call",
            ),
        ];
        match checks.iter().find(|(ok, _)| !ok) {
            Some((_, what)) => Err(invalid(what)),
            None => Ok(part),
        }
    }

    /// Whether the body is within the normative byte limit (the schema bounds characters).
    pub fn body_within_limit(&self) -> bool {
        self.part.body.len() <= MAX_BODY_BYTES
    }

    /// Whether this state waits for an answer.
    pub fn waits(&self) -> bool {
        self.state == PartState::Shown && self.part.answer.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data() -> Value {
        json!({"part": {"schema": "oqto.openui.part.v1", "partId": "part_profile_1", "profile": {"id": "oqto", "version": "0.2.2"},
            "body": "root = Stack([])", "fallback": "Pick", "answer": {"schema": {"type": "object"}, "timeoutSeconds": 600}},
            "state": "shown", "origin": {"call": "call_1"}})
    }

    #[test]
    fn parses_and_waits() -> Result<(), FrameError> {
        let part = UiPart::parse(&data())?;
        assert!(part.waits());
        assert!(part.body_within_limit());
        Ok(())
    }

    #[test]
    fn refuses_what_the_schema_refuses() {
        let mut no_fallback = data();
        if let Some(p) = no_fallback.pointer_mut("/part") {
            p.as_object_mut().map(|o| o.remove("fallback"));
        }
        assert!(UiPart::parse(&no_fallback).is_err());
        let mut state = data();
        state["state"] = json!("approved");
        assert!(UiPart::parse(&state).is_err());
        let mut short = data();
        short["part"]["partId"] = json!("short");
        assert!(UiPart::parse(&short).is_err());
        let mut extra = data();
        extra["part"]["x"] = json!(1);
        assert!(UiPart::parse(&extra).is_err());
    }
}

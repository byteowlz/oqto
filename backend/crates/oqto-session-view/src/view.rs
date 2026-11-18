//! The value one attachment keeps current (upstream's `ConversationView`), with readers for
//! the stable subset of `operations.json`. Everything else is upstream-owned and open.

use serde_json::Value;
use thiserror::Error;

use crate::frames::Entry;
use crate::ops::{self, OpRefusal};

/// Why a view frame could not be applied; the client must drop the attachment and attach
/// again.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ViewError {
    /// `seq` did not follow the previous frame.
    #[error("seq {got} does not follow {held}")]
    Sequence {
        /// The last applied seq.
        held: u64,
        /// The frame's seq.
        got: u64,
    },
    /// Frame 1 did not start with `r`.
    #[error("first view frame is not a base")]
    NoBase,
    /// An op was refused.
    #[error("op refused: {0}")]
    Refused(#[from] OpRefusal),
    /// The result is not a conversation view.
    #[error("not a conversation view: {0}")]
    Shape(&'static str),
}

/// One attachment's view: the exact wire value plus its sequence number.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct View {
    value: Value,
    seq: u64,
}

impl View {
    /// An empty view, waiting for its base.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one `view` frame all or nothing. On error nothing changed.
    pub fn apply(&mut self, seq: u64, frame_ops: &[Value]) -> Result<(), ViewError> {
        if seq != self.seq + 1 {
            return Err(ViewError::Sequence {
                held: self.seq,
                got: seq,
            });
        }
        if seq == 1
            && frame_ops
                .first()
                .and_then(|op| op.get(0))
                .and_then(Value::as_str)
                != Some("r")
        {
            return Err(ViewError::NoBase);
        }
        let mut next = self.value.clone();
        ops::apply(&mut next, frame_ops)?;
        check_shape(&next)?;
        self.value = next;
        self.seq = seq;
        Ok(())
    }

    /// The last applied seq; 0 before the base.
    pub const fn seq(&self) -> u64 {
        self.seq
    }

    /// Whether the base has arrived.
    pub const fn ready(&self) -> bool {
        self.seq > 0
    }

    /// The exact wire value (for hashes and debugging).
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// The window's entries in order.
    pub fn entries(&self) -> Vec<Entry> {
        self.value
            .get("entries")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|e| Entry::new(e.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The id just before the window; absent when the window starts the transcript.
    pub fn before(&self) -> Option<u64> {
        self.value.get("before").and_then(Value::as_u64)
    }

    /// A document by name.
    pub fn doc(&self, name: &str) -> Option<&Value> {
        self.value.get("docs").and_then(|docs| docs.get(name))
    }

    /// `docs["pi.live"].run`: present exactly while the Session works.
    pub fn run(&self) -> Option<&Value> {
        self.doc("pi.live").and_then(|live| live.get("run"))
    }

    /// Whether a run is working (never guessed from timing).
    pub fn working(&self) -> bool {
        self.run().is_some()
    }

    /// `docs["pi.live"].generation.message`: the message being streamed.
    pub fn generation_message(&self) -> Option<&Value> {
        self.doc("pi.live")
            .and_then(|live| live.get("generation"))
            .and_then(|generation| generation.get("message"))
    }

    /// `docs["pi.live"].tools[]`: running tools.
    pub fn live_tools(&self) -> Vec<LiveTool> {
        self.doc("pi.live")
            .and_then(|live| live.get("tools"))
            .and_then(Value::as_array)
            .map(|tools| tools.iter().map(LiveTool::read).collect())
            .unwrap_or_default()
    }

    /// `docs["pi.live"].runFailure`: reason and detail of the last failed run.
    pub fn run_failure(&self) -> Option<(String, Option<String>)> {
        let failure = self.doc("pi.live")?.get("runFailure")?;
        let reason = failure.get("reason")?.as_str()?.to_owned();
        let detail = failure.get("detail").map(|d| match d {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        });
        Some((reason, detail))
    }

    /// `docs["pi.inbox"].items[]`: queued inputs as (id, mode).
    pub fn inbox(&self) -> Vec<(String, String)> {
        self.doc("pi.inbox")
            .and_then(|inbox| inbox.get("items"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        let id = item.get("id").map(scalar).unwrap_or_default();
                        let mode = item.get("mode").map(scalar).unwrap_or_default();
                        (id, mode)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `docs["pi.agent"].model` as `provider/modelId`.
    pub fn model(&self) -> Option<String> {
        let model = self.doc("pi.agent")?.get("model")?;
        let provider = model.get("provider")?.as_str()?;
        let id = model.get("modelId")?.as_str()?;
        Some(format!("{provider}/{id}"))
    }

    /// `docs["pi.agent"].thinkingLevel`, when the host shows it (not yet in the stable subset).
    pub fn thinking_level(&self) -> Option<String> {
        self.doc("pi.agent")?
            .get("thinkingLevel")?
            .as_str()
            .map(str::to_owned)
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// One running tool from `docs["pi.live"].tools[]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveTool {
    /// `callId`.
    pub call_id: String,
    /// Tool name.
    pub name: String,
    /// Upstream status text.
    pub status: String,
    /// Partial output so far.
    pub output: String,
}

impl LiveTool {
    fn read(value: &Value) -> Self {
        let text = |key: &str| value.get(key).map(scalar).unwrap_or_default();
        let output = match value.get("output") {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Array(parts)) => parts.iter().map(scalar).collect(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        };
        Self {
            call_id: text("callId"),
            name: text("name"),
            status: text("status"),
            output,
        }
    }
}

/// The base and every later value must stay a conversation view (schema
/// `conversation_view`).
fn check_shape(value: &Value) -> Result<(), ViewError> {
    let object = value.as_object().ok_or(ViewError::Shape("not an object"))?;
    let conversation = object
        .get("conversation")
        .ok_or(ViewError::Shape("conversation"))?;
    if conversation.get("id").and_then(Value::as_u64).is_none() {
        return Err(ViewError::Shape("conversation.id"));
    }
    let entries = object
        .get("entries")
        .and_then(Value::as_array)
        .ok_or(ViewError::Shape("entries"))?;
    if entries
        .iter()
        .any(|entry| Entry::new(entry.clone()).is_err())
    {
        return Err(ViewError::Shape("entry"));
    }
    if object
        .get("before")
        .is_some_and(|before| before.as_u64().is_none())
    {
        return Err(ViewError::Shape("before"));
    }
    let docs = object
        .get("docs")
        .and_then(Value::as_object)
        .ok_or(ViewError::Shape("docs"))?;
    if docs.values().any(|doc| !doc.is_object()) {
        return Err(ViewError::Shape("docs value"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn base_then_streamed_text() -> Result<(), ViewError> {
        let mut view = View::new();
        view.apply(
            1,
            &[json!(["r", {"conversation": {"id": 1}, "entries": [], "docs": {"pi.live": {"run": {}, "generation": {"message": {"content": [{"type": "text", "text": "He"}]}}}}}])],
        )?;
        view.apply(
            2,
            &[json!([
                "a",
                [
                    "docs",
                    "pi.live",
                    "generation",
                    "message",
                    "content",
                    0,
                    "text"
                ],
                "llo"
            ])],
        )?;
        assert!(view.working());
        assert_eq!(
            view.generation_message()
                .and_then(|m| m.pointer("/content/0/text")),
            Some(&json!("Hello"))
        );
        Ok(())
    }

    #[test]
    fn refuses_gaps_and_missing_base() {
        let mut view = View::new();
        assert_eq!(
            view.apply(2, &[]),
            Err(ViewError::Sequence { held: 0, got: 2 })
        );
        assert_eq!(
            view.apply(1, &[json!(["s", ["x"], 1])]),
            Err(ViewError::NoBase)
        );
        assert!(matches!(
            view.apply(1, &[json!(["r", {"entries": []}])]),
            Err(ViewError::Shape(_))
        ));
        assert!(!view.ready());
    }
}

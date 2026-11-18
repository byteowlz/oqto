//! Pure projection from the client's state to what the transcript shows (like byoagnt's
//! `model.ts`): user messages with receipt status, streamed answers, tool cards with live
//! output and diffs, notes, parts, queued inputs, earlier and missing rows. No terminal code,
//! so it is tested directly.

use std::sync::Arc;

use base64::Engine;
use oqto_session_client::{Item, Request, SessionClient};
use oqto_session_view::frames::{InputKind, ReceiptStatus};
use oqto_session_view::{Entry, Receipt};
use serde_json::Value;

/// How a tool call stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    /// Called, not started.
    Pending,
    /// Running.
    Running,
    /// Finished.
    Done,
    /// Finished with an error.
    Failed,
    /// Stopped by an abort.
    Stopped,
}

/// One line of a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    /// Removed.
    Removed(String),
    /// Added.
    Added(String),
}

/// A tool call with its result or live output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCard {
    /// `callId`.
    pub call_id: String,
    /// Tool name.
    pub name: String,
    /// One line about the arguments (command, path, ...).
    pub summary: String,
    /// Status.
    pub status: ToolStatus,
    /// Output so far.
    pub output: String,
    /// For `edit`-like tools.
    pub diff: Vec<DiffLine>,
    /// Duration when finished.
    pub duration_ms: Option<u64>,
}

/// An image block from an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Stable key for the terminal's image id.
    pub key: String,
    /// MIME type.
    pub mime: String,
    /// Decoded bytes.
    pub bytes: Arc<Vec<u8>>,
}

/// One row of the transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// Older entries may exist.
    Earlier {
        /// A page is loading.
        loading: bool,
    },
    /// Entries are missing between two known ranges.
    Gap {
        /// First missing id that will be read.
        min: u64,
        /// Last.
        max: u64,
        /// A fill is running.
        loading: bool,
    },
    /// What the user sent.
    User {
        /// Text.
        text: String,
        /// Receipt status in words.
        status: String,
        /// Shown at once, not yet in the transcript.
        pending: bool,
    },
    /// What the agent answered.
    Assistant {
        /// Markdown text.
        text: String,
        /// Thinking text.
        thinking: String,
        /// Still being written.
        streaming: bool,
    },
    /// A tool call.
    Tool(ToolCard),
    /// An image.
    Image(Image),
    /// A passive note.
    Note {
        /// Entry kind.
        kind: String,
        /// Its text.
        text: String,
        /// Not yet written.
        pending: bool,
    },
    /// A generative UI part (`oqto.ui`), still as data.
    Part {
        /// The entry's `data`.
        data: Value,
    },
    /// An input waiting in the inbox.
    Queued {
        /// Its text.
        text: String,
        /// `steer` or `follow-up`.
        mode: String,
    },
    /// How the last run failed.
    Failure {
        /// Upstream reason.
        reason: String,
        /// Detail.
        detail: String,
    },
    /// Anything else: one muted line.
    Other {
        /// What it is.
        label: String,
    },
}

fn text_of(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn images_of(key: &str, content: Option<&Value>) -> Vec<Image> {
    let Some(Value::Array(blocks)) = content else {
        return Vec::new();
    };
    blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.get("type").and_then(Value::as_str) == Some("image"))
        .filter_map(|(index, b)| {
            let data = b.get("data").and_then(Value::as_str)?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .ok()?;
            let mime = b
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or("image")
                .to_owned();
            Some(Image {
                key: format!("{key}:{index}"),
                mime,
                bytes: Arc::new(bytes),
            })
        })
        .collect()
}

/// Receipt status in plain words.
pub fn status_text(receipt: Option<&Receipt>, pending: bool) -> String {
    match receipt {
        None if pending => "sending".to_owned(),
        None => String::new(),
        Some(r) => match r.status {
            ReceiptStatus::Queued => "queued".to_owned(),
            ReceiptStatus::Placed => "working".to_owned(),
            ReceiptStatus::Done => "answered".to_owned(),
            ReceiptStatus::Unanswered => {
                format!("stopped: {}", r.reason.as_deref().unwrap_or("unanswered"))
            }
        },
    }
}

fn first_message(entry: &Entry) -> Option<&Value> {
    entry.model().and_then(|m| m.get(0))
}

fn arg(args: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| args.get(*k).and_then(Value::as_str).map(str::to_owned))
}

/// One line about a tool call's arguments, and a diff for edit-like tools.
fn describe_call(name: &str, args: &Value) -> (String, Vec<DiffLine>) {
    let path = arg(args, &["path", "file_path", "filePath"]);
    match name {
        "bash" | "shell" => (
            arg(args, &["command", "cmd"]).unwrap_or_default(),
            Vec::new(),
        ),
        "edit" => {
            let old = arg(args, &["oldText", "old_string", "old"]).unwrap_or_default();
            let new = arg(args, &["newText", "new_string", "new"]).unwrap_or_default();
            let mut diff: Vec<DiffLine> = old
                .lines()
                .map(|l| DiffLine::Removed(l.to_owned()))
                .collect();
            diff.extend(new.lines().map(|l| DiffLine::Added(l.to_owned())));
            let counts = format!("+{} -{}", new.lines().count(), old.lines().count());
            (format!("{}  {counts}", path.unwrap_or_default()), diff)
        }
        "write" => {
            let size = arg(args, &["content"]).map_or(0, |c| c.len());
            (
                format!("{}  {size} bytes", path.unwrap_or_default()),
                Vec::new(),
            )
        }
        _ => {
            let summary = path.unwrap_or_else(|| match args {
                Value::Object(map) if map.is_empty() => String::new(),
                Value::Null => String::new(),
                other => other.to_string(),
            });
            (summary, Vec::new())
        }
    }
}

/// Builds the blocks in order.
#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
}

impl Builder {
    fn card(&mut self, call_id: &str) -> Option<&mut ToolCard> {
        self.blocks.iter_mut().rev().find_map(|b| match b {
            Block::Tool(card) if card.call_id == call_id => Some(card),
            _ => None,
        })
    }

    fn assistant(&mut self, message: &Value, streaming: bool) {
        let mut text = String::new();
        let mut thinking = String::new();
        let mut calls = Vec::new();
        for block in message
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => text.push_str(
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                Some("thinking") => thinking.push_str(
                    block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                Some("toolCall") => calls.push(block.clone()),
                _ => {}
            }
        }
        if let Some(Value::String(plain)) = message.get("content") {
            text.push_str(plain);
        }
        if !text.is_empty() || !thinking.is_empty() || (streaming && calls.is_empty()) {
            self.blocks.push(Block::Assistant {
                text,
                thinking,
                streaming,
            });
        }
        for call in calls {
            let name = call
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_owned();
            let call_id = call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let (summary, diff) =
                describe_call(&name, call.get("arguments").unwrap_or(&Value::Null));
            self.blocks.push(Block::Tool(ToolCard {
                call_id,
                name,
                summary,
                status: ToolStatus::Pending,
                output: String::new(),
                diff,
                duration_ms: None,
            }));
        }
        if message.get("stopReason").and_then(Value::as_str) == Some("error") {
            let detail = message
                .get("errorMessage")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            self.blocks.push(Block::Failure {
                reason: "model_error".to_owned(),
                detail,
            });
        }
    }

    fn tool_result(&mut self, entry: &Entry, message: &Value) {
        let call_id = message
            .get("toolCallId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let output = text_of(message.get("content"));
        let error = message
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let status = match (error, output.contains("aborted")) {
            (false, _) => ToolStatus::Done,
            (true, true) => ToolStatus::Stopped,
            (true, false) => ToolStatus::Failed,
        };
        let output = output
            .replace("<harness>", "")
            .replace("</harness>", "")
            .trim()
            .to_owned();
        let duration_ms = message.get("durationMs").and_then(Value::as_u64);
        if let Some(card) = self.card(call_id) {
            card.status = status;
            card.output = output;
            card.duration_ms = duration_ms;
        } else {
            let name = message
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_owned();
            self.blocks.push(Block::Tool(ToolCard {
                call_id: call_id.to_owned(),
                name,
                summary: String::new(),
                status,
                output,
                diff: Vec::new(),
                duration_ms,
            }));
        }
        for image in images_of(&entry.id().to_string(), message.get("content")) {
            self.blocks.push(Block::Image(image));
        }
    }

    fn entry(&mut self, entry: &Entry, client: &SessionClient) {
        let message = first_message(entry);
        match entry.kind() {
            "pi.user" => {
                let text = text_of(message.and_then(|m| m.get("content")));
                let status = status_text(client.receipt_for_entry(entry.id()), false);
                self.blocks.push(Block::User {
                    text,
                    status,
                    pending: false,
                });
                for image in images_of(
                    &entry.id().to_string(),
                    message.and_then(|m| m.get("content")),
                ) {
                    self.blocks.push(Block::Image(image));
                }
            }
            "pi.assistant" => {
                if let Some(message) = message {
                    self.assistant(message, false);
                }
            }
            "pi.tool-result" => {
                if let Some(message) = message {
                    self.tool_result(entry, message);
                }
            }
            "pi.system" => {
                let text = text_of(message.and_then(|m| m.get("content")));
                if !text.trim().is_empty() {
                    self.blocks.push(Block::Other {
                        label: "system message".to_owned(),
                    });
                }
            }
            "oqto.ui" => {
                // A later state of a part replaces the earlier one in place; reconciled only by partId.
                let data = entry.data().cloned().unwrap_or(Value::Null);
                let id = crate::parts::part_id(&data).map(str::to_owned);
                let earlier = self.blocks.iter_mut().find(|b| match b {
                    Block::Part { data: old } => {
                        id.is_some() && crate::parts::part_id(old) == id.as_deref()
                    }
                    _ => false,
                });
                match earlier {
                    Some(Block::Part { data: old }) => *old = data,
                    _ => self.blocks.push(Block::Part { data }),
                }
            }
            kind if kind.starts_with("pi.") => self.blocks.push(Block::Other {
                label: kind.to_owned(),
            }),
            kind => {
                let text = entry.data().map_or_else(String::new, note_text);
                self.blocks.push(Block::Note {
                    kind: kind.to_owned(),
                    text,
                    pending: false,
                });
            }
        }
    }

    fn live(&mut self, client: &SessionClient) {
        let view = client.view();
        if let Some(message) = view.generation_message() {
            self.assistant(message, true);
        }
        for tool in view.live_tools() {
            let status = match tool.status.as_str() {
                "pending" => ToolStatus::Pending,
                "done" => ToolStatus::Done,
                "failed" | "error" => ToolStatus::Failed,
                _ => ToolStatus::Running,
            };
            if let Some(card) = self.card(&tool.call_id) {
                if card.status == ToolStatus::Pending || card.status == ToolStatus::Running {
                    card.status = status;
                    if !tool.output.is_empty() {
                        card.output = tool.output;
                    }
                }
            } else {
                self.blocks.push(Block::Tool(ToolCard {
                    call_id: tool.call_id,
                    name: tool.name,
                    summary: String::new(),
                    status,
                    output: tool.output,
                    diff: Vec::new(),
                    duration_ms: None,
                }));
            }
        }
        if !view.working() {
            for block in &mut self.blocks {
                if let Block::Tool(card) = block
                    && matches!(card.status, ToolStatus::Pending | ToolStatus::Running)
                {
                    card.status = ToolStatus::Stopped;
                }
            }
        }
    }
}

fn note_text(data: &Value) -> String {
    match data {
        Value::String(text) => text.clone(),
        other => other
            .get("text")
            .and_then(Value::as_str)
            .map_or_else(|| other.to_string(), str::to_owned),
    }
}

/// The transcript, in order, as blocks.
pub fn project(client: &SessionClient) -> Vec<Block> {
    let mut builder = Builder::default();
    let (earlier_loading, gaps_loading) = client.loading();
    let mut pending = Vec::new();
    for item in client.items() {
        match item {
            Item::Earlier => builder.blocks.push(Block::Earlier {
                loading: earlier_loading,
            }),
            Item::Gap(gap) => {
                builder.blocks.push(Block::Gap {
                    min: gap.read_min(),
                    max: gap.read_max(),
                    loading: gaps_loading > 0,
                });
            }
            Item::Entry(entry) => builder.entry(entry, client),
            Item::Pending { tracked, receipt } => {
                pending.push((tracked.request.clone(), receipt.cloned()))
            }
        }
    }
    if client.view().ready() {
        builder.live(client);
    }
    let view = client.view();
    if !view.working()
        && let Some((reason, detail)) = view.run_failure()
    {
        builder.blocks.push(Block::Failure {
            reason,
            detail: detail.unwrap_or_default(),
        });
    }
    for item in view
        .doc("pi.inbox")
        .and_then(|i| i.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let text = item
            .get("content")
            .map_or_else(String::new, |c| text_of(Some(c)));
        let mode = match item.get("mode").and_then(Value::as_str) {
            Some("followUp") => "follow-up",
            Some(other) => other,
            None => "queued",
        };
        builder.blocks.push(Block::Queued {
            text,
            mode: mode.to_owned(),
        });
    }
    for (request, receipt) in pending {
        // Once the host queued an input, the inbox document shows it; never matched by text.
        if receipt
            .as_ref()
            .is_some_and(|r| r.status == ReceiptStatus::Queued)
        {
            continue;
        }
        match request {
            Request::Submit { input } => {
                let mut status = status_text(receipt.as_ref(), true);
                if input.kind == InputKind::FollowUp && receipt.is_none() {
                    status = "sending as follow-up".to_owned();
                }
                builder.blocks.push(Block::User {
                    text: input.text,
                    status,
                    pending: true,
                });
            }
            Request::Write { entry } => builder.blocks.push(Block::Note {
                kind: entry.kind,
                text: entry.data.as_ref().map_or_else(String::new, note_text),
                pending: true,
            }),
            Request::Abort => {}
        }
    }
    builder.blocks
}

/// Text of the last finished answer, for `/copy`.
pub fn last_answer(blocks: &[Block]) -> Option<&str> {
    blocks.iter().rev().find_map(|b| match b {
        Block::Assistant {
            text,
            streaming: false,
            ..
        } if !text.is_empty() => Some(text.as_str()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oqto_session_client::Options;
    use serde_json::json;

    fn client_with(entries: Value, live: Value) -> SessionClient {
        let mut client = SessionClient::new("ses", Options::default());
        client.connect();
        let attach = client.take_outgoing()[0].id().to_owned();
        client.receive(&json!({"type": "attached", "id": attach, "v": 0, "attachment": "att_1", "session": "ses", "store": "st",
            "window": {"min": 50, "max": 100}, "upstream": {"package": "@earendil-works/pi-durable", "version": "1.5.2"}, "max_frame_bytes": 8388608}));
        client.receive(&json!({"type": "view", "attachment": "att_1", "seq": 1, "ops": [["r", {"conversation": {"id": 1}, "entries": entries, "docs": {"pi.live": live, "pi.inbox": {"items": [{"id": 9, "mode": "followUp", "content": "later"}]}}}]]}));
        client
    }

    #[test]
    fn tool_round_with_live_output_and_queue() {
        let entries = json!([
            {"id": 1, "conversationId": 1, "kind": "pi.user", "model": [{"role": "user", "content": "count to 3"}]},
            {"id": 2, "conversationId": 1, "kind": "pi.assistant", "model": [{"role": "assistant", "content": [{"type": "toolCall", "id": "c1", "name": "count", "arguments": {"to": 3}}]}]}
        ]);
        let live = json!({"run": {"taskId": 3}, "tools": [{"callId": "c1", "name": "count", "status": "running", "output": "1 2"}], "generation": {"message": {"content": [{"type": "text", "text": "Count"}]}}});
        let blocks = project(&client_with(entries, live));
        assert!(matches!(&blocks[0], Block::User { text, .. } if text == "count to 3"));
        assert!(
            matches!(&blocks[1], Block::Tool(card) if card.status == ToolStatus::Running && card.output == "1 2")
        );
        assert!(
            matches!(&blocks[2], Block::Assistant { streaming: true, text, .. } if text == "Count")
        );
        assert!(matches!(&blocks[3], Block::Queued { mode, .. } if mode == "follow-up"));
    }

    #[test]
    fn a_later_part_state_replaces_the_earlier_by_part_id() {
        let part = |state: &str| json!({"part": {"schema": "oqto.openui.part.v1", "partId": "part_profile_1", "profile": {"id": "oqto", "version": "0.2.2"}, "body": "root = Stack([])", "fallback": "x"}, "state": state});
        let entries = json!([
            {"id": 1, "conversationId": 1, "kind": "oqto.ui", "data": part("shown")},
            {"id": 2, "conversationId": 1, "kind": "pi.user", "model": [{"role": "user", "content": "hi"}]},
            {"id": 3, "conversationId": 1, "kind": "oqto.ui", "data": part("withdrawn")}
        ]);
        let blocks = project(&client_with(entries, json!({})));
        let parts: Vec<&Value> = blocks
            .iter()
            .filter_map(|b| {
                if let Block::Part { data } = b {
                    Some(data)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["state"], "withdrawn");
        assert!(matches!(blocks[0], Block::Part { .. }));
    }

    #[test]
    fn edit_calls_become_diffs_and_notes_show() {
        let entries = json!([
            {"id": 1, "conversationId": 1, "kind": "pi.assistant", "model": [{"role": "assistant", "content": [{"type": "toolCall", "id": "c1", "name": "edit", "arguments": {"path": "a.rs", "oldText": "let a = 1;", "newText": "let a = 2;\nlet b = 3;"}}]}]},
            {"id": 2, "conversationId": 1, "kind": "pi.tool-result", "model": [{"role": "toolResult", "toolCallId": "c1", "toolName": "edit", "content": [{"type": "text", "text": "ok"}], "isError": false, "durationMs": 12}]},
            {"id": 3, "conversationId": 1, "kind": "app.note", "data": {"text": "remember this"}}
        ]);
        let blocks = project(&client_with(entries, json!({})));
        let Block::Tool(card) = &blocks[0] else {
            panic!("tool card")
        };
        assert_eq!(card.diff.len(), 3);
        assert_eq!(card.summary, "a.rs  +2 -1");
        assert_eq!(
            (card.status, card.duration_ms),
            (ToolStatus::Done, Some(12))
        );
        assert!(matches!(&blocks[1], Block::Note { text, .. } if text == "remember this"));
    }
}

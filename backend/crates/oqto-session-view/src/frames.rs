//! Frame shapes of `frames.schema.json`, parsed strictly: unknown fields and unknown frame
//! types are refused, and every pattern and bound of the schema is checked.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::ops;

/// The contract version this crate speaks.
pub const VERSION: u64 = 0;
/// The upstream package `attached.upstream.package` must name.
pub const UPSTREAM_PACKAGE: &str = "@earendil-works/pi-durable";

/// A frame the schema refuses.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid frame: {0}")]
pub struct FrameError(pub String);

fn invalid(why: impl Into<String>) -> FrameError {
    FrameError(why.into())
}

fn check(ok: bool, why: &str) -> Result<(), FrameError> {
    if ok { Ok(()) } else { Err(invalid(why)) }
}

/// `^[A-Za-z0-9._:-]{1,128}$`: frame ids, Session ids, attachments, request ids, stores.
pub fn valid_id(text: &str) -> bool {
    (1..=128).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

/// `^[A-Za-z0-9_-]{1,4096}$`: an opaque read cursor.
pub fn valid_cursor(text: &str) -> bool {
    (1..=4096).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// `^[a-z][a-z0-9_.-]{0,63}$`, not `^pi\.` and not `oqto.ui` (host only, revision 2.2):
/// the kind of a client's passive write.
pub fn valid_write_kind(kind: &str) -> bool {
    let mut bytes = kind.bytes();
    let first = bytes.next().is_some_and(|b| b.is_ascii_lowercase());
    first
        && kind.len() <= 64
        && bytes.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-')
        })
        && !kind.starts_with("pi.")
        && kind != crate::parts::UI_KIND
}

/// Input text: 1 to 32768 characters with at least one that is not white space.
pub fn valid_text(text: &str) -> bool {
    let count = text.chars().count();
    (1..=32768).contains(&count) && text.chars().any(|c| !c.is_whitespace() && c != '\u{feff}')
}

fn check_id(text: &str, what: &str) -> Result<(), FrameError> {
    check(valid_id(text), what)
}

/// Window of an `attach`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    /// Newest entry id the client holds plus one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<u64>,
    /// Entries the window starts with (1 to 200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<u64>,
    /// Entries the window holds before it drops the oldest (1 to 400).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u64>,
}

impl Window {
    fn validate(&self) -> Result<(), FrameError> {
        check(
            self.min.is_none_or(|n| (1..=200).contains(&n)),
            "window.min",
        )?;
        check(
            self.max.is_none_or(|n| (1..=400).contains(&n)),
            "window.max",
        )
    }
}

/// Kinds of user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    /// Starts a run; refused with `busy` while working.
    Prompt,
    /// Delivered during the current run; refused with `idle`.
    Steer,
    /// Runs after the current run, or starts one.
    FollowUp,
}

impl InputKind {
    /// The wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Steer => "steer",
            Self::FollowUp => "follow_up",
        }
    }
}

/// `submit.input`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    /// What the input asks for.
    pub kind: InputKind,
    /// The text.
    pub text: String,
}

/// `write.entry`: a passive entry the model never sees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteEntry {
    /// Client-chosen kind; `pi.` is reserved.
    pub kind: String,
    /// Any JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Order of a `read_entries`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Oldest first.
    Ascending,
    /// Newest first (the default).
    Descending,
}

/// Client to host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    /// Start observing one Session.
    Attach {
        /// Frame id.
        id: String,
        /// Contract version.
        v: u64,
        /// Session id.
        session: String,
        /// Optional window.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window: Option<Window>,
    },
    /// Stop one attachment.
    Detach {
        /// Frame id.
        id: String,
        /// Attachment id.
        attachment: String,
    },
    /// Admit user input.
    Submit {
        /// Frame id.
        id: String,
        /// Session id.
        session: String,
        /// Idempotency key.
        request_id: String,
        /// The input.
        input: Input,
    },
    /// Append a passive entry.
    Write {
        /// Frame id.
        id: String,
        /// Session id.
        session: String,
        /// Idempotency key.
        request_id: String,
        /// The entry.
        entry: WriteEntry,
    },
    /// Stop the current work and withdraw queued inputs.
    Abort {
        /// Frame id.
        id: String,
        /// Session id.
        session: String,
        /// Idempotency key.
        request_id: String,
    },
    /// Read the current receipt of a request.
    ReceiptQuery {
        /// Frame id.
        id: String,
        /// Session id.
        session: String,
        /// The request.
        request_id: String,
    },
    /// Read committed entries by id range, one page.
    ReadEntries {
        /// Frame id.
        id: String,
        /// Session id.
        session: String,
        /// Inclusive lower id bound.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<u64>,
        /// Inclusive upper id bound.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<u64>,
        /// Order; descending when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        order: Option<Order>,
        /// Page size, 1 to 200.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
        /// Continuation from the previous page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
}

impl ClientFrame {
    /// Parse and check a client frame against the schema.
    pub fn parse(value: &Value) -> Result<Self, FrameError> {
        reject_unknown(value, client_fields)?;
        let frame: Self =
            serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))?;
        frame.validate()?;
        Ok(frame)
    }

    /// The frame id.
    pub fn id(&self) -> &str {
        match self {
            Self::Attach { id, .. }
            | Self::Detach { id, .. }
            | Self::Submit { id, .. }
            | Self::Write { id, .. }
            | Self::Abort { id, .. }
            | Self::ReceiptQuery { id, .. }
            | Self::ReadEntries { id, .. } => id,
        }
    }

    /// The request id of an effect (`submit`, `write`, `abort`, `receipt_query`).
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Submit { request_id, .. }
            | Self::Write { request_id, .. }
            | Self::Abort { request_id, .. }
            | Self::ReceiptQuery { request_id, .. } => Some(request_id),
            _ => None,
        }
    }

    /// Check every schema rule serde cannot express.
    pub fn validate(&self) -> Result<(), FrameError> {
        check_id(self.id(), "id")?;
        match self {
            Self::Attach {
                session, window, ..
            } => {
                check_id(session, "session")?;
                window.as_ref().map_or(Ok(()), Window::validate)
            }
            Self::Detach { attachment, .. } => check_id(attachment, "attachment"),
            Self::Submit {
                session,
                request_id,
                input,
                ..
            } => {
                check_id(session, "session")?;
                check_id(request_id, "request_id")?;
                check(valid_text(&input.text), "input.text")
            }
            Self::Write {
                session,
                request_id,
                entry,
                ..
            } => {
                check_id(session, "session")?;
                check_id(request_id, "request_id")?;
                check(valid_write_kind(&entry.kind), "entry.kind")
            }
            Self::Abort {
                session,
                request_id,
                ..
            }
            | Self::ReceiptQuery {
                session,
                request_id,
                ..
            } => {
                check_id(session, "session")?;
                check_id(request_id, "request_id")
            }
            Self::ReadEntries {
                session,
                limit,
                cursor,
                ..
            } => {
                check_id(session, "session")?;
                check(limit.is_none_or(|n| (1..=200).contains(&n)), "limit")?;
                check(cursor.as_deref().is_none_or(valid_cursor), "cursor")
            }
        }
    }

    /// The frame as JSON.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

const fn client_fields(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind.as_bytes() {
        b"attach" => &["type", "id", "v", "session", "window"],
        b"detach" => &["type", "id", "attachment"],
        b"submit" => &["type", "id", "session", "request_id", "input"],
        b"write" => &["type", "id", "session", "request_id", "entry"],
        b"abort" | b"receipt_query" => &["type", "id", "session", "request_id"],
        b"read_entries" => &[
            "type", "id", "session", "min", "max", "order", "limit", "cursor",
        ],
        _ => return None,
    })
}

const fn host_fields(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind.as_bytes() {
        b"attached" => &[
            "type",
            "id",
            "v",
            "attachment",
            "session",
            "store",
            "window",
            "upstream",
            "max_frame_bytes",
        ],
        b"detached" => &["type", "id", "attachment"],
        b"view" => &["type", "attachment", "seq", "ops", "retired"],
        b"receipt" => &[
            "type",
            "id",
            "session",
            "request_id",
            "kind",
            "status",
            "entry",
            "answer",
            "reason",
            "detail",
        ],
        b"entries" => &["type", "id", "session", "items", "next"],
        b"ended" => &["type", "attachment", "reason"],
        b"error" => &["type", "id", "code", "message", "supported"],
        _ => return None,
    })
}

/// Unknown frame types and unknown top-level fields are refused (serde's internally tagged
/// enums cannot deny unknown fields themselves).
fn reject_unknown(
    value: &Value,
    fields: fn(&str) -> Option<&'static [&'static str]>,
) -> Result<(), FrameError> {
    let object = value.as_object().ok_or_else(|| invalid("not an object"))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("type"))?;
    let allowed = fields(kind).ok_or_else(|| invalid(format!("unknown frame type {kind}")))?;
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(invalid(format!("unknown field {key}"))),
        None => Ok(()),
    }
}

/// The window an `attached` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachedWindow {
    /// Entries the window starts with.
    pub min: u64,
    /// Entries it holds before dropping the oldest.
    pub max: u64,
}

/// Which upstream the host runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upstream {
    /// Always `@earendil-works/pi-durable`.
    pub package: String,
    /// Its version.
    pub version: String,
}

/// Kind of request a receipt reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    /// A prompt.
    Prompt,
    /// A steer.
    Steer,
    /// A follow-up.
    FollowUp,
    /// A passive write.
    Write,
    /// An abort.
    Abort,
    /// A continue the host started.
    Continue,
}

/// Receipt status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    /// Admitted, waiting in the inbox.
    Queued,
    /// In the transcript, owned by a run.
    Placed,
    /// Final: answered, appended or idle.
    Done,
    /// Final without an answer.
    Unanswered,
}

impl ReceiptStatus {
    /// `done` and `unanswered` are final.
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Done | Self::Unanswered)
    }

    /// Whether `next` may follow `self` (equal statuses repeat a receipt).
    pub const fn may_become(self, next: Self) -> bool {
        match (self, next) {
            (a, b) if a as u8 == b as u8 => true,
            (Self::Queued, _) => true,
            (Self::Placed, Self::Done | Self::Unanswered) => true,
            _ => false,
        }
    }
}

/// The durable status of one request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    /// Present when this receipt answers a request on this connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Session id.
    pub session: String,
    /// The request.
    pub request_id: String,
    /// What kind of request.
    pub kind: ReceiptKind,
    /// Its status.
    pub status: ReceiptStatus,
    /// The transcript entry of the input or write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<u64>,
    /// The entry that answers an input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<u64>,
    /// Why it ended unanswered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Upstream detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

impl Receipt {
    fn validate(&self) -> Result<(), FrameError> {
        if let Some(id) = &self.id {
            check_id(id, "id")?;
        }
        check_id(&self.session, "session")?;
        check_id(&self.request_id, "request_id")?;
        let unanswered = self.status == ReceiptStatus::Unanswered;
        check(
            self.reason.is_some() == unanswered,
            "reason belongs to unanswered",
        )?;
        if let Some(reason) = &self.reason {
            check((1..=256).contains(&reason.chars().count()), "reason length")?;
        }
        if self.status == ReceiptStatus::Queued {
            check(
                self.entry.is_none() && self.answer.is_none(),
                "queued names no entry",
            )?;
        }
        if self.status == ReceiptStatus::Placed {
            check(self.answer.is_none(), "placed has no answer")?;
        }
        match self.kind {
            ReceiptKind::Abort => check(
                self.status == ReceiptStatus::Done && self.entry.is_none() && self.answer.is_none(),
                "abort receipts are done and name no entry",
            ),
            ReceiptKind::Write => check(
                self.status != ReceiptStatus::Placed && self.answer.is_none(),
                "writes are never placed or answered",
            ),
            _ => Ok(()),
        }
    }
}

/// Why an attachment ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndedReason {
    /// Orderly stop.
    HostClosed,
    /// The Session failed; state after the last frame is unknown.
    Failed,
    /// The client's access ended.
    Denied,
}

/// Error codes, a closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Malformed or out of bounds.
    Invalid,
    /// Over a limit.
    TooLarge,
    /// Unknown request type.
    Unsupported,
    /// Version not spoken.
    UnsupportedVersion,
    /// No such Session.
    UnknownSession,
    /// No such attachment.
    UnknownAttachment,
    /// No such request.
    UnknownRequest,
    /// Same request id, other content.
    RequestConflict,
    /// Prompt while working.
    Busy,
    /// Steer while idle.
    Idle,
    /// Not allowed.
    Denied,
    /// Retry later with the same request id.
    Unavailable,
}

impl ErrorCode {
    /// Wire name.
    pub fn as_str(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }
}

/// One committed entry. Kept as raw JSON so it round-trips byte for byte into hashes and
/// caches; the accessors read the stable fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Value", into = "Value")]
pub struct Entry(Value);

impl TryFrom<Value> for Entry {
    type Error = FrameError;

    fn try_from(value: Value) -> Result<Self, FrameError> {
        Self::new(value)
    }
}

impl From<Entry> for Value {
    fn from(entry: Entry) -> Self {
        entry.0
    }
}

impl Entry {
    /// Check `id`, `conversationId` and `kind` and wrap the value.
    pub fn new(value: Value) -> Result<Self, FrameError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("entry is not an object"))?;
        check(
            object.get("id").is_some_and(|v| v.as_u64().is_some()),
            "entry.id",
        )?;
        check(
            object
                .get("conversationId")
                .is_some_and(|v| v.as_u64().is_some()),
            "entry.conversationId",
        )?;
        check(
            object
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|k| !k.is_empty()),
            "entry.kind",
        )?;
        if object.get("kind").and_then(Value::as_str) == Some(crate::parts::UI_KIND) {
            let data = object
                .get("data")
                .ok_or_else(|| invalid("oqto.ui entry without data"))?;
            crate::parts::UiPart::parse(data)?;
        }
        Ok(Self(value))
    }

    /// Entry id.
    pub fn id(&self) -> u64 {
        self.0.get("id").and_then(Value::as_u64).unwrap_or_default()
    }

    /// Entry kind, for example `pi.assistant`.
    pub fn kind(&self) -> &str {
        self.0
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    /// `data` of a passive write.
    pub fn data(&self) -> Option<&Value> {
        self.0.get("data")
    }

    /// `model`: upstream messages the model sees.
    pub fn model(&self) -> Option<&Value> {
        self.0.get("model")
    }

    /// The raw entry.
    pub const fn value(&self) -> &Value {
        &self.0
    }
}

/// Host to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostFrame {
    /// An attachment started.
    Attached {
        /// The attach frame's id.
        id: String,
        /// Version.
        v: u64,
        /// Attachment id.
        attachment: String,
        /// Session id.
        session: String,
        /// Names the storage; the cache key.
        store: String,
        /// The window in force.
        window: AttachedWindow,
        /// Upstream package and version.
        upstream: Upstream,
        /// Largest frame the host sends.
        max_frame_bytes: u64,
    },
    /// An attachment stopped on request.
    Detached {
        /// The detach frame's id.
        id: String,
        /// Attachment id.
        attachment: String,
    },
    /// Ops for one attachment's view.
    View {
        /// Attachment id.
        attachment: String,
        /// 1, 2, 3, ...
        seq: u64,
        /// Decoded ops.
        ops: Vec<Value>,
        /// Upstream retired the observed value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retired: Option<bool>,
    },
    /// A request's status.
    Receipt(Receipt),
    /// One page of a read.
    Entries {
        /// The read's id.
        id: String,
        /// Session id.
        session: String,
        /// Entries in the requested order.
        items: Vec<Entry>,
        /// Continuation when more remain.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next: Option<String>,
    },
    /// An attachment ended.
    Ended {
        /// Attachment id.
        attachment: String,
        /// Why.
        reason: EndedReason,
    },
    /// A refusal of one request.
    Error {
        /// The request's id.
        id: String,
        /// Closed set.
        code: ErrorCode,
        /// For people; never parsed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// Versions the host speaks.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        supported: Option<Vec<u64>>,
    },
}

impl HostFrame {
    /// Parse and check a host frame against the schema.
    pub fn parse(value: &Value) -> Result<Self, FrameError> {
        reject_unknown(value, host_fields)?;
        let frame: Self =
            serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))?;
        frame.validate()?;
        Ok(frame)
    }

    /// The id of the request this frame answers, if any.
    pub fn reply_to(&self) -> Option<&str> {
        match self {
            Self::Attached { id, .. }
            | Self::Detached { id, .. }
            | Self::Entries { id, .. }
            | Self::Error { id, .. } => Some(id),
            Self::Receipt(receipt) => receipt.id.as_deref(),
            Self::View { .. } | Self::Ended { .. } => None,
        }
    }

    fn validate(&self) -> Result<(), FrameError> {
        match self {
            Self::Attached {
                id,
                v,
                attachment,
                session,
                store,
                window,
                upstream,
                max_frame_bytes,
            } => {
                for (text, what) in [
                    (id, "id"),
                    (attachment, "attachment"),
                    (session, "session"),
                    (store, "store"),
                ] {
                    check_id(text, what)?;
                }
                check(*v == VERSION, "attached.v")?;
                check(
                    (1..=200).contains(&window.min) && (1..=400).contains(&window.max),
                    "window",
                )?;
                check(upstream.package == UPSTREAM_PACKAGE, "upstream.package")?;
                check(valid_version(&upstream.version), "upstream.version")?;
                check(
                    (65_536..=16_777_216).contains(max_frame_bytes),
                    "max_frame_bytes",
                )
            }
            Self::Detached { id, attachment } => {
                check_id(id, "id")?;
                check_id(attachment, "attachment")
            }
            Self::View {
                attachment,
                seq,
                ops,
                retired,
            } => {
                check_id(attachment, "attachment")?;
                check(*seq >= 1, "seq")?;
                check(retired.is_none_or(|r| r), "retired")?;
                check(ops.iter().all(ops::schema_valid), "op")
            }
            Self::Receipt(receipt) => receipt.validate(),
            Self::Entries {
                id, session, next, ..
            } => {
                check_id(id, "id")?;
                check_id(session, "session")?;
                check(next.as_deref().is_none_or(valid_cursor), "next")
            }
            Self::Ended { attachment, .. } => check_id(attachment, "attachment"),
            Self::Error {
                id,
                message,
                supported,
                ..
            } => {
                check_id(id, "id")?;
                check(
                    message.as_ref().is_none_or(|m| m.chars().count() <= 512),
                    "message",
                )?;
                check(
                    supported.as_ref().is_none_or(|s| !s.is_empty()),
                    "supported",
                )
            }
        }
    }
}

fn valid_version(text: &str) -> bool {
    let mut parts = text.splitn(3, '.');
    let digits = |part: Option<&str>| {
        part.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    };
    let major = parts.next();
    let minor = parts.next();
    let rest = parts.next();
    let patch = rest.map(|r| {
        r.split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or_default()
    });
    digits(major) && digits(minor) && digits(patch)
}

/// Whether `value` is a frame of either direction (what the bundle's fixtures test).
pub fn valid_frame(value: &Value) -> bool {
    ClientFrame::parse(value).is_ok() || HostFrame::parse(value).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refuses_unknown_fields_and_types() {
        assert!(
            ClientFrame::parse(
                &json!({"type": "abort", "id": "c1", "session": "s", "request_id": "r"})
            )
            .is_ok()
        );
        assert!(
            ClientFrame::parse(
                &json!({"type": "abort", "id": "c1", "session": "s", "request_id": "r", "x": 1})
            )
            .is_err()
        );
        assert!(ClientFrame::parse(&json!({"type": "approve", "id": "c1"})).is_err());
    }

    #[test]
    fn serializes_with_the_type_tag() {
        let frame = ClientFrame::Abort {
            id: "c1".into(),
            session: "s".into(),
            request_id: "r".into(),
        };
        assert_eq!(
            frame.to_value(),
            json!({"type": "abort", "id": "c1", "session": "s", "request_id": "r"})
        );
    }

    #[test]
    fn versions_need_three_numbers() {
        assert!(valid_version("1.5.2"));
        assert!(valid_version("1.5.2-beta"));
        assert!(!valid_version("1.5"));
    }
}

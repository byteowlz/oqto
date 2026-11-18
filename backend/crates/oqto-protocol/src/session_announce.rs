//! Versioned "I am here" hint from a Pi extension to its personal Runner.
//!
//! An announce is never authority. It carries the native session file the
//! extension has just created or switched to; the Runner admits exactly that
//! one file through its existing ownership, symlink, header, Workspace-root
//! and pin validation before anything is listed. A malformed, foreign or
//! out-of-root announce is dropped and the periodic sweep remains the fallback.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One JSONL line; larger payloads are refused before parsing.
pub const MAX_SESSION_ANNOUNCE_BYTES: usize = 8 * 1024;
pub const MAX_SESSION_ANNOUNCE_ID: usize = 256;
pub const MAX_SESSION_ANNOUNCE_PATH: usize = 4096;
pub const MAX_SESSION_ANNOUNCE_DETAIL: usize = 512;
pub const SESSION_ANNOUNCE_VERSION: u8 = 1;
pub const SESSION_ANNOUNCE_KIND: &str = "session_announce";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(try_from = "u8", into = "u8")]
pub struct AnnounceVersion(
    #[schemars(range(min = 1, max = 1))]
    #[ts(type = "1")]
    u8,
);
impl AnnounceVersion {
    pub const V1: Self = Self(SESSION_ANNOUNCE_VERSION);
}
impl From<AnnounceVersion> for u8 {
    fn from(value: AnnounceVersion) -> Self {
        value.0
    }
}
impl TryFrom<u8> for AnnounceVersion {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value == SESSION_ANNOUNCE_VERSION {
            Ok(Self::V1)
        } else {
            Err("unsupported session announce version")
        }
    }
}

/// Why the extension is announcing. All values mean "this file exists now";
/// the Runner never treats them as anything more than a hint to validate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AnnounceReason {
    /// A Pi process started and created its session file.
    Start,
    /// A new Session was created inside a running Pi.
    New,
    /// Pi switched to another Session.
    Switch,
    /// Pi forked a Session into a new file.
    Fork,
    /// Pi resumed an existing Session.
    Resume,
}

/// The hint itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionAnnounce {
    pub v: AnnounceVersion,
    pub kind: String,
    pub reason: AnnounceReason,
    /// Absolute native session file the extension is using.
    pub session_file: String,
    /// Native session id from that file's header.
    pub session_id: String,
    /// Absolute working directory Pi reported for the Session.
    pub cwd: String,
    pub pid: u32,
    /// pi-tui-rpc live socket for that Session, when the extension has one.
    /// A hint only: the Runner never requires the socket to exist.
    #[serde(default)]
    pub live_socket: Option<String>,
    /// Fork lineage, when the extension knows it. Optional in v1 so a future
    /// client can send it without a version bump.
    #[serde(default)]
    pub previous_session_file: Option<String>,
}

/// What the Runner did with the hint. `Listed` and `Duplicate` mean the file
/// passed validation; `Denied` means it exists but was refused; `Invalid`
/// means the announce itself was refused before any file was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AnnounceOutcome {
    Listed,
    Duplicate,
    Denied,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionAnnounceAck {
    pub v: AnnounceVersion,
    pub ok: bool,
    pub result: AnnounceOutcome,
    #[serde(default)]
    pub detail: String,
}
impl SessionAnnounceAck {
    pub fn new(result: AnnounceOutcome, detail: impl Into<String>) -> Self {
        let mut detail = detail.into();
        if detail.len() > MAX_SESSION_ANNOUNCE_DETAIL {
            detail.truncate(MAX_SESSION_ANNOUNCE_DETAIL);
            while !detail.is_char_boundary(detail.len()) {
                detail.pop();
            }
        }
        Self {
            v: AnnounceVersion::V1,
            ok: matches!(result, AnnounceOutcome::Listed | AnnounceOutcome::Duplicate),
            result,
            detail,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnounceError {
    TooLarge,
    Empty,
    NotUtf8,
    Malformed,
    WrongKind,
    WrongVersion,
    InvalidPath,
    InvalidId,
    InvalidPid,
}
impl AnnounceError {
    /// Stable code, also used by the case tables in the published fixtures.
    pub fn code(self) -> &'static str {
        match self {
            Self::TooLarge => "too_large",
            Self::Empty => "empty",
            Self::NotUtf8 => "not_utf8",
            Self::Malformed => "malformed",
            Self::WrongKind => "wrong_kind",
            Self::WrongVersion => "wrong_version",
            Self::InvalidPath => "invalid_path",
            Self::InvalidId => "invalid_id",
            Self::InvalidPid => "invalid_pid",
        }
    }
}
impl std::fmt::Display for AnnounceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for AnnounceError {}

/// Absolute, already-normalized path. Canonicalization needs the filesystem and
/// belongs to the Runner's admission, not to this wire check.
fn check_path(value: &str) -> Result<(), AnnounceError> {
    let ok = !value.is_empty()
        && value.len() <= MAX_SESSION_ANNOUNCE_PATH
        && value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains('\0')
        && !value.split('/').any(|part| part == "." || part == "..");
    if ok {
        Ok(())
    } else {
        Err(AnnounceError::InvalidPath)
    }
}

fn check_id(value: &str) -> Result<(), AnnounceError> {
    let ok = !value.is_empty()
        && value.len() <= MAX_SESSION_ANNOUNCE_ID
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(AnnounceError::InvalidId)
    }
}

/// Wire-shape checks only: version, kind, reason and path/id syntax.
fn validate(announce: &SessionAnnounce) -> Result<(), AnnounceError> {
    if announce.kind != SESSION_ANNOUNCE_KIND {
        return Err(AnnounceError::WrongKind);
    }
    check_path(&announce.session_file)?;
    check_path(&announce.cwd)?;
    if let Some(socket) = &announce.live_socket {
        check_path(socket)?;
    }
    if let Some(previous) = &announce.previous_session_file {
        check_path(previous)?;
    }
    check_id(&announce.session_id)?;
    if announce.pid == 0 {
        return Err(AnnounceError::InvalidPid);
    }
    Ok(())
}

/// Decode one announce line. Trailing whitespace is allowed; trailing bytes are not.
pub fn decode_session_announce(input: &[u8]) -> Result<SessionAnnounce, AnnounceError> {
    if input.len() > MAX_SESSION_ANNOUNCE_BYTES {
        return Err(AnnounceError::TooLarge);
    }
    let text = std::str::from_utf8(input).map_err(|_| AnnounceError::NotUtf8)?;
    let text = text.trim_end_matches(['\n', '\r']);
    if text.trim().is_empty() {
        return Err(AnnounceError::Empty);
    }
    // Reject unsupported versions before serde so the answer is `wrong_version`
    // rather than a lie about a malformed body.
    let probe: serde_json::Value =
        serde_json::from_str(text).map_err(|_| AnnounceError::Malformed)?;
    if probe.get("v").and_then(serde_json::Value::as_u64)
        != Some(u64::from(SESSION_ANNOUNCE_VERSION))
    {
        return Err(AnnounceError::WrongVersion);
    }
    let announce: SessionAnnounce =
        serde_json::from_value(probe).map_err(|_| AnnounceError::Malformed)?;
    validate(&announce)?;
    Ok(announce)
}

pub fn encode_session_announce(announce: &SessionAnnounce) -> Result<String, AnnounceError> {
    validate(announce)?;
    let mut line = serde_json::to_string(announce).map_err(|_| AnnounceError::Malformed)?;
    line.push('\n');
    if line.len() > MAX_SESSION_ANNOUNCE_BYTES {
        return Err(AnnounceError::TooLarge);
    }
    Ok(line)
}

pub fn decode_session_announce_ack(input: &[u8]) -> Result<SessionAnnounceAck, AnnounceError> {
    if input.len() > MAX_SESSION_ANNOUNCE_BYTES {
        return Err(AnnounceError::TooLarge);
    }
    let text = std::str::from_utf8(input).map_err(|_| AnnounceError::NotUtf8)?;
    let ack: SessionAnnounceAck = serde_json::from_str(text.trim_end_matches(['\n', '\r']))
        .map_err(|_| AnnounceError::Malformed)?;
    if ack.ok
        != matches!(
            ack.result,
            AnnounceOutcome::Listed | AnnounceOutcome::Duplicate
        )
    {
        return Err(AnnounceError::Malformed);
    }
    Ok(ack)
}

pub fn encode_session_announce_ack(ack: &SessionAnnounceAck) -> String {
    let mut line = serde_json::to_string(ack).unwrap_or_else(|_| {
        "{\"v\":1,\"ok\":false,\"result\":\"invalid\",\"detail\":\"unencodable\"}".to_string()
    });
    line.push('\n');
    line
}

#[cfg(test)]
mod tests;

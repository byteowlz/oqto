//! Destination-side filter for the client-to-daemon pi-env stream
//! (packages/env/docs/protocol.md at the bridge pin). Frames are checked
//! before any byte reaches the confined daemon: bounded length, known frame
//! type, a JSON object request whose `op` is a coding operation. Payload bytes
//! stream through without buffering a whole file. This is defense in depth,
//! not shell containment: an admitted `exec` still runs arbitrary code inside
//! the sandbox.
use anyhow::{Result, bail, ensure};

pub const MAX_ENV_FRAME: usize = 16 * 1024 * 1024;
pub const MAX_ENV_JSON: usize = 1024 * 1024;
const HEADER: usize = 13;
const REQUEST: u8 = 1;
const CANCEL: u8 = 5;
const PING: u8 = 6;

/// Operations the read/write/edit/bash tools use. `watch` and anything
/// unknown are refused in this milestone.
const ALLOWED_OPS: &[&str] = &[
    "hello",
    "lstat",
    "realpath",
    "write",
    "writeChunk",
    "truncate",
    "fsync",
    "rename",
    "mkdir",
    "rm",
    "mkdtemp",
    "open",
    "pread",
    "fstat",
    "scanLines",
    "opendir",
    "readdir",
    "close",
    "exec",
];

#[derive(Debug, Default)]
enum State {
    #[default]
    Header,
    Json {
        payload: usize,
    },
    Payload {
        remaining: usize,
    },
}

#[derive(Debug, Default)]
pub struct RequestFilter {
    state: State,
    pending: Vec<u8>,
}

impl RequestFilter {
    /// Validate `input`, appending the bytes that may be forwarded to `out`.
    /// An error means the stream must be closed before anything more reaches
    /// the daemon; bytes already in `out` belong to complete valid prefixes.
    pub fn push(&mut self, mut input: &[u8], out: &mut Vec<u8>) -> Result<()> {
        while !input.is_empty() {
            match self.state {
                State::Header => {
                    let take = (HEADER - self.pending.len()).min(input.len());
                    self.pending.extend_from_slice(&input[..take]);
                    input = &input[take..];
                    if self.pending.len() < HEADER {
                        continue;
                    }
                    let length = be32(&self.pending[0..4]);
                    let kind = self.pending[4];
                    let json = be32(&self.pending[9..13]);
                    ensure!(
                        (9..=MAX_ENV_FRAME).contains(&length),
                        "pi-env frame length out of bounds"
                    );
                    ensure!(
                        (2..=MAX_ENV_JSON).contains(&json) && json <= length - 9,
                        "pi-env JSON length out of bounds"
                    );
                    ensure!(
                        matches!(kind, REQUEST | CANCEL | PING),
                        "pi-env frame type not allowed from the origin"
                    );
                    ensure!(
                        kind == REQUEST || length - 9 - json == 0,
                        "pi-env control frame carries a payload"
                    );
                    self.state = State::Json {
                        payload: length - 9 - json,
                    };
                }
                State::Json { payload } => {
                    let json = be32(&self.pending[9..13]);
                    let have = self.pending.len() - HEADER;
                    let take = (json - have).min(input.len());
                    self.pending.extend_from_slice(&input[..take]);
                    input = &input[take..];
                    if self.pending.len() - HEADER < json {
                        continue;
                    }
                    check_json(self.pending[4], &self.pending[HEADER..])?;
                    out.extend_from_slice(&self.pending);
                    self.pending.clear();
                    self.state = if payload == 0 {
                        State::Header
                    } else {
                        State::Payload { remaining: payload }
                    };
                }
                State::Payload { remaining } => {
                    let take = remaining.min(input.len());
                    out.extend_from_slice(&input[..take]);
                    input = &input[take..];
                    self.state = if remaining == take {
                        State::Header
                    } else {
                        State::Payload {
                            remaining: remaining - take,
                        }
                    };
                }
            }
        }
        Ok(())
    }
}

fn be32(bytes: &[u8]) -> usize {
    let mut value = 0usize;
    for byte in bytes {
        value = (value << 8) | usize::from(*byte);
    }
    value
}

fn check_json(kind: u8, bytes: &[u8]) -> Result<()> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("invalid pi-env JSON"))?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("pi-env JSON must be an object"))?;
    if kind != REQUEST {
        return Ok(());
    }
    match object.get("op").and_then(serde_json::Value::as_str) {
        Some(op) if ALLOWED_OPS.contains(&op) => Ok(()),
        Some(_) => bail!("pi-env operation not admitted on remote hands"),
        None => bail!("pi-env request without op"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(kind: u8, id: u32, json: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&((9 + json.len() + payload.len()) as u32).to_be_bytes());
        out.push(kind);
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(&(json.len() as u32).to_be_bytes());
        out.extend_from_slice(json.as_bytes());
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn arbitrary_splits_preserve_bytes() -> Result<()> {
        let mut stream = frame(1, 1, r#"{"op":"hello","protocol":1}"#, b"");
        stream.extend(frame(1, 2, r#"{"op":"write","path":"/w/a"}"#, &[7u8; 5000]));
        stream.extend(frame(6, 0, "{}", b""));
        stream.extend(frame(5, 2, "{}", b""));
        for split in [1usize, 2, 3, 13, 14, 100, 4999, stream.len()] {
            let mut filter = RequestFilter::default();
            let mut out = Vec::new();
            for chunk in stream.chunks(split) {
                filter.push(chunk, &mut out)?;
            }
            assert_eq!(out, stream, "split {split}");
        }
        Ok(())
    }

    #[test]
    fn refuses_unknown_ops_types_and_oversize_before_forwarding() {
        for bad in [
            frame(1, 1, r#"{"op":"watch","targets":[]}"#, b""),
            frame(1, 1, r#"{"op":"spawnShell"}"#, b""),
            frame(1, 1, r#"["op"]"#, b""),
            frame(2, 1, "{}", b""),
            frame(5, 1, "{}", b"x"),
            frame(1, 1, "not json", b""),
        ] {
            let mut out = Vec::new();
            assert!(RequestFilter::default().push(&bad, &mut out).is_err());
            assert!(out.is_empty());
        }
        let mut huge = Vec::new();
        huge.extend_from_slice(&((MAX_ENV_FRAME + 1) as u32).to_be_bytes());
        huge.extend_from_slice(&[1, 0, 0, 0, 1, 0, 0, 0, 2]);
        let mut out = Vec::new();
        assert!(RequestFilter::default().push(&huge, &mut out).is_err());
        // A valid prefix is forwarded; the bad frame after it is not.
        let mut stream = frame(1, 1, r#"{"op":"hello"}"#, b"");
        let good = stream.len();
        stream.extend(frame(1, 2, r#"{"op":"watch"}"#, b""));
        let mut out = Vec::new();
        assert!(RequestFilter::default().push(&stream, &mut out).is_err());
        assert_eq!(out.len(), good);
    }
}

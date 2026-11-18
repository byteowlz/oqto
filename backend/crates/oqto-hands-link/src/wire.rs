//! `oqto.hands.v1` wire (DESIGN.md sections 4 to 6): strict JSON control
//! messages on a dedicated control WebSocket, and binary generation-fenced
//! frames on one data WebSocket per admitted call. No generic remote method,
//! no shell-string envelope, unknown fields and operations fail closed.
use anyhow::{Context, Result, bail, ensure};
use oqto_personal_wire::SignedRequest;
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "oqto.hands.v1";
pub const CONTROL_PATH: &str = "/hands/v1/control";
pub const DATA_PATH: &str = "/hands/v1/data";
/// Control messages and the dedicated control queue.
pub const MAX_CONTROL_MESSAGE: usize = 64 * 1024;
pub const MAX_CONTROL_QUEUE: usize = 256 * 1024;
/// One data transport message, header included.
pub const MAX_DATA_MESSAGE: usize = 256 * 1024;
pub const DATA_HEADER_LEN: usize = 50;
pub const MAX_DATA_PAYLOAD: usize = MAX_DATA_MESSAGE - DATA_HEADER_LEN;
/// Byte credit per direction per call.
pub const CREDIT_WINDOW: u32 = 256 * 1024;
pub const HEARTBEAT_MS: u64 = 5_000;
pub const HEALTH_TIMEOUT_MS: u64 = 15_000;
/// Pre-authentication challenge answer and data attach windows.
pub const CHALLENGE_TIMEOUT_MS: u64 = 5_000;
pub const ATTACH_TTL_MS: u64 = 10_000;

/// Coding operations this milestone advertises; nothing else is admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Read,
    Write,
    Edit,
    Bash,
}

/// Signed payloads (inside `SignedRequest.payload`, personal request domain).
/// The `purpose` tag cannot parse as a personal `HostAction` (tagged by
/// `operation`) and vice versa, so neither proof is accepted by the other API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "purpose", deny_unknown_fields)]
pub enum HandsProof {
    #[serde(rename = "oqto.hands.v1.open_control")]
    OpenControl {
        host_id: String,
        challenge: String,
        client_nonce: String,
        namespace: String,
        controller_epoch: u64,
        predecessor_epoch: u64,
    },
    #[serde(rename = "oqto.hands.v1.attach_data")]
    AttachData {
        host_id: String,
        challenge: String,
        control_epoch: String,
        call_id: String,
        generation: u64,
        /// pi-env stdout synchronization token. Not authority.
        sync_token: String,
    },
}

impl HandsProof {
    pub fn parse(payload: &str) -> Result<Self> {
        ensure!(payload.len() <= 4096, "hands proof too large");
        serde_json::from_str(payload).context("invalid hands proof")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingInfo {
    pub binding_id: String,
    /// Digest of the destination binding ceiling; changes invalidate leases.
    pub binding_revision: String,
    pub cwd: String,
    pub platform: String,
    pub max_lease_ms: u32,
    pub max_concurrent_calls: u8,
    pub workspace_mode: String,
    pub attention: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ready {
    pub protocol: String,
    pub host_id: String,
    pub host_boot: String,
    pub device_revision: String,
    pub control_epoch: String,
    pub controller_epoch: u64,
    pub bindings: Vec<BindingInfo>,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRequest {
    pub call_id: String,
    pub binding_id: String,
    pub binding_revision: String,
    pub policy_revision: u64,
    pub digest: String,
    pub operation: Operation,
    pub lease_ms: u32,
    pub renew_ms: u32,
    pub max_concurrent_calls: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    /// Persisted before any effect; not yet attached.
    Claimed,
    /// Attached: frames may have reached the daemon.
    Running,
    Released,
    Cancelled,
    Expired,
    Fenced,
    /// Host restart or loss while claimed/running: effects uncertain.
    Unknown,
}

impl CallState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Running => "running",
            Self::Released => "released",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::Fenced => "fenced",
            Self::Unknown => "unknown",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "claimed" => Self::Claimed,
            "running" => Self::Running,
            "released" => Self::Released,
            "cancelled" => Self::Cancelled,
            "expired" => Self::Expired,
            "fenced" => Self::Fenced,
            "unknown" => Self::Unknown,
            _ => bail!("invalid stored call state"),
        })
    }
    pub fn terminal(self) -> bool {
        !matches!(self, Self::Claimed | Self::Running)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopState {
    /// No hands generation was started.
    NotStarted,
    /// Still running.
    Pending,
    /// The supervisor confirmed the generation and its descendants are gone.
    Confirmed,
    /// Stop could not be confirmed; the binding is frozen with attention.
    Unconfirmed,
}

impl StopState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Unconfirmed => "unconfirmed",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "not_started" => Self::NotStarted,
            "pending" => Self::Pending,
            "confirmed" => Self::Confirmed,
            "unconfirmed" => Self::Unconfirmed,
            _ => bail!("invalid stored stop state"),
        })
    }
}

/// Destination dispatch receipt. Evidence for inspection; never a replay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub call_id: String,
    pub binding_id: String,
    pub generation: u64,
    pub host_boot: String,
    pub digest: String,
    pub operation: Operation,
    pub state: CallState,
    pub stop: StopState,
    pub cwd: String,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoppedGeneration {
    pub call_id: String,
    pub generation: u64,
    pub stop: StopState,
}

/// Bounded typed refusal; a refusal before claim means not started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    NotDelegated,
    UnknownBinding,
    StaleBinding,
    StalePolicy,
    StaleController,
    Attention,
    LimitExceeded,
    ConcurrencyExceeded,
    DigestMismatch,
    InvalidRequest,
    DeviceRevoked,
    Unsupported,
    NotFound,
    Busy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientControl {
    Open {
        proof: SignedRequest,
    },
    Claim(ClaimRequest),
    Renew {
        call_id: String,
        sequence: u64,
        challenge: String,
    },
    Cancel {
        call_id: String,
    },
    Release {
        call_id: String,
    },
    /// Origin revoke: persist the fence, stop and confirm, then acknowledge.
    Freeze {
        binding_id: String,
        policy_revision: u64,
    },
    Inspect {
        call_id: String,
    },
    Ping {},
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerControl {
    Challenge {
        protocol: String,
        challenge: String,
    },
    Ready(Ready),
    Claimed {
        call_id: String,
        generation: u64,
        remaining_ms: u64,
        attach_challenge: String,
    },
    /// Exact duplicate claim: inspect only, never dispatched again.
    Existing {
        receipt: Receipt,
    },
    Refused {
        call_id: Option<String>,
        code: Refusal,
    },
    RenewChallenge {
        call_id: String,
        sequence: u64,
        challenge: String,
        remaining_ms: u64,
    },
    Renewed {
        call_id: String,
        sequence: u64,
        remaining_ms: u64,
    },
    /// The lease ran out or the call was otherwise stopped by the host.
    Ended {
        receipt: Receipt,
    },
    FenceAck {
        binding_id: String,
        policy_revision: u64,
        stopped: Vec<StoppedGeneration>,
        confirmed: bool,
    },
    Inspected {
        receipt: Receipt,
    },
    Released {
        receipt: Receipt,
    },
    Pong {},
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DataHello {
    Attach { proof: SignedRequest },
    Attached { stream_epoch: String },
}

pub fn encode_json<T: Serialize>(value: &T) -> Result<String> {
    let text = serde_json::to_string(value)?;
    ensure!(
        text.len() <= MAX_CONTROL_MESSAGE,
        "control message too large"
    );
    Ok(text)
}

pub fn decode_json<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T> {
    ensure!(
        text.len() <= MAX_CONTROL_MESSAGE,
        "control message too large"
    );
    serde_json::from_str(text).context("invalid control message")
}

/// Random lower-case hex of `bytes` bytes.
pub fn random_hex(bytes: usize) -> String {
    use rand::RngCore;
    let mut buffer = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buffer);
    hex::encode(buffer)
}

pub fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataKind {
    /// pi-env stream bytes.
    Env = 1,
    /// Byte credit granted to the peer (u32 BE payload).
    Credit = 2,
    /// Graceful end of this direction.
    End = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamId {
    pub call: [u8; 16],
    pub generation: u64,
    pub stream: [u8; 16],
}

impl StreamId {
    pub fn new(call_id: &str, generation: u64, stream_epoch: &str) -> Result<Self> {
        Ok(Self {
            call: decode16(call_id)?,
            generation,
            stream: decode16(stream_epoch)?,
        })
    }
}

fn decode16(text: &str) -> Result<[u8; 16]> {
    ensure!(is_hex(text, 32), "invalid 16-byte identifier");
    let bytes = hex::decode(text)?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid 16-byte identifier"))
}

/// Header: version, kind, call id, generation, stream epoch, sequence.
pub fn encode_frame(id: &StreamId, kind: DataKind, seq: u64, payload: &[u8]) -> Result<Vec<u8>> {
    ensure!(payload.len() <= MAX_DATA_PAYLOAD, "data payload too large");
    let mut out = Vec::with_capacity(DATA_HEADER_LEN + payload.len());
    out.push(1);
    out.push(kind as u8);
    out.extend_from_slice(&id.call);
    out.extend_from_slice(&id.generation.to_be_bytes());
    out.extend_from_slice(&id.stream);
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Validates every received frame against the stream identity and sequence.
/// A mismatch, gap or repeat is a protocol failure, never a resume hint.
#[derive(Debug)]
pub struct FrameReader {
    id: StreamId,
    next_seq: u64,
}

impl FrameReader {
    pub fn new(id: StreamId) -> Self {
        Self { id, next_seq: 0 }
    }
    pub fn read<'a>(&mut self, bytes: &'a [u8]) -> Result<(DataKind, &'a [u8])> {
        ensure!(
            bytes.len() >= DATA_HEADER_LEN && bytes.len() <= MAX_DATA_MESSAGE,
            "data frame size out of bounds"
        );
        ensure!(bytes[0] == 1, "unsupported data frame version");
        let kind = match bytes[1] {
            1 => DataKind::Env,
            2 => DataKind::Credit,
            3 => DataKind::End,
            _ => bail!("unknown data frame kind"),
        };
        ensure!(bytes[2..18] == self.id.call, "frame for another call");
        let generation = u64::from_be_bytes(slice8(&bytes[18..26])?);
        ensure!(
            generation == self.id.generation,
            "frame for another hands generation"
        );
        ensure!(bytes[26..42] == self.id.stream, "frame for another stream");
        let seq = u64::from_be_bytes(slice8(&bytes[42..50])?);
        ensure!(seq == self.next_seq, "data frame sequence gap or repeat");
        self.next_seq += 1;
        let payload = &bytes[DATA_HEADER_LEN..];
        match kind {
            DataKind::Credit => ensure!(payload.len() == 4, "invalid credit frame"),
            DataKind::End => ensure!(payload.is_empty(), "invalid end frame"),
            DataKind::Env => ensure!(!payload.is_empty(), "empty data frame"),
        }
        Ok((kind, payload))
    }
}

fn slice8(bytes: &[u8]) -> Result<[u8; 8]> {
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("truncated data header"))
}

pub fn credit_value(payload: &[u8]) -> Result<u32> {
    let bytes: [u8; 4] = payload
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid credit frame"))?;
    let value = u32::from_be_bytes(bytes);
    ensure!(value > 0 && value <= CREDIT_WINDOW, "credit out of bounds");
    Ok(value)
}

/// Outbound credit: what the peer allowed us to send and no more.
#[derive(Debug)]
pub struct SendCredit {
    available: u64,
}

impl Default for SendCredit {
    fn default() -> Self {
        Self {
            available: u64::from(CREDIT_WINDOW),
        }
    }
}

impl SendCredit {
    pub fn available(&self) -> usize {
        usize::try_from(self.available).unwrap_or(usize::MAX)
    }
    pub fn spend(&mut self, bytes: usize) -> Result<()> {
        let bytes = bytes as u64;
        ensure!(bytes <= self.available, "send credit exhausted");
        self.available -= bytes;
        Ok(())
    }
    pub fn grant(&mut self, bytes: u32) -> Result<()> {
        self.available = self.available.saturating_add(u64::from(bytes));
        ensure!(
            self.available <= u64::from(CREDIT_WINDOW),
            "peer granted more credit than the window"
        );
        Ok(())
    }
}

/// Inbound credit: received bytes must stay inside what we granted.
#[derive(Debug)]
pub struct ReceiveWindow {
    outstanding: u64,
}

impl Default for ReceiveWindow {
    fn default() -> Self {
        Self {
            outstanding: u64::from(CREDIT_WINDOW),
        }
    }
}

impl ReceiveWindow {
    pub fn receive(&mut self, bytes: usize) -> Result<()> {
        let bytes = bytes as u64;
        ensure!(bytes <= self.outstanding, "peer exceeded granted credit");
        self.outstanding -= bytes;
        Ok(())
    }
    /// After downstream consumed `bytes`, return them to the peer.
    pub fn release(&mut self, bytes: usize) -> u32 {
        let bytes = u32::try_from(bytes).unwrap_or(CREDIT_WINDOW);
        self.outstanding = self.outstanding.saturating_add(u64::from(bytes));
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Result<StreamId> {
        StreamId::new(&"a".repeat(32), 7, &"b".repeat(32))
    }

    #[test]
    fn frames_round_trip_and_reject_stale_identity_and_sequence() -> Result<()> {
        let stream = id()?;
        let mut reader = FrameReader::new(stream);
        let first = encode_frame(&stream, DataKind::Env, 0, b"hello")?;
        assert_eq!(reader.read(&first)?, (DataKind::Env, &b"hello"[..]));
        // Repeat and gap.
        assert!(reader.read(&first).is_err());
        let mut reader = FrameReader::new(stream);
        assert!(
            reader
                .read(&encode_frame(&stream, DataKind::Env, 1, b"x")?)
                .is_err()
        );
        // Old generation and other stream epoch.
        let old = StreamId {
            generation: 6,
            ..stream
        };
        let mut reader = FrameReader::new(stream);
        assert!(
            reader
                .read(&encode_frame(&old, DataKind::Env, 0, b"x")?)
                .is_err()
        );
        let other = StreamId::new(&"a".repeat(32), 7, &"c".repeat(32))?;
        assert!(
            reader
                .read(&encode_frame(&other, DataKind::Env, 0, b"x")?)
                .is_err()
        );
        // Unknown kind, bad version, oversize.
        let mut bad = encode_frame(&stream, DataKind::Env, 0, b"x")?;
        bad[1] = 9;
        assert!(FrameReader::new(stream).read(&bad).is_err());
        bad[1] = 1;
        bad[0] = 2;
        assert!(FrameReader::new(stream).read(&bad).is_err());
        assert!(encode_frame(&stream, DataKind::Env, 0, &vec![0; MAX_DATA_PAYLOAD + 1]).is_err());
        Ok(())
    }

    #[test]
    fn credit_is_bounded_both_ways() -> Result<()> {
        let mut send = SendCredit::default();
        send.spend(CREDIT_WINDOW as usize)?;
        assert!(send.spend(1).is_err());
        send.grant(10)?;
        assert!(send.grant(CREDIT_WINDOW).is_err());
        let mut window = ReceiveWindow::default();
        window.receive(CREDIT_WINDOW as usize)?;
        assert!(window.receive(1).is_err());
        assert_eq!(window.release(5), 5);
        window.receive(5)?;
        assert!(credit_value(&0u32.to_be_bytes()).is_err());
        assert!(credit_value(&(CREDIT_WINDOW + 1).to_be_bytes()).is_err());
        Ok(())
    }

    #[test]
    fn strict_control_vocabulary() {
        assert!(decode_json::<ClientControl>(r#"{"type":"exec","command":"ls"}"#).is_err());
        assert!(decode_json::<ClientControl>(r#"{"type":"ping","x":1}"#).is_err());
        assert!(decode_json::<ClientControl>(r#"{"type":"ping"}"#).is_ok());
        // A personal HostAction payload is not a hands proof, and vice versa.
        assert!(HandsProof::parse(r#"{"operation":"capabilities"}"#).is_err());
        let proof = HandsProof::OpenControl {
            host_id: "h".into(),
            challenge: "c".into(),
            client_nonce: "n".into(),
            namespace: "s".into(),
            controller_epoch: 1,
            predecessor_epoch: 0,
        };
        let text = serde_json::to_string(&proof).unwrap_or_default();
        assert!(serde_json::from_str::<oqto_personal_wire::actions::HostAction>(&text).is_err());
        assert_eq!(HandsProof::parse(&text).ok(), Some(proof));
    }
}

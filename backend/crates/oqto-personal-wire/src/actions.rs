//! Signed semantic operations for direct personal hosts. Paths are relative to
//! owner-selected root IDs; they are never authority or host filesystem roots.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMediaVariant {
    Thumb,
    Full,
}

pub const ACTION_PATH: &str = "/personal/v1/action";
pub const MAX_ACTION_BYTES: usize = 1024 * 1024;
pub const MAX_FILE_CHUNK: usize = 256 * 1024;
/// Per request: the phone asks for visible tiles in batches.
pub const MAX_PROJECT_ICON_DIRECTORIES: usize = 16;
/// Per returned logo; larger files are absent, never resized or cropped.
pub const MAX_PROJECT_ICON_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostAction {
    Capabilities,
    /// Ask for destination-owner consent; pairing grants no execution.
    HandsRequest {
        request_key: String,
        session_label: String,
        command: String,
    },
    HandsRun {
        id: String,
    },
    HandsStatus {
        id: String,
    },
    HandsCancel {
        id: String,
    },
    FilesList {
        root_id: String,
        path: String,
    },
    FileRead {
        root_id: String,
        path: String,
        offset: u64,
        limit: u32,
    },
    /// None means create-only; Some is the SHA-256 baseline, never force-write.
    FileWrite {
        root_id: String,
        path: String,
        data_base64: String,
        expected_sha256: Option<String>,
    },
    FileRemove {
        root_id: String,
        path: String,
        expected_sha256: String,
    },
    DirectoryCreate {
        root_id: String,
        path: String,
    },
    TerminalsList,
    /// Open or reattach this device's terminal for the exact authorized Session cwd.
    SessionTerminalOpen {
        session_id: String,
        rows: u16,
        cols: u16,
    },
    TerminalOpen {
        cwd: String,
        rows: u16,
        cols: u16,
    },
    TerminalRead {
        terminal_id: String,
        after: u64,
    },
    TerminalWrite {
        terminal_id: String,
        data_base64: String,
    },
    TerminalResize {
        terminal_id: String,
        rows: u16,
        cols: u16,
    },
    TerminalClose {
        terminal_id: String,
    },
    SessionsList,
    /// Host-resolved Session cwd inside an existing Files grant; no attachment.
    SessionFiles {
        session_id: String,
    },
    /// Raster project logos for catalog Work directories, at most
    /// `MAX_PROJECT_ICON_DIRECTORIES` per request. Read-only, Session-observe.
    ProjectIcons {
        directories: Vec<String>,
    },
    SessionMedia {
        session_id: String,
        sha256: String,
        variant: SessionMediaVariant,
    },
    SessionHistory {
        session_id: String,
        before: Option<String>,
    },
    SessionAttach {
        session_id: String,
    },
    SessionRead {
        attachment_id: String,
    },
    /// Read the attached Pi's filtered model catalog; never acquires input.
    SessionModels {
        attachment_id: String,
    },
    SessionRequestControl {
        attachment_id: String,
    },
    SessionReleaseControl {
        attachment_id: String,
    },
    SessionInput {
        attachment_id: String,
        input: SessionInput,
    },
    SessionDetach {
        attachment_id: String,
    },
    /// Speech the Host offers through its own local engines (eaRS, spqx). A separate
    /// action, not HostCapabilities fields: installed clients parse those strictly.
    VoiceCapabilities,
    /// Live dictation: PCM16 little-endian mono audio, appended in bounded chunks.
    VoiceTranscribeOpen {
        sample_rate: u32,
        language: Option<String>,
    },
    VoiceTranscribeAppend {
        transcription_id: String,
        /// Base64 PCM16 LE mono at the opened sample rate; at most MAX_VOICE_CHUNK bytes decoded.
        audio: String,
    },
    VoiceTranscribeFinish {
        transcription_id: String,
    },
    VoiceTranscribeCancel {
        transcription_id: String,
    },
    /// Speech output: text in, PCM16 LE mono out, read incrementally.
    VoiceSpeakOpen {
        text: String,
    },
    VoiceSpeakRead {
        stream_id: String,
    },
    VoiceSpeakClose {
        stream_id: String,
    },
}

/// Decoded audio bytes per VoiceTranscribeAppend (fits the action body limit as base64).
pub const MAX_VOICE_CHUNK: usize = 512 * 1024;
/// Characters of text per VoiceSpeakOpen.
pub const MAX_VOICE_TEXT: usize = 16 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct VoiceCapabilities {
    /// eaRS reachable on the Host.
    #[serde(default)]
    pub transcribe: bool,
    /// spqx available on the Host.
    #[serde(default)]
    pub speak: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionInput {
    Prompt {
        message: String,
    },
    Steer {
        message: String,
    },
    FollowUp {
        message: String,
    },
    SetModel {
        provider: String,
        model_id: String,
    },
    SetThinkingLevel {
        level: String,
    },
    Abort,
    /// The Session's display name, as Pi's `/name` (owner 2026-11-11: rename from the phone).
    SetSessionName {
        name: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RootInfo {
    pub id: String,
    pub label: String,
    pub writable: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostCapabilities {
    pub version: u8,
    pub host_id: String,
    pub device_id: String,
    pub roots: Vec<RootInfo>,
    pub session_observe_granted: bool,
    pub session_control_granted: bool,
    pub full_user_terminal_granted: bool,
    pub files_supported: bool,
    pub terminal_supported: bool,
    /// Never inferred from raw PTY support; older Hosts default to unavailable.
    #[serde(default)]
    pub session_terminal_supported: bool,
    pub session_observe_supported: bool,
    pub session_control_supported: bool,
}

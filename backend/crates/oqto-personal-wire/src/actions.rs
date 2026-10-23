//! Signed semantic operations for direct personal hosts. Paths are relative to
//! owner-selected root IDs; they are never authority or host filesystem roots.
use serde::{Deserialize, Serialize};

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
    /// Raster project logos for catalog Work directories, at most
    /// `MAX_PROJECT_ICON_DIRECTORIES` per request. Read-only, Session-observe.
    ProjectIcons {
        directories: Vec<String>,
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionInput {
    Prompt { message: String },
    Steer { message: String },
    Abort,
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
    pub session_observe_supported: bool,
    pub session_control_supported: bool,
}

//! Multiplexed WebSocket handler for unified real-time communication.
//!
//! Provides a single WebSocket connection per user that handles multiple channels:
//! - `pi` - Pi session commands and events
//! - `files` - File operations (future)
//! - `terminal` - Terminal I/O (future)
//! - `hstry` - History queries (future)
//! - `system` - System events (connection status, errors)

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use chrono::Utc;

use base64::Engine;

use crate::auth::{Claims, CurrentUser};
use crate::local::ProcessManager;
use crate::pi::{AssistantMessageEvent, PiEvent};
use crate::runner::client::{PiSubscription, PiSubscriptionEvent, RunnerClient};
use crate::runner::protocol::{PiCreateSessionRequest, PiSessionConfig as RunnerPiSessionConfig};
use crate::session::Session;
use crate::user_plane::{DirectUserPlane, RunnerUserPlane};
use crate::ws::hub::WsHub;
use crate::ws::types::{WsCommand as LegacyWsCommand, WsEvent as LegacyWsEvent};

use super::error::ApiError;

const PI_MESSAGES_CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const PI_MESSAGES_CACHE_MAX_BYTES_PER_USER: usize = 100 * 1024 * 1024;
const PI_MESSAGES_CACHE_MAX_MESSAGES_PER_SESSION: usize = 200;

struct CachedPiMessages {
    cached_at: Instant,
    last_access: Instant,
    messages: Value,
    size_bytes: usize,
}

struct CachedPiUserMessages {
    total_bytes: usize,
    entries: HashMap<String, CachedPiMessages>,
}

static PI_MESSAGES_CACHE: Lazy<tokio::sync::RwLock<HashMap<String, CachedPiUserMessages>>> =
    Lazy::new(|| tokio::sync::RwLock::new(HashMap::new()));

fn trim_messages_for_cache(messages: &Value) -> Value {
    match messages {
        Value::Array(items) => {
            if items.len() <= PI_MESSAGES_CACHE_MAX_MESSAGES_PER_SESSION {
                Value::Array(items.clone())
            } else {
                let start = items.len() - PI_MESSAGES_CACHE_MAX_MESSAGES_PER_SESSION;
                Value::Array(items[start..].to_vec())
            }
        }
        _ => messages.clone(),
    }
}

fn estimate_messages_size(messages: &Value) -> usize {
    serde_json::to_string(messages).map(|s| s.len()).unwrap_or(0)
}

async fn cache_pi_messages(user_id: &str, session_id: &str, messages: &Value) {
    let trimmed = trim_messages_for_cache(messages);
    let size_bytes = estimate_messages_size(&trimmed);
    let now = Instant::now();
    let mut cache = PI_MESSAGES_CACHE.write().await;
    let user_cache = cache
        .entry(user_id.to_string())
        .or_insert_with(|| CachedPiUserMessages {
            total_bytes: 0,
            entries: HashMap::new(),
        });

    if let Some(existing) = user_cache.entries.remove(session_id) {
        user_cache.total_bytes = user_cache.total_bytes.saturating_sub(existing.size_bytes);
    }

    user_cache.total_bytes = user_cache.total_bytes.saturating_add(size_bytes);
    user_cache.entries.insert(
        session_id.to_string(),
        CachedPiMessages {
            cached_at: now,
            last_access: now,
            messages: trimmed,
            size_bytes,
        },
    );

    while user_cache.total_bytes > PI_MESSAGES_CACHE_MAX_BYTES_PER_USER {
        if let Some((oldest_key, oldest_entry)) = user_cache
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_access)
            .map(|(k, v)| (k.clone(), v.size_bytes))
        {
            user_cache.entries.remove(&oldest_key);
            user_cache.total_bytes = user_cache.total_bytes.saturating_sub(oldest_entry);
        } else {
            break;
        }
    }
}

struct CachedPiMessagesSnapshot {
    messages: Value,
    age: Duration,
}

async fn get_cached_pi_messages(user_id: &str, session_id: &str) -> Option<CachedPiMessagesSnapshot> {
    let mut cache = PI_MESSAGES_CACHE.write().await;
    let user_cache = cache.get_mut(user_id)?;
    if let Some(entry) = user_cache.entries.get_mut(session_id) {
        let age = entry.cached_at.elapsed();
        if age <= PI_MESSAGES_CACHE_TTL {
            entry.last_access = Instant::now();
            return Some(CachedPiMessagesSnapshot {
                messages: entry.messages.clone(),
                age,
            });
        }
        let size = entry.size_bytes;
        user_cache.entries.remove(session_id);
        user_cache.total_bytes = user_cache.total_bytes.saturating_sub(size);
    }
    None
}
use super::handlers::trx::{
    CloseTrxIssueRequest, CreateTrxIssueRequest, TrxWorkspaceQuery, UpdateTrxIssueRequest,
};
use super::state::AppState;

// ============================================================================
// Channel Types
// ============================================================================

/// Channels supported by the multiplexed WebSocket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Pi,
    Files,
    Terminal,
    Hstry,
    Trx,
    Session,
    System,
}

// ============================================================================
// Incoming Commands (Frontend -> Backend)
// ============================================================================

/// Commands sent from frontend to backend over WebSocket.
#[derive(Debug, Deserialize)]
#[serde(tag = "channel", rename_all = "snake_case")]
pub enum WsCommand {
    Pi(PiWsCommand),
    Files(FilesWsCommand),
    Terminal(TerminalWsCommand),
    Hstry(HstryWsCommand),
    Trx(TrxWsCommand),
    Session(SessionWsCommand),
}

/// Pi channel commands.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PiWsCommand {
    // === Session Lifecycle ===
    /// Create or resume a Pi session
    CreateSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        #[serde(default)]
        config: Option<PiSessionConfig>,
    },
    /// Close session (stop Pi process)
    CloseSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },
    /// Start a new session within existing Pi process
    NewSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        #[serde(default)]
        parent_session: Option<String>,
    },
    /// Switch to a different session file
    SwitchSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        session_path: String,
    },
    /// List all sessions
    ListSessions {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// Start receiving events for session
    Subscribe {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },
    /// Stop receiving events for session
    Unsubscribe {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },

    // === Prompting ===
    /// Send prompt to session
    Prompt {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        message: String,
    },
    /// Steering message (interrupt mid-run)
    Steer {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        message: String,
    },
    /// Follow-up message (queue for after completion)
    FollowUp {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        message: String,
    },
    /// Abort current operation
    Abort {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },

    // === State & Messages ===
    /// Get session state
    GetState {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },
    /// Get all messages from session
    GetMessages {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },
    /// Get session statistics
    GetSessionStats {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },
    /// Get last assistant response text
    GetLastAssistantText {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },

    // === Model Management ===
    /// Set the model for a session
    SetModel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        provider: String,
        model_id: String,
    },
    /// Cycle to next model
    CycleModel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },
    /// Get available models
    GetAvailableModels {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },

    // === Thinking Level ===
    /// Set thinking/reasoning level
    SetThinkingLevel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        level: String,
    },
    /// Cycle through thinking levels
    CycleThinkingLevel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },

    // === Compaction ===
    /// Compact conversation
    Compact {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        #[serde(default)]
        instructions: Option<String>,
    },
    /// Enable/disable auto-compaction
    SetAutoCompaction {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        enabled: bool,
    },

    // === Queue Modes ===
    /// Set steering message delivery mode
    SetSteeringMode {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        mode: String, // "all" | "one-at-a-time"
    },
    /// Set follow-up message delivery mode
    SetFollowUpMode {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        mode: String, // "all" | "one-at-a-time"
    },

    // === Retry ===
    /// Enable/disable auto-retry
    SetAutoRetry {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        enabled: bool,
    },
    /// Abort in-progress retry
    AbortRetry {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },

    // === Forking ===
    /// Fork from a previous message
    Fork {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        entry_id: String,
    },
    /// Get messages available for forking
    GetForkMessages {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },

    // === Session Metadata ===
    /// Set session display name
    SetSessionName {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        name: String,
    },
    /// Export session to HTML
    ExportHtml {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        #[serde(default)]
        output_path: Option<String>,
    },

    // === Commands/Skills ===
    /// Get available commands
    GetCommands {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },

    // === Bash ===
    /// Execute bash command
    Bash {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        command: String,
    },
    /// Abort running bash command
    AbortBash {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },

    // === Extension UI ===
    /// Send response to extension UI request
    ExtensionUiResponse {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        request_id: String,
        #[serde(default)]
        value: Option<String>,
        #[serde(default)]
        confirmed: Option<bool>,
        #[serde(default)]
        cancelled: Option<bool>,
    },
}

/// Pi session configuration for create_session command.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PiSessionConfig {
    /// Session scope: "main" for main chat, "workspace" for workspace sessions.
    /// When "main", the backend will:
    /// - Use the main chat directory as cwd
    /// - Add PERSONALITY.md, USER.md, ONBOARD.md as system_prompt_files
    #[serde(default)]
    pub scope: Option<String>,
    /// Working directory for Pi (ignored if scope="main")
    #[serde(default)]
    pub cwd: Option<String>,
    /// Provider (anthropic, openai, etc.)
    #[serde(default)]
    pub provider: Option<String>,
    /// Model ID
    #[serde(default)]
    pub model: Option<String>,
    /// Explicit session file to use
    #[serde(default)]
    pub session_file: Option<String>,
    /// Session file to continue from
    #[serde(default)]
    pub continue_session: Option<String>,
}

/// Files channel commands (placeholder for future implementation).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilesWsCommand {
    Tree {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        #[serde(default)]
        depth: Option<usize>,
        #[serde(default)]
        include_hidden: bool,
        #[serde(default)]
        workspace_path: Option<String>,
    },
    Read {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        #[serde(default)]
        workspace_path: Option<String>,
    },
    Write {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        content: String,
        #[serde(default)]
        create_parents: bool,
        #[serde(default)]
        workspace_path: Option<String>,
    },
    List {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        #[serde(default)]
        include_hidden: bool,
        #[serde(default)]
        workspace_path: Option<String>,
    },
    Stat {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        #[serde(default)]
        workspace_path: Option<String>,
    },
    Delete {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        #[serde(default)]
        recursive: bool,
        #[serde(default)]
        workspace_path: Option<String>,
    },
    CreateDirectory {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        #[serde(default)]
        create_parents: bool,
        #[serde(default)]
        workspace_path: Option<String>,
    },
    Rename {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        from: String,
        to: String,
        #[serde(default)]
        workspace_path: Option<String>,
    },
    Copy {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        from: String,
        to: String,
        #[serde(default)]
        overwrite: bool,
        #[serde(default)]
        workspace_path: Option<String>,
    },
    Move {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        from: String,
        to: String,
        #[serde(default)]
        overwrite: bool,
        #[serde(default)]
        workspace_path: Option<String>,
    },
}

/// Terminal channel commands (placeholder for future implementation).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalWsCommand {
    Open {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        terminal_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        workspace_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        cols: u16,
        rows: u16,
    },
    Input {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        terminal_id: String,
        data: String,
    },
    Resize {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        terminal_id: String,
        cols: u16,
        rows: u16,
    },
    Close {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        terminal_id: String,
    },
}

/// History channel commands (placeholder for future implementation).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HstryWsCommand {
    Query {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: Option<String>,
        query: Option<String>,
        limit: Option<u32>,
    },
}

/// Session channel commands (legacy WS protocol).
#[derive(Debug, Deserialize)]
pub struct SessionWsCommand {
    #[serde(flatten)]
    cmd: LegacyWsCommand,
}

/// TRX channel commands.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrxWsCommand {
    List {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        workspace_path: String,
    },
    Create {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        workspace_path: String,
        data: TrxIssueInput,
    },
    Update {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        workspace_path: String,
        issue_id: String,
        data: TrxIssueUpdate,
    },
    Close {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        workspace_path: String,
        issue_id: String,
        #[serde(default)]
        reason: Option<String>,
    },
    Sync {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        workspace_path: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct TrxIssueInput {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TrxIssueUpdate {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
}

// ============================================================================
// Outgoing Events (Backend -> Frontend)
// ============================================================================

/// Events sent from backend to frontend over WebSocket.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "channel", rename_all = "snake_case")]
pub enum WsEvent {
    Pi(PiWsEvent),
    Files(FilesWsEvent),
    Terminal(TerminalWsEvent),
    Hstry(HstryWsEvent),
    Trx(TrxWsEvent),
    Session(LegacyWsEvent),
    System(SystemWsEvent),
}

/// Pi channel events.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PiWsEvent {
    /// Session created/resumed
    SessionCreated {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },
    /// Session closed
    SessionClosed {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
    },
    /// Session list
    Sessions {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        sessions: Vec<PiSessionInfo>,
    },
    /// Session state
    State {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        state: Value,
    },
    /// Message start (streaming)
    MessageStart { session_id: String, role: String },
    /// Text delta (streaming)
    Text { session_id: String, data: String },
    /// Thinking delta (streaming)
    Thinking { session_id: String, data: String },
    /// Tool use event
    ToolUse {
        session_id: String,
        data: ToolUseData,
    },
    /// Tool start event
    ToolStart {
        session_id: String,
        data: ToolUseData,
    },
    /// Tool result event
    ToolResult {
        session_id: String,
        data: ToolResultData,
    },
    /// Stream complete
    Done { session_id: String },
    /// Error event
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        error: String,
    },
    /// Persistence confirmation
    Persisted {
        session_id: String,
        message_count: u64,
    },
    /// Command acknowledgement
    CommandAck {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        command: String,
    },
    /// Messages response
    Messages {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        messages: Value,
    },
    /// Stats response
    Stats {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        stats: Value,
    },
    /// Last assistant text response
    LastAssistantText {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        text: Option<String>,
    },
    /// Model changed response
    ModelChanged {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        provider: String,
        model_id: String,
    },
    /// Available models response
    AvailableModels {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        models: Value,
    },
    /// Thinking level changed response
    ThinkingLevelChanged {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        level: String,
    },
    /// Fork messages response
    ForkMessages {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        messages: Vec<ForkMessageInfo>,
    },
    /// Fork result response
    ForkResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        /// The text of the message being forked from.
        text: String,
        /// Whether an extension cancelled the fork.
        cancelled: bool,
    },
    /// Commands response
    Commands {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        commands: Vec<CommandInfo>,
    },
    /// Bash result response
    BashResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        /// Command output.
        output: String,
        /// Exit code.
        exit_code: i32,
        /// Whether the command was cancelled.
        cancelled: bool,
        /// Whether output was truncated.
        truncated: bool,
        /// Path to full output if truncated.
        #[serde(skip_serializing_if = "Option::is_none")]
        full_output_path: Option<String>,
    },
    /// Export HTML result response
    ExportHtmlResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        session_id: String,
        path: String,
    },
}

/// Session info for list response.
#[derive(Debug, Clone, Serialize)]
pub struct PiSessionInfo {
    pub session_id: String,
    pub state: String,
    pub last_activity: i64,
    pub subscriber_count: usize,
}

/// Tool use event data.
#[derive(Debug, Clone, Serialize)]
pub struct ToolUseData {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// Tool result event data.
#[derive(Debug, Clone, Serialize)]
pub struct ToolResultData {
    pub id: String,
    pub name: Option<String>,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Fork message info.
#[derive(Debug, Clone, Serialize)]
pub struct ForkMessageInfo {
    pub entry_id: String,
    pub role: String,
    pub preview: String,
    pub timestamp: Option<i64>,
}

/// Command info.
#[derive(Debug, Clone, Serialize)]
pub struct CommandInfo {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub command_type: String,
}

/// Files channel events (placeholder).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilesWsEvent {
    TreeResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        entries: Vec<FileTreeNode>,
    },
    ReadResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        truncated: Option<bool>,
    },
    WriteResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        success: bool,
    },
    ListResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        entries: Vec<crate::user_plane::DirEntry>,
    },
    StatResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        stat: Value,
    },
    DeleteResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        success: bool,
    },
    CreateDirectoryResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        path: String,
        success: bool,
    },
    RenameResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        from: String,
        to: String,
        success: bool,
    },
    CopyResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        from: String,
        to: String,
        success: bool,
    },
    MoveResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        from: String,
        to: String,
        success: bool,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        error: String,
    },
}

/// Terminal channel events.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalWsEvent {
    Opened {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        terminal_id: String,
    },
    Output {
        terminal_id: String,
        data_base64: String,
    },
    Exit {
        terminal_id: String,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        terminal_id: Option<String>,
        error: String,
    },
}

/// File tree node for tree responses.
#[derive(Debug, Clone, Serialize)]
pub struct FileTreeNode {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileTreeNode>>,
}

/// Hstry channel events.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HstryWsEvent {
    Result {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        data: Value,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        error: String,
    },
}

/// TRX channel events.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrxWsEvent {
    ListResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        issues: Value,
    },
    IssueResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        issue: Value,
    },
    SyncResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        success: bool,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        error: String,
    },
}

/// System channel events.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SystemWsEvent {
    /// Connection established
    Connected,
    /// General error not tied to specific channel
    Error { error: String },
    /// Ping for keep-alive
    Ping,
}

// ============================================================================
// Query Parameters
// ============================================================================

/// Query parameters for the multiplexed WebSocket endpoint.
#[derive(Debug, Deserialize)]
pub struct WsMultiplexedQuery {
    /// Optional authentication token (for WebSocket auth)
    #[serde(default)]
    pub token: Option<String>,
}

// ============================================================================
// WebSocket Handler
// ============================================================================

/// WebSocket endpoint for multiplexed communication.
///
/// GET /api/ws/mux
pub async fn ws_multiplexed_handler(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(_query): Query<WsMultiplexedQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    info!(
        "Multiplexed WebSocket connection request from user {}",
        user.id()
    );

    let user_id = user.id().to_string();

    Ok(ws.on_upgrade(move |socket| handle_multiplexed_ws(socket, state, user_id)))
}

/// Create a runner client for a user if multi-user mode is enabled.
fn runner_client_for_user(state: &AppState, user_id: &str) -> Option<RunnerClient> {
    // Check if we have a socket pattern configured
    if let Some(pattern) = state.runner_socket_pattern.as_deref() {
        // Get linux username from user_id if linux_users config exists
        let linux_username = state
            .linux_users
            .as_ref()
            .map(|lu| lu.linux_username(user_id))
            .unwrap_or_else(|| user_id.to_string());

        // Use for_user_with_pattern which handles both {user} and {uid} placeholders
        match RunnerClient::for_user_with_pattern(&linux_username, pattern) {
            Ok(c) if c.socket_path().exists() => return Some(c),
            Ok(_) => {}
            Err(e) => {
                warn!("Failed to create runner client for user {}: {}", user_id, e);
            }
        }
    }

    // Fallback: try default socket path for single-user mode
    let default_client = RunnerClient::default();
    if default_client.socket_path().exists() {
        return Some(default_client);
    }

    None
}

/// State for a WebSocket connection, shared between command handler and event forwarder.
struct WsConnectionState {
    /// Subscribed Pi session IDs.
    subscribed_sessions: HashSet<String>,
    /// Channel for sending events to the WebSocket writer.
    event_tx: mpsc::UnboundedSender<WsEvent>,
    /// Active Pi subscriptions (keyed by session_id).
    pi_subscriptions: HashSet<String>,
    /// Metadata for Pi sessions created via this connection.
    pi_session_meta: HashMap<String, PiSessionMeta>,
    /// Active terminal sessions keyed by terminal_id.
    terminal_sessions: HashMap<String, TerminalSession>,
}

#[derive(Clone, Debug)]
struct PiSessionMeta {
    scope: Option<String>,
    cwd: Option<std::path::PathBuf>,
}

struct TerminalSession {
    command_tx: mpsc::UnboundedSender<TerminalSessionCommand>,
    task: tokio::task::JoinHandle<()>,
}

enum TerminalSessionCommand {
    Input(String),
    Resize { cols: u16, rows: u16 },
    Close,
}

/// Handle the multiplexed WebSocket connection.
async fn handle_multiplexed_ws(socket: WebSocket, state: AppState, user_id: String) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Create channel for forwarding events to WebSocket
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<WsEvent>();

    // Send connected event
    let connected_event = WsEvent::System(SystemWsEvent::Connected);
    if let Ok(json) = serde_json::to_string(&connected_event) {
        if ws_sender.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

    info!("Multiplexed WebSocket connected for user {}", user_id);

    // Create connection state
    let conn_state = Arc::new(tokio::sync::Mutex::new(WsConnectionState {
        subscribed_sessions: HashSet::new(),
        event_tx: event_tx.clone(),
        pi_subscriptions: HashSet::new(),
        pi_session_meta: HashMap::new(),
        terminal_sessions: HashMap::new(),
    }));

    // Register this connection with the legacy WS hub for session events.
    let hub: Arc<WsHub> = state.ws_hub.clone();
    let (mut hub_rx, hub_conn_id) = hub.register_connection(&user_id);
    let mut hub_events = hub.subscribe_events();
    let hub_user_id = user_id.clone();
    let hub_for_events = hub.clone();
    let event_tx_for_hub = event_tx.clone();
    tokio::spawn(async move {
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            tokio::select! {
                Some(event) = hub_rx.recv() => {
                    let _ = event_tx_for_hub.send(WsEvent::Session(event));
                }
                Ok((session_id, event)) = hub_events.recv() => {
                    if hub_for_events.is_subscribed(&hub_user_id, &session_id) {
                        let _ = event_tx_for_hub.send(WsEvent::Session(event));
                    }
                }
                _ = ping_interval.tick() => {
                    let _ = event_tx_for_hub.send(WsEvent::Session(LegacyWsEvent::Ping));
                }
            }
        }
    });

    // Emit legacy connected event for session channel.
    let _ = event_tx.send(WsEvent::Session(LegacyWsEvent::Connected));

    // Get runner client for this user
    let runner_client = runner_client_for_user(&state, &user_id);
    if runner_client.is_none() {
        debug!(
            "No runner client available for user {}, Pi commands will fail",
            user_id
        );
    }

    // Spawn task to forward events from channel to WebSocket
    let event_writer = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event) {
                if ws_sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming messages
    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsCommand>(&text) {
                            Ok(cmd) => {
                                info!("Received WS command: {:?}", cmd);

                                let response = handle_ws_command(
                                    cmd,
                                    &user_id,
                                    &state,
                                    runner_client.as_ref(),
                                    conn_state.clone(),
                                )
                                .await;

                                if let Some(event) = response {
                                    info!("Sending WS event to client: {:?}", event);
                                    let _ = event_tx.send(event);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse WS command: {}", e);
                                let _ = event_tx.send(WsEvent::System(SystemWsEvent::Error {
                                    error: format!("Invalid command: {}", e),
                                }));
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        // Pong is handled automatically by axum's WebSocket
                        let _ = data;
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Err(e)) => {
                        warn!("WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
        }
    }

    // Cleanup
    event_writer.abort();

    // Close terminal sessions for this connection.
    {
        let mut state_guard = conn_state.lock().await;
        for (_, session) in state_guard.terminal_sessions.drain() {
            let _ = session.command_tx.send(TerminalSessionCommand::Close);
            session.task.abort();
        }
    }

    // Unsubscribe user from all legacy session subscriptions.
    for session_id in hub.user_subscriptions(&user_id) {
        hub.unsubscribe_session(&user_id, &session_id);
    }
    hub.unregister_connection(&user_id, hub_conn_id);

    info!("Multiplexed WebSocket closed for user {}", user_id);
}

/// Handle a WebSocket command and return an optional response event.
async fn handle_ws_command(
    cmd: WsCommand,
    user_id: &str,
    state: &AppState,
    runner_client: Option<&RunnerClient>,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    match cmd {
        WsCommand::Pi(pi_cmd) => {
            handle_pi_command(pi_cmd, user_id, state, runner_client, conn_state).await
        }
        WsCommand::Files(files_cmd) => handle_files_command(files_cmd, user_id, state).await,
        WsCommand::Terminal(term_cmd) => {
            handle_terminal_command(term_cmd, user_id, state, conn_state).await
        }
        WsCommand::Hstry(hstry_cmd) => handle_hstry_command(hstry_cmd, state).await,
        WsCommand::Trx(trx_cmd) => handle_trx_command(trx_cmd, user_id, state).await,
        WsCommand::Session(session_cmd) => {
            handle_session_command(session_cmd, user_id, state).await
        }
    }
}

/// Helper to extract id and session_id from a Pi command for error responses.
fn extract_pi_command_ids(cmd: &PiWsCommand) -> (Option<String>, String) {
    match cmd {
        // Session lifecycle
        PiWsCommand::CreateSession { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::CloseSession { id, session_id } => (id.clone(), session_id.clone()),
        PiWsCommand::NewSession { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::SwitchSession { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::ListSessions { id } => (id.clone(), "".to_string()),
        PiWsCommand::Subscribe { id, session_id } => (id.clone(), session_id.clone()),
        PiWsCommand::Unsubscribe { id, session_id } => (id.clone(), session_id.clone()),
        // Prompting
        PiWsCommand::Prompt { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::Steer { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::FollowUp { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::Abort { id, session_id } => (id.clone(), session_id.clone()),
        // State & Messages
        PiWsCommand::GetState { id, session_id } => (id.clone(), session_id.clone()),
        PiWsCommand::GetMessages { id, session_id } => (id.clone(), session_id.clone()),
        PiWsCommand::GetSessionStats { id, session_id } => (id.clone(), session_id.clone()),
        PiWsCommand::GetLastAssistantText { id, session_id } => (id.clone(), session_id.clone()),
        // Model
        PiWsCommand::SetModel { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::CycleModel { id, session_id } => (id.clone(), session_id.clone()),
        PiWsCommand::GetAvailableModels { id, session_id } => (id.clone(), session_id.clone()),
        // Thinking
        PiWsCommand::SetThinkingLevel { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::CycleThinkingLevel { id, session_id } => (id.clone(), session_id.clone()),
        // Compaction
        PiWsCommand::Compact { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::SetAutoCompaction { id, session_id, .. } => (id.clone(), session_id.clone()),
        // Queue modes
        PiWsCommand::SetSteeringMode { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::SetFollowUpMode { id, session_id, .. } => (id.clone(), session_id.clone()),
        // Retry
        PiWsCommand::SetAutoRetry { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::AbortRetry { id, session_id } => (id.clone(), session_id.clone()),
        // Forking
        PiWsCommand::Fork { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::GetForkMessages { id, session_id } => (id.clone(), session_id.clone()),
        // Metadata
        PiWsCommand::SetSessionName { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::ExportHtml { id, session_id, .. } => (id.clone(), session_id.clone()),
        // Commands
        PiWsCommand::GetCommands { id, session_id } => (id.clone(), session_id.clone()),
        // Bash
        PiWsCommand::Bash { id, session_id, .. } => (id.clone(), session_id.clone()),
        PiWsCommand::AbortBash { id, session_id } => (id.clone(), session_id.clone()),
        // Extension UI
        PiWsCommand::ExtensionUiResponse { id, session_id, .. } => (id.clone(), session_id.clone()),
    }
}

/// Handle Pi channel commands.
async fn handle_pi_command(
    cmd: PiWsCommand,
    user_id: &str,
    state: &AppState,
    runner_client: Option<&RunnerClient>,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    // Special case: ListSessions doesn't need a session_id
    if matches!(cmd, PiWsCommand::ListSessions { .. }) {
        if let Some(runner) = runner_client {
            match runner.pi_list_sessions().await {
                Ok(sessions) => {
                    let session_infos: Vec<PiSessionInfo> = sessions
                        .into_iter()
                        .map(|s| PiSessionInfo {
                            session_id: s.session_id,
                            state: format!("{:?}", s.state),
                            last_activity: s.last_activity,
                            subscriber_count: s.subscriber_count,
                        })
                        .collect();
                    let id = if let PiWsCommand::ListSessions { id } = &cmd {
                        id.clone()
                    } else {
                        None
                    };
                    return Some(WsEvent::Pi(PiWsEvent::Sessions {
                        id,
                        sessions: session_infos,
                    }));
                }
                Err(e) => {
                    error!("Failed to list Pi sessions: {:?}", e);
                    let id = if let PiWsCommand::ListSessions { id } = &cmd {
                        id.clone()
                    } else {
                        None
                    };
                    return Some(WsEvent::Pi(PiWsEvent::Sessions {
                        id,
                        sessions: vec![],
                    }));
                }
            }
        } else {
            let id = if let PiWsCommand::ListSessions { id } = &cmd {
                id.clone()
            } else {
                None
            };
            return Some(WsEvent::Pi(PiWsEvent::Sessions {
                id,
                sessions: vec![],
            }));
        }
    }

    // Check if runner is available
    let runner = match runner_client {
        Some(r) => r,
        None => {
            let (id, session_id) = extract_pi_command_ids(&cmd);
            return Some(WsEvent::Pi(PiWsEvent::Error {
                id,
                session_id,
                error: "Runner not available".into(),
            }));
        }
    };

    match cmd {
        PiWsCommand::CreateSession {
            id,
            session_id,
            config,
        } => {
            info!(
                "Pi create_session: user={}, session_id={}",
                user_id, session_id
            );

            let cwd = config
                .as_ref()
                .and_then(|c| c.cwd.as_ref())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("/"));

            // Build system prompt files list from cwd
            let mut system_prompt_files = Vec::new();
            let onboard_file = cwd.join("ONBOARD.md");
            if onboard_file.exists() {
                system_prompt_files.push(onboard_file);
            }
            let personality_file = cwd.join("PERSONALITY.md");
            if personality_file.exists() {
                system_prompt_files.push(personality_file);
            }
            let user_file = cwd.join("USER.md");
            if user_file.exists() {
                system_prompt_files.push(user_file);
            }

            {
                let mut state_guard = conn_state.lock().await;
                state_guard.pi_session_meta.insert(
                    session_id.clone(),
                    PiSessionMeta {
                        scope: config
                            .as_ref()
                            .and_then(|c| c.scope.as_ref())
                            .cloned()
                            .or_else(|| Some("workspace".to_string())),
                        cwd: Some(cwd.clone()),
                    },
                );
            }

            // Resolve continue_session: use explicit path if provided, otherwise
            // ensure a session file exists for this session ID.
            let session_file = if let Some(path) = config
                .as_ref()
                .and_then(|c| c.session_file.as_ref())
            {
                Some(std::path::PathBuf::from(path))
            } else if let Some(path) = config
                .as_ref()
                .and_then(|c| c.continue_session.as_ref())
            {
                Some(std::path::PathBuf::from(path))
            } else {
                let home_dir = if let Some(linux_users) = state.linux_users.as_ref() {
                    linux_users
                        .get_home_dir(user_id)
                        .ok()
                        .flatten()
                } else {
                    dirs::home_dir()
                };

                let sessions_dir = home_dir.map(|home| {
                    let safe_path = cwd
                        .to_string_lossy()
                        .trim_start_matches(&['/', '\\'][..])
                        .replace('/', "-")
                        .replace('\\', "-")
                        .replace(':', "-");
                    home.join(".pi")
                        .join("agent")
                        .join("sessions")
                        .join(format!("--{}--", safe_path))
                });

                let existing = sessions_dir.as_ref().and_then(|dir| {
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().map(|e| e == "jsonl").unwrap_or(false)
                                && path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .map(|name| name.contains(&session_id))
                                    .unwrap_or(false)
                            {
                                return Some(path);
                            }
                        }
                    }
                    None
                });

                if let Some(path) = existing {
                    Some(path)
                } else if let Some(dir) = sessions_dir {
                    if let Err(err) = runner.create_directory(&dir, true).await {
                        error!("Failed to create Pi sessions dir {:?}: {}", dir, err);
                        return Some(WsEvent::Pi(PiWsEvent::Error {
                            id,
                            session_id,
                            error: format!("Failed to create session dir: {}", err),
                        }));
                    }
                    let header = serde_json::json!({
                        "type": "session",
                        "version": 3,
                        "id": session_id,
                        "timestamp": Utc::now().to_rfc3339(),
                        "cwd": cwd.to_string_lossy(),
                    });
                    let content = format!(
                        "{}\n",
                        serde_json::to_string(&header).unwrap_or_else(|_| "{}".to_string())
                    );
                    let filename =
                        format!("{}_{}.jsonl", Utc::now().timestamp_millis(), session_id);
                    let path = dir.join(filename);

                    if let Err(err) = runner.write_file(&path, content.as_bytes(), true).await {
                        error!("Failed to seed Pi session file {:?}: {}", path, err);
                        return Some(WsEvent::Pi(PiWsEvent::Error {
                            id,
                            session_id,
                            error: format!("Failed to seed session file: {}", err),
                        }));
                    }

                    Some(path)
                } else {
                    None
                }
            };

            let pi_config = RunnerPiSessionConfig {
                cwd,
                provider: config.as_ref().and_then(|c| c.provider.clone()),
                model: config.as_ref().and_then(|c| c.model.clone()),
                session_file: session_file.clone(),
                continue_session: session_file,
                system_prompt_files,
                env: std::collections::HashMap::new(),
            };

            let req = PiCreateSessionRequest {
                session_id: session_id.clone(),
                config: pi_config,
            };

            match runner.pi_create_session(req).await {
                Ok(_) => Some(WsEvent::Pi(PiWsEvent::SessionCreated { id, session_id })),
                Err(e) => {
                    error!("Failed to create Pi session: {:?}", e);
                    Some(WsEvent::Pi(PiWsEvent::Error {
                        id,
                        session_id,
                        error: format!("Failed to create session: {}", e),
                    }))
                }
            }
        }

        PiWsCommand::Prompt {
            id,
            session_id,
            message,
        } => {
            info!(
                "Pi prompt: user={}, session_id={}, message_len={}",
                user_id,
                session_id,
                message.len()
            );

            match runner.pi_prompt(&session_id, &message).await {
                Ok(()) => None, // Events will stream via subscription
                Err(e) => {
                    error!("Failed to send Pi prompt: {:?}", e);
                    Some(WsEvent::Pi(PiWsEvent::Error {
                        id,
                        session_id,
                        error: format!("Failed to send prompt: {}", e),
                    }))
                }
            }
        }

        PiWsCommand::Steer {
            id,
            session_id,
            message,
        } => {
            info!(
                "Pi steer: user={}, session_id={}, message_len={}",
                user_id,
                session_id,
                message.len()
            );

            match runner.pi_steer(&session_id, &message).await {
                Ok(()) => None,
                Err(e) => {
                    error!("Failed to send Pi steer: {:?}", e);
                    Some(WsEvent::Pi(PiWsEvent::Error {
                        id,
                        session_id,
                        error: format!("Failed to send steer: {}", e),
                    }))
                }
            }
        }

        PiWsCommand::FollowUp {
            id,
            session_id,
            message,
        } => {
            info!(
                "Pi follow_up: user={}, session_id={}, message_len={}",
                user_id,
                session_id,
                message.len()
            );

            match runner.pi_follow_up(&session_id, &message).await {
                Ok(()) => None,
                Err(e) => {
                    error!("Failed to send Pi follow_up: {:?}", e);
                    Some(WsEvent::Pi(PiWsEvent::Error {
                        id,
                        session_id,
                        error: format!("Failed to send follow_up: {}", e),
                    }))
                }
            }
        }

        PiWsCommand::Abort { id, session_id } => {
            info!("Pi abort: user={}, session_id={}", user_id, session_id);

            match runner.pi_abort(&session_id).await {
                Ok(()) => None,
                Err(e) => {
                    error!("Failed to abort Pi session: {:?}", e);
                    Some(WsEvent::Pi(PiWsEvent::Error {
                        id,
                        session_id,
                        error: format!("Failed to abort: {}", e),
                    }))
                }
            }
        }

        PiWsCommand::Compact {
            id,
            session_id,
            instructions,
        } => {
            info!("Pi compact: user={}, session_id={}", user_id, session_id);

            match runner.pi_compact(&session_id, instructions.as_deref()).await {
                Ok(()) => None,
                Err(e) => {
                    error!("Failed to compact Pi session: {:?}", e);
                    Some(WsEvent::Pi(PiWsEvent::Error {
                        id,
                        session_id,
                        error: format!("Failed to compact: {}", e),
                    }))
                }
            }
        }

        PiWsCommand::Subscribe { id: _, session_id } => {
            info!("Pi subscribe: user={}, session_id={}", user_id, session_id);

            let mut state = conn_state.lock().await;
            state.subscribed_sessions.insert(session_id.clone());

            // Start subscription to runner if not already subscribed
            if !state.pi_subscriptions.contains(&session_id) {
                state.pi_subscriptions.insert(session_id.clone());
                let event_tx = state.event_tx.clone();
                let runner = runner.clone();
                let sid = session_id.clone();

                // Spawn task to forward Pi events from runner to WebSocket
                tokio::spawn(async move {
                    if let Err(e) = forward_pi_events(&runner, &sid, event_tx).await {
                        error!("Pi event forwarding error for session {}: {:?}", sid, e);
                    }
                });
            }

            None
        }

        PiWsCommand::Unsubscribe { id: _, session_id } => {
            info!(
                "Pi unsubscribe: user={}, session_id={}",
                user_id, session_id
            );
            let mut state = conn_state.lock().await;
            state.subscribed_sessions.remove(&session_id);
            // Note: We don't stop the subscription task - it will clean up when runner disconnects
            None
        }

        PiWsCommand::ListSessions { id } => {
            info!("Pi list_sessions: user={}", user_id);

            match runner.pi_list_sessions().await {
                Ok(sessions) => {
                    let session_infos: Vec<PiSessionInfo> = sessions
                        .into_iter()
                        .map(|s| PiSessionInfo {
                            session_id: s.session_id,
                            state: format!("{:?}", s.state),
                            last_activity: s.last_activity,
                            subscriber_count: s.subscriber_count,
                        })
                        .collect();
                    Some(WsEvent::Pi(PiWsEvent::Sessions {
                        id,
                        sessions: session_infos,
                    }))
                }
                Err(e) => {
                    error!("Failed to list Pi sessions: {:?}", e);
                    Some(WsEvent::Pi(PiWsEvent::Sessions {
                        id,
                        sessions: vec![],
                    }))
                }
            }
        }

        PiWsCommand::GetState { id, session_id } => {
            info!("Pi get_state: user={}, session_id={}", user_id, session_id);

            match runner.pi_get_state(&session_id).await {
                Ok(resp) => {
                    let state_value = serde_json::to_value(&resp.state).unwrap_or(Value::Null);
                    Some(WsEvent::Pi(PiWsEvent::State {
                        id,
                        session_id,
                        state: state_value,
                    }))
                }
                Err(e) => {
                    error!("Failed to get Pi state: {:?}", e);
                    Some(WsEvent::Pi(PiWsEvent::Error {
                        id,
                        session_id,
                        error: format!("Failed to get state: {}", e),
                    }))
                }
            }
        }

        PiWsCommand::CloseSession { id, session_id } => {
            info!(
                "Pi close_session: user={}, session_id={}",
                user_id, session_id
            );

            let mut state = conn_state.lock().await;
            state.subscribed_sessions.remove(&session_id);
            state.pi_subscriptions.remove(&session_id);
            drop(state);

            match runner.pi_close_session(&session_id).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::SessionClosed { id, session_id })),
                Err(e) => {
                    error!("Failed to close Pi session: {:?}", e);
                    Some(WsEvent::Pi(PiWsEvent::Error {
                        id,
                        session_id,
                        error: format!("Failed to close session: {}", e),
                    }))
                }
            }
        }

        // === Session Lifecycle ===
        PiWsCommand::NewSession { id, session_id, parent_session } => {
            debug!("Pi new_session: user={}, session_id={}", user_id, session_id);
            match runner.pi_new_session(&session_id, parent_session.as_deref()).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::SessionCreated {
                    id,
                    session_id,
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::SwitchSession { id, session_id, session_path } => {
            debug!("Pi switch_session: user={}, session_id={}, path={}", user_id, session_id, session_path);
            match runner.pi_switch_session(&session_id, &session_path).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::SessionCreated {
                    id,
                    session_id,
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === State & Messages ===
        PiWsCommand::GetMessages { id, session_id } => {
            debug!("Pi get_messages: user={}, session_id={}", user_id, session_id);
            
            let (session_meta, is_active) = {
                let state_guard = conn_state.lock().await;
                (
                    state_guard.pi_session_meta.get(&session_id).cloned(),
                    state_guard.pi_subscriptions.contains(&session_id),
                )
            };

            if let Some(cached) = get_cached_pi_messages(&user_id, &session_id).await {
                let use_cached = !is_active || cached.age <= Duration::from_secs(2);
                if use_cached {
                    return Some(WsEvent::Pi(PiWsEvent::Messages {
                        id,
                        session_id,
                        messages: cached.messages,
                    }));
                }
            }

            if let Some(ref meta) = session_meta {
                if meta.scope.as_deref() == Some("workspace") {
                    if let (Some(work_dir), Some(workspace_pi)) =
                        (meta.cwd.as_ref(), state.workspace_pi.as_ref())
                    {
                        match workspace_pi.get_session_messages(user_id, work_dir, &session_id) {
                            Ok(messages) if !messages.is_empty() => {
                                info!(
                                    "Pi get_messages: loaded {} messages from workspace JSONL for {}",
                                    messages.len(),
                                    session_id
                                );
                                let messages_value =
                                    serde_json::to_value(&messages).unwrap_or_default();
                                cache_pi_messages(&user_id, &session_id, &messages_value).await;
                                return Some(WsEvent::Pi(PiWsEvent::Messages {
                                    id,
                                    session_id,
                                    messages: messages_value,
                                }));
                            }
                            Ok(_) => {
                                debug!(
                                    "Pi get_messages: workspace JSONL returned empty for {}",
                                    session_id
                                );
                            }
                            Err(e) => {
                                debug!(
                                    "Pi get_messages: workspace JSONL error for {}: {}",
                                    session_id, e
                                );
                            }
                        }
                    } else {
                        debug!("Pi get_messages: workspace metadata missing for {}", session_id);
                    }
                } else if let Some(ref pi_service) = state.main_chat_pi {
                    match pi_service.get_session_messages(user_id, &session_id).await {
                        Ok(messages) if !messages.is_empty() => {
                            info!(
                                "Pi get_messages: loaded {} messages from JSONL file for {}",
                                messages.len(),
                                session_id
                            );
                            let messages_value =
                                serde_json::to_value(&messages).unwrap_or_default();
                            cache_pi_messages(&user_id, &session_id, &messages_value).await;
                            return Some(WsEvent::Pi(PiWsEvent::Messages {
                                id,
                                session_id,
                                messages: messages_value,
                            }));
                        }
                        Ok(_) => {
                            debug!("Pi get_messages: JSONL file returned empty for {}", session_id);
                        }
                        Err(e) => {
                            debug!("Pi get_messages: JSONL file error for {}: {}", session_id, e);
                        }
                    }
                }
            } else if let Some(ref pi_service) = state.main_chat_pi {
                match pi_service.get_session_messages(user_id, &session_id).await {
                    Ok(messages) if !messages.is_empty() => {
                        info!(
                            "Pi get_messages: loaded {} messages from JSONL file for {}",
                            messages.len(),
                            session_id
                        );
                        let messages_value = serde_json::to_value(&messages).unwrap_or_default();
                        cache_pi_messages(&user_id, &session_id, &messages_value).await;
                        return Some(WsEvent::Pi(PiWsEvent::Messages {
                            id,
                            session_id,
                            messages: messages_value,
                        }));
                    }
                    Ok(_) => {
                        debug!("Pi get_messages: JSONL file returned empty for {}", session_id);
                    }
                    Err(e) => {
                        debug!("Pi get_messages: JSONL file error for {}: {}", session_id, e);
                    }
                }
            }

            // JSONL empty - try hstry for historical messages
            // In multi-user mode, use runner.get_*_chat_messages() to access per-user hstry
            let is_multi_user = state.linux_users.is_some();

            if let Some(meta) = session_meta.as_ref()
                && meta.scope.as_deref() == Some("workspace")
                && let Some(work_dir) = meta.cwd.as_ref()
            {
                if is_multi_user {
                    match runner
                        .get_workspace_chat_messages(
                            work_dir.to_string_lossy().to_string(),
                            session_id.clone(),
                            None,
                        )
                        .await
                    {
                        Ok(resp) if !resp.messages.is_empty() => {
                            info!(
                                "Pi get_messages: loaded {} messages from hstry (workspace via runner) for {}",
                                resp.messages.len(),
                                session_id
                            );
                            let messages: Vec<serde_json::Value> = resp
                                .messages
                                .into_iter()
                                .map(|m| {
                                    serde_json::json!({
                                        "id": m.id,
                                        "role": m.role,
                                        "content": m.content,
                                        "timestamp": m.timestamp,
                                    })
                                })
                                .collect();
                            let messages_value = serde_json::Value::Array(messages);
                            cache_pi_messages(&user_id, &session_id, &messages_value).await;
                            return Some(WsEvent::Pi(PiWsEvent::Messages {
                                id,
                                session_id,
                                messages: messages_value,
                            }));
                        }
                        Ok(_) => {
                            debug!(
                                "Pi get_messages: hstry (workspace via runner) returned empty for {}",
                                session_id
                            );
                        }
                        Err(e) => {
                            debug!(
                                "Pi get_messages: hstry (workspace via runner) error for {}: {}",
                                session_id, e
                            );
                        }
                    }
                } else if let Some(hstry_client) = state.hstry.as_ref() {
                    match hstry_client.get_messages(&session_id, None, None).await {
                        Ok(hstry_messages) if !hstry_messages.is_empty() => {
                            info!(
                                "Pi get_messages: loaded {} messages from hstry (workspace) for {}",
                                hstry_messages.len(),
                                session_id
                            );
                            let serializable =
                                crate::hstry::proto_messages_to_serializable(hstry_messages);
                            let messages_value =
                                serde_json::to_value(&serializable).unwrap_or_default();
                            cache_pi_messages(&user_id, &session_id, &messages_value).await;
                            return Some(WsEvent::Pi(PiWsEvent::Messages {
                                id,
                                session_id,
                                messages: messages_value,
                            }));
                        }
                        Ok(_) => {
                            debug!(
                                "Pi get_messages: hstry (workspace) returned empty for {}",
                                session_id
                            );
                        }
                        Err(e) => {
                            debug!(
                                "Pi get_messages: hstry (workspace) error for {}: {}",
                                session_id, e
                            );
                        }
                    }
                }
            } else if is_multi_user {
                match runner.get_main_chat_messages(&session_id, None).await {
                    Ok(resp) if !resp.messages.is_empty() => {
                        info!(
                            "Pi get_messages: loaded {} messages from hstry (via runner) for {}",
                            resp.messages.len(),
                            session_id
                        );
                        let messages: Vec<serde_json::Value> = resp
                            .messages
                            .into_iter()
                            .map(|m| {
                                serde_json::json!({
                                    "id": m.id,
                                    "role": m.role,
                                    "content": m.content,
                                    "timestamp": m.timestamp,
                                })
                            })
                            .collect();
                        let messages_value = serde_json::Value::Array(messages);
                        cache_pi_messages(&user_id, &session_id, &messages_value).await;
                        return Some(WsEvent::Pi(PiWsEvent::Messages {
                            id,
                            session_id,
                            messages: messages_value,
                        }));
                    }
                    Ok(_) => {
                        debug!(
                            "Pi get_messages: hstry (via runner) returned empty for {}",
                            session_id
                        );
                    }
                    Err(e) => {
                        debug!(
                            "Pi get_messages: hstry (via runner) error for {}: {}",
                            session_id, e
                        );
                    }
                }
            } else if let Some(hstry_client) = state.hstry.as_ref() {
                match hstry_client.get_messages(&session_id, None, None).await {
                    Ok(hstry_messages) if !hstry_messages.is_empty() => {
                        info!(
                            "Pi get_messages: loaded {} messages from hstry for {}",
                            hstry_messages.len(),
                            session_id
                        );
                        let serializable =
                            crate::hstry::proto_messages_to_serializable(hstry_messages);
                        let messages_value =
                            serde_json::to_value(&serializable).unwrap_or_default();
                        cache_pi_messages(&user_id, &session_id, &messages_value).await;
                        return Some(WsEvent::Pi(PiWsEvent::Messages {
                            id,
                            session_id,
                            messages: messages_value,
                        }));
                    }
                    Ok(_) => {
                        debug!("Pi get_messages: hstry returned empty for {}", session_id);
                    }
                    Err(e) => {
                        debug!("Pi get_messages: hstry error for {}: {}", session_id, e);
                    }
                }
            }

            // Try to get messages from runner's Pi process last
            let runner_messages = runner.pi_get_messages(&session_id).await;

            // Return runner result (empty or error)
            match runner_messages {
                Ok(resp) if !resp.messages.is_empty() => {
                    let messages_value =
                        serde_json::to_value(&resp.messages).unwrap_or_default();
                    cache_pi_messages(&user_id, &session_id, &messages_value).await;
                    Some(WsEvent::Pi(PiWsEvent::Messages {
                        id,
                        session_id,
                        messages: messages_value,
                    }))
                }
                Ok(resp) => {
                    let messages_value =
                        serde_json::to_value(&resp.messages).unwrap_or_default();
                    cache_pi_messages(&user_id, &session_id, &messages_value).await;
                    Some(WsEvent::Pi(PiWsEvent::Messages {
                        id,
                        session_id,
                        messages: messages_value,
                    }))
                }
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::GetSessionStats { id, session_id } => {
            debug!("Pi get_session_stats: user={}, session_id={}", user_id, session_id);
            match runner.pi_get_session_stats(&session_id).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::Stats {
                    id,
                    session_id,
                    stats: serde_json::to_value(&resp.stats).unwrap_or_default(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::GetLastAssistantText { id, session_id } => {
            debug!("Pi get_last_assistant_text: user={}, session_id={}", user_id, session_id);
            match runner.pi_get_last_assistant_text(&session_id).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::LastAssistantText {
                    id,
                    session_id,
                    text: resp.text,
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Model Management ===
        PiWsCommand::SetModel { id, session_id, provider, model_id } => {
            debug!("Pi set_model: user={}, session_id={}, provider={}, model={}", user_id, session_id, provider, model_id);
            match runner.pi_set_model(&session_id, &provider, &model_id).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::ModelChanged {
                    id,
                    session_id,
                    provider: resp.model.provider.clone(),
                    model_id: resp.model.id.clone(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::CycleModel { id, session_id } => {
            debug!("Pi cycle_model: user={}, session_id={}", user_id, session_id);
            match runner.pi_cycle_model(&session_id).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::ModelChanged {
                    id,
                    session_id,
                    provider: resp.model.provider.clone(),
                    model_id: resp.model.id.clone(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::GetAvailableModels { id, session_id } => {
            debug!("Pi get_available_models: user={}, session_id={}", user_id, session_id);
            match runner.pi_get_available_models(&session_id).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::AvailableModels {
                    id,
                    session_id,
                    models: serde_json::to_value(&resp.models).unwrap_or_default(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Thinking Level ===
        PiWsCommand::SetThinkingLevel { id, session_id, level } => {
            debug!("Pi set_thinking_level: user={}, session_id={}, level={}", user_id, session_id, level);
            match runner.pi_set_thinking_level(&session_id, &level).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::ThinkingLevelChanged {
                    id,
                    session_id,
                    level: resp.level,
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::CycleThinkingLevel { id, session_id } => {
            debug!("Pi cycle_thinking_level: user={}, session_id={}", user_id, session_id);
            match runner.pi_cycle_thinking_level(&session_id).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::ThinkingLevelChanged {
                    id,
                    session_id,
                    level: resp.level,
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Compaction ===
        PiWsCommand::SetAutoCompaction { id, session_id, enabled } => {
            debug!("Pi set_auto_compaction: user={}, session_id={}, enabled={}", user_id, session_id, enabled);
            match runner.pi_set_auto_compaction(&session_id, enabled).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::CommandAck {
                    id,
                    session_id,
                    command: "set_auto_compaction".into(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Queue Modes ===
        PiWsCommand::SetSteeringMode { id, session_id, mode } => {
            debug!("Pi set_steering_mode: user={}, session_id={}, mode={}", user_id, session_id, mode);
            match runner.pi_set_steering_mode(&session_id, &mode).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::CommandAck {
                    id,
                    session_id,
                    command: "set_steering_mode".into(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::SetFollowUpMode { id, session_id, mode } => {
            debug!("Pi set_follow_up_mode: user={}, session_id={}, mode={}", user_id, session_id, mode);
            match runner.pi_set_follow_up_mode(&session_id, &mode).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::CommandAck {
                    id,
                    session_id,
                    command: "set_follow_up_mode".into(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Retry ===
        PiWsCommand::SetAutoRetry { id, session_id, enabled } => {
            debug!("Pi set_auto_retry: user={}, session_id={}, enabled={}", user_id, session_id, enabled);
            match runner.pi_set_auto_retry(&session_id, enabled).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::CommandAck {
                    id,
                    session_id,
                    command: "set_auto_retry".into(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::AbortRetry { id, session_id } => {
            debug!("Pi abort_retry: user={}, session_id={}", user_id, session_id);
            match runner.pi_abort_retry(&session_id).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::CommandAck {
                    id,
                    session_id,
                    command: "abort_retry".into(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Forking ===
        PiWsCommand::Fork { id, session_id, entry_id } => {
            debug!("Pi fork: user={}, session_id={}, entry_id={}", user_id, session_id, entry_id);
            match runner.pi_fork(&session_id, &entry_id).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::ForkResult {
                    id,
                    session_id,
                    text: resp.text,
                    cancelled: resp.cancelled,
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::GetForkMessages { id, session_id } => {
            debug!("Pi get_fork_messages: user={}, session_id={}", user_id, session_id);
            match runner.pi_get_fork_messages(&session_id).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::ForkMessages {
                    id,
                    session_id,
                    messages: resp.messages.into_iter().map(|m| ForkMessageInfo {
                        entry_id: m.entry_id,
                        role: "user".to_string(), // PiForkMessage only contains user messages for forking
                        preview: m.text,
                        timestamp: None,
                    }).collect(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Session Metadata ===
        PiWsCommand::SetSessionName { id, session_id, name } => {
            debug!("Pi set_session_name: user={}, session_id={}, name={}", user_id, session_id, name);
            match runner.pi_set_session_name(&session_id, &name).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::CommandAck {
                    id,
                    session_id,
                    command: "set_session_name".into(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::ExportHtml { id, session_id, output_path } => {
            debug!("Pi export_html: user={}, session_id={}", user_id, session_id);
            match runner.pi_export_html(&session_id, output_path.as_deref()).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::ExportHtmlResult {
                    id,
                    session_id,
                    path: resp.path,
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Commands/Skills ===
        PiWsCommand::GetCommands { id, session_id } => {
            debug!("Pi get_commands: user={}, session_id={}", user_id, session_id);
            match runner.pi_get_commands(&session_id).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::Commands {
                    id,
                    session_id,
                    commands: resp.commands.into_iter().map(|c| CommandInfo {
                        name: c.name,
                        description: c.description,
                        command_type: c.source,
                    }).collect(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Bash ===
        PiWsCommand::Bash { id, session_id, command } => {
            debug!("Pi bash: user={}, session_id={}, command={}", user_id, session_id, command);
            match runner.pi_bash(&session_id, &command).await {
                Ok(resp) => Some(WsEvent::Pi(PiWsEvent::BashResult {
                    id,
                    session_id,
                    output: resp.output,
                    exit_code: resp.exit_code,
                    cancelled: resp.cancelled,
                    truncated: resp.truncated,
                    full_output_path: resp.full_output_path,
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        PiWsCommand::AbortBash { id, session_id } => {
            debug!("Pi abort_bash: user={}, session_id={}", user_id, session_id);
            match runner.pi_abort_bash(&session_id).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::CommandAck {
                    id,
                    session_id,
                    command: "abort_bash".into(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }

        // === Extension UI ===
        PiWsCommand::ExtensionUiResponse { id, session_id, request_id, value, confirmed, cancelled } => {
            debug!("Pi extension_ui_response: user={}, session_id={}, request_id={}", user_id, session_id, request_id);
            match runner.pi_extension_ui_response(&session_id, &request_id, value.as_deref(), confirmed, cancelled).await {
                Ok(()) => Some(WsEvent::Pi(PiWsEvent::CommandAck {
                    id,
                    session_id,
                    command: "extension_ui_response".into(),
                })),
                Err(e) => Some(WsEvent::Pi(PiWsEvent::Error {
                    id,
                    session_id,
                    error: e.to_string(),
                }))
            }
        }
    }
}

/// Forward Pi events from runner subscription to WebSocket.
async fn forward_pi_events(
    runner: &RunnerClient,
    session_id: &str,
    event_tx: mpsc::UnboundedSender<WsEvent>,
) -> anyhow::Result<()> {
    let mut subscription = runner.pi_subscribe(session_id).await?;

    loop {
        match subscription.next().await {
            Some(PiSubscriptionEvent::Event(pi_event)) => {
                let ws_event = pi_event_to_ws_event(session_id, pi_event);
                if event_tx.send(ws_event).is_err() {
                    // WebSocket closed
                    break;
                }
            }
            Some(PiSubscriptionEvent::End { reason }) => {
                debug!(
                    "Pi subscription ended for session {}: {}",
                    session_id, reason
                );
                break;
            }
            Some(PiSubscriptionEvent::Error { code, message }) => {
                error!(
                    "Pi subscription error for session {}: {:?} - {}",
                    session_id, code, message
                );
                let _ = event_tx.send(WsEvent::Pi(PiWsEvent::Error {
                    id: None,
                    session_id: session_id.to_string(),
                    error: message,
                }));
                break;
            }
            None => {
                debug!("Pi subscription stream ended for session {}", session_id);
                break;
            }
        }
    }

    Ok(())
}

/// Convert a Pi event to a WebSocket event.
fn pi_event_to_ws_event(session_id: &str, event: PiEvent) -> WsEvent {
    let sid = session_id.to_string();

    match event {
        PiEvent::AgentStart => WsEvent::Pi(PiWsEvent::MessageStart {
            session_id: sid,
            role: "assistant".to_string(),
        }),
        PiEvent::AgentEnd { .. } => WsEvent::Pi(PiWsEvent::Done { session_id: sid }),
        PiEvent::TurnStart => WsEvent::Pi(PiWsEvent::MessageStart {
            session_id: sid,
            role: "assistant".to_string(),
        }),
        PiEvent::TurnEnd { .. } => WsEvent::Pi(PiWsEvent::Done { session_id: sid }),
        PiEvent::MessageStart { message } => {
            WsEvent::Pi(PiWsEvent::MessageStart {
                session_id: sid,
                role: message.role.clone(),
            })
        }
        PiEvent::MessageUpdate {
            assistant_message_event,
            ..
        } => {
            // Convert AssistantMessageEvent to appropriate WS event
            match assistant_message_event {
                AssistantMessageEvent::TextDelta { delta, .. } => WsEvent::Pi(PiWsEvent::Text {
                    session_id: sid,
                    data: delta,
                }),
                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                    WsEvent::Pi(PiWsEvent::Thinking {
                        session_id: sid,
                        data: delta,
                    })
                }
                AssistantMessageEvent::TextStart { .. }
                | AssistantMessageEvent::ThinkingStart { .. }
                | AssistantMessageEvent::ToolcallStart { .. }
                | AssistantMessageEvent::Start { .. } => {
                    // These don't produce visible output, skip
                    WsEvent::Pi(PiWsEvent::Text {
                        session_id: sid,
                        data: String::new(),
                    })
                }
                AssistantMessageEvent::TextEnd { content, .. } => WsEvent::Pi(PiWsEvent::Text {
                    session_id: sid,
                    data: content,
                }),
                AssistantMessageEvent::ThinkingEnd { content, .. } => {
                    WsEvent::Pi(PiWsEvent::Thinking {
                        session_id: sid,
                        data: content,
                    })
                }
                AssistantMessageEvent::ToolcallDelta { .. } => {
                    // Tool call deltas are JSON fragments, not user-visible.
                    WsEvent::Pi(PiWsEvent::Text {
                        session_id: sid,
                        data: String::new(),
                    })
                }
                AssistantMessageEvent::ToolcallEnd { tool_call, .. } => {
                    WsEvent::Pi(PiWsEvent::ToolUse {
                        session_id: sid,
                        data: ToolUseData {
                            id: tool_call.id.clone(),
                            name: tool_call.name.clone(),
                            input: tool_call.arguments.clone(),
                        },
                    })
                }
                AssistantMessageEvent::Done { .. } => {
                    WsEvent::Pi(PiWsEvent::Done { session_id: sid })
                }
                AssistantMessageEvent::Error { reason, error } => {
                    let error_str = error
                        .as_ref()
                        .and_then(|m| serde_json::to_string(m).ok())
                        .unwrap_or_else(|| reason.clone());
                    WsEvent::Pi(PiWsEvent::Error {
                        id: None,
                        session_id: sid,
                        error: error_str,
                    })
                }
                AssistantMessageEvent::Unknown => {
                    // Unknown event type, skip
                    WsEvent::Pi(PiWsEvent::Text {
                        session_id: sid,
                        data: String::new(),
                    })
                }
            }
        }
        PiEvent::MessageEnd { .. } => WsEvent::Pi(PiWsEvent::Done { session_id: sid }),
        PiEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => WsEvent::Pi(PiWsEvent::ToolStart {
            session_id: sid,
            data: ToolUseData {
                id: tool_call_id,
                name: tool_name,
                input: args,
            },
        }),
        PiEvent::ToolExecutionUpdate {
            tool_call_id,
            tool_name,
            partial_result,
            ..
        } => WsEvent::Pi(PiWsEvent::ToolUse {
            session_id: sid,
            data: ToolUseData {
                id: tool_call_id,
                name: tool_name,
                input: serde_json::to_value(&partial_result).unwrap_or(Value::Null),
            },
        }),
        PiEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
            is_error,
        } => WsEvent::Pi(PiWsEvent::ToolResult {
            session_id: sid,
            data: ToolResultData {
                id: tool_call_id,
                name: Some(tool_name),
                content: serde_json::to_value(&result).unwrap_or(Value::Null),
                is_error: Some(is_error),
            },
        }),
        PiEvent::AutoCompactionStart { .. } => WsEvent::Pi(PiWsEvent::State {
            id: None,
            session_id: sid,
            state: serde_json::json!({"compacting": true}),
        }),
        PiEvent::AutoCompactionEnd { .. } => WsEvent::Pi(PiWsEvent::State {
            id: None,
            session_id: sid,
            state: serde_json::json!({"compacting": false}),
        }),
        PiEvent::AutoRetryStart { .. } => WsEvent::Pi(PiWsEvent::State {
            id: None,
            session_id: sid,
            state: serde_json::json!({"retrying": true}),
        }),
        PiEvent::AutoRetryEnd { .. } => WsEvent::Pi(PiWsEvent::State {
            id: None,
            session_id: sid,
            state: serde_json::json!({"retrying": false}),
        }),
        PiEvent::ExtensionUiRequest(_) => {
            // Extension UI requests need special handling - for now just acknowledge
            WsEvent::Pi(PiWsEvent::State {
                id: None,
                session_id: sid,
                state: serde_json::json!({"extension_ui_request": true}),
            })
        }
        PiEvent::HookError { error, .. } => WsEvent::Pi(PiWsEvent::Error {
            id: None,
            session_id: sid,
            error,
        }),
        PiEvent::Unknown => {
            debug!("Received unknown Pi event type");
            WsEvent::Pi(PiWsEvent::State {
                id: None,
                session_id: sid,
                state: Value::Null,
            })
        }
    }
}

/// Handle Files channel commands.
async fn handle_files_command(
    cmd: FilesWsCommand,
    user_id: &str,
    state: &AppState,
) -> Option<WsEvent> {
    let id = match &cmd {
        FilesWsCommand::Tree { id, .. }
        | FilesWsCommand::Read { id, .. }
        | FilesWsCommand::Write { id, .. }
        | FilesWsCommand::List { id, .. }
        | FilesWsCommand::Stat { id, .. }
        | FilesWsCommand::Delete { id, .. }
        | FilesWsCommand::CreateDirectory { id, .. }
        | FilesWsCommand::Rename { id, .. }
        | FilesWsCommand::Copy { id, .. }
        | FilesWsCommand::Move { id, .. } => id.clone(),
    };

    let workspace_path = match &cmd {
        FilesWsCommand::Tree { workspace_path, .. }
        | FilesWsCommand::Read { workspace_path, .. }
        | FilesWsCommand::Write { workspace_path, .. }
        | FilesWsCommand::List { workspace_path, .. }
        | FilesWsCommand::Stat { workspace_path, .. }
        | FilesWsCommand::Delete { workspace_path, .. }
        | FilesWsCommand::CreateDirectory { workspace_path, .. }
        | FilesWsCommand::Rename { workspace_path, .. }
        | FilesWsCommand::Copy { workspace_path, .. }
        | FilesWsCommand::Move { workspace_path, .. } => workspace_path.as_deref(),
    };

    let workspace_root = match resolve_workspace_root(workspace_path) {
        Ok(path) => path,
        Err(err) => {
            return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
        }
    };

    let linux_username = state
        .linux_users
        .as_ref()
        .map(|lu| lu.linux_username(user_id))
        .unwrap_or_else(|| user_id.to_string());
    let user_plane: Arc<dyn crate::user_plane::UserPlane> =
        if let Some(pattern) = state.runner_socket_pattern.as_deref() {
            match RunnerUserPlane::for_user_with_pattern(&linux_username, pattern) {
                Ok(plane) => Arc::new(plane),
                Err(err) => {
                    warn!(
                        "Failed to create RunnerUserPlane for {}: {:#}, falling back to direct",
                        linux_username, err
                    );
                    Arc::new(DirectUserPlane::new(&workspace_root))
                }
            }
        } else {
            Arc::new(DirectUserPlane::new(&workspace_root))
        };

    fn build_tree<'a>(
        user_plane: &'a Arc<dyn crate::user_plane::UserPlane>,
        workspace_root: &'a std::path::Path,
        relative_path: &'a str,
        depth: usize,
        include_hidden: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<FileTreeNode>, String>> + Send + 'a>> {
        Box::pin(async move {
            let resolved = resolve_workspace_child(workspace_root, relative_path)?;
            let entries = user_plane
                .list_directory(&resolved, include_hidden)
                .await
                .map_err(|e| format!("list_directory failed for {}: {:#}", resolved.display(), e))?;

            let mut nodes = Vec::new();
            for entry in entries {
                let child_path = join_relative_path(relative_path, &entry.name);
                let children = if entry.is_dir && depth > 1 {
                    Some(
                        build_tree(
                            user_plane,
                            workspace_root,
                            &child_path,
                            depth - 1,
                            include_hidden,
                        )
                        .await?,
                    )
                } else {
                    None
                };
                nodes.push(map_tree_node(&entry, child_path, children));
            }
            Ok(nodes)
        })
    }

    fn copy_recursive<'a>(
        user_plane: &'a Arc<dyn crate::user_plane::UserPlane>,
        from_path: &'a std::path::Path,
        to_path: &'a std::path::Path,
        overwrite: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let from_stat = user_plane
                .stat(from_path)
                .await
                .map_err(|e| e.to_string())?;
            if !from_stat.exists {
                return Err("source path does not exist".into());
            }

            let dest_stat = user_plane
                .stat(to_path)
                .await
                .map_err(|e| e.to_string())?;
            if dest_stat.exists {
                if !overwrite {
                    return Err("destination already exists".into());
                }
                user_plane
                    .delete_path(to_path, true)
                    .await
                    .map_err(|e| e.to_string())?;
            }

            if from_stat.is_dir {
                user_plane
                    .create_directory(to_path, true)
                    .await
                    .map_err(|e| e.to_string())?;
                let entries = user_plane
                    .list_directory(from_path, true)
                    .await
                    .map_err(|e| e.to_string())?;
                for entry in entries {
                    let child_from = from_path.join(&entry.name);
                    let child_to = to_path.join(&entry.name);
                    copy_recursive(user_plane, &child_from, &child_to, overwrite).await?;
                }
                Ok(())
            } else {
                let content = user_plane
                    .read_file(from_path, None, None)
                    .await
                    .map_err(|e| e.to_string())?;
                user_plane
                    .write_file(to_path, &content.content, true)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
        })
    }

    match cmd {
        FilesWsCommand::Tree {
            id,
            path,
            depth,
            include_hidden,
            ..
        } => {
            let max_depth = depth.unwrap_or(6).max(1);
            match build_tree(&user_plane, &workspace_root, &path, max_depth, include_hidden).await {
                Ok(entries) => Some(WsEvent::Files(FilesWsEvent::TreeResult {
                    id,
                    path,
                    entries,
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error { id, error: err })),
            }
        }
        FilesWsCommand::Read { id, path, .. } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane.read_file(&resolved, None, None).await {
                Ok(content) => {
                    let encoded = base64::engine::general_purpose::STANDARD
                        .encode(content.content);
                    Some(WsEvent::Files(FilesWsEvent::ReadResult {
                        id,
                        path,
                        content: encoded,
                        size: Some(content.size),
                        truncated: Some(content.truncated),
                    }))
                }
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::Write {
            id,
            path,
            content,
            create_parents,
            ..
        } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let decoded = match base64::engine::general_purpose::STANDARD.decode(content) {
                Ok(bytes) => bytes,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error {
                        id,
                        error: format!("invalid base64 content: {}", err),
                    }));
                }
            };
            match user_plane
                .write_file(&resolved, &decoded, create_parents)
                .await
            {
                Ok(()) => Some(WsEvent::Files(FilesWsEvent::WriteResult {
                    id,
                    path,
                    success: true,
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::List {
            id,
            path,
            include_hidden,
            ..
        } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane.list_directory(&resolved, include_hidden).await {
                Ok(entries) => Some(WsEvent::Files(FilesWsEvent::ListResult { id, path, entries })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::Stat { id, path, .. } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane.stat(&resolved).await {
                Ok(stat) => Some(WsEvent::Files(FilesWsEvent::StatResult {
                    id,
                    path,
                    stat: serde_json::to_value(&stat).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::Delete {
            id,
            path,
            recursive,
            ..
        } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane.delete_path(&resolved, recursive).await {
                Ok(()) => Some(WsEvent::Files(FilesWsEvent::DeleteResult {
                    id,
                    path,
                    success: true,
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::CreateDirectory {
            id,
            path,
            create_parents,
            ..
        } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane
                .create_directory(&resolved, create_parents)
                .await
            {
                Ok(()) => Some(WsEvent::Files(FilesWsEvent::CreateDirectoryResult {
                    id,
                    path,
                    success: true,
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::Rename {
            id,
            from,
            to,
            ..
        } => {
            let from_resolved = match resolve_workspace_child(&workspace_root, &from) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let to_resolved = match resolve_workspace_child(&workspace_root, &to) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let copy_result = copy_recursive(&user_plane, &from_resolved, &to_resolved, true).await;
            let result = match copy_result {
                Ok(()) => user_plane
                    .delete_path(&from_resolved, true)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => Some(WsEvent::Files(FilesWsEvent::RenameResult {
                    id,
                    from,
                    to,
                    success: true,
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error { id, error: err })),
            }
        }
        FilesWsCommand::Copy {
            id,
            from,
            to,
            overwrite,
            ..
        } => {
            let from_resolved = match resolve_workspace_child(&workspace_root, &from) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let to_resolved = match resolve_workspace_child(&workspace_root, &to) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match copy_recursive(&user_plane, &from_resolved, &to_resolved, overwrite).await {
                Ok(()) => Some(WsEvent::Files(FilesWsEvent::CopyResult {
                    id,
                    from,
                    to,
                    success: true,
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error { id, error: err })),
            }
        }
        FilesWsCommand::Move {
            id,
            from,
            to,
            overwrite,
            ..
        } => {
            let from_resolved = match resolve_workspace_child(&workspace_root, &from) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let to_resolved = match resolve_workspace_child(&workspace_root, &to) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let copy_result = copy_recursive(&user_plane, &from_resolved, &to_resolved, overwrite).await;
            let result = match copy_result {
                Ok(()) => user_plane
                    .delete_path(&from_resolved, true)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => Some(WsEvent::Files(FilesWsEvent::MoveResult {
                    id,
                    from,
                    to,
                    success: true,
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error { id, error: err })),
            }
        }
    }
}

async fn resolve_terminal_session(
    user_id: &str,
    state: &AppState,
    workspace_path: Option<&str>,
    session_id: Option<&str>,
) -> Result<Session, String> {
    info!(
        "resolve_terminal_session: user={}, workspace_path={:?}, session_id={:?}",
        user_id, workspace_path, session_id
    );
    if let Some(session_id) = session_id {
        let session = state
            .sessions
            .for_user(user_id)
            .get_session(session_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Session not found".to_string())?;
        let session = crate::api::proxy::builder::ensure_session_for_io_proxy(
            state,
            user_id,
            session_id,
            session,
        )
        .await
        .map_err(|_| "Failed to resume session for terminal".to_string())?;
        return Ok(session);
    }

    let workspace_path = workspace_path.ok_or_else(|| "workspace_path is required".to_string())?;
    let session = state
        .sessions
        .for_user(user_id)
        .get_or_create_io_session_for_workspace(workspace_path)
        .await
        .map_err(|e| e.to_string())?;
    let session_id = session.id.clone();
    let session = crate::api::proxy::builder::ensure_session_for_io_proxy(
        state,
        user_id,
        &session_id,
        session,
    )
    .await
    .map_err(|_| "Failed to resume session for terminal".to_string())?;
    Ok(session)
}

enum TtydConnection {
    Unix(
        tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>,
    ),
    Tcp(
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ),
}

enum TtydConnectionWrite {
    Unix(
        futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>,
            tokio_tungstenite::tungstenite::Message,
        >,
    ),
    Tcp(
        futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            tokio_tungstenite::tungstenite::Message,
        >,
    ),
}

enum TtydConnectionRead {
    Unix(
        futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>>,
    ),
    Tcp(
        futures::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
    ),
}

impl TtydConnection {
    fn split(self) -> (TtydConnectionWrite, TtydConnectionRead) {
        match self {
            TtydConnection::Unix(ws) => {
                let (write, read) = ws.split();
                (TtydConnectionWrite::Unix(write), TtydConnectionRead::Unix(read))
            }
            TtydConnection::Tcp(ws) => {
                let (write, read) = ws.split();
                (TtydConnectionWrite::Tcp(write), TtydConnectionRead::Tcp(read))
            }
        }
    }
}

impl TtydConnectionWrite {
    async fn send(
        &mut self,
        msg: tokio_tungstenite::tungstenite::Message,
    ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        match self {
            TtydConnectionWrite::Unix(w) => w.send(msg).await,
            TtydConnectionWrite::Tcp(w) => w.send(msg).await,
        }
    }
}

impl TtydConnectionRead {
    async fn next(
        &mut self,
    ) -> Option<
        Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>,
    > {
        match self {
            TtydConnectionRead::Unix(r) => r.next().await,
            TtydConnectionRead::Tcp(r) => r.next().await,
        }
    }
}

async fn connect_ttyd_socket(session_id: &str, ttyd_port: u16) -> anyhow::Result<TtydConnection> {
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let socket_path = ProcessManager::ttyd_socket_path(session_id);
    if socket_path.exists() {
        use tokio::net::UnixStream;
        use tokio_tungstenite::client_async;

        let stream = UnixStream::connect(&socket_path).await?;
        let mut request = "ws://localhost/ws".into_client_request()?;
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "tty".parse().unwrap());
        let (socket, _response) = client_async(request, stream).await?;
        return Ok(TtydConnection::Unix(socket));
    }

    let url = format!("ws://localhost:{}/ws", ttyd_port);
    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "tty".parse().unwrap());
    let (socket, _response) = connect_async(request).await?;
    Ok(TtydConnection::Tcp(socket))
}

async fn start_terminal_task(
    terminal_id: String,
    session_id: String,
    ttyd_port: u16,
    cols: u16,
    rows: u16,
    event_tx: mpsc::UnboundedSender<WsEvent>,
) -> Result<
    (
        mpsc::UnboundedSender<TerminalSessionCommand>,
        tokio::task::JoinHandle<()>,
    ),
    String,
> {
    let (command_tx, mut command_rx) = mpsc::unbounded_channel::<TerminalSessionCommand>();

    let task = tokio::spawn(async move {
        let timeout = crate::api::proxy::builder::DEFAULT_WS_TIMEOUT;
        let start = tokio::time::Instant::now();
        let mut attempts: u32 = 0;
        let socket = loop {
            attempts += 1;
            match connect_ttyd_socket(&session_id, ttyd_port).await {
                Ok(socket) => break socket,
                Err(err) => {
                    if start.elapsed() >= timeout {
                        let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Error {
                            id: None,
                            terminal_id: Some(terminal_id.clone()),
                            error: format!("ttyd not available: {}", err),
                        }));
                        return;
                    }
                }
            }
            let backoff_ms = (attempts.min(20) as u64) * 100;
            tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
        };

        let (mut ttyd_write, mut ttyd_read) = socket.split();

        let init_msg = serde_json::json!({
            "AuthToken": "",
            "columns": cols,
            "rows": rows,
        });
        let init_text = init_msg.to_string();
        if ttyd_write
            .send(tokio_tungstenite::tungstenite::Message::Binary(
                init_text.as_bytes().to_vec().into(),
            ))
            .await
            .is_err()
        {
            let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Error {
                id: None,
                terminal_id: Some(terminal_id.clone()),
                error: "Failed to initialize terminal".into(),
            }));
            return;
        }

        let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Opened {
            id: None,
            terminal_id: terminal_id.clone(),
        }));

        loop {
            tokio::select! {
                Some(cmd) = command_rx.recv() => {
                    match cmd {
                        TerminalSessionCommand::Input(data) => {
                            let mut payload = Vec::with_capacity(1 + data.len());
                            payload.push(b'0');
                            payload.extend_from_slice(data.as_bytes());
                            let _ = ttyd_write.send(tokio_tungstenite::tungstenite::Message::Binary(payload.into())).await;
                        }
                        TerminalSessionCommand::Resize { cols, rows } => {
                            let resize = serde_json::json!({
                                "columns": cols,
                                "rows": rows,
                            });
                            let mut payload = vec![b'1'];
                            payload.extend_from_slice(resize.to_string().as_bytes());
                            let _ = ttyd_write.send(tokio_tungstenite::tungstenite::Message::Binary(payload.into())).await;
                        }
                        TerminalSessionCommand::Close => {
                            let _ = ttyd_write.send(tokio_tungstenite::tungstenite::Message::Close(None)).await;
                            break;
                        }
                    }
                }
                msg = ttyd_read.next() => {
                    match msg {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(data))) => {
                            if data.is_empty() {
                                continue;
                            }
                            let (prefix, payload) = data.split_at(1);
                            if prefix[0] == b'0' {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
                                let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Output {
                                    terminal_id: terminal_id.clone(),
                                    data_base64: encoded,
                                }));
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                            let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
                            let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Output {
                                terminal_id: terminal_id.clone(),
                                data_base64: encoded,
                            }));
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => break,
                        Some(Err(_)) | None => break,
                        _ => {}
                    }
                }
            }
        }

        let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Exit {
            terminal_id,
        }));
    });

    Ok((command_tx, task))
}

/// Handle Terminal channel commands.
async fn handle_terminal_command(
    cmd: TerminalWsCommand,
    user_id: &str,
    state: &AppState,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    match cmd {
        TerminalWsCommand::Open {
            id,
            terminal_id,
            workspace_path,
            session_id,
            cols,
            rows,
        } => {
            info!(
                "Terminal open: user={}, workspace_path={:?}, session_id={:?}, terminal_id={:?}",
                user_id, workspace_path, session_id, terminal_id
            );
            let session = match resolve_terminal_session(
                user_id,
                state,
                workspace_path.as_deref(),
                session_id.as_deref(),
            )
            .await
            {
                Ok(session) => session,
                Err(err) => {
                    return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                        id,
                        terminal_id,
                        error: err,
                    }));
                }
            };

            info!(
                "Terminal session resolved: id={}, workspace_path={:?}, ttyd_port={}",
                session.id, session.workspace_path, session.ttyd_port
            );

            if session.ttyd_port == 0 {
                warn!("Terminal not available: ttyd_port=0 for session {}", session.id);
                return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id,
                    error: "Terminal is not available for this session".into(),
                }));
            }

            let terminal_id = terminal_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let mut state_guard = conn_state.lock().await;
            if state_guard.terminal_sessions.contains_key(&terminal_id) {
                info!("Terminal already exists: {}", terminal_id);
                return Some(WsEvent::Terminal(TerminalWsEvent::Opened { id, terminal_id }));
            }

            let event_tx = state_guard.event_tx.clone();
            let session_id = session.id.clone();
            let ttyd_port = session.ttyd_port as u16;
            drop(state_guard);
            info!(
                "Starting terminal task: terminal_id={}, session_id={}, ttyd_port={}",
                terminal_id, session_id, ttyd_port
            );

            let (command_tx, task) = match start_terminal_task(
                terminal_id.clone(),
                session_id,
                ttyd_port,
                cols,
                rows,
                event_tx.clone(),
            )
            .await
            {
                Ok(result) => result,
                Err(err) => {
                    return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                        id,
                        terminal_id: Some(terminal_id),
                        error: err,
                    }));
                }
            };

            let mut state_guard = conn_state.lock().await;
            state_guard.terminal_sessions.insert(
                terminal_id.clone(),
                TerminalSession {
                    command_tx,
                    task,
                },
            );

            Some(WsEvent::Terminal(TerminalWsEvent::Opened { id, terminal_id }))
        }
        TerminalWsCommand::Input { id, terminal_id, data } => {
            let state_guard = conn_state.lock().await;
            if let Some(session) = state_guard.terminal_sessions.get(&terminal_id) {
                let _ = session.command_tx.send(TerminalSessionCommand::Input(data));
                None
            } else {
                Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id: Some(terminal_id),
                    error: "Terminal session not found".into(),
                }))
            }
        }
        TerminalWsCommand::Resize {
            id,
            terminal_id,
            cols,
            rows,
        } => {
            let state_guard = conn_state.lock().await;
            if let Some(session) = state_guard.terminal_sessions.get(&terminal_id) {
                let _ = session
                    .command_tx
                    .send(TerminalSessionCommand::Resize { cols, rows });
                None
            } else {
                Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id: Some(terminal_id),
                    error: "Terminal session not found".into(),
                }))
            }
        }
        TerminalWsCommand::Close { id, terminal_id } => {
            let mut state_guard = conn_state.lock().await;
            if let Some(session) = state_guard.terminal_sessions.remove(&terminal_id) {
                let _ = session.command_tx.send(TerminalSessionCommand::Close);
                session.task.abort();
                None
            } else {
                Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id: Some(terminal_id),
                    error: "Terminal session not found".into(),
                }))
            }
        }
    }
}

/// Handle Hstry channel commands.
async fn handle_hstry_command(cmd: HstryWsCommand, state: &AppState) -> Option<WsEvent> {
    let HstryWsCommand::Query {
        id,
        session_id,
        query,
        limit,
    } = cmd;

    let query = query.unwrap_or_default();
    if query.trim().is_empty() && session_id.is_none() {
        return Some(WsEvent::Hstry(HstryWsEvent::Result {
            id,
            data: serde_json::json!({"hits":[],"total":0}),
        }));
    }

    if let Some(session_id) = session_id {
        let limit = limit.unwrap_or(0) as i64;
        let client = match state.hstry.as_ref() {
            Some(client) => client,
            None => {
                return Some(WsEvent::Hstry(HstryWsEvent::Error {
                    id,
                    error: "hstry client is not configured".into(),
                }));
            }
        };
        match client.get_messages(&session_id, None, Some(limit)).await {
            Ok(messages) => {
                let serializable = crate::hstry::proto_messages_to_serializable(messages);
                let data = serde_json::to_value(serializable).unwrap_or(Value::Null);
                Some(WsEvent::Hstry(HstryWsEvent::Result { id, data }))
            }
            Err(err) => Some(WsEvent::Hstry(HstryWsEvent::Error {
                id,
                error: err.to_string(),
            })),
        }
    } else {
        let hits = match crate::history::search_hstry(&query, limit.unwrap_or(50) as usize).await {
            Ok(hits) => hits,
            Err(err) => {
                return Some(WsEvent::Hstry(HstryWsEvent::Error {
                    id,
                    error: err.to_string(),
                }));
            }
        };
        let data = serde_json::to_value(hits).unwrap_or(Value::Null);
        Some(WsEvent::Hstry(HstryWsEvent::Result { id, data }))
    }
}

/// Handle TRX channel commands.
async fn handle_trx_command(
    cmd: TrxWsCommand,
    user_id: &str,
    state: &AppState,
) -> Option<WsEvent> {
    let now = Utc::now().timestamp() + 3600;
    let user = CurrentUser {
        claims: Claims {
            sub: user_id.to_string(),
            iss: None,
            aud: None,
            exp: now,
            iat: None,
            nbf: None,
            jti: None,
            email: None,
            name: None,
            preferred_username: None,
            roles: vec![],
            role: None,
        },
    };

    match cmd {
        TrxWsCommand::List { id, workspace_path } => {
            let query = TrxWorkspaceQuery { workspace_path };
            match super::handlers::list_trx_issues(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Query(query),
            )
            .await
            {
                Ok(axum::Json(issues)) => Some(WsEvent::Trx(TrxWsEvent::ListResult {
                    id,
                    issues: serde_json::to_value(issues).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        TrxWsCommand::Create {
            id,
            workspace_path,
            data,
        } => {
            let query = TrxWorkspaceQuery { workspace_path };
            let request = CreateTrxIssueRequest {
                title: data.title,
                description: data.description,
                issue_type: data.issue_type.unwrap_or_else(|| "task".to_string()),
                priority: data.priority.unwrap_or(2),
                parent_id: data.parent_id,
            };
            match super::handlers::create_trx_issue(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Query(query),
                axum::Json(request),
            )
            .await
            {
                Ok(axum::Json(issue)) => Some(WsEvent::Trx(TrxWsEvent::IssueResult {
                    id,
                    issue: serde_json::to_value(issue).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        TrxWsCommand::Update {
            id,
            workspace_path,
            issue_id,
            data,
        } => {
            let query = TrxWorkspaceQuery { workspace_path };
            let request = UpdateTrxIssueRequest {
                title: data.title,
                description: data.description,
                status: data.status,
                priority: data.priority,
            };
            match super::handlers::update_trx_issue(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Path(issue_id),
                axum::extract::Query(query),
                axum::Json(request),
            )
            .await
            {
                Ok(axum::Json(issue)) => Some(WsEvent::Trx(TrxWsEvent::IssueResult {
                    id,
                    issue: serde_json::to_value(issue).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        TrxWsCommand::Close {
            id,
            workspace_path,
            issue_id,
            reason,
        } => {
            let query = TrxWorkspaceQuery { workspace_path };
            let request = CloseTrxIssueRequest { reason };
            match super::handlers::close_trx_issue(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Path(issue_id),
                axum::extract::Query(query),
                axum::Json(request),
            )
            .await
            {
                Ok(axum::Json(issue)) => Some(WsEvent::Trx(TrxWsEvent::IssueResult {
                    id,
                    issue: serde_json::to_value(issue).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        TrxWsCommand::Sync { id, workspace_path } => {
            let query = TrxWorkspaceQuery { workspace_path };
            match super::handlers::sync_trx(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Query(query),
            )
            .await
            {
                Ok(axum::Json(resp)) => Some(WsEvent::Trx(TrxWsEvent::SyncResult {
                    id,
                    success: resp.get("synced").and_then(|v| v.as_bool()).unwrap_or(false),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
    }
}

async fn handle_session_command(
    cmd: SessionWsCommand,
    user_id: &str,
    state: &AppState,
) -> Option<WsEvent> {
    let hub = state.ws_hub.clone();
    let session_id = extract_legacy_session_id(&cmd.cmd);
    if let Err(err) = crate::ws::handler::handle_command(&hub, state, user_id, cmd.cmd).await {
        return Some(WsEvent::Session(LegacyWsEvent::Error {
            message: err.to_string(),
            session_id,
        }));
    }
    None
}

fn extract_legacy_session_id(cmd: &LegacyWsCommand) -> Option<String> {
    use crate::ws::types::WsCommand as Legacy;
    match cmd {
        Legacy::Subscribe { session_id }
        | Legacy::Unsubscribe { session_id }
        | Legacy::SendMessage { session_id, .. }
        | Legacy::SendParts { session_id, .. }
        | Legacy::Abort { session_id }
        | Legacy::PermissionReply { session_id, .. }
        | Legacy::QuestionReply { session_id, .. }
        | Legacy::QuestionReject { session_id, .. }
        | Legacy::RefreshSession { session_id }
        | Legacy::GetMessages { session_id, .. } => Some(session_id.clone()),
        Legacy::Pong | Legacy::A2uiAction { .. } => None,
    }
}

fn resolve_workspace_root(
    workspace_path: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let Some(workspace_path) = workspace_path else {
        return Err("workspace_path is required".into());
    };
    let root = std::path::PathBuf::from(workspace_path);
    if root.as_os_str().is_empty() {
        return Err("workspace_path is required".into());
    }
    Ok(root)
}

fn resolve_workspace_child(
    workspace_root: &std::path::Path,
    path: &str,
) -> Result<std::path::PathBuf, String> {
    let trimmed = if path.trim().is_empty() { "." } else { path };
    let candidate = std::path::PathBuf::from(trimmed);
    if candidate.is_absolute() {
        if !candidate.starts_with(workspace_root) {
            return Err("path is outside workspace".into());
        }
        return Ok(candidate);
    }
    let cleaned = trimmed.trim_start_matches('/');
    Ok(workspace_root.join(cleaned))
}

fn join_relative_path(base: &str, name: &str) -> String {
    if base.is_empty() || base == "." {
        name.to_string()
    } else {
        format!("{}/{}", base.trim_end_matches('/'), name)
    }
}

fn map_tree_node(
    entry: &crate::user_plane::DirEntry,
    path: String,
    children: Option<Vec<FileTreeNode>>,
) -> FileTreeNode {
    FileTreeNode {
        name: entry.name.clone(),
        path,
        node_type: if entry.is_dir { "directory" } else { "file" }.to_string(),
        size: if entry.is_dir { None } else { Some(entry.size) },
        modified: if entry.modified_at > 0 {
            Some(entry.modified_at / 1000)
        } else {
            None
        },
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pi_prompt_command() {
        let json = r#"{"channel":"pi","type":"prompt","session_id":"ses_123","message":"hello"}"#;
        let cmd: WsCommand = serde_json::from_str(json).unwrap();
        match cmd {
            WsCommand::Pi(PiWsCommand::Prompt {
                session_id,
                message,
                ..
            }) => {
                assert_eq!(session_id, "ses_123");
                assert_eq!(message, "hello");
            }
            _ => panic!("Expected Pi Prompt command"),
        }
    }

    #[test]
    fn test_parse_pi_subscribe_command() {
        let json = r#"{"channel":"pi","type":"subscribe","session_id":"ses_456"}"#;
        let cmd: WsCommand = serde_json::from_str(json).unwrap();
        match cmd {
            WsCommand::Pi(PiWsCommand::Subscribe { session_id, .. }) => {
                assert_eq!(session_id, "ses_456");
            }
            _ => panic!("Expected Pi Subscribe command"),
        }
    }

    #[test]
    fn test_serialize_pi_event() {
        let event = WsEvent::Pi(PiWsEvent::Text {
            session_id: "ses_123".into(),
            data: "Hello world".into(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""channel":"pi""#));
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""session_id":"ses_123""#));
    }

    #[test]
    fn test_serialize_system_connected() {
        let event = WsEvent::System(SystemWsEvent::Connected);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""channel":"system""#));
        assert!(json.contains(r#""type":"connected""#));
    }
}

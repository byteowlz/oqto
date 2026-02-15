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
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

use chrono::Utc;

use base64::Engine;

use crate::auth::{Claims, CurrentUser};
use crate::local::ProcessManager;

use crate::runner::client::{PiSubscriptionEvent, RunnerClient};
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
    serde_json::to_string(messages)
        .map(|s| s.len())
        .unwrap_or(0)
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

async fn get_cached_pi_messages(
    user_id: &str,
    session_id: &str,
) -> Option<CachedPiMessagesSnapshot> {
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
    Agent,
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
    Agent(octo_protocol::commands::Command),
    Files(FilesWsCommand),
    Terminal(TerminalWsCommand),
    Hstry(HstryWsCommand),
    Trx(TrxWsCommand),
    Session(SessionWsCommand),
}

/// Files channel commands.
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
///
/// All agent events (streaming, command responses, lifecycle) flow through
/// `WsEvent::Agent` as canonical `octo_protocol::events::Event` values.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "channel", rename_all = "snake_case")]
pub enum WsEvent {
    /// Canonical agent events (streaming, state, command responses, delegation, etc.).
    /// Serializes as `{"channel": "agent", "session_id": ..., "event": ..., ...}`.
    #[serde(rename = "agent")]
    Agent(octo_protocol::events::Event),
    Files(FilesWsEvent),
    Terminal(TerminalWsEvent),
    Hstry(HstryWsEvent),
    Trx(TrxWsEvent),
    Session(LegacyWsEvent),
    System(SystemWsEvent),
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
                                debug!("Received WS command: {:?}", cmd);

                                let response = handle_ws_command(
                                    cmd,
                                    &user_id,
                                    &state,
                                    runner_client.as_ref(),
                                    conn_state.clone(),
                                )
                                .await;

                                if let Some(event) = response {
                                    debug!("Sending WS event to client: {:?}", event);
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
    if let Some(logger) = state.audit_logger.as_ref() {
        let (label, session_id, workspace_path) = ws_command_summary(&cmd);
        logger
            .log_ws_command(
                user_id,
                &label,
                session_id.as_deref(),
                workspace_path.as_deref(),
            )
            .await;
    }

    match cmd {
        WsCommand::Agent(agent_cmd) => {
            handle_agent_command(agent_cmd, user_id, state, runner_client, conn_state).await
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

fn ws_command_summary(cmd: &WsCommand) -> (String, Option<String>, Option<String>) {
    match cmd {
        WsCommand::Agent(agent_cmd) => {
            let label = match agent_cmd.payload {
                octo_protocol::commands::CommandPayload::SessionCreate { .. } => {
                    "agent.session_create"
                }
                octo_protocol::commands::CommandPayload::SessionClose => "agent.session_close",
                octo_protocol::commands::CommandPayload::SessionNew { .. } => "agent.session_new",
                octo_protocol::commands::CommandPayload::SessionSwitch { .. } => {
                    "agent.session_switch"
                }
                octo_protocol::commands::CommandPayload::Prompt { .. } => "agent.prompt",
                octo_protocol::commands::CommandPayload::Steer { .. } => "agent.steer",
                octo_protocol::commands::CommandPayload::FollowUp { .. } => "agent.follow_up",
                octo_protocol::commands::CommandPayload::Abort => "agent.abort",
                octo_protocol::commands::CommandPayload::InputResponse { .. } => {
                    "agent.input_response"
                }
                octo_protocol::commands::CommandPayload::GetState => "agent.get_state",
                octo_protocol::commands::CommandPayload::GetMessages => "agent.get_messages",
                octo_protocol::commands::CommandPayload::GetStats => "agent.get_stats",
                octo_protocol::commands::CommandPayload::GetModels { .. } => "agent.get_models",
                octo_protocol::commands::CommandPayload::GetCommands => "agent.get_commands",
                octo_protocol::commands::CommandPayload::GetForkPoints => "agent.get_fork_points",
                octo_protocol::commands::CommandPayload::ListSessions => "agent.list_sessions",
                octo_protocol::commands::CommandPayload::SetModel { .. } => "agent.set_model",
                octo_protocol::commands::CommandPayload::CycleModel => "agent.cycle_model",
                octo_protocol::commands::CommandPayload::SetThinkingLevel { .. } => {
                    "agent.set_thinking_level"
                }
                octo_protocol::commands::CommandPayload::CycleThinkingLevel => {
                    "agent.cycle_thinking_level"
                }
                octo_protocol::commands::CommandPayload::SetAutoCompaction { .. } => {
                    "agent.set_auto_compaction"
                }
                octo_protocol::commands::CommandPayload::SetAutoRetry { .. } => {
                    "agent.set_auto_retry"
                }
                octo_protocol::commands::CommandPayload::Compact { .. } => "agent.compact",
                octo_protocol::commands::CommandPayload::AbortRetry => "agent.abort_retry",
                octo_protocol::commands::CommandPayload::SetSessionName { .. } => {
                    "agent.set_session_name"
                }
                octo_protocol::commands::CommandPayload::Fork { .. } => "agent.fork",
                octo_protocol::commands::CommandPayload::Delegate(_) => "agent.delegate",
                octo_protocol::commands::CommandPayload::DelegateCancel(_) => {
                    "agent.delegate_cancel"
                }
            };
            (label.to_string(), Some(agent_cmd.session_id.clone()), None)
        }
        WsCommand::Files(files_cmd) => {
            let label = match files_cmd {
                FilesWsCommand::Tree { .. } => "files.tree",
                FilesWsCommand::Read { .. } => "files.read",
                FilesWsCommand::Write { .. } => "files.write",
                FilesWsCommand::List { .. } => "files.list",
                FilesWsCommand::Stat { .. } => "files.stat",
                FilesWsCommand::Delete { .. } => "files.delete",
                FilesWsCommand::CreateDirectory { .. } => "files.create_dir",
                FilesWsCommand::Rename { .. } => "files.rename",
                FilesWsCommand::Copy { .. } => "files.copy",
                FilesWsCommand::Move { .. } => "files.move",
            };
            let workspace_path = match files_cmd {
                FilesWsCommand::Tree { workspace_path, .. }
                | FilesWsCommand::Read { workspace_path, .. }
                | FilesWsCommand::Write { workspace_path, .. }
                | FilesWsCommand::List { workspace_path, .. }
                | FilesWsCommand::Stat { workspace_path, .. }
                | FilesWsCommand::Delete { workspace_path, .. }
                | FilesWsCommand::CreateDirectory { workspace_path, .. }
                | FilesWsCommand::Rename { workspace_path, .. }
                | FilesWsCommand::Copy { workspace_path, .. }
                | FilesWsCommand::Move { workspace_path, .. } => workspace_path.clone(),
            };
            (label.to_string(), None, workspace_path)
        }
        WsCommand::Terminal(term_cmd) => {
            let label = match term_cmd {
                TerminalWsCommand::Open { .. } => "terminal.open",
                TerminalWsCommand::Input { .. } => "terminal.input",
                TerminalWsCommand::Resize { .. } => "terminal.resize",
                TerminalWsCommand::Close { .. } => "terminal.close",
            };
            let (session_id, workspace_path) = match term_cmd {
                TerminalWsCommand::Open {
                    session_id,
                    workspace_path,
                    ..
                } => (session_id.clone(), workspace_path.clone()),
                _ => (None, None),
            };
            (label.to_string(), session_id, workspace_path)
        }
        WsCommand::Hstry(_) => ("hstry.query".to_string(), None, None),
        WsCommand::Trx(trx_cmd) => {
            let label = match trx_cmd {
                TrxWsCommand::List { .. } => "trx.list",
                TrxWsCommand::Create { .. } => "trx.create",
                TrxWsCommand::Update { .. } => "trx.update",
                TrxWsCommand::Close { .. } => "trx.close",
                TrxWsCommand::Sync { .. } => "trx.sync",
            };
            let workspace_path = match trx_cmd {
                TrxWsCommand::List { workspace_path, .. }
                | TrxWsCommand::Create { workspace_path, .. }
                | TrxWsCommand::Update { workspace_path, .. }
                | TrxWsCommand::Close { workspace_path, .. }
                | TrxWsCommand::Sync { workspace_path, .. } => Some(workspace_path.clone()),
            };
            (label.to_string(), None, workspace_path)
        }
        WsCommand::Session(session_cmd) => {
            let session_id = extract_legacy_session_id(&session_cmd.cmd);
            ("session.legacy".to_string(), session_id, None)
        }
    }
}

/// Build a canonical `CommandResponse` event wrapped in `WsEvent::Agent`.
fn agent_response_with_runner(
    runner_id: &str,
    session_id: &str,
    id: Option<String>,
    cmd: &str,
    result: Result<Option<Value>, String>,
) -> WsEvent {
    let (success, data, error) = match result {
        Ok(data) => (true, data, None),
        Err(e) => (false, None, Some(e)),
    };
    WsEvent::Agent(octo_protocol::events::Event {
        session_id: session_id.to_string(),
        runner_id: runner_id.to_string(),
        ts: Utc::now().timestamp_millis(),
        payload: octo_protocol::events::EventPayload::Response(
            octo_protocol::events::CommandResponse {
                id: id.unwrap_or_default(),
                cmd: cmd.to_string(),
                success,
                data,
                error,
            },
        ),
    })
}

/// Handle canonical agent commands.
///
/// Every command gets a `CommandResponse` event back (or `None` for fire-and-forget
/// commands like prompt/steer/abort where streaming events are the real response).
async fn handle_agent_command(
    cmd: octo_protocol::commands::Command,
    user_id: &str,
    state: &AppState,
    runner_client: Option<&RunnerClient>,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    use octo_protocol::commands::CommandPayload;

    let id = cmd.id.clone();
    let session_id = cmd.session_id.clone();
    let runner_id = cmd.runner_id.clone().unwrap_or_else(|| "local".to_string());
    let agent_response =
        |session_id: &str, id: Option<String>, cmd: &str, result: Result<Option<Value>, String>| {
            agent_response_with_runner(&runner_id, session_id, id, cmd, result)
        };

    // Check if runner is available
    let runner = match runner_client {
        Some(r) => r,
        None => {
            return Some(agent_response(
                &session_id,
                id,
                "error",
                Err("Runner not available".into()),
            ));
        }
    };

    match cmd.payload {
        CommandPayload::SessionCreate { config } => {
            info!(
                "agent session.create: user={}, session_id={}",
                user_id, session_id
            );

            // If this connection already has an active subscription for
            // this session, return success immediately. This handles the
            // common case of React StrictMode double-invoke or reconnection
            // re-sending session.create for a session that's already alive.
            {
                let state_guard = conn_state.lock().await;
                if state_guard.pi_subscriptions.contains(&session_id) {
                    debug!(
                        "agent session.create: session {} already subscribed, returning success",
                        session_id
                    );
                    return Some(agent_response(
                        &session_id,
                        id,
                        "session.create",
                        Ok(Some(serde_json::json!({ "session_id": session_id }))),
                    ));
                }
            }

            let cwd = config
                .cwd
                .as_ref()
                .map(|s| std::path::PathBuf::from(s))
                .unwrap_or_else(|| std::path::PathBuf::from("/"));

            {
                let mut state_guard = conn_state.lock().await;
                state_guard.pi_session_meta.insert(
                    session_id.clone(),
                    PiSessionMeta {
                        scope: Some(config.harness.clone()),
                        cwd: Some(cwd.clone()),
                    },
                );
            }

            // If no explicit continue_session was provided, try to find an
            // existing Pi JSONL session file for this session ID. This enables
            // resuming external sessions (started in Pi directly, not through
            // Octo) so the agent has the full conversation context.
            let continue_session = if config.continue_session.is_some() {
                config.continue_session.map(std::path::PathBuf::from)
            } else {
                crate::pi::session_files::find_session_file_async(
                    session_id.clone(),
                    Some(cwd.clone()),
                )
                .await
            };

            if continue_session.is_some() {
                debug!(
                    "agent session.create: found session file for {}: {:?}",
                    session_id,
                    continue_session.as_ref().unwrap()
                );
            }

            let pi_config = RunnerPiSessionConfig {
                cwd,
                provider: config.provider,
                model: config.model,
                session_file: None,
                continue_session,
                env: std::collections::HashMap::new(),
            };

            let req = PiCreateSessionRequest {
                session_id: session_id.clone(),
                config: pi_config,
            };

            match runner.pi_create_session(req).await {
                Ok(_resp) => {
                    // Session stored under the provisional ID. Pi may
                    // assign a different real ID -- the runner re-keys
                    // its map in the background, and the frontend learns
                    // about it via the get_state response.

                    // Auto-subscribe to events for the session.
                    // We MUST wait for the subscription to be established
                    // before returning the session.create response, otherwise
                    // the frontend may send a prompt before events are being
                    // forwarded, causing streaming to silently fail.
                    let mut state_guard = conn_state.lock().await;
                    if !state_guard.pi_subscriptions.contains(&session_id) {
                        state_guard.subscribed_sessions.insert(session_id.clone());
                        state_guard.pi_subscriptions.insert(session_id.clone());
                        let event_tx = state_guard.event_tx.clone();
                        let runner = runner.clone();
                        let sid = session_id.clone();

                        // Use a oneshot channel to wait for subscription confirmation
                        let (sub_ready_tx, sub_ready_rx) = oneshot::channel::<()>();
                        let runner_id = runner_id.clone();
                        tokio::spawn(async move {
                            if let Err(e) = forward_pi_events(
                                &runner,
                                &sid,
                                event_tx,
                                Some(sub_ready_tx),
                                runner_id,
                            )
                            .await
                            {
                                error!("Event forwarding error for session {}: {:?}", sid, e);
                            }
                        });

                        // Wait for the subscription to be confirmed (with timeout)
                        drop(state_guard);
                        match tokio::time::timeout(Duration::from_secs(5), sub_ready_rx).await {
                            Ok(Ok(())) => {
                                debug!("Event subscription established for session {}", session_id);
                            }
                            Ok(Err(_)) => {
                                warn!(
                                    "Event subscription sender dropped for session {} (forward_pi_events may have failed early)",
                                    session_id
                                );
                            }
                            Err(_) => {
                                warn!(
                                    "Timed out waiting for event subscription for session {}",
                                    session_id
                                );
                            }
                        }
                    } else {
                        drop(state_guard);
                    }

                    Some(agent_response(
                        &session_id,
                        id,
                        "session.create",
                        Ok(Some(serde_json::json!({ "session_id": session_id }))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.create",
                    Err(format!("Failed to create session: {}", e)),
                )),
            }
        }

        CommandPayload::SessionClose => {
            info!(
                "agent session.close: user={}, session_id={}",
                user_id, session_id
            );

            let mut state_guard = conn_state.lock().await;
            state_guard.subscribed_sessions.remove(&session_id);
            state_guard.pi_subscriptions.remove(&session_id);
            drop(state_guard);

            match runner.pi_close_session(&session_id).await {
                Ok(()) => Some(agent_response(&session_id, id, "session.close", Ok(None))),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.close",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SessionNew { parent_session } => {
            debug!(
                "agent session.new: user={}, session_id={}",
                user_id, session_id
            );
            match runner
                .pi_new_session(&session_id, parent_session.as_deref())
                .await
            {
                Ok(()) => Some(agent_response(
                    &session_id,
                    id,
                    "session.new",
                    Ok(Some(serde_json::json!({ "session_id": session_id }))),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.new",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SessionSwitch { session_path } => {
            debug!(
                "agent session.switch: user={}, session_id={}, path={}",
                user_id, session_id, session_path
            );
            match runner.pi_switch_session(&session_id, &session_path).await {
                Ok(()) => Some(agent_response(
                    &session_id,
                    id,
                    "session.switch",
                    Ok(Some(serde_json::json!({ "session_id": session_id }))),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.switch",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::Prompt {
            message, client_id, ..
        } => {
            info!(
                "agent prompt: user={}, session_id={}, len={}, client_id={:?}",
                user_id,
                session_id,
                message.len(),
                client_id
            );
            match runner.pi_prompt(&session_id, &message, client_id).await {
                Ok(()) => None, // Streaming events are the response
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "prompt",
                    Err(format!("Failed to send prompt: {}", e)),
                )),
            }
        }

        CommandPayload::Steer { message } => {
            info!(
                "agent steer: user={}, session_id={}, len={}",
                user_id,
                session_id,
                message.len()
            );
            match runner.pi_steer(&session_id, &message).await {
                Ok(()) => None,
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "steer",
                    Err(format!("Failed to send steer: {}", e)),
                )),
            }
        }

        CommandPayload::FollowUp { message } => {
            info!(
                "agent follow_up: user={}, session_id={}, len={}",
                user_id,
                session_id,
                message.len()
            );
            match runner.pi_follow_up(&session_id, &message).await {
                Ok(()) => None,
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "follow_up",
                    Err(format!("Failed to send follow_up: {}", e)),
                )),
            }
        }

        CommandPayload::Abort => {
            info!("agent abort: user={}, session_id={}", user_id, session_id);
            match runner.pi_abort(&session_id).await {
                Ok(()) => None,
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "abort",
                    Err(format!("Failed to abort: {}", e)),
                )),
            }
        }

        CommandPayload::InputResponse {
            request_id,
            value,
            confirmed,
            cancelled,
        } => {
            debug!(
                "agent input_response: user={}, session_id={}, req={}",
                user_id, session_id, request_id
            );
            match runner
                .pi_extension_ui_response(
                    &session_id,
                    &request_id,
                    value.as_deref(),
                    confirmed,
                    cancelled,
                )
                .await
            {
                Ok(()) => Some(agent_response(&session_id, id, "input_response", Ok(None))),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "input_response",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::GetState => {
            debug!(
                "agent get_state: user={}, session_id={}",
                user_id, session_id
            );
            match runner.pi_get_state(&session_id).await {
                Ok(resp) => {
                    let state_value = serde_json::to_value(&resp.state).unwrap_or(Value::Null);
                    Some(agent_response(
                        &session_id,
                        id,
                        "get_state",
                        Ok(Some(state_value)),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "get_state",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::GetMessages => {
            debug!(
                "agent get_messages: user={}, session_id={}",
                user_id, session_id
            );
            handle_get_messages(
                id,
                &session_id,
                user_id,
                state,
                runner,
                conn_state,
                &runner_id,
            )
            .await
        }

        CommandPayload::GetStats => {
            debug!(
                "agent get_stats: user={}, session_id={}",
                user_id, session_id
            );
            match runner.pi_get_session_stats(&session_id).await {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "get_stats",
                    Ok(Some(serde_json::to_value(&resp.stats).unwrap_or_default())),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "get_stats",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::GetModels { workdir } => {
            debug!(
                "agent get_models: user={}, session_id={}, workdir={:?}",
                user_id, session_id, workdir
            );
            match runner
                .pi_get_available_models(&session_id, workdir.as_deref())
                .await
            {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "get_models",
                    Ok(Some(serde_json::to_value(&resp.models).unwrap_or_default())),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "get_models",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::GetCommands => {
            debug!(
                "agent get_commands: user={}, session_id={}",
                user_id, session_id
            );
            match runner.pi_get_commands(&session_id).await {
                Ok(resp) => {
                    let commands: Vec<Value> = resp
                        .commands
                        .into_iter()
                        .map(|c| {
                            serde_json::json!({
                                "name": c.name,
                                "description": c.description,
                                "type": c.source,
                            })
                        })
                        .collect();
                    Some(agent_response(
                        &session_id,
                        id,
                        "get_commands",
                        Ok(Some(Value::Array(commands))),
                    ))
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("PiSessionNotFound") || msg.contains("SessionNotFound") {
                        return Some(agent_response(
                            &session_id,
                            id,
                            "get_commands",
                            Ok(Some(Value::Array(Vec::new()))),
                        ));
                    }
                    Some(agent_response(&session_id, id, "get_commands", Err(msg)))
                }
            }
        }

        CommandPayload::GetForkPoints => {
            debug!(
                "agent get_fork_points: user={}, session_id={}",
                user_id, session_id
            );
            match runner.pi_get_fork_messages(&session_id).await {
                Ok(resp) => {
                    let messages: Vec<Value> = resp
                        .messages
                        .into_iter()
                        .map(|m| {
                            serde_json::json!({
                                "entry_id": m.entry_id,
                                "role": "user",
                                "preview": m.text,
                            })
                        })
                        .collect();
                    Some(agent_response(
                        &session_id,
                        id,
                        "get_fork_points",
                        Ok(Some(Value::Array(messages))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "get_fork_points",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetModel { provider, model_id } => {
            debug!(
                "agent set_model: user={}, session_id={}, {}:{}",
                user_id, session_id, provider, model_id
            );
            match runner.pi_set_model(&session_id, &provider, &model_id).await {
                Ok(resp) => {
                    // Emit ConfigModelChanged event so the frontend UI updates.
                    let config_event = WsEvent::Agent(octo_protocol::events::Event {
                        session_id: session_id.clone(),
                        runner_id: runner_id.clone(),
                        ts: Utc::now().timestamp_millis(),
                        payload: octo_protocol::events::EventPayload::ConfigModelChanged {
                            provider: resp.model.provider.clone(),
                            model_id: resp.model.id.clone(),
                        },
                    });
                    let state_guard = conn_state.lock().await;
                    let _ = state_guard.event_tx.send(config_event);
                    drop(state_guard);

                    // Update hstry conversation with new model/provider
                    if let Some(hstry) = state.hstry.as_ref() {
                        let model_id_clone = resp.model.id.clone();
                        let provider_clone = resp.model.provider.clone();
                        let sid = session_id.clone();
                        let hstry = hstry.clone();
                        tokio::spawn(async move {
                            if let Err(e) = hstry
                                .update_conversation(
                                    &sid,
                                    None,
                                    None,
                                    Some(model_id_clone),
                                    Some(provider_clone),
                                    None,
                                    None,
                                    None,
                                    None,
                                )
                                .await
                            {
                                debug!("Failed to update hstry model on set_model: {}", e);
                            }
                        });
                    }

                    Some(agent_response(
                        &session_id,
                        id,
                        "set_model",
                        Ok(Some(serde_json::json!({
                            "provider": resp.model.provider,
                            "model_id": resp.model.id,
                        }))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_model",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::CycleModel => {
            debug!(
                "agent cycle_model: user={}, session_id={}",
                user_id, session_id
            );
            match runner.pi_cycle_model(&session_id).await {
                Ok(resp) => {
                    // Emit ConfigModelChanged event so the frontend UI updates.
                    let config_event = WsEvent::Agent(octo_protocol::events::Event {
                        session_id: session_id.clone(),
                        runner_id: runner_id.clone(),
                        ts: Utc::now().timestamp_millis(),
                        payload: octo_protocol::events::EventPayload::ConfigModelChanged {
                            provider: resp.model.provider.clone(),
                            model_id: resp.model.id.clone(),
                        },
                    });
                    let state_guard = conn_state.lock().await;
                    let _ = state_guard.event_tx.send(config_event);
                    drop(state_guard);

                    Some(agent_response(
                        &session_id,
                        id,
                        "cycle_model",
                        Ok(Some(serde_json::json!({
                            "provider": resp.model.provider,
                            "model_id": resp.model.id,
                        }))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "cycle_model",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetThinkingLevel { level } => {
            debug!(
                "agent set_thinking_level: user={}, session_id={}, level={}",
                user_id, session_id, level
            );
            match runner.pi_set_thinking_level(&session_id, &level).await {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "set_thinking_level",
                    Ok(Some(serde_json::json!({ "level": resp.level }))),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_thinking_level",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::CycleThinkingLevel => {
            debug!(
                "agent cycle_thinking_level: user={}, session_id={}",
                user_id, session_id
            );
            match runner.pi_cycle_thinking_level(&session_id).await {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "cycle_thinking_level",
                    Ok(Some(serde_json::json!({ "level": resp.level }))),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "cycle_thinking_level",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetAutoCompaction { enabled } => {
            debug!(
                "agent set_auto_compaction: user={}, session_id={}, enabled={}",
                user_id, session_id, enabled
            );
            match runner.pi_set_auto_compaction(&session_id, enabled).await {
                Ok(()) => Some(agent_response(
                    &session_id,
                    id,
                    "set_auto_compaction",
                    Ok(None),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_auto_compaction",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetAutoRetry { enabled } => {
            debug!(
                "agent set_auto_retry: user={}, session_id={}, enabled={}",
                user_id, session_id, enabled
            );
            match runner.pi_set_auto_retry(&session_id, enabled).await {
                Ok(()) => Some(agent_response(&session_id, id, "set_auto_retry", Ok(None))),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_auto_retry",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::Compact { instructions } => {
            info!("agent compact: user={}, session_id={}", user_id, session_id);
            match runner
                .pi_compact(&session_id, instructions.as_deref())
                .await
            {
                Ok(()) => None, // Compaction events stream via subscription
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "compact",
                    Err(format!("Failed to compact: {}", e)),
                )),
            }
        }

        CommandPayload::AbortRetry => {
            debug!(
                "agent abort_retry: user={}, session_id={}",
                user_id, session_id
            );
            match runner.pi_abort_retry(&session_id).await {
                Ok(()) => Some(agent_response(&session_id, id, "abort_retry", Ok(None))),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "abort_retry",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetSessionName { name } => {
            debug!(
                "agent set_session_name: user={}, session_id={}, name={}",
                user_id, session_id, name
            );
            match runner.pi_set_session_name(&session_id, &name).await {
                Ok(()) => Some(agent_response(
                    &session_id,
                    id,
                    "set_session_name",
                    Ok(None),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_session_name",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::Fork { entry_id } => {
            debug!(
                "agent fork: user={}, session_id={}, entry_id={}",
                user_id, session_id, entry_id
            );
            match runner.pi_fork(&session_id, &entry_id).await {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "fork",
                    Ok(Some(serde_json::json!({
                        "text": resp.text,
                        "cancelled": resp.cancelled,
                    }))),
                )),
                Err(e) => Some(agent_response(&session_id, id, "fork", Err(e.to_string()))),
            }
        }

        CommandPayload::ListSessions => {
            debug!("agent list_sessions: user={}", user_id);
            match runner.pi_list_sessions().await {
                Ok(sessions) => {
                    let sessions_json: Vec<Value> = sessions
                        .iter()
                        .map(|s| {
                            let mut obj = serde_json::json!({
                                "session_id": s.session_id,
                                "state": s.state,
                                "cwd": s.cwd,
                                "provider": s.provider,
                                "model": s.model,
                                "last_activity": s.last_activity,
                                "subscriber_count": s.subscriber_count,
                            });
                            if let Some(ref hid) = s.hstry_id {
                                obj["hstry_id"] = serde_json::Value::String(hid.clone());
                            }
                            obj
                        })
                        .collect();
                    Some(agent_response(
                        &session_id,
                        id,
                        "list_sessions",
                        Ok(Some(serde_json::json!({ "sessions": sessions_json }))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "list_sessions",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::Delegate(_) | CommandPayload::DelegateCancel(_) => {
            // Delegation not yet implemented
            Some(agent_response(
                &session_id,
                id,
                "delegate",
                Err("Delegation not yet implemented".into()),
            ))
        }
    }
}

/// Handle get_messages command with multi-source message loading.
///
/// Tries sources in order: cache, workspace JSONL, main chat JSONL,
/// hstry (via runner or direct), then runner's live Pi process.
async fn handle_get_messages(
    id: Option<String>,
    session_id: &str,
    user_id: &str,
    state: &AppState,
    runner: &RunnerClient,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
    runner_id: &str,
) -> Option<WsEvent> {
    let agent_response =
        |session_id: &str, id: Option<String>, cmd: &str, result: Result<Option<Value>, String>| {
            agent_response_with_runner(runner_id, session_id, id, cmd, result)
        };

    let (session_meta, is_active) = {
        let state_guard = conn_state.lock().await;
        (
            state_guard.pi_session_meta.get(session_id).cloned(),
            state_guard.pi_subscriptions.contains(session_id),
        )
    };

    // Check cache
    if let Some(cached) = get_cached_pi_messages(user_id, session_id).await {
        let use_cached = !is_active || cached.age <= Duration::from_secs(2);
        if use_cached {
            return Some(agent_response(
                session_id,
                id,
                "get_messages",
                Ok(Some(serde_json::json!({ "messages": cached.messages }))),
            ));
        }
    }

    // Try hstry for historical messages
    let is_multi_user = state.linux_users.is_some();

    if let Some(meta) = session_meta.as_ref()
        && (meta.scope.as_deref() == Some("workspace") || meta.scope.as_deref() == Some("pi"))
        && let Some(work_dir) = meta.cwd.as_ref()
    {
        if is_multi_user {
            match runner
                .get_workspace_chat_messages(
                    work_dir.to_string_lossy().to_string(),
                    session_id.to_string(),
                    None,
                )
                .await
            {
                Ok(resp) if !resp.messages.is_empty() => {
                    let messages: Vec<serde_json::Value> = resp
                        .messages
                        .into_iter()
                        .map(|m| serde_json::json!({
                            "id": m.id, "role": m.role, "content": m.content, "timestamp": m.timestamp,
                        }))
                        .collect();
                    let messages_value = serde_json::Value::Array(messages);
                    cache_pi_messages(user_id, session_id, &messages_value).await;
                    return Some(agent_response(
                        session_id,
                        id,
                        "get_messages",
                        Ok(Some(serde_json::json!({ "messages": messages_value }))),
                    ));
                }
                Ok(_) => {}
                Err(e) => {
                    debug!(
                        "get_messages: hstry (workspace via runner) error for {}: {}",
                        session_id, e
                    );
                }
            }
        } else if let Some(hstry_client) = state.hstry.as_ref() {
            match hstry_client.get_messages(session_id, None, None).await {
                Ok(hstry_messages) if !hstry_messages.is_empty() => {
                    let serializable = crate::hstry::proto_messages_to_serializable(hstry_messages);
                    let messages_value = serde_json::to_value(&serializable).unwrap_or_default();
                    cache_pi_messages(user_id, session_id, &messages_value).await;
                    return Some(agent_response(
                        session_id,
                        id,
                        "get_messages",
                        Ok(Some(serde_json::json!({ "messages": messages_value }))),
                    ));
                }
                Ok(_) => {}
                Err(e) => {
                    debug!(
                        "get_messages: hstry (workspace) error for {}: {}",
                        session_id, e
                    );
                }
            }
        }
    } else if is_multi_user {
        match runner.get_main_chat_messages(session_id, None).await {
            Ok(resp) if !resp.messages.is_empty() => {
                let messages: Vec<serde_json::Value> = resp
                    .messages
                    .into_iter()
                    .map(|m| serde_json::json!({
                        "id": m.id, "role": m.role, "content": m.content, "timestamp": m.timestamp,
                    }))
                    .collect();
                let messages_value = serde_json::Value::Array(messages);
                cache_pi_messages(user_id, session_id, &messages_value).await;
                return Some(agent_response(
                    session_id,
                    id,
                    "get_messages",
                    Ok(Some(serde_json::json!({ "messages": messages_value }))),
                ));
            }
            Ok(_) => {}
            Err(e) => {
                debug!(
                    "get_messages: hstry (via runner) error for {}: {}",
                    session_id, e
                );
            }
        }
    } else if let Some(hstry_client) = state.hstry.as_ref() {
        match hstry_client.get_messages(session_id, None, None).await {
            Ok(hstry_messages) if !hstry_messages.is_empty() => {
                let serializable = crate::hstry::proto_messages_to_serializable(hstry_messages);
                let messages_value = serde_json::to_value(&serializable).unwrap_or_default();
                cache_pi_messages(user_id, session_id, &messages_value).await;
                return Some(agent_response(
                    session_id,
                    id,
                    "get_messages",
                    Ok(Some(serde_json::json!({ "messages": messages_value }))),
                ));
            }
            Ok(_) => {}
            Err(e) => {
                debug!("get_messages: hstry error for {}: {}", session_id, e);
            }
        }
    }

    // Last resort: try runner's live Pi process
    match runner.pi_get_messages(session_id).await {
        Ok(resp) => {
            let messages_value = serde_json::to_value(&resp.messages).unwrap_or_default();
            cache_pi_messages(user_id, session_id, &messages_value).await;
            Some(agent_response(
                session_id,
                id,
                "get_messages",
                Ok(Some(serde_json::json!({ "messages": messages_value }))),
            ))
        }
        Err(e) => Some(agent_response(
            session_id,
            id,
            "get_messages",
            Err(e.to_string()),
        )),
    }
}

/// Forward canonical events from runner subscription to WebSocket.
///
/// The runner's PiTranslator has already converted native Pi events to
/// canonical format. We just wrap them as `WsEvent::Agent` and send.
///
/// If `sub_ready_tx` is provided, signals it once the runner subscription
/// is confirmed. This allows callers to wait for the subscription before
/// sending prompts, preventing the race where events are missed.
async fn forward_pi_events(
    runner: &RunnerClient,
    session_id: &str,
    event_tx: mpsc::UnboundedSender<WsEvent>,
    sub_ready_tx: Option<oneshot::Sender<()>>,
    runner_id: String,
) -> anyhow::Result<()> {
    info!(
        "forward_pi_events: connecting subscription for session {}",
        session_id
    );
    let mut subscription = runner.pi_subscribe(session_id).await?;
    info!(
        "forward_pi_events: subscription established for session {}",
        session_id
    );

    // Signal that the subscription is ready
    if let Some(tx) = sub_ready_tx {
        let _ = tx.send(());
    }

    loop {
        match subscription.next().await {
            Some(PiSubscriptionEvent::Event(canonical_event)) => {
                if event_tx.send(WsEvent::Agent(canonical_event)).is_err() {
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
                // Emit error as canonical agent.error event
                let error_event = octo_protocol::events::Event {
                    session_id: session_id.to_string(),
                    runner_id: runner_id.clone(),
                    ts: chrono::Utc::now().timestamp_millis(),
                    payload: octo_protocol::events::EventPayload::AgentError {
                        error: format!("Subscription error ({:?}): {}", code, message),
                        recoverable: false,
                        phase: None,
                    },
                };
                let _ = event_tx.send(WsEvent::Agent(error_event));
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

// NOTE: The old pi_event_to_ws_event() function has been removed.
// Streaming events now flow as canonical events through the PiTranslator
// in pi_manager.rs and are forwarded directly via WsEvent::Agent.

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
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<FileTreeNode>, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            let resolved = resolve_workspace_child(workspace_root, relative_path)?;
            let entries = user_plane
                .list_directory(&resolved, include_hidden)
                .await
                .map_err(|e| {
                    format!("list_directory failed for {}: {:#}", resolved.display(), e)
                })?;

            // Separate directories (need recursive fetch) from files (instant)
            let mut file_nodes = Vec::new();
            let mut dir_entries = Vec::new();

            for entry in entries {
                let child_path = join_relative_path(relative_path, &entry.name);
                if entry.is_dir && depth > 1 {
                    dir_entries.push((entry, child_path));
                } else {
                    file_nodes.push(map_tree_node(&entry, child_path, None));
                }
            }

            // Fetch all subdirectories concurrently
            let dir_futures: Vec<_> = dir_entries
                .iter()
                .map(|(_, child_path)| {
                    build_tree(
                        user_plane,
                        workspace_root,
                        child_path,
                        depth - 1,
                        include_hidden,
                    )
                })
                .collect();

            let dir_results = futures::future::join_all(dir_futures).await;

            // Build directory nodes from results, preserving original order
            let mut nodes = Vec::with_capacity(file_nodes.len() + dir_entries.len());
            for ((entry, child_path), result) in dir_entries.into_iter().zip(dir_results) {
                let children = Some(result?);
                nodes.push(map_tree_node(&entry, child_path, children));
            }
            nodes.append(&mut file_nodes);

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

            let dest_stat = user_plane.stat(to_path).await.map_err(|e| e.to_string())?;
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
            match build_tree(
                &user_plane,
                &workspace_root,
                &path,
                max_depth,
                include_hidden,
            )
            .await
            {
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
                    let encoded = base64::engine::general_purpose::STANDARD.encode(content.content);
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
                Ok(entries) => Some(WsEvent::Files(FilesWsEvent::ListResult {
                    id,
                    path,
                    entries,
                })),
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
            match user_plane.create_directory(&resolved, create_parents).await {
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
        FilesWsCommand::Rename { id, from, to, .. } => {
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
            let copy_result =
                copy_recursive(&user_plane, &from_resolved, &to_resolved, overwrite).await;
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
            state, user_id, session_id, session,
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
    Unix(tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>),
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
    Unix(futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>>),
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
                (
                    TtydConnectionWrite::Unix(write),
                    TtydConnectionRead::Unix(read),
                )
            }
            TtydConnection::Tcp(ws) => {
                let (write, read) = ws.split();
                (
                    TtydConnectionWrite::Tcp(write),
                    TtydConnectionRead::Tcp(read),
                )
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

        let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Exit { terminal_id }));
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
                warn!(
                    "Terminal not available: ttyd_port=0 for session {}",
                    session.id
                );
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
                return Some(WsEvent::Terminal(TerminalWsEvent::Opened {
                    id,
                    terminal_id,
                }));
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
            state_guard
                .terminal_sessions
                .insert(terminal_id.clone(), TerminalSession { command_tx, task });

            Some(WsEvent::Terminal(TerminalWsEvent::Opened {
                id,
                terminal_id,
            }))
        }
        TerminalWsCommand::Input {
            id,
            terminal_id,
            data,
        } => {
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
                let data = serde_json::to_value(&serializable).unwrap_or(Value::Null);
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
async fn handle_trx_command(cmd: TrxWsCommand, user_id: &str, state: &AppState) -> Option<WsEvent> {
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
                    success: resp
                        .get("synced")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
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
    _user_id: &str,
    _state: &AppState,
) -> Option<WsEvent> {
    let session_id = extract_legacy_session_id(&cmd.cmd);
    // Legacy Session channel commands targeted the OpenCode HTTP API which has been removed.
    // All agent interaction now flows through the Agent channel.
    Some(WsEvent::Session(LegacyWsEvent::Error {
        message: "Legacy session channel is deprecated. Use the agent channel instead.".to_string(),
        session_id,
    }))
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

fn resolve_workspace_root(workspace_path: Option<&str>) -> Result<std::path::PathBuf, String> {
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
    fn test_parse_agent_prompt_command() {
        let json = r#"{"channel":"agent","session_id":"ses_123","cmd":"prompt","message":"hello"}"#;
        let cmd: WsCommand = serde_json::from_str(json).unwrap();
        match cmd {
            WsCommand::Agent(octo_protocol::commands::Command {
                session_id,
                payload: octo_protocol::commands::CommandPayload::Prompt { message, .. },
                ..
            }) => {
                assert_eq!(session_id, "ses_123");
                assert_eq!(message, "hello");
            }
            _ => panic!("Expected Agent Prompt command"),
        }
    }

    #[test]
    fn test_parse_agent_get_state_command() {
        let json = r#"{"channel":"agent","session_id":"ses_456","cmd":"get_state"}"#;
        let cmd: WsCommand = serde_json::from_str(json).unwrap();
        match cmd {
            WsCommand::Agent(octo_protocol::commands::Command {
                session_id,
                payload: octo_protocol::commands::CommandPayload::GetState,
                ..
            }) => {
                assert_eq!(session_id, "ses_456");
            }
            _ => panic!("Expected Agent GetState command"),
        }
    }

    #[test]
    fn test_serialize_agent_command_response() {
        use octo_protocol::events::{CommandResponse, EventPayload};

        let event = WsEvent::Agent(octo_protocol::events::Event {
            session_id: "ses_123".into(),
            runner_id: "local".into(),
            ts: 1738764000000,
            payload: EventPayload::Response(CommandResponse {
                id: "req-1".into(),
                cmd: "session.create".into(),
                success: true,
                data: None,
                error: None,
            }),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""channel":"agent""#));
        assert!(json.contains(r#""event":"response""#));
        assert!(json.contains(r#""session_id":"ses_123""#));
        assert!(json.contains(r#""cmd":"session.create""#));
    }

    #[test]
    fn test_serialize_system_connected() {
        let event = WsEvent::System(SystemWsEvent::Connected);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""channel":"system""#));
        assert!(json.contains(r#""type":"connected""#));
    }

    #[test]
    fn test_serialize_canonical_agent_event() {
        use octo_protocol::events::{AgentPhase, EventPayload};

        let event = WsEvent::Agent(octo_protocol::events::Event {
            session_id: "ses_abc".into(),
            runner_id: "local".into(),
            ts: 1738764000000,
            payload: EventPayload::StreamTextDelta {
                message_id: "msg-1".into(),
                delta: "Hello".into(),
                content_index: 0,
            },
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""channel":"agent""#));
        assert!(json.contains(r#""event":"stream.text_delta""#));
        assert!(json.contains(r#""session_id":"ses_abc""#));
        assert!(json.contains(r#""delta":"Hello""#));
    }

    #[test]
    fn test_serialize_canonical_agent_idle() {
        use octo_protocol::events::EventPayload;

        let event = WsEvent::Agent(octo_protocol::events::Event {
            session_id: "ses_abc".into(),
            runner_id: "local".into(),
            ts: 1738764000000,
            payload: EventPayload::AgentIdle,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""channel":"agent""#));
        assert!(json.contains(r#""event":"agent.idle""#));
    }
}

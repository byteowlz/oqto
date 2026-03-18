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
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use chrono::Utc;

use base64::Engine;

use crate::auth::{Claims, CurrentUser};
use crate::local::ProcessManager;

use crate::runner::client::{PiSubscriptionEvent, RunnerClient};
use crate::runner::protocol::{PiCreateSessionRequest, PiSessionConfig as RunnerPiSessionConfig};
use crate::runner::router::{
    ExecutionTarget, resolve_runner_for_target, resolve_target_for_workspace_path,
};
use crate::session::Session;
use crate::session_target::{SessionTargetRecord, SessionTargetScope};
use crate::user_plane::{MeteredUserPlane, RunnerUserPlane, UserPlane, UserPlanePath};
use crate::ws::hub::WsHub;
use crate::ws::types::{WsCommand as LegacyWsCommand, WsEvent as LegacyHubEvent};

use super::error::ApiError;

const PI_MESSAGES_CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const PI_MESSAGES_CACHE_MAX_BYTES_PER_USER: usize = 100 * 1024 * 1024;
const PI_MESSAGES_CACHE_MAX_MESSAGES_PER_SESSION: usize = 200;
const RECENT_CLIENT_IDS_MAX_PER_SESSION: usize = 512;

// File tree traversal budgets (reliability + latency guardrails)
const TREE_MAX_DEPTH: usize = 8;
const TREE_MAX_NODES: usize = 20_000;
const TREE_MAX_TIME_MS: u64 = 3_000;
const TREE_MAX_CONCURRENCY: usize = 16;
const TREE_PAGE_DEFAULT_LIMIT: usize = 1_000;
const TREE_PAGE_MAX_LIMIT: usize = 5_000;

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

static RECENT_CLIENT_IDS: Lazy<tokio::sync::RwLock<HashMap<String, Vec<String>>>> =
    Lazy::new(|| tokio::sync::RwLock::new(HashMap::new()));

mod agent;
mod files;
mod history;
mod system;
mod terminal;

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

fn normalized_client_id(client_id: Option<&str>) -> Option<&str> {
    let client_id = client_id?;
    if client_id.is_empty() {
        return None;
    }
    Some(client_id)
}

async fn has_accepted_client_id(session_id: &str, client_id: Option<&str>) -> bool {
    let Some(client_id) = normalized_client_id(client_id) else {
        return false;
    };

    let map = RECENT_CLIENT_IDS.read().await;
    map.get(session_id)
        .is_some_and(|ids| ids.iter().any(|id| id == client_id))
}

async fn mark_client_id_accepted(session_id: &str, client_id: Option<&str>) {
    let Some(client_id) = normalized_client_id(client_id) else {
        return;
    };

    let mut map = RECENT_CLIENT_IDS.write().await;
    let ids = map.entry(session_id.to_string()).or_default();
    if ids.iter().any(|id| id == client_id) {
        return;
    }
    ids.push(client_id.to_string());
    if ids.len() > RECENT_CLIENT_IDS_MAX_PER_SESSION {
        let overflow = ids.len() - RECENT_CLIENT_IDS_MAX_PER_SESSION;
        ids.drain(0..overflow);
    }
}

async fn clear_client_ids_for_session(session_id: &str) {
    let mut map = RECENT_CLIENT_IDS.write().await;
    map.remove(session_id);
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
    Bus,
}

// ============================================================================
// Incoming Commands (Frontend -> Backend)
// ============================================================================

/// Commands sent from frontend to backend over WebSocket.
#[derive(Debug, Deserialize)]
#[serde(tag = "channel", rename_all = "snake_case")]
pub enum WsCommand {
    Agent(oqto_protocol::commands::Command),
    Files(FilesWsCommand),
    Terminal(TerminalWsCommand),
    Hstry(HstryWsCommand),
    Trx(TrxWsCommand),
    Session(SessionWsCommand),
    Bus(crate::bus::BusCommand),
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
        offset: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
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
    /// Copy a file or directory from one workspace to another.
    /// Both workspaces must belong to the current user (validated against sessions).
    CopyToWorkspace {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Workspace path of the source (must match a session's workspace_path).
        source_workspace_path: String,
        /// Relative path within the source workspace.
        source_path: String,
        /// Workspace path of the target (must match a session's workspace_path).
        target_workspace_path: String,
        /// Relative path within the target workspace.
        target_path: String,
    },
    /// Start watching a workspace directory for file changes.
    /// Sends FileChanged events when files are created, modified, or deleted.
    /// Only one watcher per workspace per connection; subsequent calls replace the previous watcher.
    WatchFiles {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        workspace_path: String,
    },
    /// Stop watching a workspace directory for file changes.
    UnwatchFiles {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        workspace_path: String,
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
/// `WsEvent::Agent` as canonical `oqto_protocol::events::Event` values.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "channel", rename_all = "snake_case")]
pub enum WsEvent {
    /// Canonical agent events (streaming, state, command responses, delegation, etc.).
    /// Serializes as `{"channel": "agent", "session_id": ..., "event": ..., ...}`.
    #[serde(rename = "agent")]
    Agent(oqto_protocol::events::Event),
    Files(FilesWsEvent),
    Terminal(TerminalWsEvent),
    Hstry(HstryWsEvent),
    Trx(TrxWsEvent),
    System(SystemWsEvent),
    Bus(crate::bus::BusWsEvent),
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
        #[serde(skip_serializing_if = "Option::is_none")]
        truncated: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        visited_nodes: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        elapsed_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        next_offset: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_entries: Option<usize>,
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
    CopyToWorkspaceResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        source_workspace_path: String,
        target_workspace_path: String,
        files_copied: usize,
        success: bool,
    },
    /// Emitted when a file or directory changes in a watched workspace.
    FileChanged {
        /// Type of change: "file_created", "file_modified", "file_deleted",
        /// "dir_created", "dir_deleted"
        event_type: String,
        /// Relative path within the workspace
        path: String,
        /// "file" or "directory"
        entry_type: String,
        /// Workspace path being watched
        workspace_path: String,
    },
    WatchFilesResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        workspace_path: String,
        success: bool,
    },
    UnwatchFilesResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        workspace_path: String,
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

#[derive(Debug, Clone)]
struct TreeBuildResult {
    nodes: Vec<FileTreeNode>,
    next_offset: Option<usize>,
    total_entries: usize,
}

#[derive(Debug, Clone)]
struct TreeTraversalContext {
    deadline: Instant,
    max_nodes: usize,
    visited_nodes: Arc<std::sync::atomic::AtomicUsize>,
    stop_reason: Arc<Mutex<Option<String>>>,
    semaphore: Arc<Semaphore>,
}

impl TreeTraversalContext {
    fn new(max_nodes: usize, max_time: Duration, max_concurrency: usize) -> Self {
        Self {
            deadline: Instant::now() + max_time,
            max_nodes,
            visited_nodes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            stop_reason: Arc::new(Mutex::new(None)),
            semaphore: Arc::new(Semaphore::new(max_concurrency.max(1))),
        }
    }

    async fn mark_stop_reason_if_empty(&self, reason: &str) {
        let mut guard = self.stop_reason.lock().await;
        if guard.is_none() {
            *guard = Some(reason.to_string());
        }
    }

    async fn should_stop(&self) -> bool {
        if Instant::now() >= self.deadline {
            self.mark_stop_reason_if_empty("timeout").await;
            return true;
        }
        if self
            .visited_nodes
            .load(std::sync::atomic::Ordering::Relaxed)
            >= self.max_nodes
        {
            self.mark_stop_reason_if_empty("max_nodes").await;
            return true;
        }
        false
    }

    async fn try_visit_node(&self) -> bool {
        let next = self
            .visited_nodes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        if next > self.max_nodes {
            self.mark_stop_reason_if_empty("max_nodes").await;
            return false;
        }
        true
    }

    async fn stop_reason(&self) -> Option<String> {
        self.stop_reason.lock().await.clone()
    }

    fn visited_nodes(&self) -> usize {
        self.visited_nodes
            .load(std::sync::atomic::Ordering::Relaxed)
    }
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
    /// General error. If this was caused by a specific command, includes correlation ID.
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        error: String,
    },
    /// Ping for keep-alive
    Ping,
    /// Shared workspace membership/metadata changed.
    #[serde(rename = "shared_workspace.updated")]
    SharedWorkspaceUpdated {
        workspace_id: String,
        change_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<Value>,
    },
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
    let is_admin = user.is_admin();

    Ok(ws.on_upgrade(move |socket| {
        handle_multiplexed_ws(socket, state, user_id, is_admin)
    }))
}

/// Create a runner client for a user if multi-user mode is enabled.
fn runner_client_for_user(state: &AppState, user_id: &str) -> Option<RunnerClient> {
    runner_client_for_linux_user(state, user_id, None)
}

/// Resolve a runner client for a specific Linux username.
///
/// If `linux_username_override` is provided, use that instead of looking up
/// from user_id. This is used for shared workspaces where the runner runs
/// as the shared workspace's Linux user, not the requesting user's.
fn runner_client_for_linux_user(
    state: &AppState,
    user_id: &str,
    linux_username_override: Option<&str>,
) -> Option<RunnerClient> {
    // Check if we have a socket pattern configured
    if let Some(pattern) = state.runner_socket_pattern.as_deref() {
        let linux_username = linux_username_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                state
                    .linux_users
                    .as_ref()
                    .map(|lu| lu.linux_username(user_id))
                    .unwrap_or_else(|| user_id.to_string())
            });

        // Use for_user_with_pattern which handles both {user} and {uid} placeholders.
        // Don't pre-check socket existence -- the runner client retries on
        // transient connection failures during service restarts.
        match RunnerClient::for_user_with_pattern(&linux_username, pattern) {
            Ok(c) => return Some(c),
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

/// Resolve the runner client for a given workspace path.
///
/// If the path is inside a shared workspace, returns a runner client for the
/// shared workspace's Linux user. Otherwise returns the user's personal runner.
async fn runner_client_for_path(
    state: &AppState,
    user_id: &str,
    workspace_path: Option<&str>,
) -> Option<(RunnerClient, ExecutionTarget)> {
    let path = workspace_path?;
    match resolve_target_for_workspace_path(state, user_id, path).await {
        Ok(target) => match resolve_runner_for_target(state, user_id, &target).await {
            Ok(Some(client)) => Some((client, target)),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(path = %path, error = %e, "runner client resolution failed for workspace path target");
                None
            }
        },
        Err(e) => {
            tracing::warn!(path = %path, error = %e, "runner target resolution failed for workspace path");
            None
        }
    }
}

/// State for a WebSocket connection, shared between command handler and event forwarder.
struct WsConnectionState {
    /// Subscribed Pi session IDs.
    subscribed_sessions: HashSet<String>,
    /// Channel for sending events to the WebSocket writer.
    event_tx: mpsc::UnboundedSender<WsEvent>,
    /// Active Pi subscriptions (keyed by session_id).
    pi_subscriptions: HashSet<String>,
    /// Forwarder tasks for Pi subscriptions (keyed by session_id).
    /// Aborted on session close/delete and WebSocket disconnect to prevent
    /// leaked subscription tasks across reconnect storms.
    pi_forwarders: HashMap<String, tokio::task::JoinHandle<()>>,
    /// Per-session response watchdogs for in-flight prompt/steer/follow_up.
    /// If no agent progress event arrives within the timeout, we emit
    /// canonical terminal events (agent.error + agent.idle) so the UI never
    /// remains in an unrecoverable working state.
    response_watchdogs: HashMap<String, tokio::task::JoinHandle<()>>,
    /// Metadata for Pi sessions created via this connection.
    pi_session_meta: HashMap<String, PiSessionMeta>,
    /// Active terminal sessions keyed by terminal_id.
    terminal_sessions: HashMap<String, TerminalSession>,
    /// Active file watchers keyed by workspace_path.
    /// The JoinHandle is aborted when the watcher is replaced or the connection closes.
    file_watchers: HashMap<String, tokio::task::JoinHandle<()>>,
    /// Runner overrides for sessions in shared workspaces.
    /// When a session is created with a cwd inside a shared workspace, the runner
    /// for that workspace's Linux user is stored here so subsequent commands
    /// (prompt, get_state, etc.) route to the correct runner.
    session_runner_overrides: HashMap<String, RunnerClient>,
    /// Bus subscriber ID for this connection.
    bus_subscriber_id: crate::bus::SubscriberId,
}

#[derive(Clone, Debug)]
struct PiSessionMeta {
    scope: Option<String>,
    cwd: Option<std::path::PathBuf>,
}

struct TerminalSession {
    owner_user_id: String,
    session_id: String,
    workspace_path: Option<String>,
    command_tx: mpsc::UnboundedSender<TerminalSessionCommand>,
    task: tokio::task::JoinHandle<()>,
}

enum TerminalSessionCommand {
    Input(String),
    Resize { cols: u16, rows: u16 },
    Close,
}

/// Handle the multiplexed WebSocket connection.
async fn handle_multiplexed_ws(
    socket: WebSocket,
    state: AppState,
    user_id: String,
    is_admin: bool,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Create channel for forwarding events to WebSocket
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<WsEvent>();

    // Send connected event
    let connected_event = WsEvent::System(SystemWsEvent::Connected);
    if let Ok(json) = serde_json::to_string(&connected_event)
        && ws_sender.send(Message::Text(json.into())).await.is_err()
    {
        return;
    }

    info!("Multiplexed WebSocket connected for user {}", user_id);

    // Create connection state
    let conn_state = Arc::new(tokio::sync::Mutex::new(WsConnectionState {
        subscribed_sessions: HashSet::new(),
        event_tx: event_tx.clone(),
        pi_subscriptions: HashSet::new(),
        pi_forwarders: HashMap::new(),
        response_watchdogs: HashMap::new(),
        pi_session_meta: HashMap::new(),
        terminal_sessions: HashMap::new(),
        file_watchers: HashMap::new(),
        session_runner_overrides: HashMap::new(),
        bus_subscriber_id: 0, // Set after bus registration
    }));

    // Register this connection with the legacy WS hub only for non-agent
    // system-level broadcasts that have not been fully migrated yet.
    // We intentionally do NOT forward legacy agent/session events.
    let hub: Arc<WsHub> = state.ws_hub.clone();
    let (mut hub_rx, hub_conn_id) = hub.register_connection(&user_id);
    let mut hub_events = hub.subscribe_events();
    let hub_user_id = user_id.clone();
    let hub_for_events = hub.clone();
    let event_tx_for_hub = event_tx.clone();
    let hub_forwarder = tokio::spawn(async move {
        // Keep websocket traffic flowing frequently enough for intermediate
        // proxies/NATs that enforce short idle timeouts.
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(10));

        let convert_hub_event = |event: LegacyHubEvent| -> Option<WsEvent> {
            match event {
                LegacyHubEvent::SharedWorkspaceUpdated {
                    workspace_id,
                    change,
                    detail,
                } => Some(WsEvent::System(SystemWsEvent::SharedWorkspaceUpdated {
                    workspace_id,
                    change_type: change,
                    detail,
                })),
                _ => None,
            }
        };

        loop {
            tokio::select! {
                maybe_event = hub_rx.recv() => {
                    let Some(event) = maybe_event else {
                        break;
                    };
                    if let Some(mapped) = convert_hub_event(event)
                        && event_tx_for_hub.send(mapped).is_err()
                    {
                        break;
                    }
                }
                hub_event = hub_events.recv() => {
                    match hub_event {
                        Ok((session_id, event)) => {
                            if hub_for_events.is_subscribed(&hub_user_id, &session_id)
                                && let Some(mapped) = convert_hub_event(event)
                                && event_tx_for_hub.send(mapped).is_err()
                            {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(missed = n, user_id = %hub_user_id, "hub event forwarder lagged");
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                _ = ping_interval.tick() => {
                    if event_tx_for_hub
                        .send(WsEvent::System(SystemWsEvent::Ping))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    // Runner client is resolved lazily on first command, not at connect time.
    // The runner may still be starting when the WebSocket connects.
    let runner_client: std::sync::Arc<tokio::sync::Mutex<Option<RunnerClient>>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(runner_client_for_user(
            &state, &user_id,
        )));

    // Spawn task to forward events from channel to WebSocket
    let event_writer = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event)
                && ws_sender.send(Message::Text(json.into())).await.is_err()
            {
                break;
            }
        }
    });

    // Register bus subscriber for this WS connection.
    let (bus_sub_id, mut bus_rx) = state.bus.register(&user_id);
    {
        let mut cs = conn_state.lock().await;
        cs.bus_subscriber_id = bus_sub_id;
    }
    let event_tx_for_bus = event_tx.clone();
    let bus_forwarder = tokio::spawn(async move {
        while let Some(bus_event) = bus_rx.recv().await {
            let ws_event = WsEvent::Bus(crate::bus::BusWsEvent::Event(bus_event));
            if event_tx_for_bus.send(ws_event).is_err() {
                break;
            }
        }
    });

    // Per-channel workers avoid head-of-line blocking.
    // A slow files.tree must never block agent prompt/abort/session commands.
    let (agent_cmd_tx, agent_cmd_rx) = mpsc::channel::<WsCommand>(256);
    let (files_cmd_tx, files_cmd_rx) = mpsc::channel::<WsCommand>(128);
    let (misc_cmd_tx, misc_cmd_rx) = mpsc::channel::<WsCommand>(128);

    let agent_worker = spawn_ws_command_worker(
        "agent",
        agent_cmd_rx,
        user_id.clone(),
        is_admin,
        state.clone(),
        runner_client.clone(),
        conn_state.clone(),
        event_tx.clone(),
    );
    let files_worker = spawn_ws_command_worker(
        "files",
        files_cmd_rx,
        user_id.clone(),
        is_admin,
        state.clone(),
        runner_client.clone(),
        conn_state.clone(),
        event_tx.clone(),
    );
    let misc_worker = spawn_ws_command_worker(
        "misc",
        misc_cmd_rx,
        user_id.clone(),
        is_admin,
        state.clone(),
        runner_client.clone(),
        conn_state.clone(),
        event_tx.clone(),
    );

    // Handle incoming messages
    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsCommand>(&text) {
                            Ok(cmd) => {
                                debug!("Received WS command: {:?}", cmd);

                                let target_tx = match &cmd {
                                    WsCommand::Agent(_) => &agent_cmd_tx,
                                    WsCommand::Files(_) => &files_cmd_tx,
                                    WsCommand::Bus(_) => &misc_cmd_tx,
                                    _ => &misc_cmd_tx,
                                };

                                if let Err(err) = target_tx.try_send(cmd) {
                                    let (cmd, reason) = match err {
                                        tokio::sync::mpsc::error::TrySendError::Full(cmd) => {
                                            (cmd, "Server busy: command queue is full")
                                        }
                                        tokio::sync::mpsc::error::TrySendError::Closed(cmd) => {
                                            (cmd, "Server unavailable: command worker stopped")
                                        }
                                    };
                                    let cmd_id = ws_command_id(&cmd);
                                    let _ = event_tx.send(WsEvent::System(SystemWsEvent::Error {
                                        id: cmd_id,
                                        error: reason.to_string(),
                                    }));
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse WS command: {}", e);
                                let _ = event_tx.send(WsEvent::System(SystemWsEvent::Error {
                                    id: None,
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
    hub_forwarder.abort();
    bus_forwarder.abort();
    state.bus.unregister(bus_sub_id);
    agent_worker.abort();
    files_worker.abort();
    misc_worker.abort();

    // Close terminal sessions and file watchers for this connection.
    {
        let mut state_guard = conn_state.lock().await;
        for (_, session) in state_guard.terminal_sessions.drain() {
            let _ = session.command_tx.send(TerminalSessionCommand::Close);
            session.task.abort();
        }
        for (_, handle) in state_guard.file_watchers.drain() {
            handle.abort();
        }
        for (_, handle) in state_guard.pi_forwarders.drain() {
            handle.abort();
        }
        for (_, handle) in state_guard.response_watchdogs.drain() {
            handle.abort();
        }
    }

    // Unregister this connection from the legacy hub.
    hub.unregister_connection(&user_id, hub_conn_id);

    info!("Multiplexed WebSocket closed for user {}", user_id);
}

fn spawn_ws_command_worker(
    worker_name: &'static str,
    mut rx: mpsc::Receiver<WsCommand>,
    user_id: String,
    is_admin: bool,
    state: AppState,
    runner_client: Arc<tokio::sync::Mutex<Option<RunnerClient>>>,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
    event_tx: mpsc::UnboundedSender<WsEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            process_ws_command(
                worker_name,
                cmd,
                &user_id,
                is_admin,
                &state,
                &runner_client,
                &conn_state,
                &event_tx,
            )
            .await;
        }
    })
}

async fn process_ws_command(
    worker_name: &str,
    cmd: WsCommand,
    user_id: &str,
    is_admin: bool,
    state: &AppState,
    runner_client: &Arc<tokio::sync::Mutex<Option<RunnerClient>>>,
    conn_state: &Arc<tokio::sync::Mutex<WsConnectionState>>,
    event_tx: &mpsc::UnboundedSender<WsEvent>,
) {
    // Lazily resolve runner client if not yet available.
    // The runner may start after the WebSocket connects.
    {
        let mut rc = runner_client.lock().await;
        if rc.is_none() {
            *rc = runner_client_for_user(state, user_id);
            if rc.is_some() {
                debug!(
                    "Runner client resolved lazily for user {} (worker={})",
                    user_id, worker_name
                );
            }
        }
    }

    // Clone the runner client ref so we don't hold the Mutex across handler execution.
    let rc_snapshot = {
        let guard = runner_client.lock().await;
        guard.clone()
    };

    let cmd_id = ws_command_id(&cmd);

    if matches!(&cmd, WsCommand::Agent(_))
        && let Some(client) = rc_snapshot.as_ref()
        && let Err(err) = client.ensure_ready_with_recovery().await
    {
        let _ = event_tx.send(WsEvent::System(SystemWsEvent::Error {
            id: cmd_id.clone(),
            error: format!(
                "Runner unavailable after bounded recovery attempts: {}",
                err
            ),
        }));
        return;
    }

    // Hard deadline on the whole command handler.
    // Prevents a single stuck command from occupying a worker forever.
    const WS_COMMAND_TIMEOUT: Duration = Duration::from_secs(45);
    let response = match tokio::time::timeout(
        WS_COMMAND_TIMEOUT,
        handle_ws_command(
            cmd,
            user_id,
            is_admin,
            state,
            rc_snapshot.as_ref(),
            conn_state.clone(),
        ),
    )
    .await
    {
        Ok(resp) => resp,
        Err(_) => {
            error!(
                "WS command timed out after {:?} for user {} (worker={})",
                WS_COMMAND_TIMEOUT, user_id, worker_name
            );
            Some(WsEvent::System(SystemWsEvent::Error {
                id: cmd_id,
                error: "Command timed out".to_string(),
            }))
        }
    };

    if let Some(event) = response {
        debug!("Sending WS event to client: {:?}", event);
        let _ = event_tx.send(event);
    }
}

fn normalize_path_lexical(base: &std::path::Path, raw: &std::path::Path) -> std::path::PathBuf {
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        base.join(raw)
    };

    let mut normalized = std::path::PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::Prefix(prefix) => {
                normalized.push(prefix.as_os_str());
            }
            std::path::Component::RootDir => {
                normalized.push(component.as_os_str());
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                let _ = normalized.pop();
            }
            std::path::Component::Normal(seg) => normalized.push(seg),
        }
    }

    normalized
}

fn resolve_workspace_path_for_validation(
    path: &str,
    user_home: &std::path::Path,
) -> std::path::PathBuf {
    let raw_path = std::path::PathBuf::from(path);
    let resolved = if raw_path.is_absolute() {
        raw_path.clone()
    } else {
        user_home.join(&raw_path)
    };

    resolved
        .canonicalize()
        .unwrap_or_else(|_| normalize_path_lexical(user_home, &raw_path))
}

/// Check that a workspace path belongs to the requesting user.
///
/// In multi-user mode, the workspace_path must be either:
/// 1. Under the user's home directory (personal workspace), OR
/// 2. Under a shared workspace where the user is a member
///
/// Returns an error WsEvent if validation fails, None if OK.
async fn validate_workspace_path_for_user(
    workspace_path: Option<&str>,
    user_id: &str,
    state: &AppState,
) -> Option<WsEvent> {
    let path = match workspace_path {
        Some(p) if !p.is_empty() => p,
        _ => return None, // No path to validate
    };

    // Only enforce in multi-user mode with linux_users configured
    let lu = match state.linux_users.as_ref() {
        Some(lu) => lu,
        None => return None,
    };

    let linux_username = lu.linux_username(user_id);
    let user_home = std::path::PathBuf::from(format!("/home/{linux_username}"));

    // Canonicalize if possible; otherwise fall back to lexical normalization.
    // Relative paths are resolved against user home so common values like "."
    // are treated as the user's workspace root.
    let canonical = resolve_workspace_path_for_validation(path, &user_home);

    // Allow if path is under user's personal home
    if canonical.starts_with(&user_home) {
        return None;
    }

    // Allow if path is inside a shared workspace where user is a member
    if let Some(sw_service) = state.shared_workspaces.as_ref() {
        let canonical_str = canonical.to_string_lossy();
        if let Ok(Some(_)) = sw_service
            .check_access_for_path(&canonical_str, user_id)
            .await
        {
            return None; // User has access to this shared workspace
        }
    }

    error!(
        user_id = %user_id,
        workspace_path = %path,
        resolved_path = %canonical.display(),
        expected_prefix = %user_home.display(),
        "SECURITY: workspace path does not belong to user and is not a shared workspace"
    );
    Some(WsEvent::System(SystemWsEvent::Error {
        id: None,
        error: "Access denied: workspace path does not belong to this user".to_string(),
    }))
}

/// Handle a WebSocket command and return an optional response event.
async fn handle_ws_command(
    cmd: WsCommand,
    user_id: &str,
    is_admin: bool,
    state: &AppState,
    runner_client: Option<&RunnerClient>,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    // SECURITY: Validate workspace paths belong to this user before processing
    {
        let (_, _, workspace_path) = ws_command_summary(&cmd);
        if let Some(err_event) =
            validate_workspace_path_for_user(workspace_path.as_deref(), user_id, state).await
        {
            return Some(err_event);
        }
    }

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
            agent::handle_agent_command(agent_cmd, user_id, state, runner_client, conn_state).await
        }
        WsCommand::Files(files_cmd) => {
            files::handle_files_command(files_cmd, user_id, state, conn_state).await
        }
        WsCommand::Terminal(term_cmd) => {
            terminal::handle_terminal_command(term_cmd, user_id, state, conn_state).await
        }
        WsCommand::Hstry(hstry_cmd) => history::handle_hstry_command(hstry_cmd, state).await,
        WsCommand::Trx(trx_cmd) => history::handle_trx_command(trx_cmd, user_id, state).await,
        WsCommand::Session(session_cmd) => {
            history::handle_session_command(session_cmd, user_id, state).await
        }
        WsCommand::Bus(bus_cmd) => {
            system::handle_bus_command(bus_cmd, user_id, is_admin, state, conn_state).await
        }
    }
}

fn sort_dir_entries(
    mut entries: Vec<crate::user_plane::DirEntry>,
) -> Vec<crate::user_plane::DirEntry> {
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
            .then_with(|| a.name.cmp(&b.name))
    });
    entries
}

fn resolve_tree_depth(depth: Option<usize>) -> usize {
    depth.unwrap_or(2).clamp(1, TREE_MAX_DEPTH)
}

fn resolve_tree_page_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(TREE_PAGE_DEFAULT_LIMIT)
        .clamp(1, TREE_PAGE_MAX_LIMIT)
}

fn ws_command_id(cmd: &WsCommand) -> Option<String> {
    match cmd {
        WsCommand::Agent(agent_cmd) => agent_cmd.id.clone(),
        WsCommand::Files(files_cmd) => match files_cmd {
            FilesWsCommand::Tree { id, .. }
            | FilesWsCommand::Read { id, .. }
            | FilesWsCommand::Write { id, .. }
            | FilesWsCommand::List { id, .. }
            | FilesWsCommand::Stat { id, .. }
            | FilesWsCommand::Delete { id, .. }
            | FilesWsCommand::CreateDirectory { id, .. }
            | FilesWsCommand::Rename { id, .. }
            | FilesWsCommand::Copy { id, .. }
            | FilesWsCommand::Move { id, .. }
            | FilesWsCommand::CopyToWorkspace { id, .. }
            | FilesWsCommand::WatchFiles { id, .. }
            | FilesWsCommand::UnwatchFiles { id, .. } => id.clone(),
        },
        WsCommand::Terminal(term_cmd) => match term_cmd {
            TerminalWsCommand::Open { id, .. }
            | TerminalWsCommand::Input { id, .. }
            | TerminalWsCommand::Resize { id, .. }
            | TerminalWsCommand::Close { id, .. } => id.clone(),
        },
        WsCommand::Hstry(hstry_cmd) => match hstry_cmd {
            HstryWsCommand::Query { id, .. } => id.clone(),
        },
        WsCommand::Trx(trx_cmd) => match trx_cmd {
            TrxWsCommand::List { id, .. }
            | TrxWsCommand::Create { id, .. }
            | TrxWsCommand::Update { id, .. }
            | TrxWsCommand::Close { id, .. }
            | TrxWsCommand::Sync { id, .. } => id.clone(),
        },
        WsCommand::Session(_) => None,
        WsCommand::Bus(bus_cmd) => match bus_cmd {
            crate::bus::BusCommand::Publish { id, .. }
            | crate::bus::BusCommand::Subscribe { id, .. }
            | crate::bus::BusCommand::Unsubscribe { id, .. } => id.clone(),
        },
    }
}

fn ws_command_summary(cmd: &WsCommand) -> (String, Option<String>, Option<String>) {
    match cmd {
        WsCommand::Agent(agent_cmd) => {
            let label = match agent_cmd.payload {
                oqto_protocol::commands::CommandPayload::SessionCreate { .. } => {
                    "agent.session_create"
                }
                oqto_protocol::commands::CommandPayload::SessionClose => "agent.session_close",
                oqto_protocol::commands::CommandPayload::SessionDelete => "agent.session_delete",
                oqto_protocol::commands::CommandPayload::SessionNew { .. } => "agent.session_new",
                oqto_protocol::commands::CommandPayload::SessionSwitch { .. } => {
                    "agent.session_switch"
                }
                oqto_protocol::commands::CommandPayload::SessionRestart => "agent.session_restart",
                oqto_protocol::commands::CommandPayload::Prompt { .. } => "agent.prompt",
                oqto_protocol::commands::CommandPayload::Steer { .. } => "agent.steer",
                oqto_protocol::commands::CommandPayload::FollowUp { .. } => "agent.follow_up",
                oqto_protocol::commands::CommandPayload::Abort => "agent.abort",
                oqto_protocol::commands::CommandPayload::InputResponse { .. } => {
                    "agent.input_response"
                }
                oqto_protocol::commands::CommandPayload::GetState => "agent.get_state",
                oqto_protocol::commands::CommandPayload::GetMessages => "agent.get_messages",
                oqto_protocol::commands::CommandPayload::GetStats => "agent.get_stats",
                oqto_protocol::commands::CommandPayload::GetModels { .. } => "agent.get_models",
                oqto_protocol::commands::CommandPayload::GetCommands => "agent.get_commands",
                oqto_protocol::commands::CommandPayload::GetForkPoints => "agent.get_fork_points",
                oqto_protocol::commands::CommandPayload::ListSessions => "agent.list_sessions",
                oqto_protocol::commands::CommandPayload::SetModel { .. } => "agent.set_model",
                oqto_protocol::commands::CommandPayload::CycleModel => "agent.cycle_model",
                oqto_protocol::commands::CommandPayload::SetThinkingLevel { .. } => {
                    "agent.set_thinking_level"
                }
                oqto_protocol::commands::CommandPayload::CycleThinkingLevel => {
                    "agent.cycle_thinking_level"
                }
                oqto_protocol::commands::CommandPayload::SetAutoCompaction { .. } => {
                    "agent.set_auto_compaction"
                }
                oqto_protocol::commands::CommandPayload::SetAutoRetry { .. } => {
                    "agent.set_auto_retry"
                }
                oqto_protocol::commands::CommandPayload::Compact { .. } => "agent.compact",
                oqto_protocol::commands::CommandPayload::AbortRetry => "agent.abort_retry",
                oqto_protocol::commands::CommandPayload::SetSessionName { .. } => {
                    "agent.set_session_name"
                }
                oqto_protocol::commands::CommandPayload::Fork { .. } => "agent.fork",
                oqto_protocol::commands::CommandPayload::Delegate(_) => "agent.delegate",
                oqto_protocol::commands::CommandPayload::DelegateCancel(_) => {
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
                FilesWsCommand::CopyToWorkspace { .. } => "files.copy_to_workspace",
                FilesWsCommand::WatchFiles { .. } => "files.watch",
                FilesWsCommand::UnwatchFiles { .. } => "files.unwatch",
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
                FilesWsCommand::CopyToWorkspace {
                    source_workspace_path,
                    ..
                } => Some(source_workspace_path.clone()),
                FilesWsCommand::WatchFiles { workspace_path, .. }
                | FilesWsCommand::UnwatchFiles { workspace_path, .. } => {
                    Some(workspace_path.clone())
                }
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
            let session_id = history::extract_legacy_session_id(&session_cmd.cmd);
            ("session.legacy".to_string(), session_id, None)
        }
        WsCommand::Bus(bus_cmd) => {
            let label = match &bus_cmd {
                crate::bus::BusCommand::Publish { topic, .. } => format!("bus.publish.{}", topic),
                crate::bus::BusCommand::Subscribe { .. } => "bus.subscribe".to_string(),
                crate::bus::BusCommand::Unsubscribe { .. } => "bus.unsubscribe".to_string(),
            };
            (label, None, None)
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
    WsEvent::Agent(oqto_protocol::events::Event {
        session_id: session_id.to_string(),
        runner_id: runner_id.to_string(),
        ts: Utc::now().timestamp_millis(),
        payload: oqto_protocol::events::EventPayload::Response(
            oqto_protocol::events::CommandResponse {
                id: id.unwrap_or_default(),
                cmd: cmd.to_string(),
                success,
                data,
                error,
            },
        ),
    })
}

const RESPONSE_WATCHDOG_TIMEOUT: Duration = Duration::from_secs(45);

async fn clear_response_watchdog(
    conn_state: &Arc<tokio::sync::Mutex<WsConnectionState>>,
    session_id: &str,
) {
    let mut state_guard = conn_state.lock().await;
    if let Some(handle) = state_guard.response_watchdogs.remove(session_id) {
        handle.abort();
    }
}

async fn emit_terminal_send_failure(
    conn_state: &Arc<tokio::sync::Mutex<WsConnectionState>>,
    session_id: &str,
    runner_id: &str,
    error: String,
) {
    clear_response_watchdog(conn_state, session_id).await;

    let event_tx = {
        let state_guard = conn_state.lock().await;
        state_guard.event_tx.clone()
    };

    let error_event = oqto_protocol::events::Event {
        session_id: session_id.to_string(),
        runner_id: runner_id.to_string(),
        ts: Utc::now().timestamp_millis(),
        payload: oqto_protocol::events::EventPayload::AgentError {
            error,
            recoverable: true,
            phase: Some(oqto_protocol::events::AgentPhase::Generating),
        },
    };
    let _ = event_tx.send(WsEvent::Agent(error_event));

    let idle_event = oqto_protocol::events::Event {
        session_id: session_id.to_string(),
        runner_id: runner_id.to_string(),
        ts: Utc::now().timestamp_millis(),
        payload: oqto_protocol::events::EventPayload::AgentIdle,
    };
    let _ = event_tx.send(WsEvent::Agent(idle_event));
}

async fn arm_response_watchdog(
    conn_state: &Arc<tokio::sync::Mutex<WsConnectionState>>,
    session_id: &str,
    runner_id: &str,
    event_tx: mpsc::UnboundedSender<WsEvent>,
) {
    // Replace any existing watchdog for this session.
    clear_response_watchdog(conn_state, session_id).await;

    let conn_state_for_task = Arc::clone(conn_state);
    let session_id_owned = session_id.to_string();
    let runner_id_owned = runner_id.to_string();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(RESPONSE_WATCHDOG_TIMEOUT).await;

        {
            let mut state_guard = conn_state_for_task.lock().await;
            state_guard.response_watchdogs.remove(&session_id_owned);
        }

        let error_event = oqto_protocol::events::Event {
            session_id: session_id_owned.clone(),
            runner_id: runner_id_owned.clone(),
            ts: Utc::now().timestamp_millis(),
            payload: oqto_protocol::events::EventPayload::AgentError {
                error: "No agent progress received in time. Session recovered to idle; you can retry your message.".to_string(),
                recoverable: true,
                phase: Some(oqto_protocol::events::AgentPhase::Generating),
            },
        };
        let _ = event_tx.send(WsEvent::Agent(error_event));

        let idle_event = oqto_protocol::events::Event {
            session_id: session_id_owned,
            runner_id: runner_id_owned,
            ts: Utc::now().timestamp_millis(),
            payload: oqto_protocol::events::EventPayload::AgentIdle,
        };
        let _ = event_tx.send(WsEvent::Agent(idle_event));
    });

    let mut state_guard = conn_state.lock().await;
    state_guard
        .response_watchdogs
        .insert(session_id.to_string(), handle);
}

/// Handle canonical agent commands.
/// Tag a user message with `[DisplayName]` if the session belongs to a shared workspace.
/// Returns the original message unchanged if not in a shared workspace or if the cwd
/// cannot be resolved.
async fn tag_shared_workspace_message(
    state: &AppState,
    conn_state: &Arc<tokio::sync::Mutex<WsConnectionState>>,
    session_id: &str,
    user_id: &str,
    message: &str,
) -> String {
    let Some(ref sw_service) = state.shared_workspaces else {
        return message.to_string();
    };
    let cwd = {
        let state_guard = conn_state.lock().await;
        state_guard
            .pi_session_meta
            .get(session_id)
            .and_then(|m| m.cwd.as_ref())
            .map(|p| p.to_string_lossy().to_string())
    };
    let Some(cwd) = cwd else {
        tracing::warn!(
            session_id = %session_id,
            "no cwd in session meta, cannot tag shared workspace message"
        );
        return message.to_string();
    };
    let display_name = state
        .users
        .get_user(user_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.display_name.clone())
        .unwrap_or_else(|| user_id.to_string());
    let result = sw_service
        .prepend_user_name(&cwd, &display_name, message)
        .await;
    tracing::debug!(
        session_id = %session_id,
        cwd = %cwd,
        display_name = %display_name,
        tagged = result.as_ref().map(|r| r.as_str() != message).unwrap_or(false),
        "shared workspace user tag"
    );
    result.unwrap_or_else(|_| message.to_string())
}

/// Broadcast a user message to all other session subscribers so they see it live.
///
/// The sender already shows the message optimistically. This sends
/// `StreamMessageStart` + `StreamTextDelta` + `StreamMessageEnd` events
/// to other users watching the same session via the hub.
async fn broadcast_user_message(
    state: &AppState,
    session_id: &str,
    user_id: &str,
    message: &str,
    client_id: Option<String>,
) {
    let now = chrono::Utc::now().timestamp_millis();
    let msg_id = format!("user-{}", now);
    let user_message = oqto_protocol::messages::Message {
        id: msg_id.clone(),
        idx: 0,
        role: oqto_protocol::messages::Role::User,
        client_id,
        sender: None,
        parts: vec![hstry_core::parts::Part::Text {
            id: format!("part-{}", now),
            text: message.to_string(),
            format: None,
        }],
        created_at: now,
        model: None,
        provider: None,
        stop_reason: None,
        usage: None,
        tool_call_id: None,
        tool_name: None,
        is_error: None,
        metadata: None,
    };
    let events = vec![
        oqto_protocol::events::Event {
            session_id: session_id.to_string(),
            runner_id: String::new(),
            ts: now,
            payload: oqto_protocol::events::EventPayload::StreamMessageStart {
                message_id: msg_id.clone(),
                role: "user".to_string(),
            },
        },
        oqto_protocol::events::Event {
            session_id: session_id.to_string(),
            runner_id: String::new(),
            ts: now,
            payload: oqto_protocol::events::EventPayload::StreamTextDelta {
                message_id: msg_id.clone(),
                delta: message.to_string(),
                content_index: 0,
            },
        },
        oqto_protocol::events::Event {
            session_id: session_id.to_string(),
            runner_id: String::new(),
            ts: now,
            payload: oqto_protocol::events::EventPayload::StreamMessageEnd {
                message: user_message,
            },
        },
    ];
    for event in &events {
        let legacy = crate::ws::types::WsEvent::AgentEvent {
            session_id: session_id.to_string(),
            event: serde_json::to_value(event).unwrap_or_default(),
        };
        state
            .ws_hub
            .send_to_session_except(session_id, legacy, user_id)
            .await;
    }
}

/// Every command gets a `CommandResponse` event back (or `None` for fire-and-forget
/// commands like prompt/steer/abort where streaming events are the real response).
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

    // When the session is ACTIVE (has a running Pi process + subscription),
    // prefer Pi's live messages over hstry. Pi has the complete current-turn
    // context including messages not yet persisted to hstry. Without this,
    // the frontend misses in-progress tool calls, streaming responses, and
    // any messages between the last hstry persist and now.
    if is_active {
        // Use a short timeout: Pi may be busy with an LLM request and the
        // runner's get_messages RPC can hang for 10+s. Fall through to
        // hstry/cache quickly rather than blocking the WS response.
        let pi_result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            runner.agent_get_messages(session_id),
        )
        .await;
        match pi_result {
            Ok(Ok(resp)) => {
                let messages_value = serde_json::to_value(&resp.messages).unwrap_or_default();
                cache_pi_messages(user_id, session_id, &messages_value).await;
                return Some(agent_response(
                    session_id,
                    id,
                    "get_messages",
                    Ok(Some(serde_json::json!({ "messages": messages_value }))),
                ));
            }
            Ok(Err(e)) => {
                // Pi process may have exited between subscription and now.
                // Fall through to hstry/cache as fallback.
                debug!(
                    "get_messages: active session Pi query failed for {}: {}, falling through to hstry",
                    session_id, e
                );
            }
            Err(_) => {
                debug!(
                    "get_messages: Pi query timed out for active session {}, falling through to hstry",
                    session_id,
                );
            }
        }
    }

    // Check cache (only for inactive sessions or when Pi query failed above)
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
                    let serializable =
                        crate::history::proto_messages_to_serializable(hstry_messages);
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
                let serializable = crate::history::proto_messages_to_serializable(hstry_messages);
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
    match runner.agent_get_messages(session_id).await {
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
    user_id: &str,
    event_tx: mpsc::UnboundedSender<WsEvent>,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
    sub_ready_tx: Option<oneshot::Sender<()>>,
    runner_id: String,
) -> anyhow::Result<()> {
    info!(
        "forward_pi_events: connecting subscription for session {}",
        session_id
    );
    let mut subscription = runner.agent_subscribe(session_id).await?;
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
                // Any real agent event means the command made progress.
                clear_response_watchdog(&conn_state, session_id).await;

                // Invalidate the messages cache on agent.idle so subsequent
                // get_messages requests fetch fresh data from hstry instead
                // of serving stale cached messages.
                if matches!(
                    canonical_event.payload,
                    oqto_protocol::events::EventPayload::AgentIdle
                ) {
                    let mut cache = PI_MESSAGES_CACHE.write().await;
                    if let Some(user_cache) = cache.get_mut(user_id) {
                        if let Some(entry) = user_cache.entries.remove(session_id) {
                            user_cache.total_bytes =
                                user_cache.total_bytes.saturating_sub(entry.size_bytes);
                        }
                    }
                }
                if event_tx.send(WsEvent::Agent(canonical_event)).is_err() {
                    // WebSocket closed
                    break;
                }
            }
            Some(PiSubscriptionEvent::End { reason }) => {
                clear_response_watchdog(&conn_state, session_id).await;
                debug!(
                    "Pi subscription ended for session {}: {}",
                    session_id, reason
                );
                break;
            }
            Some(PiSubscriptionEvent::Error { code, message }) => {
                clear_response_watchdog(&conn_state, session_id).await;
                error!(
                    "Pi subscription error for session {}: {:?} - {}",
                    session_id, code, message
                );
                // Emit error as canonical agent.error event
                let error_event = oqto_protocol::events::Event {
                    session_id: session_id.to_string(),
                    runner_id: runner_id.clone(),
                    ts: chrono::Utc::now().timestamp_millis(),
                    payload: oqto_protocol::events::EventPayload::AgentError {
                        error: format!("Subscription error ({:?}): {}", code, message),
                        recoverable: false,
                        phase: None,
                    },
                };
                let _ = event_tx.send(WsEvent::Agent(error_event));
                break;
            }
            None => {
                clear_response_watchdog(&conn_state, session_id).await;
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

/// Emit a workspace/files.* bus event (fire-and-forget).
fn emit_file_bus_event(
    bus: &Arc<crate::bus::BusEngine>,
    user_id: &str,
    workspace_path: Option<&str>,
    topic: &str,
    payload: serde_json::Value,
) {
    use crate::bus::{BusEvent, BusScope, EventSource};

    let scope_id = workspace_path.unwrap_or("local").to_string();
    // Use Service source: file ops are already authorized by the file handler,
    // so the bus event is a system notification, not a user-initiated publish.
    let event = BusEvent::new(
        BusScope::Workspace,
        scope_id,
        format!("files.{}", topic),
        payload,
        EventSource::Service {
            service: "files".to_string(),
            user_id: Some(user_id.to_string()),
        },
    );
    let topic_log = format!("files.{}", topic);
    let bus = bus.clone();
    tokio::spawn(async move {
        match bus.publish_internal(event).await {
            Ok(()) => log::debug!("Bus: emitted file event {}", topic_log),
            Err(e) => log::warn!("Bus: failed to emit file event {}: {}", topic_log, e),
        }
    });
}

/// Handle Files channel commands.
async fn handle_copy_to_workspace(
    id: Option<String>,
    source_workspace_path: &str,
    source_path: &str,
    target_workspace_path: &str,
    target_path: &str,
    user_id: &str,
    state: &AppState,
) -> Option<WsEvent> {
    // Validate that both workspace paths belong to the current user's sessions.
    let sessions = match state.sessions.for_user(user_id).list_sessions().await {
        Ok(s) => s,
        Err(err) => {
            return Some(WsEvent::Files(FilesWsEvent::Error {
                id,
                error: format!("Failed to list sessions: {}", err),
            }));
        }
    };

    let user_workspace_paths: std::collections::HashSet<&str> =
        sessions.iter().map(|s| s.workspace_path.as_str()).collect();

    if !user_workspace_paths.contains(source_workspace_path) {
        warn!(
            user_id = user_id,
            source_workspace_path = source_workspace_path,
            "Cross-workspace copy denied: source workspace not owned by user"
        );
        return Some(WsEvent::Files(FilesWsEvent::Error {
            id,
            error: "Source workspace does not belong to any of your sessions".into(),
        }));
    }

    if !user_workspace_paths.contains(target_workspace_path) {
        warn!(
            user_id = user_id,
            target_workspace_path = target_workspace_path,
            "Cross-workspace copy denied: target workspace not owned by user"
        );
        return Some(WsEvent::Files(FilesWsEvent::Error {
            id,
            error: "Target workspace does not belong to any of your sessions".into(),
        }));
    }

    if source_workspace_path == target_workspace_path {
        return Some(WsEvent::Files(FilesWsEvent::Error {
            id,
            error: "Source and target workspaces are the same; use regular copy instead".into(),
        }));
    }

    // Resolve workspace roots and paths
    let source_root = std::path::PathBuf::from(source_workspace_path);
    let target_root = std::path::PathBuf::from(target_workspace_path);

    let source_resolved = match resolve_workspace_child(&source_root, source_path) {
        Ok(p) => p,
        Err(err) => {
            return Some(WsEvent::Files(FilesWsEvent::Error {
                id,
                error: format!("Invalid source path: {}", err),
            }));
        }
    };

    let target_resolved = match resolve_workspace_child(&target_root, target_path) {
        Ok(p) => p,
        Err(err) => {
            return Some(WsEvent::Files(FilesWsEvent::Error {
                id,
                error: format!("Invalid target path: {}", err),
            }));
        }
    };

    // Create user plane (same user for both workspaces)
    let linux_username = state
        .linux_users
        .as_ref()
        .map(|lu| lu.linux_username(user_id))
        .unwrap_or_else(|| user_id.to_string());
    let is_multi_user = state.linux_users.is_some();
    let user_plane: Arc<dyn UserPlane> =
        if let Some(pattern) = state.runner_socket_pattern.as_deref() {
            match RunnerUserPlane::for_user_with_pattern(&linux_username, pattern) {
                Ok(plane) => {
                    let base: Arc<dyn UserPlane> = Arc::new(plane);
                    Arc::new(MeteredUserPlane::new(
                        base,
                        UserPlanePath::Runner,
                        state.user_plane_metrics.clone(),
                    ))
                }
                Err(err) => {
                    error!(
                        "Failed to create RunnerUserPlane for {}: {:#}",
                        linux_username, err
                    );
                    return Some(WsEvent::Files(FilesWsEvent::Error {
                        id,
                        error: "File access unavailable: user runner not reachable".into(),
                    }));
                }
            }
        } else if is_multi_user {
            error!("Multi-user mode without runner_socket_pattern configured");
            return Some(WsEvent::Files(FilesWsEvent::Error {
                id,
                error: "File access not configured for multi-user mode".into(),
            }));
        } else {
            match RunnerUserPlane::new_default() {
                Ok(plane) => {
                    let base: Arc<dyn UserPlane> = Arc::new(plane);
                    Arc::new(MeteredUserPlane::new(
                        base,
                        UserPlanePath::Runner,
                        state.user_plane_metrics.clone(),
                    ))
                }
                Err(runner_err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error {
                        id,
                        error: format!(
                            "File access unavailable: runner not reachable ({:#})",
                            runner_err
                        ),
                    }));
                }
            }
        };

    // Perform the copy (recursive for directories, returns file count)
    fn copy_recursive_cross<'a>(
        user_plane: &'a Arc<dyn crate::user_plane::UserPlane>,
        from_path: &'a std::path::Path,
        to_path: &'a std::path::Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let from_stat = user_plane
                .stat(from_path)
                .await
                .map_err(|e| e.to_string())?;
            if !from_stat.exists {
                return Err("source path does not exist".into());
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
                let mut total = 0;
                for entry in entries {
                    let child_from = from_path.join(&entry.name);
                    let child_to = to_path.join(&entry.name);
                    total += copy_recursive_cross(user_plane, &child_from, &child_to).await?;
                }
                Ok(total)
            } else {
                let content = user_plane
                    .read_file(from_path, None, None)
                    .await
                    .map_err(|e| e.to_string())?;
                user_plane
                    .write_file(to_path, &content.content, true)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(1)
            }
        })
    }

    info!(
        user_id = user_id,
        source = %source_resolved.display(),
        target = %target_resolved.display(),
        "Cross-workspace copy"
    );

    match copy_recursive_cross(&user_plane, &source_resolved, &target_resolved).await {
        Ok(files_copied) => {
            info!(
                user_id = user_id,
                files_copied = files_copied,
                "Cross-workspace copy complete"
            );
            Some(WsEvent::Files(FilesWsEvent::CopyToWorkspaceResult {
                id,
                source_workspace_path: source_workspace_path.to_string(),
                target_workspace_path: target_workspace_path.to_string(),
                files_copied,
                success: true,
            }))
        }
        Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
            id,
            error: format!("Cross-workspace copy failed: {}", err),
        })),
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

// ============================================================================
// File watcher
// ============================================================================

/// Start watching a workspace directory for file changes.
async fn handle_watch_files(
    id: Option<String>,
    workspace_path: &str,
    user_id: &str,
    _state: &AppState,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    use notify::{
        RecursiveMode, Watcher,
        event::{CreateKind, EventKind, RemoveKind},
    };

    let resolved_path = std::path::PathBuf::from(workspace_path);
    if !resolved_path.is_dir() {
        return Some(WsEvent::Files(FilesWsEvent::Error {
            id,
            error: format!("Not a directory: {workspace_path}"),
        }));
    }

    let workspace_key = workspace_path.to_string();
    let watch_dir = resolved_path.clone();

    // Get the event sender from connection state
    let event_tx = {
        let state_guard = conn_state.lock().await;
        state_guard.event_tx.clone()
    };

    // Stop existing watcher for this workspace if any
    {
        let mut state_guard = conn_state.lock().await;
        if let Some(handle) = state_guard.file_watchers.remove(&workspace_key) {
            handle.abort();
        }
    }

    let ws_workspace_path = workspace_key.clone();

    // Spawn watcher task
    let (notify_tx, mut notify_rx) = mpsc::channel::<notify::Result<notify::Event>>(256);

    let handle = tokio::spawn(async move {
        // Create the inotify watcher (must be created on the async runtime thread)
        let tx_for_watcher = notify_tx.clone();
        let mut watcher = match notify::recommended_watcher(move |res| {
            let _ = tx_for_watcher.blocking_send(res);
        }) {
            Ok(w) => w,
            Err(e) => {
                warn!(
                    "Failed to create file watcher for {}: {:?}",
                    ws_workspace_path, e
                );
                return;
            }
        };

        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::Recursive) {
            warn!("Failed to watch {}: {:?}", watch_dir.display(), e);
            return;
        }

        debug!("File watcher started for {}", ws_workspace_path);

        // Debounce: collect events and flush after 300ms of quiet
        let debounce = Duration::from_millis(300);
        let mut pending: HashMap<std::path::PathBuf, EventKind> = HashMap::new();
        let mut deadline: Option<tokio::time::Instant> = None;

        loop {
            tokio::select! {
                event = notify_rx.recv() => {
                    match event {
                        Some(Ok(ev)) => {
                            for path in ev.paths {
                                pending.insert(path, ev.kind);
                            }
                            deadline = Some(tokio::time::Instant::now() + debounce);
                        }
                        Some(Err(e)) => {
                            warn!("File watcher error: {:?}", e);
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep_until(deadline.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(3600))), if deadline.is_some() => {
                    let batch: HashMap<_, _> = std::mem::take(&mut pending);
                    deadline = None;

                    for (path, kind) in batch {
                        if !path.starts_with(&watch_dir) {
                            continue;
                        }

                        let is_dir = match tokio::fs::metadata(&path).await {
                            Ok(m) => m.is_dir(),
                            Err(_) => matches!(
                                kind,
                                EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder)
                            ),
                        };

                        let event_type = match kind {
                            EventKind::Create(_) => {
                                if is_dir { "dir_created" } else { "file_created" }
                            }
                            EventKind::Modify(_) => {
                                if is_dir { continue; } else { "file_modified" }
                            }
                            EventKind::Remove(_) => {
                                if is_dir { "dir_deleted" } else { "file_deleted" }
                            }
                            _ => continue,
                        };

                        // Compute relative path
                        let rel = path.strip_prefix(&watch_dir)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if rel.is_empty() {
                            continue;
                        }

                        // Skip hidden files / .git internals to reduce noise
                        if rel.starts_with('.') || rel.contains("/.") {
                            continue;
                        }

                        let ws_event = WsEvent::Files(FilesWsEvent::FileChanged {
                            event_type: event_type.to_string(),
                            path: rel,
                            entry_type: if is_dir { "directory".to_string() } else { "file".to_string() },
                            workspace_path: ws_workspace_path.clone(),
                        });

                        if event_tx.send(ws_event).is_err() {
                            // Connection closed
                            break;
                        }
                    }
                }
            }
        }

        debug!("File watcher stopped for {}", ws_workspace_path);
        // `watcher` is dropped here, which stops inotify
    });

    // Store the watcher handle
    {
        let mut state_guard = conn_state.lock().await;
        state_guard
            .file_watchers
            .insert(workspace_key.clone(), handle);
    }

    info!(
        "File watcher started for workspace {} (user {})",
        workspace_path, user_id
    );

    Some(WsEvent::Files(FilesWsEvent::WatchFilesResult {
        id,
        workspace_path: workspace_key,
        success: true,
    }))
}

/// Stop watching a workspace directory.
async fn handle_unwatch_files(
    id: Option<String>,
    workspace_path: &str,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    let mut state_guard = conn_state.lock().await;
    if let Some(handle) = state_guard.file_watchers.remove(workspace_path) {
        handle.abort();
        info!("File watcher stopped for workspace {}", workspace_path);
    }
    Some(WsEvent::Files(FilesWsEvent::UnwatchFilesResult {
        id,
        workspace_path: workspace_path.to_string(),
        success: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_agent_prompt_command() {
        let json = r#"{"channel":"agent","session_id":"ses_123","cmd":"prompt","message":"hello"}"#;
        let cmd: WsCommand = serde_json::from_str(json).unwrap();
        match cmd {
            WsCommand::Agent(oqto_protocol::commands::Command {
                session_id,
                payload: oqto_protocol::commands::CommandPayload::Prompt { message, .. },
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
            WsCommand::Agent(oqto_protocol::commands::Command {
                session_id,
                payload: oqto_protocol::commands::CommandPayload::GetState,
                ..
            }) => {
                assert_eq!(session_id, "ses_456");
            }
            _ => panic!("Expected Agent GetState command"),
        }
    }

    #[test]
    fn test_ws_command_id_for_files_tree() {
        let json = r#"{"channel":"files","type":"tree","id":"req-9","path":".","workspace_path":"/tmp/ws"}"#;
        let cmd: WsCommand = serde_json::from_str(json).unwrap();
        assert_eq!(ws_command_id(&cmd), Some("req-9".to_string()));
    }

    #[test]
    fn test_sort_dir_entries_dirs_first_then_name() {
        let entries = vec![
            crate::user_plane::DirEntry {
                name: "zeta.txt".to_string(),
                is_dir: false,
                is_symlink: false,
                size: 1,
                modified_at: 0,
            },
            crate::user_plane::DirEntry {
                name: "beta".to_string(),
                is_dir: true,
                is_symlink: false,
                size: 0,
                modified_at: 0,
            },
            crate::user_plane::DirEntry {
                name: "Alpha".to_string(),
                is_dir: true,
                is_symlink: false,
                size: 0,
                modified_at: 0,
            },
            crate::user_plane::DirEntry {
                name: "a.txt".to_string(),
                is_dir: false,
                is_symlink: false,
                size: 1,
                modified_at: 0,
            },
        ];

        let sorted = sort_dir_entries(entries);
        let names: Vec<_> = sorted.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["Alpha", "beta", "a.txt", "zeta.txt"]);
    }

    #[test]
    fn test_resolve_tree_depth_bounds() {
        assert_eq!(resolve_tree_depth(None), 2);
        assert_eq!(resolve_tree_depth(Some(0)), 1);
        assert_eq!(resolve_tree_depth(Some(1)), 1);
        assert_eq!(resolve_tree_depth(Some(4)), 4);
        assert_eq!(resolve_tree_depth(Some(999)), TREE_MAX_DEPTH);
    }

    #[test]
    fn test_resolve_tree_page_limit_bounds() {
        assert_eq!(resolve_tree_page_limit(None), TREE_PAGE_DEFAULT_LIMIT);
        assert_eq!(resolve_tree_page_limit(Some(0)), 1);
        assert_eq!(resolve_tree_page_limit(Some(50)), 50);
        assert_eq!(resolve_tree_page_limit(Some(999999)), TREE_PAGE_MAX_LIMIT);
    }

    #[test]
    fn test_serialize_tree_result_truncated_metadata() {
        let event = WsEvent::Files(FilesWsEvent::TreeResult {
            id: Some("req-tree".to_string()),
            path: ".".to_string(),
            entries: Vec::new(),
            truncated: Some(true),
            stop_reason: Some("max_nodes".to_string()),
            visited_nodes: Some(20_000),
            elapsed_ms: Some(123),
            next_offset: Some(1000),
            total_entries: Some(5000),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"tree_result""#));
        assert!(json.contains(r#""truncated":true"#));
        assert!(json.contains(r#""stop_reason":"max_nodes""#));
        assert!(json.contains(r#""visited_nodes":20000"#));
        assert!(json.contains(r#""next_offset":1000"#));
        assert!(json.contains(r#""total_entries":5000"#));
    }

    #[test]
    fn test_serialize_agent_command_response() {
        use oqto_protocol::events::{CommandResponse, EventPayload};

        let event = WsEvent::Agent(oqto_protocol::events::Event {
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
    fn test_serialize_system_error_with_id() {
        let event = WsEvent::System(SystemWsEvent::Error {
            id: Some("req-42".to_string()),
            error: "Command timed out".to_string(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""channel":"system""#));
        assert!(json.contains(r#""type":"error""#));
        assert!(json.contains(r#""id":"req-42""#));
    }

    #[test]
    fn test_serialize_canonical_agent_event() {
        use oqto_protocol::events::EventPayload;

        let event = WsEvent::Agent(oqto_protocol::events::Event {
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
        use oqto_protocol::events::EventPayload;

        let event = WsEvent::Agent(oqto_protocol::events::Event {
            session_id: "ses_abc".into(),
            runner_id: "local".into(),
            ts: 1738764000000,
            payload: EventPayload::AgentIdle,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""channel":"agent""#));
        assert!(json.contains(r#""event":"agent.idle""#));
    }

    #[test]
    fn test_resolve_workspace_path_for_validation_accepts_relative_dot() {
        let user_home = std::path::PathBuf::from("/home/oqto_usr_wismut");
        let resolved = resolve_workspace_path_for_validation(".", &user_home);
        assert!(resolved.starts_with(&user_home));
    }

    #[test]
    fn test_resolve_workspace_path_for_validation_accepts_relative_child() {
        let user_home = std::path::PathBuf::from("/home/oqto_usr_wismut");
        let resolved = resolve_workspace_path_for_validation("./oqto/main", &user_home);
        assert!(resolved.starts_with(&user_home));
        assert!(resolved.to_string_lossy().contains("/oqto/main"));
    }

    #[test]
    fn test_resolve_workspace_path_for_validation_blocks_parent_escape() {
        let user_home = std::path::PathBuf::from("/home/oqto_usr_wismut");
        let resolved =
            resolve_workspace_path_for_validation("../oqto_other_user/secret", &user_home);
        assert!(!resolved.starts_with(&user_home));
    }

    #[tokio::test]
    async fn test_emit_terminal_send_failure_emits_error_and_idle_and_clears_watchdog() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<WsEvent>();
        let watchdog = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        let mut response_watchdogs = HashMap::new();
        response_watchdogs.insert("ses-test".to_string(), watchdog);

        let conn_state = Arc::new(tokio::sync::Mutex::new(WsConnectionState {
            subscribed_sessions: HashSet::new(),
            event_tx: event_tx.clone(),
            pi_subscriptions: HashSet::new(),
            pi_forwarders: HashMap::new(),
            response_watchdogs,
            pi_session_meta: HashMap::new(),
            terminal_sessions: HashMap::new(),
            file_watchers: HashMap::new(),
            session_runner_overrides: HashMap::new(),
            bus_subscriber_id: 0,
        }));

        emit_terminal_send_failure(
            &conn_state,
            "ses-test",
            "runner-test",
            "send failed".to_string(),
        )
        .await;

        let first = event_rx.recv().await.expect("first event");
        let second = event_rx.recv().await.expect("second event");

        match first {
            WsEvent::Agent(event) => match event.payload {
                oqto_protocol::events::EventPayload::AgentError { error, .. } => {
                    assert!(error.contains("send failed"));
                }
                other => panic!("expected agent.error, got {other:?}"),
            },
            other => panic!("expected agent event, got {other:?}"),
        }

        match second {
            WsEvent::Agent(event) => {
                assert!(matches!(event.payload, oqto_protocol::events::EventPayload::AgentIdle));
            }
            other => panic!("expected agent event, got {other:?}"),
        }

        let state_guard = conn_state.lock().await;
        assert!(!state_guard.response_watchdogs.contains_key("ses-test"));
    }
}

//! Pi agent service for Main Chat.
//!
//! Manages Pi subprocesses for each user's Main Chat. Each user gets one Pi
//! process that persists across requests, enabling streaming and maintaining
//! session state.
//!
//! ## Session Lifecycle
//!
//! Sessions are managed with smart continuation vs fresh start logic:
//! - **Continue** if: last activity < 4 hours AND session file < 500KB
//! - **Fresh start** if: session is stale, too large, or user requests new session
//!
//! On fresh start, context is injected from:
//! 1. Last session's compaction summary (from main_chat.db)
//! 2. Recent mmry entries (decisions, handoffs, insights)
//!
//! ## Runtime Modes
//!
//! Pi can run in different isolation modes:
//! - **Local**: Direct subprocess on host (single-user mode)
//! - **Runner**: Via octo-runner daemon (multi-user isolation)
//! - **Container**: HTTP client to pi-bridge in container

use anyhow::{Context, Result};
use base64::Engine;
use chrono::{TimeZone, Utc};
use log::{debug, info, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use tokio::sync::{Mutex, RwLock, broadcast};
use uuid::Uuid;

use crate::local::LinuxUsersConfig;
use crate::pi::{
    AgentMessage, AssistantMessageEvent, CompactionResult, ContainerPiRuntime, LocalPiRuntime,
    PiCommand, PiEvent, PiProcess, PiRuntime, PiSpawnConfig, PiState, RunnerPiRuntime,
    SessionStats,
};
use crate::runner::client::RunnerClient;
use crate::workspace;

/// Session freshness thresholds
const SESSION_MAX_AGE_HOURS: u64 = 4;
const SESSION_MAX_SIZE_BYTES: u64 = 500 * 1024; // 500KB

const BOOTSTRAP_MESSAGES_EN: &[&str] = &[
    "Hello, I'm your new assistant. What would you like to call me, and what language should we use?",
    "Hi! I'm your new assistant. What name should I use for myself? Also, what's your preferred language?",
    "Welcome. I'm your new assistant. Please tell me the name you'd like me to use and your preferred language.",
];

const BOOTSTRAP_MESSAGES_DE: &[&str] = &[
    "Hallo, ich bin dein neuer Assistent. Wie soll ich heißen, und welche Sprache bevorzugst du?",
    "Hi! Ich bin dein neuer Assistent. Welchen Namen soll ich verwenden? Und in welcher Sprache sollen wir kommunizieren?",
    "Willkommen. Ich bin dein neuer Assistent. Bitte sag mir, wie ich heißen soll und welche Sprache du bevorzugst.",
];

/// Pi runtime mode determines how Pi processes are spawned and isolated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PiRuntimeMode {
    /// Direct subprocess on host (single-user mode, no isolation).
    #[default]
    Local,
    /// Via octo-runner daemon (multi-user isolation, processes run as separate Linux users).
    Runner,
    /// HTTP client to pi-bridge in container (container mode).
    Container,
}

impl std::fmt::Display for PiRuntimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PiRuntimeMode::Local => write!(f, "local"),
            PiRuntimeMode::Runner => write!(f, "runner"),
            PiRuntimeMode::Container => write!(f, "container"),
        }
    }
}

/// Configuration for the Pi service.
#[derive(Debug, Clone)]
pub struct MainChatPiServiceConfig {
    /// Path to the Pi CLI executable (e.g., "pi" or "/usr/local/bin/pi")
    pub pi_executable: String,
    /// Default provider for new sessions
    pub default_provider: Option<String>,
    /// Default model for new sessions
    pub default_model: Option<String>,
    /// Extension files to load (passed via --extension)
    pub extensions: Vec<String>,
    /// Maximum session age before forcing fresh start (hours)
    pub max_session_age_hours: u64,
    /// Maximum session file size before forcing fresh start (bytes)
    pub max_session_size_bytes: u64,
    /// Runtime mode for Pi process isolation.
    pub runtime_mode: PiRuntimeMode,
    /// Runner socket path pattern (for Runner mode).
    /// Use {user} placeholder for username, e.g., "/run/octo/runner-{user}.sock"
    pub runner_socket_pattern: Option<String>,
    /// Pi bridge URL (for Container mode).
    /// e.g., "http://localhost:41824"
    pub bridge_url: Option<String>,
    /// Whether to sandbox Pi processes (only applies to Runner mode).
    /// The runner loads sandbox config from /etc/octo/sandbox.toml.
    pub sandboxed: bool,

    /// Idle timeout in seconds before stopping inactive Pi processes.
    /// Default: 300 (5 minutes).
    pub idle_timeout_secs: u64,
}

impl Default for MainChatPiServiceConfig {
    fn default() -> Self {
        Self {
            pi_executable: "pi".to_string(),
            default_provider: None,
            default_model: None,
            extensions: Vec::new(),
            max_session_age_hours: SESSION_MAX_AGE_HOURS,
            max_session_size_bytes: SESSION_MAX_SIZE_BYTES,
            runtime_mode: PiRuntimeMode::Local,
            runner_socket_pattern: None,
            bridge_url: None,
            sandboxed: false,

            idle_timeout_secs: 300,
        }
    }
}

/// Information about the last Pi session for a directory.
#[derive(Debug, Clone)]
pub struct LastSessionInfo {
    /// File size in bytes
    pub size: u64,
    /// Last modification time
    pub modified: SystemTime,
}

/// A Pi session file on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionFile {
    /// Session ID (UUID from filename)
    pub id: String,
    /// Session start timestamp (ISO 8601)
    pub started_at: String,
    /// File size in bytes
    pub size: u64,
    /// Last modification time (Unix timestamp ms)
    pub modified_at: i64,
    /// Title (derived from first user message, or None)
    pub title: Option<String>,
    /// Human-readable ID (e.g., "cold-lamp-verb")
    /// Parsed from auto-generated title format: <workdir>: <title> [readable_id]
    pub readable_id: Option<String>,
    /// Parent session ID (if this session was spawned as a child)
    pub parent_id: Option<String>,
    /// Number of messages in session
    pub message_count: usize,
    /// Workspace path (cwd stored in JSONL header)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    /// Session directory (JSONL header)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_dir: Option<String>,
}

/// A message from a Pi session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionMessage {
    /// Message ID
    pub id: String,
    /// Role: user, assistant, system
    pub role: String,
    /// Content (text or structured)
    pub content: Value,
    /// Tool call ID (toolResult messages only)
    #[serde(rename = "toolCallId", skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool name (toolResult messages only)
    #[serde(rename = "toolName", skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Tool error flag (toolResult messages only)
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Timestamp (Unix ms)
    pub timestamp: i64,
    /// Usage stats (for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
}

/// A single search result for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSearchResult {
    /// Source file path
    pub source_path: String,
    /// Line number in the source file
    pub line_number: usize,
    /// Agent type (e.g., "pi", "claude", "opencode")
    #[serde(default)]
    pub agent: String,
    /// Match score
    #[serde(default)]
    pub score: f64,
    /// Full content of the match
    #[serde(default)]
    pub content: Option<String>,
    /// Short snippet around the match
    #[serde(default)]
    pub snippet: Option<String>,
    /// Session title if available
    #[serde(default)]
    pub title: Option<String>,
    /// Match type (e.g., "exact", "fuzzy")
    #[serde(default)]
    pub match_type: Option<String>,
    /// Timestamp when the message was created
    #[serde(default)]
    pub created_at: Option<i64>,
    /// Message ID for direct navigation
    #[serde(default)]
    pub message_id: Option<String>,
}

#[derive(Debug, Clone)]
enum StreamPart {
    Text(String),
    Thinking(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        name: Option<String>,
        content: Value,
        is_error: bool,
    },
    Error {
        reason: String,
    },
}

#[derive(Debug, Default, Clone)]
struct StreamSnapshot {
    is_streaming: bool,
    has_message: bool,
    parts: Vec<StreamPart>,
}

impl StreamSnapshot {
    fn reset(&mut self) {
        self.is_streaming = false;
        self.has_message = false;
        self.parts.clear();
    }

    fn push_text(&mut self, delta: &str) {
        match self.parts.last_mut() {
            Some(StreamPart::Text(existing)) => existing.push_str(delta),
            _ => self.parts.push(StreamPart::Text(delta.to_string())),
        }
    }

    fn push_thinking(&mut self, delta: &str) {
        match self.parts.last_mut() {
            Some(StreamPart::Thinking(existing)) => existing.push_str(delta),
            _ => self.parts.push(StreamPart::Thinking(delta.to_string())),
        }
    }

    fn apply_event(&mut self, event: &PiEvent) {
        match event {
            PiEvent::AgentStart => {
                self.is_streaming = true;
            }
            PiEvent::AgentEnd { .. } => {
                self.reset();
            }
            PiEvent::MessageStart { message } => {
                if message.role == "assistant" {
                    self.is_streaming = true;
                    self.has_message = true;
                    self.parts.clear();
                }
            }
            PiEvent::MessageUpdate {
                assistant_message_event,
                message,
            } => match assistant_message_event {
                AssistantMessageEvent::TextDelta { delta, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    self.push_text(delta);
                }
                AssistantMessageEvent::TextEnd { content, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    if !content.is_empty() {
                        self.push_text(content);
                    }
                }
                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    self.push_thinking(delta);
                }
                AssistantMessageEvent::ThinkingEnd { content, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    if !content.is_empty() {
                        self.push_thinking(content);
                    }
                }
                AssistantMessageEvent::ToolcallEnd { tool_call, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    self.parts.push(StreamPart::ToolUse {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        input: tool_call.arguments.clone(),
                    });
                }
                AssistantMessageEvent::Error { reason, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    // Store error as a special part
                    self.parts.push(StreamPart::Error {
                        reason: reason.clone(),
                    });
                }
                _ => {}
            },
            PiEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
            } => {
                let content = serde_json::to_value(result).unwrap_or(Value::Null);
                self.parts.push(StreamPart::ToolResult {
                    id: tool_call_id.clone(),
                    name: Some(tool_name.clone()),
                    content,
                    is_error: *is_error,
                });
            }
            _ => {}
        }
    }

    fn to_ws_events(&self) -> Vec<Value> {
        if !self.is_streaming || !self.has_message {
            return Vec::new();
        }

        let mut events = Vec::with_capacity(self.parts.len() + 1);
        events.push(json!({"type": "message_start", "role": "assistant"}));

        for part in &self.parts {
            match part {
                StreamPart::Text(text) => {
                    events.push(json!({"type": "text", "data": text}));
                }
                StreamPart::Thinking(text) => {
                    events.push(json!({"type": "thinking", "data": text}));
                }
                StreamPart::ToolUse { id, name, input } => {
                    events.push(json!({
                        "type": "tool_use",
                        "data": {
                            "id": id,
                            "name": name,
                            "input": input
                        }
                    }));
                }
                StreamPart::ToolResult {
                    id,
                    name,
                    content,
                    is_error,
                } => {
                    events.push(json!({
                        "type": "tool_result",
                        "data": {
                            "id": id,
                            "name": name,
                            "content": content,
                            "isError": is_error
                        }
                    }));
                }
                StreamPart::Error { reason } => {
                    events.push(json!({
                        "type": "error",
                        "data": reason
                    }));
                }
            }
        }

        events
    }
}

/// Handle to a user's Pi session.
pub struct UserPiSession {
    /// The Pi process for this user (trait object for runtime polymorphism).
    process: Arc<tokio::sync::RwLock<Box<dyn PiProcess>>>,
    /// Snapshot of the currently streaming assistant message for WS replay.
    stream_snapshot: Arc<Mutex<StreamSnapshot>>,
    /// Session ID (Pi session file ID, not user ID).
    _session_id: String,
    /// Last activity timestamp (updated on every command).
    last_activity: Arc<RwLock<std::time::Instant>>,
    /// Whether the agent is currently streaming/processing.
    is_streaming: Arc<RwLock<bool>>,
    /// Single-writer guard for persistence (prevents duplicate saves across WS connections).
    persistence_writer_claimed: Arc<AtomicBool>,
}

/// Guard that releases the persistence writer claim when dropped.
pub struct PersistenceWriterGuard {
    claimed: Arc<AtomicBool>,
}

impl Drop for PersistenceWriterGuard {
    fn drop(&mut self) {
        self.claimed.store(false, Ordering::Release);
    }
}

/// How often to run the cleanup task (1 minute).
const CLEANUP_INTERVAL_SECS: u64 = 60;

/// Key for user sessions map: (user_id, session_id).
type SessionKey = (String, String);

/// Service for managing Pi sessions for Main Chat users.
pub struct MainChatPiService {
    /// Configuration.
    config: MainChatPiServiceConfig,
    /// Active sessions keyed by (user_id, session_id).
    /// Multiple sessions per user are allowed.
    sessions: RwLock<HashMap<SessionKey, Arc<UserPiSession>>>,
    /// Currently active session ID for each user.
    /// Commands are routed to this session.
    active_session: RwLock<HashMap<String, String>>,
    /// Base workspace directory.
    workspace_dir: PathBuf,
    /// Single-user mode.
    single_user: bool,
    /// Main Chat persistent store (for injecting session summaries).
    main_chat: Arc<crate::main_chat::MainChatService>,
    /// Linux user isolation configuration (multi-user mode).
    linux_users: Option<LinuxUsersConfig>,
    /// Idle timeout in seconds (sessions idle longer than this may be cleaned up).
    idle_timeout_secs: u64,
}

impl MainChatPiService {
    /// Create a new Pi service.
    pub fn new(
        workspace_dir: PathBuf,
        single_user: bool,
        config: MainChatPiServiceConfig,
        main_chat: Arc<crate::main_chat::MainChatService>,
        linux_users: Option<LinuxUsersConfig>,
    ) -> Self {
        info!(
            "MainChatPiService initialized with runtime mode: {}",
            config.runtime_mode
        );

        let idle_timeout_secs = config.idle_timeout_secs;

        Self {
            config,
            sessions: RwLock::new(HashMap::new()),
            active_session: RwLock::new(HashMap::new()),
            workspace_dir,
            single_user,
            main_chat,
            linux_users,
            idle_timeout_secs,
        }
    }

    /// Start the background cleanup task for idle sessions.
    /// Should be called once after creating the service.
    pub fn start_cleanup_task(self: &Arc<Self>) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
            loop {
                interval.tick().await;
                service.cleanup_idle_sessions().await;
            }
        });
    }

    /// Clean up idle sessions that are not streaming.
    async fn cleanup_idle_sessions(&self) {
        let now = std::time::Instant::now();
        let timeout = Duration::from_secs(self.idle_timeout_secs);

        let mut to_remove = Vec::new();

        // Find idle sessions
        {
            let sessions = self.sessions.read().await;
            for (key, session) in sessions.iter() {
                let is_streaming = *session.is_streaming.read().await;
                if is_streaming {
                    // Never kill streaming sessions
                    continue;
                }

                let last_activity = *session.last_activity.read().await;
                if now.duration_since(last_activity) > timeout {
                    info!("Session ({}, {}) is idle, will be cleaned up", key.0, key.1);
                    to_remove.push(key.clone());
                }
            }
        }

        // Remove idle sessions
        if !to_remove.is_empty() {
            let mut sessions = self.sessions.write().await;
            for key in to_remove {
                if let Some(session) = sessions.get(&key) {
                    // Double-check it's still idle (not streaming)
                    let is_streaming = *session.is_streaming.read().await;
                    if !is_streaming {
                        info!("Cleaning up idle session ({}, {})", key.0, key.1);
                        sessions.remove(&key);
                    }
                }
            }
        }
    }

    /// Create a runtime for a specific user (needed for Runner mode).
    fn create_runtime_for_user(&self, user_id: &str) -> Arc<dyn PiRuntime> {
        match self.config.runtime_mode {
            PiRuntimeMode::Local => Arc::new(LocalPiRuntime::new()),
            PiRuntimeMode::Runner => {
                // Create runner client - uses XDG_RUNTIME_DIR by default
                let client = if let Some(pattern) = self.config.runner_socket_pattern.as_deref() {
                    // Use for_user_with_pattern which handles both {user} and {uid} placeholders
                    match RunnerClient::for_user_with_pattern(user_id, pattern) {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Failed to create runner client for user {}: {}", user_id, e);
                            RunnerClient::default()
                        }
                    }
                } else {
                    RunnerClient::default()
                };
                debug!(
                    "Runner socket for user {}: {:?}",
                    user_id,
                    client.socket_path()
                );
                Arc::new(RunnerPiRuntime::new(client))
            }
            PiRuntimeMode::Container => Arc::new(ContainerPiRuntime::new()),
        }
    }

    /// Get the Main Chat directory for a user.
    fn get_main_chat_dir(&self, user_id: &str) -> PathBuf {
        if self.single_user {
            self.workspace_dir.join("main")
        } else {
            self.workspace_dir.join(user_id).join("main")
        }
    }

    /// Public accessor for Main Chat directory.
    pub fn main_chat_dir(&self, user_id: &str) -> PathBuf {
        self.get_main_chat_dir(user_id)
    }

    /// Get the Pi agent directory for a working directory.
    fn get_pi_agent_dir(&self, user_id: &str) -> PathBuf {
        let home = if self.single_user || self.linux_users.is_none() {
            dirs::home_dir()
        } else if let Some(linux_users) = self.linux_users.as_ref() {
            match linux_users.get_home_dir(user_id) {
                Ok(Some(home)) => Some(home),
                Ok(None) => {
                    warn!("Linux user home not found for user {}", user_id);
                    None
                }
                Err(err) => {
                    warn!("Failed to resolve linux user home for {}: {}", user_id, err);
                    None
                }
            }
        } else {
            None
        };

        home.map(|home| home.join(".pi").join("agent"))
            .unwrap_or_else(|| PathBuf::from("/nonexistent/.pi/agent"))
    }

    /// Resolve the repo root for a working directory (fallbacks to the directory itself).
    fn resolve_repo_root(&self, work_dir: &Path) -> PathBuf {
        let mut current = work_dir;
        loop {
            if current.join(".git").exists() {
                return current.to_path_buf();
            }
            match current.parent() {
                Some(parent) => current = parent,
                None => break,
            }
        }
        work_dir.to_path_buf()
    }

    /// Get the Pi sessions directory for a working directory.
    /// Pi stores sessions in ~/.pi/agent/sessions/--<path>--/
    /// We scope to repo root when available to avoid per-workspace collisions.
    fn get_pi_sessions_dir(&self, user_id: &str, work_dir: &Path) -> PathBuf {
        let repo_root = self.resolve_repo_root(work_dir);
        let escaped_path = repo_root
            .to_string_lossy()
            .replace('/', "-")
            .trim_start_matches('-')
            .to_string();
        // Pi stores sessions under a directory name wrapped in double-dashes.
        // Example: `--home-user-.local-share-octo-users-main--`
        self.get_pi_agent_dir(user_id)
            .join("sessions")
            .join(format!("--{}--", escaped_path))
    }

    /// Public accessor for Pi sessions directory for a work dir.
    pub fn sessions_dir_for_workdir(&self, user_id: &str, work_dir: &Path) -> PathBuf {
        self.get_pi_sessions_dir(user_id, work_dir)
    }

    async fn ensure_session_file(
        &self,
        user_id: &str,
        work_dir: &Path,
        session_id: &str,
    ) -> Result<()> {
        let sessions_dir = self.get_pi_sessions_dir(user_id, work_dir);
        if self.find_session_file_anywhere(user_id, session_id).await.is_some() {
            return Ok(());
        }
        let bootstrap_jsonl = self
            .build_bootstrap_session_jsonl(user_id, work_dir, &sessions_dir, session_id)
            .await?;
        let header = json!({
            "type": "session",
            "id": session_id,
            "timestamp": Utc::now().to_rfc3339(),
            "cwd": work_dir.to_string_lossy(),
            "readable_id": serde_json::Value::Null,
            "session_dir": sessions_dir.to_string_lossy(),
        });
        let content = if let Some(jsonl) = bootstrap_jsonl {
            jsonl
        } else {
            format!("{}\n", serde_json::to_string(&header)?)
        };

        if let Some(client) = self.runner_client_for_user(user_id) {
            let listing = client.list_directory(&sessions_dir, false).await;
            if let Ok(listing) = listing {
                if listing
                    .entries
                    .iter()
                    .any(|entry| entry.name.ends_with(".jsonl") && entry.name.contains(session_id))
                {
                    return Ok(());
                }
            } else {
                let _ = client.create_directory(&sessions_dir, true).await;
            }

            let filename = format!("{}_{}.jsonl", Utc::now().timestamp_millis(), session_id);
            let path = sessions_dir.join(filename);
            client
                .write_file(path, content.as_bytes(), true)
                .await
                .context("writing session file via runner")?;
            return Ok(());
        }

        if !sessions_dir.exists() {
            std::fs::create_dir_all(&sessions_dir).context("creating sessions directory")?;
        } else if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if filename.contains(session_id) {
                        return Ok(());
                    }
                }
            }
        }

        let filename = format!("{}_{}.jsonl", Utc::now().timestamp_millis(), session_id);
        let path = sessions_dir.join(filename);
        std::fs::write(&path, content).context("writing session file")?;
        Ok(())
    }

    async fn build_bootstrap_session_jsonl(
        &self,
        user_id: &str,
        work_dir: &Path,
        sessions_dir: &Path,
        session_id: &str,
    ) -> Result<Option<String>> {
        if !self.bootstrap_file_exists(user_id, work_dir).await {
            return Ok(None);
        }
        if self.sessions_dir_has_jsonl(user_id, sessions_dir).await {
            return Ok(None);
        }
        if let Some(meta) = self.load_workspace_meta_for_user(user_id, work_dir).await {
            if meta.bootstrap_pending == Some(false) {
                return Ok(None);
            }
        }

        let language = self.bootstrap_language(user_id, work_dir).await;
        let message = Self::pick_bootstrap_message(&language);
        let content = json!([{ "type": "text", "text": message }]);
        let now_ms = Utc::now().timestamp_millis();
        let jsonl = Self::build_pi_session_jsonl(
            session_id,
            &work_dir.to_string_lossy(),
            now_ms,
            None,
            None,
            vec![("assistant".to_string(), content, now_ms)],
        );
        Ok(Some(jsonl))
    }

    async fn load_workspace_meta_for_user(
        &self,
        user_id: &str,
        work_dir: &Path,
    ) -> Option<workspace::WorkspaceMeta> {
        let meta_path = workspace::workspace_meta_path(work_dir);

        if let Some(client) = self.runner_client_for_user(user_id) {
            let content = client
                .read_file(meta_path.clone(), None, None)
                .await
                .ok()?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content.content_base64)
                .ok()?;
            let text = String::from_utf8(bytes).ok()?;
            return workspace::parse_workspace_meta(&text);
        }

        workspace::load_workspace_meta(work_dir)
    }

    async fn bootstrap_file_exists(&self, user_id: &str, work_dir: &Path) -> bool {
        let path = work_dir.join("BOOTSTRAP.md");
        if let Some(client) = self.runner_client_for_user(user_id) {
            return client.stat(path).await.is_ok();
        }
        path.exists()
    }

    async fn sessions_dir_has_jsonl(&self, user_id: &str, sessions_dir: &Path) -> bool {
        if let Some(client) = self.runner_client_for_user(user_id) {
            if let Ok(listing) = client.list_directory(sessions_dir, false).await {
                return listing.entries.iter().any(|entry| entry.name.ends_with(".jsonl"));
            }
            return false;
        }

        let Ok(entries) = std::fs::read_dir(sessions_dir) else {
            return false;
        };
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().map(|e| e == "jsonl").unwrap_or(false))
    }

    async fn bootstrap_language(&self, user_id: &str, work_dir: &Path) -> String {
        self.load_workspace_meta_for_user(user_id, work_dir)
            .await
            .and_then(|meta| meta.language)
            .map(|l| l.trim().to_lowercase())
            .unwrap_or_else(|| "en".to_string())
    }

    fn pick_bootstrap_message(language: &str) -> &'static str {
        let messages = if language.starts_with("de") {
            BOOTSTRAP_MESSAGES_DE
        } else {
            BOOTSTRAP_MESSAGES_EN
        };
        let mut rng = rand::rng();
        let idx = rng.random_range(0..messages.len());
        messages[idx]
    }

    fn map_pi_role(role: &str) -> &str {
        match role {
            "user" => "user",
            "assistant" => "assistant",
            "tool" | "toolResult" => "toolResult",
            "system" => "custom",
            _ => "assistant",
        }
    }

    fn derive_provider(model: &str) -> &'static str {
        let lower = model.to_lowercase();
        if lower.contains("claude") || lower.contains("anthropic") {
            return "anthropic";
        }
        if lower.contains("gpt") || lower.contains("openai") || lower.contains("codex") {
            return "openai";
        }
        if lower.contains("gemini") || lower.contains("google") {
            return "google";
        }
        if lower.contains("llama") || lower.contains("meta") {
            return "meta";
        }
        "unknown"
    }

    fn build_pi_session_jsonl(
        session_id: &str,
        cwd: &str,
        created_at_ms: i64,
        title: Option<String>,
        model: Option<String>,
        messages: Vec<(String, serde_json::Value, i64)>,
    ) -> String {
        let mut lines = Vec::new();
        let header = json!({
            "type": "session",
            "version": 3,
            "id": session_id,
            "timestamp": Utc.timestamp_millis_opt(created_at_ms)
                .single()
                .unwrap_or_else(Utc::now)
                .to_rfc3339(),
            "cwd": cwd,
        });
        lines.push(serde_json::to_string(&header).unwrap_or_else(|_| "{}".to_string()));

        let mut last_entry_id: Option<String> = None;
        if let Some(model) = model.clone() {
            let model_entry = json!({
                "type": "model_change",
                "id": Uuid::new_v4().simple().to_string(),
                "parentId": last_entry_id,
                "timestamp": Utc.timestamp_millis_opt(created_at_ms)
                    .single()
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339(),
                "provider": Self::derive_provider(&model),
                "modelId": model,
            });
            if let Ok(line) = serde_json::to_string(&model_entry) {
                last_entry_id = model_entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                lines.push(line);
            }
        }

        if let Some(title) = title {
            let info_entry = json!({
                "type": "session_info",
                "id": Uuid::new_v4().simple().to_string(),
                "parentId": last_entry_id,
                "timestamp": Utc.timestamp_millis_opt(created_at_ms)
                    .single()
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339(),
                "name": title,
            });
            if let Ok(line) = serde_json::to_string(&info_entry) {
                last_entry_id = info_entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                lines.push(line);
            }
        }

        for (role, content, timestamp_ms) in messages {
            let msg_entry = json!({
                "type": "message",
                "id": Uuid::new_v4().simple().to_string(),
                "parentId": last_entry_id,
                "timestamp": Utc.timestamp_millis_opt(timestamp_ms)
                    .single()
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339(),
                "message": {
                    "role": Self::map_pi_role(&role),
                    "content": content,
                    "timestamp": timestamp_ms,
                }
            });
            if let Ok(line) = serde_json::to_string(&msg_entry) {
                last_entry_id = msg_entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                lines.push(line);
            }
        }

        lines.join("\n") + "\n"
    }

    async fn write_session_file(
        &self,
        user_id: &str,
        sessions_dir: &Path,
        session_id: &str,
        content: String,
    ) -> Result<PathBuf> {
        let filename = format!("{}_{}.jsonl", Utc::now().timestamp_millis(), session_id);
        let path = sessions_dir.join(filename);

        if let Some(client) = self.runner_client_for_user(user_id) {
            let _ = client.create_directory(sessions_dir, true).await;
            client
                .write_file(path.clone(), content.as_bytes(), true)
                .await
                .context("writing session file via runner")?;
            return Ok(path);
        }

        if !sessions_dir.exists() {
            std::fs::create_dir_all(sessions_dir).context("creating sessions directory")?;
        }
        std::fs::write(&path, content).context("writing session file")?;
        Ok(path)
    }

    async fn rehydrate_session_from_hstry(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Option<PathBuf>> {
        let work_dir = self.get_main_chat_dir(user_id);
        let sessions_dir = self.get_pi_sessions_dir(user_id, &work_dir);

        if let Some(client) = self.runner_client_for_user(user_id) {
            let resp = client
                .get_main_chat_messages(session_id, None)
                .await
                .context("runner get_main_chat_messages")?;
            if resp.messages.is_empty() {
                return Ok(None);
            }

            let created_at_ms = resp
                .messages
                .first()
                .map(|m| m.timestamp)
                .unwrap_or_else(|| Utc::now().timestamp_millis());
            let messages = resp
                .messages
                .into_iter()
                .map(|m| (m.role, m.content, m.timestamp))
                .collect();
            let jsonl = Self::build_pi_session_jsonl(
                session_id,
                &work_dir.to_string_lossy(),
                created_at_ms,
                None,
                None,
                messages,
            );
            let path = self
                .write_session_file(user_id, &sessions_dir, session_id, jsonl)
                .await?;
            return Ok(Some(path));
        }

        let Some(db_path) = crate::history::hstry_db_path() else {
            return Ok(None);
        };
        let pool = crate::history::repository::open_hstry_pool(&db_path).await?;

        let conv_row = sqlx::query(
            r#"
            SELECT id, external_id, title, created_at, model, workspace
            FROM conversations
            WHERE source_id = 'pi' AND (external_id = ? OR readable_id = ? OR id = ?)
            LIMIT 1
            "#,
        )
        .bind(session_id)
        .bind(session_id)
        .bind(session_id)
        .fetch_optional(&pool)
        .await?;

        let Some(conv_row) = conv_row else {
            return Ok(None);
        };

        let conversation_id: String = conv_row.try_get("id")?;
        let title: Option<String> = conv_row.try_get("title").ok();
        let model: Option<String> = conv_row.try_get("model").ok();
        let workspace: Option<String> = conv_row.try_get("workspace").ok();
        let created_at: i64 = conv_row.try_get("created_at").unwrap_or_else(|_| 0);
        let created_at_ms = created_at * 1000;

        let rows = sqlx::query(
            r#"
            SELECT role, content, created_at, parts_json
            FROM messages
            WHERE conversation_id = ?
            ORDER BY idx
            "#,
        )
        .bind(&conversation_id)
        .fetch_all(&pool)
        .await?;

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let role: String = row
                .try_get("role")
                .unwrap_or_else(|_| "assistant".to_string());
            let content_raw: String = row.try_get("content").unwrap_or_default();
            let created_at: Option<i64> = row.try_get("created_at").ok();
            let parts_json: Option<String> = row.try_get("parts_json").ok();

            let content = if let Some(parts_json) = parts_json.as_deref()
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(parts_json)
                && v.is_array()
            {
                v
            } else {
                serde_json::json!([{ "type": "text", "text": content_raw }])
            };

            let timestamp_ms = created_at
                .map(|ts| ts * 1000)
                .unwrap_or_else(|| Utc::now().timestamp_millis());
            messages.push((role, content, timestamp_ms));
        }

        let cwd = workspace.unwrap_or_else(|| work_dir.to_string_lossy().to_string());
        let jsonl = Self::build_pi_session_jsonl(
            session_id,
            &cwd,
            if created_at_ms > 0 {
                created_at_ms
            } else {
                Utc::now().timestamp_millis()
            },
            title,
            model,
            messages,
        );

        let path = self
            .write_session_file(user_id, &sessions_dir, session_id, jsonl)
            .await?;
        Ok(Some(path))
    }

    /// Create a runner client for a user if available.
    fn runner_client_for_user(&self, user_id: &str) -> Option<RunnerClient> {
        self.linux_users.as_ref()?;
        let pattern = self.config.runner_socket_pattern.as_deref()?;
        // Use for_user_with_pattern which handles both {user} and {uid} placeholders
        match RunnerClient::for_user_with_pattern(user_id, pattern) {
            Ok(c) if c.socket_path().exists() => Some(c),
            Ok(_) => None,
            Err(e) => {
                warn!("Failed to create runner client for user {}: {}", user_id, e);
                None
            }
        }
    }

    /// List session file entries for a user.
    async fn list_session_entries(
        &self,
        user_id: &str,
        sessions_dir: &PathBuf,
    ) -> Result<Vec<(PathBuf, u64, i64)>> {
        if let Some(client) = self.runner_client_for_user(user_id) {
            let listing = client
                .list_directory(sessions_dir, false)
                .await
                .context("listing session directory via runner")?;
            let mut entries = Vec::new();
            for entry in listing.entries {
                if !entry.name.ends_with(".jsonl") {
                    continue;
                }
                let path = sessions_dir.join(&entry.name);
                entries.push((path, entry.size, entry.modified_at));
            }
            return Ok(entries);
        }

        if !sessions_dir.exists() {
            debug!("Pi sessions directory does not exist: {:?}", sessions_dir);
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        if let Ok(read_dir) = std::fs::read_dir(sessions_dir) {
            for entry in read_dir.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false)
                    && let Ok(metadata) = entry.metadata()
                {
                    let modified_at = metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    entries.push((path, metadata.len(), modified_at));
                }
            }
        }

        Ok(entries)
    }

    async fn list_all_session_entries(&self, user_id: &str) -> Result<Vec<(PathBuf, u64, i64)>> {
        let sessions_root = self.get_pi_agent_dir(user_id).join("sessions");
        let mut entries: Vec<(PathBuf, u64, i64)> = Vec::new();

        if let Some(client) = self.runner_client_for_user(user_id) {
            let listing = client.list_directory(&sessions_root, false).await?;
            for entry in listing.entries.iter().filter(|e| e.is_dir) {
                let dir_path = sessions_root.join(&entry.name);
                let sub = client.list_directory(&dir_path, false).await?;
                for file in sub.entries.iter() {
                    if file.is_dir || !file.name.ends_with(".jsonl") {
                        continue;
                    }
                    entries.push((
                        dir_path.join(&file.name),
                        file.size,
                        file.modified_at,
                    ));
                }
            }
            return Ok(entries);
        }

        if !sessions_root.exists() {
            return Ok(entries);
        }

        let roots = std::fs::read_dir(&sessions_root)
            .context("reading sessions root directory")?;
        for root in roots.filter_map(|e| e.ok()) {
            let root_path = root.path();
            if !root_path.is_dir() {
                continue;
            }
            let dir_entries = std::fs::read_dir(&root_path)
                .context("reading sessions directory")?;
            for entry in dir_entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path
                    .extension()
                    .map(|e| e == "jsonl")
                    .unwrap_or(false)
                {
                    continue;
                }
                let modified_at = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
                entries.push((path, size, modified_at));
            }
        }

        Ok(entries)
    }

    /// Find the most recent Pi session file for a directory.
    async fn find_last_session(
        &self,
        user_id: &str,
        work_dir: &PathBuf,
    ) -> Result<Option<LastSessionInfo>> {
        let sessions_dir = self.get_pi_sessions_dir(user_id, work_dir);
        let entries = self.list_session_entries(user_id, &sessions_dir).await?;
        let mut latest: Option<LastSessionInfo> = None;

        for (_path, size, modified_at) in entries {
            let modified =
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(modified_at as u64);
            let info = LastSessionInfo { size, modified };
            if latest
                .as_ref()
                .map(|l| modified > l.modified)
                .unwrap_or(true)
            {
                latest = Some(info);
            }
        }

        Ok(latest)
    }

    /// List all Pi sessions for a user.
    pub async fn list_sessions(&self, user_id: &str) -> Result<Vec<PiSessionFile>> {
        let mut sessions_by_id: std::collections::HashMap<String, PiSessionFile> =
            std::collections::HashMap::new();
        let entries = self.list_all_session_entries(user_id).await?;

        if let Some(client) = self.runner_client_for_user(user_id) {
            for (path, size, modified_at) in entries {
                let content = client
                    .read_file(&path, None, None)
                    .await
                    .context("reading session file via runner")?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(content.content_base64)
                    .context("decoding session file base64")?;
                let reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
                if let Some(session) = Self::parse_session_reader(reader, size, modified_at) {
                    sessions_by_id
                        .entry(session.id.clone())
                        .and_modify(|existing| {
                            if session.modified_at > existing.modified_at {
                                *existing = session.clone();
                            }
                        })
                        .or_insert(session);
                }
            }
        } else {
            for (path, size, modified_at) in entries {
                if let Some(session) = Self::parse_session_file(&path, size, modified_at) {
                    sessions_by_id
                        .entry(session.id.clone())
                        .and_modify(|existing| {
                            if session.modified_at > existing.modified_at {
                                *existing = session.clone();
                            }
                        })
                        .or_insert(session);
                }
            }
        }

        let mut sessions: Vec<PiSessionFile> = sessions_by_id.into_values().collect();

        // Sort by modified_at descending (most recently active first)
        sessions.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

        Ok(sessions)
    }

    /// Search for sessions matching a query string.
    /// Supports fuzzy matching on session ID and title.
    /// Returns sessions sorted by match quality (best first).
    pub async fn search_sessions(&self, user_id: &str, query: &str) -> Result<Vec<PiSessionFile>> {
        let all_sessions = self.list_sessions(user_id).await?;
        let query_lower = query.to_lowercase();

        // Score each session by match quality
        let mut scored: Vec<(i32, PiSessionFile)> = all_sessions
            .into_iter()
            .filter_map(|session| {
                let score = self.compute_match_score(&session, &query_lower);
                if score > 0 {
                    Some((score, session))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending (best matches first)
        scored.sort_by(|a, b| b.0.cmp(&a.0));

        Ok(scored.into_iter().map(|(_, s)| s).collect())
    }

    /// Update the title of a Pi session.
    /// This modifies the session header line in the JSONL file.
    pub async fn update_session_title(
        &self,
        user_id: &str,
        session_id: &str,
        title: &str,
    ) -> Result<PiSessionFile> {
        use std::io::{BufRead, BufReader, Write};

        let session_path = self
            .resolve_session_file_path(user_id, session_id)
            .await?;

        let mut lines: Vec<String> = if let Some(client) = self.runner_client_for_user(user_id) {
            let content = client
                .read_file(&session_path, None, None)
                .await
                .context("reading session file via runner")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content.content_base64)
                .context("decoding session file base64")?;
            let reader = BufReader::new(std::io::Cursor::new(bytes));
            reader.lines().collect::<std::io::Result<_>>()?
        } else {
            let file = std::fs::File::open(&session_path).context("opening session file")?;
            let reader = BufReader::new(file);
            reader.lines().collect::<std::io::Result<_>>()?
        };

        if lines.is_empty() {
            anyhow::bail!("Session file is empty");
        }

        // Parse and update the header (first line)
        let mut header: serde_json::Value =
            serde_json::from_str(&lines[0]).context("parsing session header")?;

        if header.get("type").and_then(|t| t.as_str()) != Some("session") {
            anyhow::bail!("Invalid session file: missing session header");
        }

        // Update or add the title field
        header["title"] = serde_json::Value::String(title.to_string());
        lines[0] = serde_json::to_string(&header)?;

        let updated_text = lines.join("\n") + "\n";
        if let Some(client) = self.runner_client_for_user(user_id) {
            client
                .write_file(&session_path, updated_text.as_bytes(), false)
                .await
                .context("writing session file via runner")?;
        } else {
            let mut file =
                std::fs::File::create(&session_path).context("creating session file for write")?;
            write!(file, "{}", updated_text)?;
        }

        let size = updated_text.len() as u64;
        let modified_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let reader = BufReader::new(std::io::Cursor::new(updated_text));
        Self::parse_session_reader(reader, size, modified_at)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse updated session"))
    }

    /// Compute a match score for a session against a query.
    /// Returns 0 if no match, higher scores for better matches.
    fn compute_match_score(&self, session: &PiSessionFile, query: &str) -> i32 {
        let mut score = 0;

        // Exact ID match (highest priority)
        if session.id.to_lowercase() == query {
            return 1000;
        }

        // ID contains query
        if session.id.to_lowercase().contains(query) {
            score += 100;
        }

        // ID starts with query (partial match)
        if session.id.to_lowercase().starts_with(query) {
            score += 50;
        }

        // Title matching
        if let Some(ref title) = session.title {
            let title_lower = title.to_lowercase();

            // Exact title match
            if title_lower == query {
                return 900;
            }

            // Title contains query
            if title_lower.contains(query) {
                score += 80;
            }

            // Word-level fuzzy matching
            let query_words: Vec<&str> = query.split_whitespace().collect();
            let title_words: Vec<&str> = title_lower.split_whitespace().collect();

            for qw in &query_words {
                for tw in &title_words {
                    if tw.starts_with(qw) {
                        score += 20;
                    } else if Self::levenshtein_distance(qw, tw) <= 2 {
                        score += 10; // Typo tolerance
                    }
                }
            }
        }

        // Fuzzy match on ID (for typos in readable IDs like "adj-noun-noun")
        let id_parts: Vec<&str> = session.id.split('-').collect();
        let query_parts: Vec<&str> = query.split('-').collect();
        for qp in &query_parts {
            for ip in &id_parts {
                if ip.to_lowercase().starts_with(&qp.to_lowercase()) {
                    score += 15;
                } else if Self::levenshtein_distance(&qp.to_lowercase(), &ip.to_lowercase()) <= 2 {
                    score += 5;
                }
            }
        }

        score
    }

    /// Compute Levenshtein distance between two strings.
    fn levenshtein_distance(a: &str, b: &str) -> usize {
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();
        let a_len = a_chars.len();
        let b_len = b_chars.len();

        if a_len == 0 {
            return b_len;
        }
        if b_len == 0 {
            return a_len;
        }

        let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

        for i in 0..=a_len {
            matrix[i][0] = i;
        }
        for j in 0..=b_len {
            matrix[0][j] = j;
        }

        for i in 1..=a_len {
            for j in 1..=b_len {
                let cost = if a_chars[i - 1] == b_chars[j - 1] {
                    0
                } else {
                    1
                };
                matrix[i][j] = (matrix[i - 1][j] + 1)
                    .min(matrix[i][j - 1] + 1)
                    .min(matrix[i - 1][j - 1] + cost);
            }
        }

        matrix[a_len][b_len]
    }

    /// Parse a Pi session file to extract metadata.
    fn parse_session_reader<R: std::io::BufRead>(
        mut reader: R,
        size: u64,
        modified_ms: i64,
    ) -> Option<PiSessionFile> {
        // Fast path: parse only header + first user message.
        let mut first_line = String::new();
        reader.read_line(&mut first_line).ok()?;
        if first_line.trim().is_empty() {
            return None;
        }

        let header: Value = serde_json::from_str(&first_line).ok()?;
        if header.get("type").and_then(|t| t.as_str()) != Some("session") {
            return None;
        }

        if header.get("deleted").and_then(|v| v.as_bool()) == Some(true) {
            return None;
        }

        let id = header.get("id").and_then(|v| v.as_str())?.to_string();
        let started_at = header
            .get("timestamp")
            .and_then(|v| v.as_str())?
            .to_string();
        let workspace_path = header
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let session_dir = header
            .get("session_dir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Check for explicit title in header first (set via rename)
        let mut title = header
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let parent_id = header
            .get("parentSession")
            .and_then(|v| v.as_str())
            .and_then(Self::read_parent_session_id);
        let mut message_count = 0usize;
        let mut session_info_name: Option<String> = None;

        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                continue;
            }

            let entry: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if entry.get("type").and_then(|t| t.as_str()) == Some("session_info") {
                if let Some(name) = entry.get("name").and_then(|v| v.as_str()) {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        session_info_name = Some(trimmed.to_string());
                    }
                }
                continue;
            }

            if entry.get("type").and_then(|t| t.as_str()) != Some("message") {
                continue;
            }

            message_count += 1;

            // Only extract title from first user message if no explicit title set
            if title.is_none()
                && let Some(msg) = entry.get("message")
                && msg.get("role").and_then(|r| r.as_str()) == Some("user")
                && let Some(content) = msg.get("content")
            {
                title = Self::extract_title_from_content(content);
                // Stop early once we have a title.
                break;
            }
        }

        // Prefer session_info name if present (latest session_info wins)
        if let Some(info_name) = session_info_name {
            title = Some(info_name);
        }

        // Parse title to extract readable_id (format: <workdir>: <title> [readable_id])
        let parsed_title = title
            .as_ref()
            .map(|t| crate::pi::session_parser::ParsedTitle::parse(t));

        let readable_id = parsed_title
            .as_ref()
            .and_then(|p| p.get_readable_id())
            .map(String::from);

        // Optionally strip workspace and ID from title for cleaner display
        // This preserves the original auto-generated format in the file but returns cleaner version
        let display_title = if let Some(parsed) = parsed_title {
            parsed.display_title().to_string()
        } else {
            title.clone().unwrap_or_default()
        };

        Some(PiSessionFile {
            id,
            started_at,
            size,
            modified_at: modified_ms,
            title: if display_title.is_empty() {
                None
            } else {
                Some(display_title)
            },
            readable_id,
            parent_id,
            message_count,
            workspace_path,
            session_dir,
        })
    }

    fn parse_session_file(
        path: &std::path::Path,
        size: u64,
        modified_ms: i64,
    ) -> Option<PiSessionFile> {
        let file = std::fs::File::open(path).ok()?;
        let reader = std::io::BufReader::new(file);
        Self::parse_session_reader(reader, size, modified_ms)
    }

    /// Soft-delete a Pi session by marking the JSONL header as deleted.
    pub async fn delete_session_file(&self, user_id: &str, session_id: &str) -> Result<bool> {
        use std::io::{BufRead, BufReader};

        let session_path = self
            .resolve_session_file_path(user_id, session_id)
            .await?;

        let mut lines: Vec<String> = if let Some(client) = self.runner_client_for_user(user_id) {
            let content = client
                .read_file(&session_path, None, None)
                .await
                .context("reading session file via runner")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content.content_base64)
                .context("decoding session file base64")?;
            let reader = BufReader::new(std::io::Cursor::new(bytes));
            reader.lines().collect::<std::io::Result<_>>()?
        } else {
            let file = std::fs::File::open(&session_path).context("opening session file")?;
            let reader = BufReader::new(file);
            reader.lines().collect::<std::io::Result<_>>()?
        };

        if lines.is_empty() {
            anyhow::bail!("Session file is empty");
        }

        let mut header: serde_json::Value =
            serde_json::from_str(&lines[0]).context("parsing session header")?;

        if header.get("type").and_then(|t| t.as_str()) != Some("session") {
            anyhow::bail!("Invalid session file: missing session header");
        }

        if header.get("deleted").and_then(|v| v.as_bool()) == Some(true) {
            return Ok(false);
        }

        header["deleted"] = serde_json::Value::Bool(true);
        header["deleted_at"] = serde_json::Value::String(Utc::now().to_rfc3339());
        lines[0] = serde_json::to_string(&header)?;

        let updated_text = lines.join("\n") + "\n";
        if let Some(client) = self.runner_client_for_user(user_id) {
            client
                .write_file(&session_path, updated_text.as_bytes(), false)
                .await
                .context("writing session file via runner")?;
        } else {
            std::fs::write(&session_path, updated_text).context("writing session file")?;
        }

        Ok(true)
    }

    /// Resolve a parent session ID from a session file path.
    fn read_parent_session_id(path: &str) -> Option<String> {
        use std::io::{BufRead, BufReader};

        let file = std::fs::File::open(path).ok()?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if line.trim().is_empty() {
            return None;
        }
        let header: Value = serde_json::from_str(&line).ok()?;
        if header.get("type").and_then(|t| t.as_str()) != Some("session") {
            return None;
        }
        header
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Extract a title from message content (first ~50 chars of text).
    fn extract_title_from_content(content: &Value) -> Option<String> {
        if let Some(text) = content.as_str() {
            return Some(Self::truncate_title(text));
        }

        if let Some(arr) = content.as_array() {
            for block in arr {
                if block.get("type").and_then(|t| t.as_str()) == Some("text")
                    && let Some(text) = block.get("text").and_then(|t| t.as_str())
                {
                    return Some(Self::truncate_title(text));
                }
            }
        }

        None
    }

    /// Truncate text to ~50 chars for title.
    fn truncate_title(text: &str) -> String {
        let text = text.trim();
        if text.len() <= 50 {
            text.to_string()
        } else {
            format!("{}...", &text[..47])
        }
    }

    /// Get messages from a specific Pi session file.
    pub async fn get_session_messages(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<PiSessionMessage>> {
        use std::io::{BufRead, BufReader};

        let session_file = self
            .resolve_session_file_path(user_id, session_id)
            .await?;

        let reader: Box<dyn BufRead> = if let Some(client) = self.runner_client_for_user(user_id) {
            let content = client
                .read_file(&session_file, None, None)
                .await
                .context("reading session file via runner")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content.content_base64)
                .context("decoding session file base64")?;
            Box::new(BufReader::new(std::io::Cursor::new(bytes)))
        } else {
            let file = std::fs::File::open(&session_file).context("opening session file")?;
            Box::new(BufReader::new(file))
        };

        let mut messages = Vec::new();

        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                continue;
            }

            if let Ok(entry) = serde_json::from_str::<Value>(&line)
                && entry.get("type").and_then(|t| t.as_str()) == Some("message")
                && let Some(msg) = entry.get("message")
            {
                let id = entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let role = msg
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user")
                    .to_string();
                let content = msg.get("content").cloned().unwrap_or(Value::Null);
                let tool_call_id = msg
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let tool_name = msg
                    .get("toolName")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let is_error = msg.get("isError").and_then(|v| v.as_bool());
                let timestamp = msg.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
                let usage = msg.get("usage").cloned();

                messages.push(PiSessionMessage {
                    id,
                    role,
                    content,
                    tool_call_id,
                    tool_name,
                    is_error,
                    timestamp,
                    usage,
                });
            }
        }

        Ok(messages)
    }

    /// Search within a specific session using hstry, with fallback to direct text search.
    /// Returns search results from the session's content.
    /// Supports both Pi sessions (.jsonl) and OpenCode sessions (.json).
    pub async fn search_in_session(
        &self,
        _user_id: &str,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSearchResult>> {
        // First try hstry search filtered by session ID
        let hstry_results = self
            .search_in_session_via_hstry(session_id, query, limit)
            .await;

        if let Ok(ref results) = hstry_results
            && !results.is_empty()
        {
            return hstry_results;
        }

        // Fallback: direct text search in OpenCode message parts
        self.search_in_opencode_session(session_id, query, limit)
            .await
    }

    /// Search using hstry and filter by session ID.
    async fn search_in_session_via_hstry(
        &self,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSearchResult>> {
        let search_limit = limit.saturating_mul(6).max(10);
        let hits = crate::history::search_hstry(query, search_limit).await?;

        let mut results = Vec::new();
        for hit in hits {
            let external_id = hit.external_id.clone().unwrap_or_default();
            let source_path = hit
                .source_path
                .clone()
                .unwrap_or_else(|| format!("hstry:{}:{}", hit.source_id, hit.conversation_id));

            let matches_session = external_id == session_id
                || hit.conversation_id == session_id
                || source_path.contains(session_id);
            if !matches_session {
                continue;
            }

            let timestamp = hit
                .created_at
                .or(hit.conv_updated_at)
                .map(|dt| dt.timestamp_millis())
                .or_else(|| Some(hit.conv_created_at.timestamp_millis()));

            results.push(SessionSearchResult {
                source_path,
                line_number: (hit.message_idx.max(0) as usize) + 1,
                agent: hit.source_id.clone(),
                score: f64::from(hit.score),
                content: Some(hit.content.clone()),
                snippet: Some(hit.snippet.clone()),
                title: hit.title.clone(),
                match_type: None,
                created_at: timestamp,
                message_id: None,
            });

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    /// Direct text search in OpenCode session message parts.
    /// Fallback when hstry hasn't indexed the session.
    async fn search_in_opencode_session(
        &self,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSearchResult>> {
        let home = std::env::var("HOME").context("HOME not set")?;
        let messages_dir = PathBuf::from(&home)
            .join(".local/share/opencode/storage/message")
            .join(session_id);

        if !messages_dir.exists() {
            return Ok(Vec::new());
        }

        let parts_dir = PathBuf::from(&home).join(".local/share/opencode/storage/part");

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        let mut line_number = 0;

        // Read message metadata to get message IDs
        let mut message_ids: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&messages_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false)
                    && let Some(filename) = path.file_stem().and_then(|s| s.to_str())
                {
                    message_ids.push(filename.to_string());
                }
            }
        }

        // Sort by message ID (they're roughly chronological)
        message_ids.sort();

        // Search through message parts
        for msg_id in message_ids {
            let msg_parts_dir = parts_dir.join(&msg_id);
            if !msg_parts_dir.exists() {
                continue;
            }

            if let Ok(part_entries) = std::fs::read_dir(&msg_parts_dir) {
                for part_entry in part_entries.filter_map(|e| e.ok()) {
                    let part_path = part_entry.path();
                    if !part_path.extension().map(|e| e == "json").unwrap_or(false) {
                        continue;
                    }

                    line_number += 1;

                    // Read and search the part content
                    if let Ok(content) = std::fs::read_to_string(&part_path) {
                        // Parse JSON to extract text content
                        if let Ok(part_json) = serde_json::from_str::<serde_json::Value>(&content) {
                            let text = part_json.get("text").and_then(|v| v.as_str()).unwrap_or("");

                            if text.to_lowercase().contains(&query_lower) {
                                // Create a snippet around the match
                                let snippet = Self::create_snippet(text, &query_lower, 100);

                                results.push(SessionSearchResult {
                                    source_path: part_path.to_string_lossy().to_string(),
                                    line_number,
                                    agent: "opencode".to_string(),
                                    score: 1.0,
                                    content: Some(text.to_string()),
                                    snippet: Some(snippet),
                                    title: None,
                                    match_type: Some("keyword".to_string()),
                                    created_at: part_json
                                        .get("time")
                                        .and_then(|t| t.get("created"))
                                        .and_then(|c| c.as_i64()),
                                    message_id: Some(msg_id.clone()),
                                });

                                if results.len() >= limit {
                                    return Ok(results);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Create a snippet around the first match of query in text.
    fn create_snippet(text: &str, query: &str, context_chars: usize) -> String {
        let text_lower = text.to_lowercase();
        if let Some(pos) = text_lower.find(query) {
            let start = pos.saturating_sub(context_chars);
            let end = (pos + query.len() + context_chars).min(text.len());

            // Find word boundaries
            let snippet_start = text[..start].rfind(' ').map(|p| p + 1).unwrap_or(start);
            let snippet_end = text[end..].find(' ').map(|p| end + p).unwrap_or(end);

            let mut snippet = String::new();
            if snippet_start > 0 {
                snippet.push_str("...");
            }
            snippet.push_str(&text[snippet_start..snippet_end]);
            if snippet_end < text.len() {
                snippet.push_str("...");
            }
            snippet
        } else {
            text.chars().take(200).collect()
        }
    }

    /// Find a Pi session file by ID (.jsonl format).
    async fn find_session_file(
        &self,
        user_id: &str,
        sessions_dir: &std::path::Path,
        session_id: &str,
    ) -> Result<PathBuf> {
        let entries = self
            .list_session_entries(user_id, &sessions_dir.to_path_buf())
            .await?;

        let mut best: Option<(i64, PathBuf)> = None;
        for (path, _size, modified_at) in entries {
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if filename.contains(session_id) {
                    match best {
                        Some((best_ts, _)) if modified_at <= best_ts => {}
                        _ => best = Some((modified_at, path)),
                    }
                }
            }
        }

        if let Some((_, path)) = best {
            return Ok(path);
        }

        anyhow::bail!("Session not found: {}", session_id)
    }

    async fn find_session_file_anywhere(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Option<PathBuf> {
        let sessions_root = self.get_pi_agent_dir(user_id).join("sessions");
        if let Some(client) = self.runner_client_for_user(user_id) {
            let listing = client.list_directory(&sessions_root, false).await.ok()?;
            let mut best: Option<(i64, PathBuf)> = None;
            for entry in listing.entries.iter().filter(|e| e.is_dir) {
                let dir_path = sessions_root.join(&entry.name);
                let sub = client.list_directory(&dir_path, false).await.ok()?;
                for file in sub.entries.iter() {
                    if file.is_dir || !file.name.ends_with(".jsonl") {
                        continue;
                    }
                    if !file.name.contains(session_id) {
                        continue;
                    }
                    match best {
                        Some((best_ts, _)) if file.modified_at <= best_ts => {}
                        _ => best = Some((file.modified_at, dir_path.join(&file.name))),
                    }
                }
            }
            return best.map(|(_, path)| path);
        }

        if !sessions_root.exists() {
            return None;
        }

        let mut best: Option<(i64, PathBuf)> = None;
        let roots = std::fs::read_dir(&sessions_root).ok()?;
        for root in roots.filter_map(|e| e.ok()) {
            let root_path = root.path();
            if !root_path.is_dir() {
                continue;
            }
            let entries = match std::fs::read_dir(&root_path) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !filename.contains(session_id) {
                        continue;
                    }
                    let modified_at = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    match best {
                        Some((best_ts, _)) if modified_at <= best_ts => {}
                        _ => best = Some((modified_at, path)),
                    }
                }
            }
        }

        best.map(|(_, path)| path)
    }

    async fn read_session_header(
        &self,
        user_id: &str,
        session_path: &std::path::Path,
    ) -> Result<Value> {
        if let Some(client) = self.runner_client_for_user(user_id) {
            let content = client
                .read_file(session_path, None, None)
                .await
                .context("reading session file via runner")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content.content_base64)
                .context("decoding session file base64")?;
            let mut reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
            let mut first_line = String::new();
            reader.read_line(&mut first_line).context("reading header line")?;
            return serde_json::from_str(&first_line).context("parsing session header");
        }

        let file = std::fs::File::open(session_path).context("opening session file")?;
        let mut reader = std::io::BufReader::new(file);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).context("reading header line")?;
        serde_json::from_str(&first_line).context("parsing session header")
    }

    fn work_dir_from_header(header: &Value) -> Option<PathBuf> {
        header
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
    }

    async fn resolve_session_file_path(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<PathBuf> {
        if let Some(path) = self.find_session_file_anywhere(user_id, session_id).await {
            return Ok(path);
        }
        if let Ok(Some(path)) = self.rehydrate_session_from_hstry(user_id, session_id).await {
            return Ok(path);
        }
        anyhow::bail!("Session not found: {}", session_id)
    }

    /// Get the file path for a session by ID (public wrapper for find_session_file).
    /// Returns None if the session doesn't exist on disk.
    pub async fn get_session_file_path(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Option<PathBuf> {
        self.find_session_file_anywhere(user_id, session_id).await
    }

    /// Resume a specific Pi session by ID.
    ///
    /// If a Pi process is already running for this session, returns it.
    /// Otherwise, spawns a new Pi process for the session.
    /// The previous active session is kept alive (not killed) for idle cleanup later.
    pub async fn resume_session(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Arc<UserPiSession>> {
        let key = (user_id.to_string(), session_id.to_string());

        // Check if we already have a process for this session
        {
            let sessions = self.sessions.read().await;
            if let Some(existing) = sessions.get(&key) {
                info!("Reusing existing Pi process for session {}", session_id);
                // Update active session and return existing
                let mut active = self.active_session.write().await;
                active.insert(user_id.to_string(), session_id.to_string());
                return Ok(Arc::clone(existing));
            }
        }

        // Resolve the session file (any workspace) and work dir from header.
        let session_file = self
            .resolve_session_file_path(user_id, session_id)
            .await?;
        let header = self.read_session_header(user_id, &session_file).await?;
        let work_dir = Self::work_dir_from_header(&header)
            .unwrap_or_else(|| self.get_main_chat_dir(user_id));
        info!(
            "Resuming Pi session {} from {:?} (cwd={:?})",
            session_id, session_file, work_dir
        );

        // Spawn a new process for this session
        info!(
            "Spawning new Pi process for session {} (user {})",
            session_id, user_id
        );

        let session = self
            .create_session_with_resume(
                user_id,
                Some(session_id),
                Some(session_file),
                Some(work_dir),
            )
            .await?;
        let session = Arc::new(session);

        // Store in cache and set as active
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(key, Arc::clone(&session));
        }
        {
            let mut active = self.active_session.write().await;
            active.insert(user_id.to_string(), session_id.to_string());
        }

        Ok(session)
    }

    /// Create a Pi session, optionally resuming a specific session.
    async fn create_session_with_resume(
        &self,
        user_id: &str,
        resume_session_id: Option<&str>,
        resume_session_file: Option<PathBuf>,
        work_dir_override: Option<PathBuf>,
    ) -> Result<UserPiSession> {
        let work_dir = work_dir_override.unwrap_or_else(|| self.get_main_chat_dir(user_id));

        if !work_dir.exists() {
            anyhow::bail!("Main Chat directory does not exist for user: {}", user_id);
        }

        info!(
            "Starting Pi session for user {} in {:?}, resume={:?}, provider={:?}, model={:?}, mode={}",
            user_id,
            work_dir,
            resume_session_id,
            self.config.default_provider,
            self.config.default_model,
            self.config.runtime_mode
        );

        // Build system prompt files
        let mut append_system_prompt = Vec::new();
        let bootstrap_file = work_dir.join("BOOTSTRAP.md");
        if bootstrap_file.exists() {
            append_system_prompt.push(bootstrap_file);
        }
        let onboard_file = work_dir.join("ONBOARD.md");
        if onboard_file.exists() {
            append_system_prompt.push(onboard_file);
        }
        let personality_file = work_dir.join("PERSONALITY.md");
        if personality_file.exists() {
            append_system_prompt.push(personality_file);
        }
        let user_file = work_dir.join("USER.md");
        if user_file.exists() {
            append_system_prompt.push(user_file);
        }

        // If resuming, don't inject context (session already has it).
        // If starting fresh via this path, skip injection here and rely on the normal create_session() flow.

        // Find session file if resuming
        let session_file = if resume_session_id.is_some() {
            if let Some(path) = resume_session_file {
                Some(path)
            } else {
                let session_id = resume_session_id.expect("resume session id");
                Some(self.resolve_session_file_path(user_id, session_id).await?)
            }
        } else {
            None
        };

        // Build spawn config
        let spawn_config = PiSpawnConfig {
            work_dir: work_dir.clone(),
            pi_executable: self.config.pi_executable.clone(),
            continue_session: false, // We use session_file instead
            session_file,
            provider: self.config.default_provider.clone(),
            model: self.config.default_model.clone(),
            append_system_prompt,
            extensions: self.config.extensions.clone(),
            env: std::collections::HashMap::new(),
            sandboxed: self.config.sandboxed,
        };

        // Create runtime and spawn
        let runtime = self.create_runtime_for_user(user_id);
        let process = runtime.spawn(spawn_config).await?;

        // Session ID comes from the resume parameter, or will be fetched from Pi state later
        let session_id = resume_session_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("pending-{}", uuid::Uuid::new_v4()));

        Ok(UserPiSession {
            process: Arc::new(tokio::sync::RwLock::new(process)),
            stream_snapshot: Arc::new(Mutex::new(StreamSnapshot::default())),
            _session_id: session_id,
            last_activity: Arc::new(RwLock::new(std::time::Instant::now())),
            is_streaming: Arc::new(RwLock::new(false)),
            persistence_writer_claimed: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Check if a session should be continued or if we need a fresh start.
    fn should_continue_session(&self, last_session: &LastSessionInfo) -> bool {
        let now = SystemTime::now();

        // Check age
        let age = now
            .duration_since(last_session.modified)
            .unwrap_or(Duration::MAX);
        let max_age = Duration::from_secs(self.config.max_session_age_hours * 3600);

        if age > max_age {
            info!(
                "Session too old ({:?} > {:?}), starting fresh",
                age, max_age
            );
            return false;
        }

        // Check size
        if last_session.size > self.config.max_session_size_bytes {
            info!(
                "Session file too large ({} > {} bytes), starting fresh",
                last_session.size, self.config.max_session_size_bytes
            );
            return false;
        }

        info!(
            "Session is fresh (age={:?}, size={}), continuing",
            age, last_session.size
        );
        true
    }

    /// Get or create a Pi session for a user.
    /// Returns the currently active session, or creates a new one if none exists.
    pub async fn get_or_create_session(&self, user_id: &str) -> Result<Arc<UserPiSession>> {
        // Check if we have an active session for this user
        let active_session_id = {
            let active = self.active_session.read().await;
            active.get(user_id).cloned()
        };

        if let Some(session_id) = active_session_id {
            let key = (user_id.to_string(), session_id.clone());
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(&key) {
                return Ok(Arc::clone(session));
            }
            // Active session no longer exists (cleaned up?), clear it
            drop(sessions);
            let mut active = self.active_session.write().await;
            active.remove(user_id);
        }

        // No active session - create a new one
        let session = self.create_session(user_id, false).await?;
        let session = Arc::new(session);

        // Get the session ID from the Pi process state
        let session_id = session
            .get_state()
            .await
            .ok()
            .and_then(|s| s.session_id)
            .unwrap_or_else(|| format!("unknown-{}", uuid::Uuid::new_v4()));

        if let Err(err) = self
            .ensure_session_file(user_id, &self.get_main_chat_dir(user_id), &session_id)
            .await
        {
            warn!(
                "Failed to create Pi session file for {}: {}",
                session_id, err
            );
        }

        let key = (user_id.to_string(), session_id.clone());

        // Store in cache and set as active
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(key, Arc::clone(&session));
        }
        {
            let mut active = self.active_session.write().await;
            active.insert(user_id.to_string(), session_id.clone());
        }

        if let Err(err) = session.set_auto_retry(true).await {
            warn!(
                "Failed to enable auto-retry for session {}: {}",
                session_id, err
            );
        }

        Ok(session)
    }

    /// Create a new Pi session for a user.
    ///
    /// # Arguments
    /// * `user_id` - The user ID
    /// * `force_fresh` - If true, always start a fresh session regardless of staleness
    async fn create_session(&self, user_id: &str, force_fresh: bool) -> Result<UserPiSession> {
        let work_dir = self.get_main_chat_dir(user_id);

        // Ensure the directory exists
        if !work_dir.exists() {
            anyhow::bail!("Main Chat directory does not exist for user: {}", user_id);
        }

        // Determine if we should continue or start fresh
        let last_session = self.find_last_session(user_id, &work_dir).await?;
        let should_continue = !force_fresh
            && last_session
                .as_ref()
                .map(|s| self.should_continue_session(s))
                .unwrap_or(false);

        info!(
            "Starting Pi session for user {} in {:?}, continue={}, provider={:?}, model={:?}, mode={}",
            user_id,
            work_dir,
            should_continue,
            self.config.default_provider,
            self.config.default_model,
            self.config.runtime_mode
        );

        // Build system prompt files
        let mut append_system_prompt = Vec::new();
        let onboard_file = work_dir.join("ONBOARD.md");
        if onboard_file.exists() {
            append_system_prompt.push(onboard_file);
        }
        let personality_file = work_dir.join("PERSONALITY.md");
        if personality_file.exists() {
            append_system_prompt.push(personality_file);
        }
        let user_file = work_dir.join("USER.md");
        if user_file.exists() {
            append_system_prompt.push(user_file);
        }

        // On a fresh start, inject the most recent persisted summary/handoff so Pi has context.
        if !should_continue
            && let Ok(entries) = self
                .main_chat
                .get_recent_history_filtered(user_id, &["summary", "handoff", "decision"], 20)
                .await
        {
            let mut injected = String::new();
            for entry in entries.into_iter().rev() {
                injected.push_str(&format!(
                    "## {}\n{}\n\n",
                    entry.entry_type.to_uppercase(),
                    entry.content.trim()
                ));
            }

            if !injected.trim().is_empty() {
                let inject_path = work_dir.join("CONTEXT_INJECT.md");
                if let Err(e) = std::fs::write(&inject_path, injected) {
                    debug!("Failed to write CONTEXT_INJECT.md: {}", e);
                } else {
                    append_system_prompt.push(inject_path);
                }
            }
        }

        // Build environment for container mode
        let mut env = HashMap::new();
        if let Some(ref bridge_url) = self.config.bridge_url {
            env.insert("PI_BRIDGE_URL".to_string(), bridge_url.clone());
        }

        // Build spawn config
        let spawn_config = PiSpawnConfig {
            work_dir: work_dir.clone(),
            pi_executable: self.config.pi_executable.clone(),
            continue_session: should_continue,
            session_file: None,
            provider: self.config.default_provider.clone(),
            model: self.config.default_model.clone(),
            extensions: self.config.extensions.clone(),
            append_system_prompt,
            env,
            sandboxed: self.config.sandboxed,
        };

        // Get the appropriate runtime for this user
        let runtime = self.create_runtime_for_user(user_id);

        // Spawn the process
        let process = runtime.spawn(spawn_config).await.with_context(|| {
            format!(
                "Failed to spawn Pi process for user {} in {:?}",
                user_id, work_dir
            )
        })?;

        Ok(UserPiSession::from_process(process))
    }

    /// Close a specific session for a user.
    /// If `force` is false, will not close a streaming session.
    pub async fn close_session(
        &self,
        user_id: &str,
        session_id: &str,
        force: bool,
    ) -> Result<bool> {
        let key = (user_id.to_string(), session_id.to_string());

        // Check if session is streaming
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(&key) {
                let is_streaming = *session.is_streaming.read().await;
                if is_streaming && !force {
                    info!(
                        "Cannot close session {} for user {} - still streaming (use force=true)",
                        session_id, user_id
                    );
                    return Ok(false);
                }
            }
        }

        let mut sessions = self.sessions.write().await;
        if sessions.remove(&key).is_some() {
            info!("Closed Pi session {} for user {}", session_id, user_id);
            // Clear from active if this was the active session
            let mut active = self.active_session.write().await;
            if active.get(user_id) == Some(&session_id.to_string()) {
                active.remove(user_id);
            }
        }
        Ok(true)
    }

    /// Close all sessions for a user.
    /// If `force` is false, will not close streaming sessions.
    pub async fn close_all_sessions(&self, user_id: &str, force: bool) -> Result<()> {
        let keys_to_remove: Vec<_> = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .filter(|((uid, _), _session)| {
                    if uid != user_id {
                        return false;
                    }
                    if force {
                        return true;
                    }
                    // Only include non-streaming sessions
                    // Note: This is a sync check, but is_streaming is behind RwLock
                    // For simplicity, we'll collect all and check in the removal loop
                    true
                })
                .map(|(k, _)| k.clone())
                .collect()
        };

        let mut sessions = self.sessions.write().await;
        for key in keys_to_remove {
            if !force && let Some(session) = sessions.get(&key) {
                let is_streaming = *session.is_streaming.read().await;
                if is_streaming {
                    continue; // Skip streaming sessions
                }
            }
            sessions.remove(&key);
        }

        // Clear active session
        let mut active = self.active_session.write().await;
        active.remove(user_id);

        info!("Closed all sessions for user {}", user_id);
        Ok(())
    }

    /// Check if a session ID is currently active for a user.
    pub async fn is_active_session(&self, user_id: &str, session_id: &str) -> bool {
        let active = self.active_session.read().await;
        active
            .get(user_id)
            .map(|active_id| active_id == session_id)
            .unwrap_or(false)
    }

    /// Get the active session for a user (without creating).
    pub async fn get_session(&self, user_id: &str) -> Option<Arc<UserPiSession>> {
        let active_session_id = {
            let active = self.active_session.read().await;
            active.get(user_id).cloned()
        }?;

        let key = (user_id.to_string(), active_session_id);
        let sessions = self.sessions.read().await;
        sessions.get(&key).cloned()
    }

    /// Check if a user has any active session.
    pub async fn has_session(&self, user_id: &str) -> bool {
        let active = self.active_session.read().await;
        active.contains_key(user_id)
    }

    /// Reset a user's Pi session - closes the current active session and creates a fresh one.
    /// This re-reads PERSONALITY.md and USER.md files.
    /// If force is false, will not close a streaming session.
    pub async fn reset_session(&self, user_id: &str, force: bool) -> Result<Arc<UserPiSession>> {
        // Get current active session ID
        let active_session_id = {
            let active = self.active_session.read().await;
            active.get(user_id).cloned()
        };

        // Close existing active session if any
        if let Some(session_id) = active_session_id {
            let closed = self.close_session(user_id, &session_id, force).await?;
            if !closed {
                anyhow::bail!("Cannot reset: active session is still streaming");
            }
        }

        // Create a fresh session (force_fresh=true ensures no --continue flag)
        let session = self.create_session(user_id, true).await?;
        let session = Arc::new(session);

        // Get the session ID from the Pi process state
        let session_id = session
            .get_state()
            .await
            .ok()
            .and_then(|s| s.session_id)
            .unwrap_or_else(|| format!("unknown-{}", uuid::Uuid::new_v4()));

        if let Err(err) = self
            .ensure_session_file(user_id, &self.get_main_chat_dir(user_id), &session_id)
            .await
        {
            warn!(
                "Failed to create Pi session file for {}: {}",
                session_id, err
            );
        }

        let key = (user_id.to_string(), session_id.clone());

        // Store in cache and set as active
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(key, Arc::clone(&session));
        }
        {
            let mut active = self.active_session.write().await;
            active.insert(user_id.to_string(), session_id);
        }

        info!("Reset Pi session for user {}", user_id);
        Ok(session)
    }
}

impl UserPiSession {
    pub(crate) fn from_process(process: Box<dyn PiProcess>) -> Self {
        let stream_snapshot = Arc::new(Mutex::new(StreamSnapshot::default()));
        let stream_snapshot_task = Arc::clone(&stream_snapshot);
        let is_streaming = Arc::new(RwLock::new(false));
        let is_streaming_task = Arc::clone(&is_streaming);
        let last_activity = Arc::new(RwLock::new(std::time::Instant::now()));
        let last_activity_task = Arc::clone(&last_activity);

        let mut event_rx = process.subscribe();
        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        *last_activity_task.write().await = std::time::Instant::now();

                        match &event {
                            PiEvent::AgentStart | PiEvent::MessageStart { .. } => {
                                *is_streaming_task.write().await = true;
                            }
                            PiEvent::AgentEnd { .. } => {
                                *is_streaming_task.write().await = false;
                            }
                            _ => {}
                        }

                        let mut snapshot = stream_snapshot_task.lock().await;
                        snapshot.apply_event(&event);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let mut snapshot = stream_snapshot_task.lock().await;
                        snapshot.reset();
                    }
                }
            }
        });

        let session_id = format!("pending-{}", uuid::Uuid::new_v4());

        UserPiSession {
            process: Arc::new(tokio::sync::RwLock::new(process)),
            stream_snapshot,
            _session_id: session_id,
            last_activity,
            is_streaming,
            persistence_writer_claimed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn is_streaming(&self) -> bool {
        *self.is_streaming.read().await
    }

    pub async fn last_activity_elapsed(&self) -> std::time::Duration {
        std::time::Instant::now() - *self.last_activity.read().await
    }

    // session_id is currently stored for future session switching features.

    /// Claim exclusive persistence for this session (used to prevent duplicate WS saves).
    pub fn claim_persistence_writer(&self) -> Option<PersistenceWriterGuard> {
        if self
            .persistence_writer_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            Some(PersistenceWriterGuard {
                claimed: Arc::clone(&self.persistence_writer_claimed),
            })
        } else {
            None
        }
    }

    /// Send a prompt to the agent.
    pub async fn prompt(&self, message: &str) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::Prompt {
                id: None,
                message: message.to_string(),
                images: None,
                streaming_behavior: None,
            })
            .await?;
        Ok(())
    }

    /// Abort the current operation.
    pub async fn abort(&self) -> Result<()> {
        let process = self.process.read().await;
        process.send_command(PiCommand::Abort { id: None }).await?;
        Ok(())
    }

    /// Queue a steering message to interrupt the agent mid-run.
    pub async fn steer(&self, message: &str) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::Steer {
                id: None,
                message: message.to_string(),
            })
            .await?;
        Ok(())
    }

    /// Queue a follow-up message for after the agent finishes.
    pub async fn follow_up(&self, message: &str) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::FollowUp {
                id: None,
                message: message.to_string(),
            })
            .await?;
        Ok(())
    }

    /// Get current state.
    pub async fn get_state(&self) -> Result<PiState> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::GetState { id: None })
            .await?;
        if !response.success {
            anyhow::bail!("get_state failed: {:?}", response.error);
        }
        let data = response.data.context("get_state returned no data")?;
        serde_json::from_value(data).context("failed to parse state")
    }

    /// Get current session_id, falling back to the cached id when Pi state omits it.
    pub async fn get_session_id(&self) -> Option<String> {
        self.get_state()
            .await
            .ok()
            .and_then(|s| s.session_id)
            .or_else(|| Some(self._session_id.clone()))
    }

    /// Get all messages.
    pub async fn get_messages(&self) -> Result<Vec<AgentMessage>> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::GetMessages { id: None })
            .await?;
        if !response.success {
            anyhow::bail!("get_messages failed: {:?}", response.error);
        }
        let data = response.data.context("get_messages returned no data")?;
        let messages_data = data
            .get("messages")
            .context("no messages field in response")?;
        serde_json::from_value(messages_data.clone()).context("failed to parse messages")
    }

    /// Subscribe to events.
    ///
    /// Note: This requires awaiting to get the process lock, then returns
    /// the receiver synchronously.
    pub async fn subscribe(&self) -> broadcast::Receiver<PiEvent> {
        let process = self.process.read().await;
        process.subscribe()
    }

    /// Get the current streaming snapshot as WS events (for replay on reconnect).
    pub async fn stream_snapshot_events(&self) -> Vec<Value> {
        let snapshot = self.stream_snapshot.lock().await;
        snapshot.to_ws_events()
    }

    /// Compact the session context.
    pub async fn compact(&self, custom_instructions: Option<&str>) -> Result<CompactionResult> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::Compact {
                id: None,
                custom_instructions: custom_instructions.map(|s| s.to_string()),
            })
            .await?;
        if !response.success {
            anyhow::bail!("compact failed: {:?}", response.error);
        }
        let data = response.data.context("compact returned no data")?;
        serde_json::from_value(data).context("failed to parse compaction result")
    }

    /// Start a new session (clear history).
    pub async fn new_session(&self) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::NewSession {
                id: None,
                parent_session: None,
            })
            .await?;
        Ok(())
    }

    /// Set the current model.
    pub async fn set_model(&self, provider: &str, model_id: &str) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::SetModel {
                id: None,
                provider: provider.to_string(),
                model_id: model_id.to_string(),
            })
            .await?;
        Ok(())
    }

    /// Enable or disable auto-retry.
    pub async fn set_auto_retry(&self, enabled: bool) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::SetAutoRetry {
                id: None,
                enabled,
            })
            .await?;
        Ok(())
    }

    /// Get session statistics.
    pub async fn get_session_stats(&self) -> Result<SessionStats> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::GetSessionStats { id: None })
            .await?;
        if !response.success {
            anyhow::bail!("get_session_stats failed: {:?}", response.error);
        }
        let data = response
            .data
            .context("get_session_stats returned no data")?;
        serde_json::from_value(data).context("failed to parse session stats")
    }

    /// Get available models.
    pub async fn get_available_models(&self) -> Result<Vec<crate::pi::PiModel>> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::GetAvailableModels { id: None })
            .await?;
        if !response.success {
            anyhow::bail!("get_available_models failed: {:?}", response.error);
        }
        let data = response
            .data
            .context("get_available_models returned no data")?;
        let models = data.get("models").context("no models field in response")?;
        serde_json::from_value(models.clone()).context("failed to parse models")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn assistant_message() -> AgentMessage {
        AgentMessage {
            role: "assistant".to_string(),
            content: Value::Null,
            timestamp: None,
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn test_stream_snapshot_replay_events() {
        let mut snapshot = StreamSnapshot::default();

        snapshot.apply_event(&PiEvent::AgentStart);
        snapshot.apply_event(&PiEvent::MessageStart {
            message: assistant_message(),
        });
        snapshot.apply_event(&PiEvent::MessageUpdate {
            message: assistant_message(),
            assistant_message_event: AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "Hello".to_string(),
                partial: Value::Null,
            },
        });
        snapshot.apply_event(&PiEvent::MessageUpdate {
            message: assistant_message(),
            assistant_message_event: AssistantMessageEvent::ThinkingDelta {
                content_index: 0,
                delta: "Hmm".to_string(),
                partial: Value::Null,
            },
        });
        snapshot.apply_event(&PiEvent::MessageUpdate {
            message: assistant_message(),
            assistant_message_event: AssistantMessageEvent::ToolcallEnd {
                content_index: 0,
                tool_call: crate::pi::ToolCall {
                    id: "tool-1".to_string(),
                    name: "echo".to_string(),
                    arguments: json!({"value": 1}),
                },
                partial: Value::Null,
            },
        });
        snapshot.apply_event(&PiEvent::ToolExecutionEnd {
            tool_call_id: "tool-1".to_string(),
            tool_name: "echo".to_string(),
            result: crate::pi::ToolResult {
                content: vec![crate::pi::ContentBlock::Text {
                    text: "ok".to_string(),
                }],
                details: None,
            },
            is_error: false,
        });

        let events = snapshot.to_ws_events();
        assert_eq!(events.len(), 5);
        assert_eq!(events[0]["type"], "message_start");
        assert_eq!(events[1]["type"], "text");
        assert_eq!(events[2]["type"], "thinking");
        assert_eq!(events[3]["type"], "tool_use");
        assert_eq!(events[4]["type"], "tool_result");
    }

    #[test]
    fn test_pi_sessions_dir_escaping() {
        let main_chat = Arc::new(crate::main_chat::MainChatService::new(
            PathBuf::from("/tmp/test"),
            true,
        ));
        let service = MainChatPiService::new(
            PathBuf::from("/tmp/test"),
            true,
            MainChatPiServiceConfig::default(),
            main_chat,
            None,
        );

        let work_dir = PathBuf::from("/home/user/.local/share/octo/users/main");
        let sessions_dir = service.get_pi_sessions_dir("user", &work_dir);

        // Should escape slashes and wrap with double-dashes
        assert!(
            sessions_dir
                .to_string_lossy()
                .contains("--home-user-.local-share-octo-users-main--")
        );
    }

    #[test]
    fn test_session_freshness_by_age() {
        let main_chat = Arc::new(crate::main_chat::MainChatService::new(
            PathBuf::from("/tmp/test"),
            true,
        ));
        let service = MainChatPiService::new(
            PathBuf::from("/tmp/test"),
            true,
            MainChatPiServiceConfig {
                max_session_age_hours: 1, // 1 hour for testing
                ..Default::default()
            },
            main_chat,
            None,
        );

        // Fresh session (now)
        let fresh = LastSessionInfo {
            size: 1000,
            modified: SystemTime::now(),
        };
        assert!(service.should_continue_session(&fresh));

        // Stale session (2 hours ago)
        let stale = LastSessionInfo {
            size: 1000,
            modified: SystemTime::now() - Duration::from_secs(2 * 3600),
        };
        assert!(!service.should_continue_session(&stale));
    }

    #[test]
    fn test_session_freshness_by_size() {
        let main_chat = Arc::new(crate::main_chat::MainChatService::new(
            PathBuf::from("/tmp/test"),
            true,
        ));
        let service = MainChatPiService::new(
            PathBuf::from("/tmp/test"),
            true,
            MainChatPiServiceConfig {
                max_session_size_bytes: 1000, // 1KB for testing
                ..Default::default()
            },
            main_chat,
            None,
        );

        // Small session
        let small = LastSessionInfo {
            size: 500,
            modified: SystemTime::now(),
        };
        assert!(service.should_continue_session(&small));

        // Large session
        let large = LastSessionInfo {
            size: 2000,
            modified: SystemTime::now(),
        };
        assert!(!service.should_continue_session(&large));
    }

    #[tokio::test]
    #[ignore] // Requires pi to be installed
    async fn test_pi_service_creation() {
        let temp = TempDir::new().unwrap();
        let main_dir = temp.path().join("main");
        std::fs::create_dir_all(&main_dir).unwrap();

        // Create minimal pi settings
        let pi_dir = main_dir.join(".pi");
        std::fs::create_dir_all(&pi_dir).unwrap();
        std::fs::write(
            pi_dir.join("settings.json"),
            r#"{"defaultProvider": "openai", "defaultModel": "gpt-4o-mini"}"#,
        )
        .unwrap();

        let main_chat = Arc::new(crate::main_chat::MainChatService::new(
            temp.path().to_path_buf(),
            true,
        ));
        let service = MainChatPiService::new(
            temp.path().to_path_buf(),
            true,
            MainChatPiServiceConfig::default(),
            main_chat,
            None,
        );

        // This would fail without pi installed
        let session = service.get_or_create_session("test").await;
        assert!(session.is_ok() || session.is_err()); // Just check it doesn't panic
    }
}

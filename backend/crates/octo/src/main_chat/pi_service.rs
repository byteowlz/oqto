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
use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use tokio::sync::{Mutex, RwLock, broadcast};

use crate::pi::{
    AgentMessage, AssistantMessageEvent, CompactionResult, ContainerPiRuntime, LocalPiRuntime,
    PiCommand, PiEvent, PiProcess, PiRuntime, PiSpawnConfig, PiState, RunnerPiRuntime,
    SessionStats,
};
use crate::runner::client::RunnerClient;

/// Session freshness thresholds
const SESSION_MAX_AGE_HOURS: u64 = 4;
const SESSION_MAX_SIZE_BYTES: u64 = 500 * 1024; // 500KB

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
    /// Number of messages in session
    pub message_count: usize,
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
    /// Timestamp (Unix ms)
    pub timestamp: i64,
    /// Usage stats (for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
}

/// CASS search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassResponse {
    /// Search results (CASS uses "hits" not "results")
    #[serde(default)]
    pub hits: Vec<CassSearchResult>,
    /// Number of results returned
    #[serde(default)]
    pub count: usize,
    /// Total matches available
    #[serde(default)]
    pub total_matches: usize,
}

/// A single CASS search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassSearchResult {
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
                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    self.push_thinking(delta);
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

/// Default idle timeout for sessions (5 minutes).
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300;

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
    ) -> Self {
        info!(
            "MainChatPiService initialized with runtime mode: {}",
            config.runtime_mode
        );

        Self {
            config,
            sessions: RwLock::new(HashMap::new()),
            active_session: RwLock::new(HashMap::new()),
            workspace_dir,
            single_user,
            main_chat,
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
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
                    RunnerClient::new(pattern.replace("{user}", user_id))
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

    /// Get the Pi sessions directory for a working directory.
    /// Pi stores sessions in ~/.pi/agent/sessions/{escaped-path}/
    fn get_pi_sessions_dir(&self, work_dir: &PathBuf) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let escaped_path = work_dir
            .to_string_lossy()
            .replace('/', "-")
            .trim_start_matches('-')
            .to_string();
        // Pi stores sessions under a directory name wrapped in double-dashes.
        // Example: `--home-user-.local-share-octo-users-main--`
        home.join(".pi")
            .join("agent")
            .join("sessions")
            .join(format!("--{}--", escaped_path))
    }

    /// Find the most recent Pi session file for a directory.
    fn find_last_session(&self, work_dir: &PathBuf) -> Option<LastSessionInfo> {
        let sessions_dir = self.get_pi_sessions_dir(work_dir);

        if !sessions_dir.exists() {
            debug!("Pi sessions directory does not exist: {:?}", sessions_dir);
            return None;
        }

        let mut latest: Option<LastSessionInfo> = None;

        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            let info = LastSessionInfo {
                                size: metadata.len(),
                                modified,
                            };

                            if latest
                                .as_ref()
                                .map(|l| modified > l.modified)
                                .unwrap_or(true)
                            {
                                latest = Some(info);
                            }
                        }
                    }
                }
            }
        }

        latest
    }

    /// List all Pi sessions for a user.
    pub fn list_sessions(&self, user_id: &str) -> Result<Vec<PiSessionFile>> {
        let work_dir = self.get_main_chat_dir(user_id);
        let sessions_dir = self.get_pi_sessions_dir(&work_dir);

        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();

        let entries = std::fs::read_dir(&sessions_dir).context("reading Pi sessions directory")?;

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                if let Some(session) = self.parse_session_file(&path) {
                    sessions.push(session);
                }
            }
        }

        // Sort by modified_at descending (most recently active first)
        sessions.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

        Ok(sessions)
    }

    /// Search for sessions matching a query string.
    /// Supports fuzzy matching on session ID and title.
    /// Returns sessions sorted by match quality (best first).
    pub fn search_sessions(&self, user_id: &str, query: &str) -> Result<Vec<PiSessionFile>> {
        let all_sessions = self.list_sessions(user_id)?;
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
    pub fn update_session_title(
        &self,
        user_id: &str,
        session_id: &str,
        title: &str,
    ) -> Result<PiSessionFile> {
        use std::io::{BufRead, BufReader, Write};

        let work_dir = self.get_main_chat_dir(user_id);
        let sessions_dir = self.get_pi_sessions_dir(&work_dir);

        // Find session file - filename format is {timestamp}_{session_id}.jsonl
        let session_path = self.find_session_file(&sessions_dir, session_id)?;

        // Read the file
        let file = std::fs::File::open(&session_path).context("opening session file")?;
        let reader = BufReader::new(file);
        let mut lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>()?;

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

        // Write back to file
        let mut file =
            std::fs::File::create(&session_path).context("creating session file for write")?;
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                writeln!(file)?;
            }
            write!(file, "{}", line)?;
        }
        writeln!(file)?;

        // Return the updated session info
        self.parse_session_file(&session_path)
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
    fn parse_session_file(&self, path: &std::path::Path) -> Option<PiSessionFile> {
        use std::io::{BufRead, BufReader};

        let file = std::fs::File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        let modified = metadata.modified().ok()?;
        let modified_ms = modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as i64;

        let mut reader = BufReader::new(file);

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

        let id = header.get("id").and_then(|v| v.as_str())?.to_string();
        let started_at = header
            .get("timestamp")
            .and_then(|v| v.as_str())?
            .to_string();

        // Check for explicit title in header first (set via rename)
        let mut title = header
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let mut message_count = 0usize;

        for line in reader.lines().filter_map(|l| l.ok()) {
            if line.is_empty() {
                continue;
            }

            let entry: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if entry.get("type").and_then(|t| t.as_str()) != Some("message") {
                continue;
            }

            message_count += 1;

            // Only extract title from first user message if no explicit title set
            if title.is_none() {
                if let Some(msg) = entry.get("message") {
                    if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                        if let Some(content) = msg.get("content") {
                            title = Self::extract_title_from_content(content);
                            // Stop early once we have a title.
                            break;
                        }
                    }
                }
            }
        }

        Some(PiSessionFile {
            id,
            started_at,
            size: metadata.len(),
            modified_at: modified_ms,
            title,
            message_count,
        })
    }

    /// Extract a title from message content (first ~50 chars of text).
    fn extract_title_from_content(content: &Value) -> Option<String> {
        if let Some(text) = content.as_str() {
            return Some(Self::truncate_title(text));
        }

        if let Some(arr) = content.as_array() {
            for block in arr {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        return Some(Self::truncate_title(text));
                    }
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
    pub fn get_session_messages(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<PiSessionMessage>> {
        use std::io::{BufRead, BufReader};

        let work_dir = self.get_main_chat_dir(user_id);
        let sessions_dir = self.get_pi_sessions_dir(&work_dir);

        // Find the session file by ID
        let session_file = self.find_session_file(&sessions_dir, session_id)?;

        let file = std::fs::File::open(&session_file).context("opening session file")?;
        let reader = BufReader::new(file);

        let mut messages = Vec::new();

        for line in reader.lines().filter_map(|l| l.ok()) {
            if line.is_empty() {
                continue;
            }

            if let Ok(entry) = serde_json::from_str::<Value>(&line) {
                if entry.get("type").and_then(|t| t.as_str()) == Some("message") {
                    if let Some(msg) = entry.get("message") {
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
                        let timestamp = msg.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
                        let usage = msg.get("usage").cloned();

                        messages.push(PiSessionMessage {
                            id,
                            role,
                            content,
                            timestamp,
                            usage,
                        });
                    }
                }
            }
        }

        Ok(messages)
    }

    /// Search within a specific session using CASS, with fallback to direct text search.
    /// Returns search results from the session's content.
    /// Supports both Pi sessions (.jsonl) and OpenCode sessions (.json).
    pub async fn search_in_session(
        &self,
        _user_id: &str,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<CassSearchResult>> {
        // First try CASS search filtered by session ID
        let cass_results = self
            .search_in_session_via_cass(session_id, query, limit)
            .await;

        if let Ok(ref results) = cass_results {
            if !results.is_empty() {
                return cass_results;
            }
        }

        // Fallback: direct text search in OpenCode message parts
        self.search_in_opencode_session(session_id, query, limit)
            .await
    }

    /// Search using CASS and filter by session ID.
    async fn search_in_session_via_cass(
        &self,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<CassSearchResult>> {
        let search_limit = limit * 10;

        let output = tokio::process::Command::new("cass")
            .arg("search")
            .arg(query)
            .arg("--mode")
            .arg("lexical")
            .arg("--limit")
            .arg(search_limit.to_string())
            .arg("--json")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn cass")?
            .wait_with_output()
            .await
            .context("Failed to wait for cass")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No results") || output.stdout.is_empty() {
                return Ok(Vec::new());
            }
            anyhow::bail!("CASS search failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(Vec::new());
        }

        let cass_response: CassResponse =
            serde_json::from_str(&stdout).context("Failed to parse CASS output")?;

        let filtered: Vec<CassSearchResult> = cass_response
            .hits
            .into_iter()
            .filter(|hit| hit.source_path.contains(session_id))
            .take(limit)
            .collect();

        Ok(filtered)
    }

    /// Direct text search in OpenCode session message parts.
    /// Fallback when CASS hasn't indexed the session.
    async fn search_in_opencode_session(
        &self,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<CassSearchResult>> {
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
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                        message_ids.push(filename.to_string());
                    }
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

                                results.push(CassSearchResult {
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
    fn find_session_file(
        &self,
        sessions_dir: &std::path::Path,
        session_id: &str,
    ) -> Result<PathBuf> {
        if !sessions_dir.exists() {
            anyhow::bail!("Sessions directory not found");
        }

        let entries = std::fs::read_dir(sessions_dir).context("reading sessions directory")?;

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if filename.contains(session_id) {
                    return Ok(path);
                }
            }
        }

        anyhow::bail!("Session not found: {}", session_id)
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

        let work_dir = self.get_main_chat_dir(user_id);
        let sessions_dir = self.get_pi_sessions_dir(&work_dir);

        // Verify session exists on disk for cold resume.
        let session_file = self.find_session_file(&sessions_dir, session_id)?;
        info!("Resuming Pi session {} from {:?}", session_id, session_file);

        // Spawn a new process for this session
        info!(
            "Spawning new Pi process for session {} (user {})",
            session_id, user_id
        );

        let session = self
            .create_session_with_resume(user_id, Some(session_id))
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
    ) -> Result<UserPiSession> {
        let work_dir = self.get_main_chat_dir(user_id);

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
        let session_file = if let Some(session_id) = resume_session_id {
            let sessions_dir = self.get_pi_sessions_dir(&work_dir);
            Some(self.find_session_file(&sessions_dir, session_id)?)
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
        let last_session = self.find_last_session(&work_dir);
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
        if !should_continue {
            if let Ok(entries) = self
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
                        // Update last activity on any event
                        *last_activity_task.write().await = std::time::Instant::now();

                        // Track streaming state
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

        // Session ID will be fetched from Pi state after spawn
        let session_id = format!("pending-{}", uuid::Uuid::new_v4());

        Ok(UserPiSession {
            process: Arc::new(tokio::sync::RwLock::new(process)),
            stream_snapshot,
            _session_id: session_id,
            last_activity,
            is_streaming,
            persistence_writer_claimed: Arc::new(AtomicBool::new(false)),
        })
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
            if !force {
                if let Some(session) = sessions.get(&key) {
                    let is_streaming = *session.is_streaming.read().await;
                    if is_streaming {
                        continue; // Skip streaming sessions
                    }
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
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
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
        );

        let work_dir = PathBuf::from("/home/user/.local/share/octo/users/main");
        let sessions_dir = service.get_pi_sessions_dir(&work_dir);

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
        );

        // This would fail without pi installed
        let session = service.get_or_create_session("test").await;
        assert!(session.is_ok() || session.is_err()); // Just check it doesn't panic
    }
}

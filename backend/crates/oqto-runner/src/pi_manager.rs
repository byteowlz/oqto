//! Pi Session Manager for oqto-runner.
//!
//! Manages multiple Pi agent processes with:
//! - Process lifecycle (spawn, shutdown)
//! - Command routing (prompt, steer, follow_up, abort, compact)
//! - Event broadcasting to subscribers
//! - State tracking (Starting, Idle, Streaming, Compacting, Stopping)
//! - Idle session cleanup
//! - Persistence to oqto-log on AgentEnd

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, OwnedSemaphorePermit, RwLock, Semaphore, broadcast, mpsc, oneshot};

use crate::agent_browser::{agent_browser_session_dir, browser_session_name};
use crate::pi_translator::PiTranslator;
use crate::protocol::{ChatMessageProto, PiSessionInfo, PiSessionState, agent_msg_to_chat_proto};
use oqto_pi::{AgentMessage, PiCommand, PiEvent, PiMessage, PiResponse, PiState, SessionStats};
use oqto_protocol::events::{AgentPhase, Event as CanonicalEvent, EventPayload};
use oqto_sandbox::{SandboxConfig, configure_bwrap_pre_exec};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Pi session manager.
#[derive(Debug, Clone)]
pub struct PiManagerConfig {
    /// Path to the Pi binary.
    pub pi_binary: PathBuf,
    /// Default working directory for sessions.
    pub default_cwd: PathBuf,
    /// Idle timeout before session cleanup (seconds).
    pub idle_timeout_secs: u64,
    /// Cleanup check interval (seconds).
    pub cleanup_interval_secs: u64,
    /// Sandbox configuration (if sandboxing is enabled).
    pub sandbox_config: Option<SandboxConfig>,
    /// Runner identifier (human-readable).
    pub runner_id: String,
    /// Directory for persisting the model cache across restarts.
    /// Each workdir gets its own JSON file: `<cache_dir>/models/<hash>.json`
    pub model_cache_dir: Option<PathBuf>,
}

impl Default for PiManagerConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let state_dir = std::env::var("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".local").join("state"));

        // Prefer /usr/local/bin/pi (wrapper that sets PI_PACKAGE_DIR and uses bun)
        // over ~/.bun/bin/pi (symlink with #!/usr/bin/env node shebang that fails
        // when node is not installed).
        let pi_binary = {
            let system_pi = PathBuf::from("/usr/local/bin/pi");
            if system_pi.exists() {
                system_pi
            } else {
                PathBuf::from(&home).join(".bun/bin/pi")
            }
        };

        Self {
            pi_binary,
            default_cwd: PathBuf::from(&home).join("projects"),
            idle_timeout_secs: 300, // 5 minutes
            cleanup_interval_secs: 60,
            sandbox_config: None,
            runner_id: "local".to_string(),
            model_cache_dir: Some(state_dir.join("oqto").join("model-cache")),
        }
    }
}

// ============================================================================
// Model Cache Persistence
// ============================================================================

/// On-disk format for a persisted model cache entry.
#[derive(Debug, Serialize, Deserialize)]
struct ModelCacheEntry {
    workdir: String,
    models: serde_json::Value,
    /// Unix timestamp (seconds) when this cache entry was written.
    /// Entries older than [`MODEL_CACHE_TTL`] are ignored on load.
    #[serde(default)]
    cached_at: u64,
}

/// How long a persisted model cache entry stays valid (1 hour).
/// After this, the entry is discarded and models are re-fetched from
/// models.json + ephemeral Pi so new provider models are picked up.
const MODEL_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

// ============================================================================
// Session Configuration (per-session)
// ============================================================================

/// Configuration for creating a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionConfig {
    /// Working directory for Pi.
    pub cwd: PathBuf,
    /// Provider (anthropic, openai, etc.).
    #[serde(default)]
    pub provider: Option<String>,
    /// Model ID.
    #[serde(default)]
    pub model: Option<String>,
    /// Explicit session file to use (new or resume).
    #[serde(default)]
    pub session_file: Option<PathBuf>,
    /// Session file to continue from.
    #[serde(default)]
    pub continue_session: Option<PathBuf>,
    /// Environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for PiSessionConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            cwd: PathBuf::from(home).join("projects"),
            provider: None,
            model: None,
            session_file: None,
            continue_session: None,
            env: HashMap::new(),
        }
    }
}

// ============================================================================
// Session State
// ============================================================================

// PiSessionState and PiSessionInfo are imported from crate::runner::protocol
// to avoid duplication.  The runner daemon (oqto-runner) and the client both
// use the protocol types directly.

// ============================================================================
// Fork Result
// ============================================================================

/// Result of a fork operation.
#[derive(Debug)]
pub struct ForkResult {
    /// The text of the message being forked from.
    pub text: String,
    /// Whether the fork was cancelled (e.g., by an extension).
    pub cancelled: bool,
    /// Pi's native session ID for the new forked session.
    pub new_session_id: Option<String>,
    /// Path to the new forked session's JSONL file.
    pub new_session_file: Option<String>,
}

// ============================================================================
// Internal Session Command
// ============================================================================

/// Commands sent to a session's command loop.
/// Commands sent to a Pi session's command loop.
///
/// Note: Commands that need responses (GetState, GetMessages, etc.) don't include
/// oneshot senders here. Instead, response coordination happens via the shared
/// `pending_responses` map - the caller registers a waiter before sending the command,
/// and the reader task routes the response back.
#[derive(Debug)]
pub enum PiSessionCommand {
    // ========================================================================
    // Prompting
    // ========================================================================
    /// Send a user prompt with optional client-generated ID for matching.
    Prompt {
        message: String,
        client_id: Option<String>,
    },
    /// Send a steering message (interrupt mid-run).
    Steer {
        message: String,
        client_id: Option<String>,
    },
    /// Send a follow-up message (queue for after completion).
    FollowUp {
        message: String,
        client_id: Option<String>,
    },
    /// Abort current operation.
    Abort,

    // ========================================================================
    // Session Management
    // ========================================================================
    /// Start a new session (optionally forking from parent).
    NewSession(Option<String>),
    /// Switch to a different session file.
    SwitchSession(String),
    /// Set session display name.
    SetSessionName(String),
    /// Export session to HTML.
    ExportHtml(Option<String>),

    // ========================================================================
    // State Queries (response via pending_responses)
    // ========================================================================
    /// Get current Pi state.
    GetState { request_id: String },
    /// Get all messages in the conversation.
    GetMessages,
    /// Get the last assistant message text.
    GetLastAssistantText,
    /// Get session statistics.
    GetSessionStats,
    /// Get available commands.
    GetCommands,

    // ========================================================================
    // Model Configuration
    // ========================================================================
    /// Set the model.
    SetModel { provider: String, model_id: String },
    /// Cycle to the next model.
    CycleModel,
    /// Get available models (response via pending_responses).
    GetAvailableModels,

    // ========================================================================
    // Thinking Configuration
    // ========================================================================
    /// Set thinking level.
    SetThinkingLevel(String),
    /// Cycle thinking level.
    CycleThinkingLevel,

    // ========================================================================
    // Queue Modes
    // ========================================================================
    /// Set steering message delivery mode.
    SetSteeringMode(String),
    /// Set follow-up message delivery mode.
    SetFollowUpMode(String),

    // ========================================================================
    // Compaction
    // ========================================================================
    /// Compact conversation history.
    Compact(Option<String>),
    /// Enable/disable auto-compaction.
    SetAutoCompaction(bool),

    // ========================================================================
    // Retry
    // ========================================================================
    /// Enable/disable auto-retry.
    SetAutoRetry(bool),
    /// Abort in-progress retry.
    AbortRetry,

    // ========================================================================
    // Forking (response via pending_responses)
    // ========================================================================
    /// Fork from a previous user message.
    Fork {
        entry_id: String,
        request_id: String,
    },
    /// Get messages available for forking.
    GetForkMessages { request_id: String },

    // ========================================================================
    // Bash Execution (response via pending_responses)
    // ========================================================================
    /// Execute a shell command.
    Bash(String),
    /// Abort running bash command.
    AbortBash,

    // ========================================================================
    // Extension UI
    // ========================================================================
    /// Respond to extension UI dialog.
    ExtensionUiResponse {
        id: String,
        value: Option<String>,
        confirmed: Option<bool>,
        cancelled: Option<bool>,
    },

    // ========================================================================
    // Lifecycle
    // ========================================================================
    /// Close the session.
    Close,
}

// ============================================================================
// Event Wrapper
// ============================================================================

/// Canonical event wrapper for the event distribution channel.
///
/// The pi_manager translates native Pi events into canonical events using
/// `PiTranslator` and distributes them. One native Pi event may produce
/// multiple canonical events, so each is distributed individually.
pub type PiEventWrapper = CanonicalEvent;

// ============================================================================
// Per-Subscriber Event Distribution
// ============================================================================

/// Thread-safe collection of per-subscriber unbounded channels.
///
/// Unlike `tokio::broadcast`, this guarantees **zero event loss**: each
/// subscriber gets its own unbounded `mpsc` channel. A slow subscriber
/// does not cause other subscribers to lose events. Dead subscribers
/// (closed channels) are pruned lazily on each `publish()` call.
#[derive(Clone)]
pub struct EventSubscribers {
    inner: Arc<RwLock<Vec<mpsc::UnboundedSender<PiEventWrapper>>>>,
}

impl EventSubscribers {
    /// Create a new empty subscriber set.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a new subscriber. Returns the receiving end of the channel.
    pub async fn subscribe(&self) -> mpsc::UnboundedReceiver<PiEventWrapper> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner.write().await.push(tx);
        rx
    }

    /// Publish an event to all subscribers. Dead subscribers are removed.
    pub async fn publish(&self, event: &PiEventWrapper) {
        let mut subs = self.inner.write().await;
        subs.retain(|tx| tx.send(event.clone()).is_ok());
    }

    /// Number of active subscribers.
    pub async fn subscriber_count(&self) -> usize {
        let subs = self.inner.read().await;
        subs.len()
    }

    /// Remove all subscribers (e.g., on session shutdown).
    pub async fn clear(&self) {
        self.inner.write().await.clear();
    }
}

impl Default for EventSubscribers {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Internal Session Structure
// ============================================================================

/// Pending response waiters - maps request ID to oneshot sender.
type PendingResponses = Arc<RwLock<HashMap<String, oneshot::Sender<PiResponse>>>>;

/// FIFO queue of pending client_ids for optimistic message matching.
///
/// Each outbound user command with a client_id pushes one entry.
/// It is retained as a fallback ledger for sessions where oqto-bridge queue
/// events are unavailable.
type PendingClientId = Arc<Mutex<VecDeque<String>>>;

/// The Pi external_id for a session.
///
/// Empty until Pi reports its native `sessionId` via `get_state`, then fixed
/// to that Pi ID for oqto-log source identity.
type SessionExternalId = Arc<RwLock<String>>;

#[derive(Debug, Deserialize)]
struct OqtoQueueEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(rename = "clientId")]
    client_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct StatsDelta {
    tokens_in: i64,
    tokens_out: i64,
    cache_read: i64,
    cache_write: i64,
    cost_usd: f64,
}

/// Thread-safe message buffer.
///
/// The single source of truth for messages in an active session. Populated
/// from AgentEnd messages and seeded from oqto-log on session resume.
/// `get_messages` for active sessions returns this buffer directly, bypassing
/// the Pi `get_messages` RPC entirely.
type MessageBuffer = Arc<RwLock<Vec<ChatMessageProto>>>;

/// Internal session state (held by the manager).
struct PiSession {
    /// Session ID (Oqto UUID -- the routing key used by frontend/API).
    id: String,
    /// Session configuration.
    #[allow(dead_code)]
    config: PiSessionConfig,
    /// Last known active provider from Pi `get_state`.
    active_provider: Arc<RwLock<Option<String>>>,
    /// Last known active model from Pi `get_state`.
    active_model: Arc<RwLock<Option<String>>>,
    /// Child process.
    process: Child,
    /// Current state.
    state: Arc<RwLock<PiSessionState>>,
    /// The Pi external_id for this session.
    /// Empty until Pi's native session ID is known via `get_state`.
    session_external_id: SessionExternalId,
    /// Last activity timestamp (shared with reader/command tasks).
    last_activity: Arc<RwLock<Instant>>,
    /// Per-subscriber event distribution (replaces broadcast to prevent event loss).
    subscribers: EventSubscribers,
    /// Command sender to the session task.
    cmd_tx: mpsc::Sender<PiSessionCommand>,
    /// Pending response waiters (shared with reader task).
    pending_responses: PendingResponses,
    /// Fork transaction semaphore (single in-flight fork per session).
    fork_txn: Arc<Semaphore>,
    /// Pending client_id queue (shared between command and reader tasks).
    #[allow(dead_code)]
    pending_client_id: PendingClientId,
    /// Authoritative message buffer for this active session.
    /// Populated on AgentEnd and seeded from oqto-log on resume.
    /// `get_message_buffer()` returns this directly -- no Pi RPC needed.
    message_buffer: MessageBuffer,
    /// Handle to the background reader task.
    _reader_handle: tokio::task::JoinHandle<()>,
    /// Handle to the command processor task.
    _cmd_handle: tokio::task::JoinHandle<()>,
}

impl PiSession {
    async fn subscriber_count(&self) -> usize {
        self.subscribers.subscriber_count().await
    }

    /// Return the OS PID of the child process, if available.
    fn child_pid(&self) -> Option<u32> {
        self.process.id()
    }
}

// ============================================================================
// Pi Session Manager
// ============================================================================

/// Manager for Pi agent sessions.
///
/// Handles multiple concurrent Pi sessions with:
/// - Session lifecycle management
/// - Command routing
/// - Event broadcasting
/// - State tracking
/// - Idle cleanup
/// - Persistence to oqto-log
pub struct PiSessionManager {
    /// Active sessions.
    sessions: RwLock<HashMap<String, PiSession>>,
    /// Manager configuration.
    config: PiManagerConfig,
    /// Shutdown signal sender.
    shutdown_tx: broadcast::Sender<()>,
    /// Sessions currently being created (guards against concurrent creation).
    /// Holds session IDs that are in the process of being spawned but not yet
    /// inserted into the `sessions` map. Prevents the TOQTOU race in
    /// `get_or_create_session` where two concurrent callers both pass the
    /// `contains_key` check and each spawn a separate Pi process.
    creating: tokio::sync::Mutex<std::collections::HashSet<String>>,
    /// Cached model lists per workdir (populated when any session in that workdir fetches models).
    /// Key: canonical workdir path, Value: (models JSON, unix timestamp when cached).
    model_cache: RwLock<HashMap<String, (serde_json::Value, u64)>>,
    /// Last observed mtime of ~/.pi/agent/models.json. Used to invalidate the
    /// model_cache when the file is updated (e.g., after admin syncs models).
    models_json_mtime: RwLock<Option<std::time::SystemTime>>,
    /// Map Pi native session IDs back to the runner session key.
    session_aliases: Arc<RwLock<HashMap<String, String>>>,
}

impl PiSessionManager {
    /// Create a new Pi session manager.
    pub fn new(config: PiManagerConfig) -> Arc<Self> {
        let (shutdown_tx, _) = broadcast::channel(1);

        // Load persisted model cache from disk
        let model_cache = Self::load_model_cache_from_disk(config.model_cache_dir.as_deref());

        Arc::new(Self {
            sessions: RwLock::new(HashMap::new()),
            config,
            shutdown_tx,
            creating: tokio::sync::Mutex::new(std::collections::HashSet::new()),
            model_cache: RwLock::new(model_cache),
            models_json_mtime: RwLock::new(None),
            session_aliases: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a new session.
    ///
    /// Returns the **real** session ID assigned by Pi (which may differ from
    /// the provisional `session_id` passed by the caller). For resumed
    /// sessions the IDs typically match; for brand-new sessions Pi generates
    /// its own ID and the runner re-keys the session map.
    pub async fn create_session(
        self: &Arc<Self>,
        session_id: String,
        config: PiSessionConfig,
    ) -> Result<String> {
        // Check if session already exists
        {
            let sessions = self.sessions.read().await;
            if sessions.contains_key(&session_id) {
                anyhow::bail!("Session '{}' already exists", session_id);
            }
        }

        info!("Creating Pi session '{}' in {:?}", session_id, config.cwd);

        // Use explicit session file if provided (for resuming).
        // For new sessions, do NOT pass --session: let Pi create its
        // own session file and generate its own ID. The runner will
        // learn Pi's real session ID via get_state after startup and
        // re-key the session map accordingly.
        //
        // If neither session_file nor continue_session is set, try to
        // find an existing JSONL session file for this session ID. This
        // enables resuming external sessions (started in Pi directly,
        // not through Oqto) so the agent has the full conversation context.
        //
        // The JSONL filename uses Pi's native UUID, but session_id may be
        // an Oqto ID. If direct lookup fails, resolve the Pi native ID via
        // oqto-log external_id and retry.
        let continue_session = if config.continue_session.is_some() {
            config.continue_session.clone()
        } else {
            let mut found =
                oqto_pi::session_files::find_session_file(&session_id, Some(&config.cwd));
            if found.is_none()
                && let Ok(home) = std::env::var("HOME")
                && let Some(pi_id) = oqto_history::oqto_log::ops::find_external_by_session(
                    std::path::Path::new(&home),
                    &session_id,
                )
                .await
            {
                found = oqto_pi::session_files::find_session_file(&pi_id, Some(&config.cwd));
            }
            found
        };

        if let Some(ref cs) = continue_session
            && config.continue_session.is_none()
        {
            info!(
                "Auto-discovered Pi session file for '{}': {:?}",
                session_id, cs
            );
        }

        let session_file = config
            .session_file
            .as_ref()
            .or(continue_session.as_ref())
            .cloned();

        let browser_session_id = browser_session_name(&session_id);
        let socket_dir_override = config
            .env
            .get("AGENT_BROWSER_SOCKET_DIR")
            .map(String::as_str);
        let session_socket_dir =
            agent_browser_session_dir(&browser_session_id, socket_dir_override);
        if let Err(err) = std::fs::create_dir_all(&session_socket_dir) {
            warn!(
                "Failed to create agent-browser socket dir {}: {}",
                session_socket_dir.display(),
                err
            );
        }
        #[cfg(unix)]
        if let Err(err) =
            std::fs::set_permissions(&session_socket_dir, std::fs::Permissions::from_mode(0o700))
        {
            warn!(
                "Failed to set permissions for agent-browser socket dir {}: {}",
                session_socket_dir.display(),
                err
            );
        }

        let session_socket_dir_str = session_socket_dir.to_string_lossy().to_string();

        // Build Pi arguments
        let mut pi_args: Vec<String> = vec!["--mode".to_string(), "rpc".to_string()];

        if let Some(ref provider) = config.provider {
            pi_args.push("--provider".to_string());
            pi_args.push(provider.clone());
        }
        if let Some(ref model) = config.model {
            pi_args.push("--model".to_string());
            pi_args.push(model.clone());
        }
        if let Some(ref session_file) = session_file {
            pi_args.push("--session".to_string());
            pi_args.push(session_file.to_string_lossy().to_string());
        }
        // Build command - either direct or via bwrap sandbox
        let mut bwrap_pre_exec_config: Option<SandboxConfig> = None;
        let mut cmd = if let Some(ref sandbox_config) = self.config.sandbox_config {
            if sandbox_config.enabled {
                // Merge with workspace-specific config (can only add restrictions)
                let mut effective_config = sandbox_config.with_workspace_config(&config.cwd);
                if !effective_config
                    .extra_rw_bind
                    .contains(&session_socket_dir_str)
                {
                    effective_config
                        .extra_rw_bind
                        .push(session_socket_dir_str.clone());
                }

                // Build bwrap args for the workspace
                match effective_config.build_bwrap_args_for_user(&config.cwd, None) {
                    Some(bwrap_args) => {
                        // Command: bwrap [bwrap_args] -- pi [pi_args]
                        let mut cmd = Command::new("bwrap");

                        // Add bwrap args
                        for arg in &bwrap_args {
                            cmd.arg(arg);
                        }

                        // Add Pi binary and args
                        cmd.arg(&self.config.pi_binary);
                        for arg in &pi_args {
                            cmd.arg(arg);
                        }

                        bwrap_pre_exec_config = Some(effective_config.clone());

                        info!(
                            "Sandboxing Pi session '{}' with profile '{}' ({} bwrap args)",
                            session_id,
                            effective_config.profile,
                            bwrap_args.len()
                        );
                        debug!(
                            "bwrap command: bwrap {} {} {:?}",
                            bwrap_args.join(" "),
                            self.config.pi_binary.display(),
                            pi_args
                        );

                        cmd
                    }
                    None => {
                        // SECURITY: bwrap not available but sandbox was requested
                        error!(
                            "SECURITY: Sandbox requested for Pi session '{}' but bwrap not available. \
                             Refusing to run unsandboxed.",
                            session_id
                        );
                        anyhow::bail!(
                            "Sandbox requested but bwrap not available. \
                             Install bubblewrap (bwrap) or disable sandboxing."
                        );
                    }
                }
            } else {
                // Sandbox config exists but is disabled
                let mut cmd = Command::new(&self.config.pi_binary);
                for arg in &pi_args {
                    cmd.arg(arg);
                }
                cmd.current_dir(&config.cwd);
                cmd
            }
        } else {
            // No sandbox config - run Pi directly
            let mut cmd = Command::new(&self.config.pi_binary);
            for arg in &pi_args {
                cmd.arg(arg);
            }
            cmd.current_dir(&config.cwd);
            cmd
        };

        // Set environment variables
        cmd.envs(&config.env);
        if !config.env.contains_key("AGENT_BROWSER_SOCKET_DIR") {
            cmd.env("AGENT_BROWSER_SOCKET_DIR", &session_socket_dir_str);
        }
        if !config.env.contains_key("AGENT_BROWSER_SESSION") {
            cmd.env("AGENT_BROWSER_SESSION", &browser_session_id);
        }
        // Set OQTO_SESSION_ID so agents can use oqtoctl a2ui commands
        if !config.env.contains_key("OQTO_SESSION_ID") {
            cmd.env("OQTO_SESSION_ID", &session_id);
        }

        // Configure pipes
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if let Some(pre_exec_cfg) = bwrap_pre_exec_config.as_ref() {
            configure_bwrap_pre_exec(cmd.as_std_mut(), pre_exec_cfg, &config.cwd)
                .context("Failed to configure bwrap pre-exec hooks")?;
        }

        // Spawn the process
        let mut child = cmd.spawn().context("Failed to spawn Pi process")?;
        let pid = child.id().unwrap_or(0);
        info!(
            "Spawned Pi process for session '{}' (pid={})",
            session_id, pid
        );

        // Take ownership of pipes
        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;
        let stderr = child.stderr.take();

        // Create channels
        // Per-subscriber event distribution. Each subscriber (browser tab)
        // gets its own unbounded mpsc channel, guaranteeing zero event loss.
        // Unlike broadcast, a slow subscriber never causes events to be
        // dropped for other subscribers (or itself).
        let subscribers = EventSubscribers::new();
        let (cmd_tx, cmd_rx) = mpsc::channel::<PiSessionCommand>(32);

        // Shared state for the session
        let state = Arc::new(RwLock::new(PiSessionState::Starting));
        let last_activity = Arc::new(RwLock::new(Instant::now()));
        let pending_responses: PendingResponses = Arc::new(RwLock::new(HashMap::new()));
        let active_provider = Arc::new(RwLock::new(config.provider.clone()));
        let active_model = Arc::new(RwLock::new(config.model.clone()));
        // Fork transaction semaphore: allow exactly one in-flight fork per session
        let fork_txn = Arc::new(Semaphore::new(1));
        // Pending client_id queue for optimistic message matching
        let pending_client_id: PendingClientId = Arc::new(Mutex::new(VecDeque::new()));
        // session external_id -- starts as Oqto UUID, updated to Pi native ID by reader task
        let initial_external_id = if session_id.starts_with("oqto-") {
            String::new()
        } else {
            session_id.clone()
        };
        let session_external_id: SessionExternalId = Arc::new(RwLock::new(initial_external_id));

        // Seed message buffer from oqto-log so resumed sessions have history immediately.
        let seed_messages = if let Ok(home) = std::env::var("HOME") {
            oqto_history::oqto_log::projector::project_session_messages_auto(
                std::path::Path::new(&home),
                &session_id,
                None,
            )
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let message_buffer: MessageBuffer = Arc::new(RwLock::new(seed_messages));

        // Spawn stdout reader task
        let reader_handle = {
            let session_id = session_id.clone();
            let subscribers_for_reader = subscribers.clone();
            let state = Arc::clone(&state);
            let last_activity = Arc::clone(&last_activity);
            let work_dir = config.cwd.clone();
            let pending_responses = Arc::clone(&pending_responses);
            let pending_client_id = Arc::clone(&pending_client_id);
            let cmd_tx_for_reader = cmd_tx.clone();
            let external_id_ref = Arc::clone(&session_external_id);
            let session_aliases = Arc::clone(&self.session_aliases);
            let msg_buf = Arc::clone(&message_buffer);
            let active_provider_for_reader = Arc::clone(&active_provider);
            let active_model_for_reader = Arc::clone(&active_model);

            let runner_id = self.config.runner_id.clone();
            tokio::spawn(async move {
                Self::stdout_reader_task(
                    session_id,
                    stdout,
                    stderr,
                    subscribers_for_reader,
                    state,
                    last_activity,
                    work_dir,
                    pending_responses,
                    pending_client_id,
                    cmd_tx_for_reader,
                    external_id_ref,
                    session_aliases,
                    runner_id,
                    msg_buf,
                    active_provider_for_reader,
                    active_model_for_reader,
                )
                .await;
            })
        };

        // Spawn command processor task
        let cmd_handle = {
            let session_id = session_id.clone();
            let state = Arc::clone(&state);
            let last_activity = Arc::clone(&last_activity);
            let pending_client_id = Arc::clone(&pending_client_id);

            tokio::spawn(async move {
                Self::command_processor_task(
                    session_id,
                    stdin,
                    cmd_rx,
                    state,
                    last_activity,
                    pending_client_id,
                )
                .await;
            })
        };

        let requested_provider = config.provider.clone();
        let requested_model = config.model.clone();
        let requested_workdir = config.cwd.to_string_lossy().to_string();

        // Store the session (stdin is owned by the command processor task)
        let session = PiSession {
            id: session_id.clone(),
            config,
            process: child,
            state: Arc::clone(&state),
            session_external_id,
            active_provider,
            active_model,
            last_activity: Arc::clone(&last_activity),
            subscribers,
            cmd_tx,
            pending_responses,
            fork_txn,
            pending_client_id,
            message_buffer,
            _reader_handle: reader_handle,
            _cmd_handle: cmd_handle,
        };

        // Store under the caller-provided ID. This is the routing key
        // used for all subsequent commands and event forwarding. Pi may
        // internally use a different session ID (visible in get_state),
        // but the runner's map always uses the caller's key. No re-keying.
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session);
        }

        if let (Some(provider), Some(model_id)) = (requested_provider, requested_model)
            && let Err(err) = self
                .apply_initial_model_selection(
                    &session_id,
                    &provider,
                    &model_id,
                    Some(&requested_workdir),
                )
                .await
        {
            warn!(
                "Session '{}' initial model sync failed (provider='{}', model='{}'): {}. Continuing with Pi default.",
                session_id, provider, model_id, err
            );
        }

        info!("Session '{}' created successfully", session_id);
        Ok(session_id)
    }

    /// Get or create a session.
    ///
    /// Returns the real session ID (may differ from `session_id` for new
    /// sessions where Pi assigns its own ID).
    ///
    /// This method is safe against concurrent calls with the same session ID.
    /// A creation-in-progress guard prevents the TOQTOU race where two callers
    /// both see the session as absent and each spawn a separate Pi process.
    pub async fn get_or_create_session(
        self: &Arc<Self>,
        session_id: &str,
        config: PiSessionConfig,
    ) -> Result<String> {
        if let Some(existing) = self.resolve_session_key(session_id).await {
            debug!(
                "Session '{}' already exists (alias '{}')",
                session_id, existing
            );
            return Ok(existing);
        }

        // Acquire creation lock to prevent concurrent spawns for the same ID.
        {
            let mut creating = self.creating.lock().await;
            // Re-check under lock: another caller may have finished creating
            // between our read above and acquiring this lock.
            if let Some(existing) = self.resolve_session_key(session_id).await {
                debug!("Session '{}' created by concurrent caller", session_id);
                return Ok(existing);
            }
            if creating.contains(session_id) {
                // Another task is currently creating this session. Return
                // success -- the caller will find it in the map shortly.
                info!(
                    "Session '{}' creation already in progress, skipping duplicate spawn",
                    session_id
                );
                return Ok(session_id.to_string());
            }
            creating.insert(session_id.to_string());
        }

        // Spawn the session (the creating guard is held in the set).
        // Hard timeout so session.create cannot hang forever and wedge callers.
        let result = match tokio::time::timeout(
            std::time::Duration::from_secs(20),
            self.create_session(session_id.to_string(), config),
        )
        .await
        {
            Ok(res) => res,
            Err(_) => anyhow::bail!("timed out creating session '{}' after 20s", session_id),
        };

        // Remove from creation set regardless of success/failure.
        {
            let mut creating = self.creating.lock().await;
            creating.remove(session_id);
        }

        result
    }

    async fn apply_initial_model_selection(
        &self,
        session_id: &str,
        provider: &str,
        model_id: &str,
        workdir: Option<&str>,
    ) -> Result<()> {
        match self.set_model(session_id, provider, model_id).await {
            Ok(_) => {
                if let Err(err) = self
                    .set_session_model_cache(session_id, provider, model_id)
                    .await
                {
                    warn!(
                        "Session '{}' updated model but failed to update cache: {}",
                        session_id, err
                    );
                }
                info!(
                    "Session '{}' applied startup model selection: {}/{}",
                    session_id, provider, model_id
                );
                Ok(())
            }
            Err(err) => {
                let available = self
                    .get_available_models(session_id, workdir)
                    .await
                    .unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
                if !model_available_for_provider(&available, provider, model_id) {
                    warn!(
                        "Session '{}' requested startup model '{}/{}' is unavailable; continuing with Pi default",
                        session_id, provider, model_id
                    );
                    return Ok(());
                }
                Err(err)
            }
        }
    }

    /// Send a prompt to a session.
    pub async fn prompt(
        &self,
        session_id: &str,
        message: &str,
        client_id: Option<String>,
    ) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::Prompt {
                message: message.to_string(),
                client_id,
            },
        )
        .await
    }

    /// Send a steering message to a session.
    pub async fn steer(&self, session_id: &str, message: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::Steer {
                message: message.to_string(),
                client_id: None,
            },
        )
        .await
    }

    /// Send a steering message with client_id to a session.
    pub async fn steer_with_client_id(
        &self,
        session_id: &str,
        message: &str,
        client_id: Option<String>,
    ) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::Steer {
                message: message.to_string(),
                client_id,
            },
        )
        .await
    }

    /// Send a follow-up message to a session.
    pub async fn follow_up(&self, session_id: &str, message: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::FollowUp {
                message: message.to_string(),
                client_id: None,
            },
        )
        .await
    }

    /// Send a follow-up message with client_id to a session.
    pub async fn follow_up_with_client_id(
        &self,
        session_id: &str,
        message: &str,
        client_id: Option<String>,
    ) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::FollowUp {
                message: message.to_string(),
                client_id,
            },
        )
        .await
    }

    /// Abort current operation in a session.
    pub async fn abort(&self, session_id: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::Abort).await
    }

    /// Compact conversation history in a session.
    pub async fn compact(&self, session_id: &str, instructions: Option<&str>) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::Compact(instructions.map(String::from)),
        )
        .await
    }

    /// Subscribe to events from a session.
    pub async fn subscribe(
        &self,
        session_id: &str,
    ) -> Result<mpsc::UnboundedReceiver<PiEventWrapper>> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&resolved_id)
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;

        Ok(session.subscribers.subscribe().await)
    }

    /// List all sessions.
    ///
    /// Returns the Oqto session ID (the key in the sessions map) as the
    /// session_id.  This is the same ID used in broadcast events and the
    /// one the frontend should use for all commands. The Pi native ID is
    /// an external harness identity stored in oqto-log.
    pub async fn list_sessions(&self) -> Vec<PiSessionInfo> {
        let snapshots: Vec<(
            String,
            Arc<RwLock<String>>,
            Arc<RwLock<PiSessionState>>,
            Arc<RwLock<Instant>>,
            usize,
            PathBuf,
            Arc<RwLock<Option<String>>>,
            Arc<RwLock<Option<String>>>,
        )> = {
            let sessions = self.sessions.read().await;
            let mut snaps = Vec::with_capacity(sessions.len());
            for s in sessions.values() {
                snaps.push((
                    s.id.clone(),
                    Arc::clone(&s.session_external_id),
                    Arc::clone(&s.state),
                    Arc::clone(&s.last_activity),
                    s.subscriber_count().await,
                    s.config.cwd.clone(),
                    Arc::clone(&s.active_provider),
                    Arc::clone(&s.active_model),
                ));
            }
            snaps
        };

        let mut infos = Vec::with_capacity(snapshots.len());
        for (
            id,
            external_id,
            state,
            last_activity_arc,
            subscriber_count,
            cwd,
            active_provider,
            active_model,
        ) in snapshots
        {
            let provider = active_provider.read().await.clone();
            let model = active_model.read().await.clone();
            let current_state = *state.read().await;
            let eid = external_id.read().await.clone();
            let external_id = if eid.is_empty() || eid == id {
                None
            } else {
                Some(eid)
            };
            infos.push(PiSessionInfo {
                session_id: id,
                hstry_id: external_id,
                state: current_state,
                last_activity: {
                    // Convert Instant to Unix timestamp in milliseconds.
                    // last_activity is an Instant of when the activity happened;
                    // elapsed() gives duration since then. Subtract from now.
                    let last_activity = *last_activity_arc.read().await;
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;
                    let elapsed_ms = last_activity.elapsed().as_millis() as i64;
                    now_ms - elapsed_ms
                },
                subscriber_count,
                cwd,
                provider,
                model,
            });
        }

        infos
    }

    /// Resolve the session external_id for a session.
    ///
    /// Returns Pi's native session ID when known; otherwise falls back to the
    /// platform_id for lookup-only paths.
    pub async fn session_external_id(&self, session_id: &str) -> String {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(&resolved_id) {
            let external_id = session.session_external_id.read().await.clone();
            if external_id.trim().is_empty() {
                resolved_id
            } else {
                external_id
            }
        } else {
            // Session not running: use platform_id fallback for lookup paths.
            session_id.to_string()
        }
    }

    /// Get state of a specific session.
    pub async fn get_state(&self, session_id: &str) -> Result<PiState> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let (runner_state, pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&resolved_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            let current_state = *session.state.read().await;
            (
                current_state,
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let request_id = format!(
            "get_state_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );

        // Register waiter before sending command
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send the command
        cmd_tx
            .send(PiSessionCommand::GetState {
                request_id: request_id.clone(),
            })
            .await
            .context("Failed to send GetState command")?;

        // Wait for response with timeout
        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetState response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetState failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        // Parse state from response data
        let data = response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetState response missing data"))?;
        let mut state: PiState =
            serde_json::from_value(data).context("Failed to parse PiState from response")?;

        // Override streaming/compacting flags with runner's tracked state
        // This fixes issues where Pi doesn't correctly clear its isStreaming flag
        state.is_streaming = runner_state == PiSessionState::Streaming;
        state.is_compacting = runner_state == PiSessionState::Compacting;

        // Always return the Oqto session ID (the key used in events and
        // commands), not Pi's internal native ID.
        state.session_id = Some(resolved_id);

        Ok(state)
    }

    /// Start a new session within the same Pi process.
    pub async fn new_session(&self, session_id: &str, parent_session: Option<&str>) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::NewSession(parent_session.map(String::from)),
        )
        .await
    }

    /// Get all messages from a session.
    /// Check if the given session (or its resolved key) has an active Pi process.
    /// Get a clone of the session config (for forking/cloning sessions).
    pub async fn get_session_config(&self, session_id: &str) -> Option<PiSessionConfig> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let sessions = self.sessions.read().await;
        sessions.get(&resolved_id).map(|s| s.config.clone())
    }

    /// Update the cached provider/model for a live session.
    ///
    /// This cache is used by `list_sessions()` so frontend model indicators
    /// reflect the same model Pi reports via `get_state`.
    pub async fn set_session_model_cache(
        &self,
        session_id: &str,
        provider: &str,
        model_id: &str,
    ) -> Result<()> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&resolved_id)
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
        session.config.provider = Some(provider.to_string());
        session.config.model = Some(model_id.to_string());
        *session.active_provider.write().await = Some(provider.to_string());
        *session.active_model.write().await = Some(model_id.to_string());
        Ok(())
    }

    /// Try to begin a fork transaction for a session.
    /// Returns None if another fork is already in progress.
    pub async fn try_begin_fork_transaction(
        &self,
        session_id: &str,
    ) -> Result<Option<OwnedSemaphorePermit>> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let fork_txn = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&resolved_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            Arc::clone(&session.fork_txn)
        };

        match fork_txn.try_acquire_owned() {
            Ok(permit) => Ok(Some(permit)),
            Err(tokio::sync::TryAcquireError::NoPermits) => Ok(None),
            Err(tokio::sync::TryAcquireError::Closed) => Ok(None),
        }
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let sessions = self.sessions.read().await;
        sessions.contains_key(&resolved_id)
    }

    /// Return the in-memory message buffer for an active session.
    ///
    /// This is the authoritative source of messages for active sessions.
    /// Returns `None` if the session doesn't exist (inactive/dead).
    /// The buffer is populated from:
    ///   - oqto-log seed on session creation (existing history)
    ///   - AgentEnd events (complete message list from Pi)
    ///   - Incremental persist responses (mid-stream updates)
    pub async fn get_message_buffer(&self, session_id: &str) -> Option<Vec<ChatMessageProto>> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(&resolved_id) {
            Some(session.message_buffer.read().await.clone())
        } else {
            None
        }
    }

    pub async fn get_messages(&self, session_id: &str) -> Result<serde_json::Value> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let request_id = "get_messages".to_string();

        // Get session and register response waiter
        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&resolved_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        // Register waiter before sending command
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send the command
        cmd_tx
            .send(PiSessionCommand::GetMessages)
            .await
            .context("Failed to send GetMessages command")?;

        // Wait for response with timeout
        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetMessages response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetMessages failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetMessages response missing data"))
    }

    /// Set the model for a session.
    pub async fn set_model(
        &self,
        session_id: &str,
        provider: &str,
        model_id: &str,
    ) -> Result<PiResponse> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let request_id = "set_model".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&resolved_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::SetModel {
                provider: provider.to_string(),
                model_id: model_id.to_string(),
            })
            .await
            .context("Failed to send SetModel command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for SetModel response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "SetModel failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        Ok(response)
    }

    /// Get available models.
    pub async fn get_available_models(
        &self,
        session_id: &str,
        workdir: Option<&str>,
    ) -> Result<serde_json::Value> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let request_id = "get_available_models".to_string();

        // Try to get the live session
        let session_info = {
            let sessions = self.sessions.read().await;
            sessions.get(&resolved_id).map(|s| {
                (
                    Arc::clone(&s.pending_responses),
                    s.cmd_tx.clone(),
                    s.config.cwd.to_string_lossy().to_string(),
                )
            })
        };

        // Invalidate cache when the admin syncs models (file mtime changes).
        if self.models_json_changed().await {
            info!("models.json changed on disk, invalidating model cache");
            self.model_cache.write().await.clear();
        }

        // Resolve the workdir for caching.
        let target_workdir = if let Some((_, _, ref wd)) = session_info {
            wd.clone()
        } else {
            let resolved_workdir = workdir.and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then_some(trimmed.to_string())
            });
            resolved_workdir
                .unwrap_or_else(|| self.config.default_cwd.to_string_lossy().to_string())
        };

        // Read cache as fallback, but do not return early.
        // OAuth/login-backed model availability can change independently of
        // models.json, so we must still probe live Pi sources.
        let cached_models = self.get_cached_models_for_workdir(&target_workdir).await;

        // Cache miss or expired — build a fresh model list.
        //
        // We gather models from up to three sources and merge them:
        //  1. Live session Pi (if one exists for this session)
        //  2. models.json on disk (eavs-provisioned, instant)
        //  3. Ephemeral Pi RPC (user OAuth/API key models, best-effort)
        //
        // Source 1 and 3 both come from Pi, but a live session's Pi process
        // caches its model list at startup and never refreshes it. So a
        // long-running session won't see newly released models. The ephemeral
        // Pi spawns a fresh process that discovers the latest provider models.

        // Source 1: live session
        let session_models = if let Some((pending_responses, cmd_tx, _)) = session_info {
            let (tx, rx) = oneshot::channel();
            {
                let mut pending = pending_responses.write().await;
                pending.insert(request_id.clone(), tx);
            }

            cmd_tx
                .send(PiSessionCommand::GetAvailableModels)
                .await
                .context("Failed to send GetAvailableModels command")?;

            match tokio::time::timeout(Duration::from_secs(10), rx).await {
                Ok(Ok(response)) if response.success => {
                    let data = response.data.unwrap_or(serde_json::Value::Array(vec![]));
                    if let Some(inner) = data.get("models") {
                        inner.clone()
                    } else if data.is_array() {
                        data
                    } else {
                        serde_json::Value::Array(vec![])
                    }
                }
                Ok(Ok(response)) => {
                    warn!(
                        "GetAvailableModels failed: {}",
                        response.error.unwrap_or_else(|| "unknown".to_string())
                    );
                    serde_json::Value::Array(vec![])
                }
                Ok(Err(e)) => {
                    warn!("GetAvailableModels channel error: {}", e);
                    serde_json::Value::Array(vec![])
                }
                Err(_) => {
                    warn!("GetAvailableModels timeout");
                    serde_json::Value::Array(vec![])
                }
            }
        } else {
            serde_json::Value::Array(vec![])
        };

        // Source 2: models.json on disk
        let disk_models = self.load_models_from_disk().await;

        // Source 3: ephemeral Pi (picks up newly released provider models)
        // Keep model-loading latency low for UI responsiveness. Disk models are
        // already available immediately, so ephemeral Pi probing should be
        // best-effort and aggressively bounded.
        let pi_models = match tokio::time::timeout(
            Duration::from_secs(3),
            self.fetch_models_ephemeral(&target_workdir),
        )
        .await
        {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => {
                debug!("Ephemeral Pi model fetch failed (non-fatal): {}", e);
                serde_json::Value::Array(vec![])
            }
            Err(_) => {
                debug!("Ephemeral Pi model fetch timed out (non-fatal)");
                serde_json::Value::Array(vec![])
            }
        };

        // Prefer disk models (admin/eavs-managed) as authoritative.
        // Ephemeral/session sources may include provider catalogs that are not
        // actually configured for this deployment (e.g. built-in OpenAI/Gemini
        // catalogs without usable credentials). To avoid surfacing unavailable
        // providers in the UI, only merge ephemeral/session models whose
        // provider is already present in disk models.
        //
        // Built-in Pi providers often use bare names (e.g. "openai-codex") while
        // EAVS-managed entries are prefixed ("eavs-openai-codex"). We allow
        // ephemeral/session models whose provider name matches either the exact
        // disk provider name or the disk name with "eavs-" stripped.
        let mut models = if let Some(disk_arr) = disk_models.as_array() {
            if !disk_arr.is_empty() {
                let allowed_providers: std::collections::HashSet<String> = disk_arr
                    .iter()
                    .filter_map(|m| {
                        m.get("provider")
                            .and_then(|p| p.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect();

                // Build a set of bare provider names (without "eavs-" prefix) so
                // that ephemeral Pi built-in providers match their EAVS-managed
                // counterparts.
                let allowed_bare: std::collections::HashSet<&str> = allowed_providers
                    .iter()
                    .filter_map(|p| p.strip_prefix("eavs-"))
                    .collect();

                // Direct Pi OAuth providers are configured in Pi auth storage,
                // not in oqto-managed models.json. Keep them visible even when
                // disk models are present (e.g. default/eavs-only disk config).
                let direct_pi_oauth_providers: std::collections::HashSet<&str> =
                    ["openai-codex"].into_iter().collect();

                let filter_by_provider = |value: &serde_json::Value| -> serde_json::Value {
                    let filtered = value
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter(|m| {
                                    m.get("provider")
                                        .and_then(|p| p.as_str())
                                        .map(|p| {
                                            allowed_providers.contains(p)
                                                || allowed_bare.contains(p)
                                                || direct_pi_oauth_providers.contains(p)
                                        })
                                        .unwrap_or(false)
                                })
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    serde_json::Value::Array(filtered)
                };

                let pi_filtered = filter_by_provider(&pi_models);
                let session_filtered = filter_by_provider(&session_models);
                Self::merge_model_lists(
                    &Self::merge_model_lists(&pi_filtered, &session_filtered),
                    &disk_models,
                )
            } else {
                // No disk models: keep previous merge behavior.
                Self::merge_model_lists(
                    &Self::merge_model_lists(&pi_models, &session_models),
                    &disk_models,
                )
            }
        } else {
            Self::merge_model_lists(
                &Self::merge_model_lists(&pi_models, &session_models),
                &disk_models,
            )
        };

        // If all live sources failed, fall back to the last known good cache.
        if models.as_array().is_none_or(|a| a.is_empty())
            && let Some(cached) = cached_models
        {
            models = cached;
        }

        if models.as_array().is_some_and(|a| !a.is_empty()) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            {
                let mut cache = self.model_cache.write().await;
                cache.insert(target_workdir.clone(), (models.clone(), now));
            }
            Self::persist_model_cache_to_disk(
                self.config.model_cache_dir.as_deref(),
                &target_workdir,
                &models,
            );
        }
        Ok(models)
    }

    /// Get cached models for a specific workdir (called directly by runner for dead sessions).
    /// Returns `None` if the entry is expired (older than [`MODEL_CACHE_TTL`]).
    pub async fn get_cached_models_for_workdir(&self, workdir: &str) -> Option<serde_json::Value> {
        let cache = self.model_cache.read().await;
        if let Some((models, cached_at)) = cache.get(workdir) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if now.saturating_sub(*cached_at) <= MODEL_CACHE_TTL.as_secs() {
                return Some(models.clone());
            }
        }
        None
    }

    // ========================================================================
    // Model cache persistence
    // ========================================================================

    /// Compute a stable filename for a workdir path.
    fn model_cache_filename(workdir: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        workdir.hash(&mut hasher);
        format!("{:016x}.json", hasher.finish())
    }

    /// Load all persisted model cache files from disk.
    fn load_model_cache_from_disk(
        cache_dir: Option<&std::path::Path>,
    ) -> HashMap<String, (serde_json::Value, u64)> {
        let Some(dir) = cache_dir else {
            return HashMap::new();
        };
        let mut map = HashMap::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return map;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match std::fs::read_to_string(&path) {
                    Ok(contents) => match serde_json::from_str::<ModelCacheEntry>(&contents) {
                        Ok(entry) => {
                            // Skip expired entries (cached_at == 0 means legacy
                            // entry without timestamp -- treat as expired).
                            let age_secs = now.saturating_sub(entry.cached_at);
                            if entry.cached_at == 0 || age_secs > MODEL_CACHE_TTL.as_secs() {
                                info!(
                                    "Skipping expired model cache for '{}' (age {}s)",
                                    entry.workdir, age_secs
                                );
                                // Clean up stale file
                                let _ = std::fs::remove_file(&path);
                                continue;
                            }
                            info!(
                                "Loaded cached models for workdir '{}' ({} models, age {}s)",
                                entry.workdir,
                                entry.models.as_array().map(|a| a.len()).unwrap_or(0),
                                age_secs,
                            );
                            map.insert(entry.workdir, (entry.models, entry.cached_at));
                        }
                        Err(e) => {
                            warn!("Failed to parse model cache file {:?}: {}", path, e);
                        }
                    },
                    Err(e) => {
                        warn!("Failed to read model cache file {:?}: {}", path, e);
                    }
                }
            }
        }
        if !map.is_empty() {
            info!("Loaded model cache for {} workdir(s) from disk", map.len());
        }
        map
    }

    /// Persist the model cache for a single workdir to disk.
    fn persist_model_cache_to_disk(
        cache_dir: Option<&std::path::Path>,
        workdir: &str,
        models: &serde_json::Value,
    ) {
        let Some(dir) = cache_dir else {
            return;
        };
        if let Err(e) = std::fs::create_dir_all(dir) {
            warn!("Failed to create model cache dir {:?}: {}", dir, e);
            return;
        }
        let filename = Self::model_cache_filename(workdir);
        let path = dir.join(filename);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = ModelCacheEntry {
            workdir: workdir.to_string(),
            models: models.clone(),
            cached_at: now,
        };
        match serde_json::to_string_pretty(&entry) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    warn!("Failed to write model cache file {:?}: {}", path, e);
                }
            }
            Err(e) => {
                warn!("Failed to serialize model cache for '{}': {}", workdir, e);
            }
        }
    }

    /// Check if ~/.pi/agent/models.json has been modified since we last read it.
    async fn models_json_changed(&self) -> bool {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let models_path = PathBuf::from(&home).join(".pi/agent/models.json");
        let current_mtime = std::fs::metadata(&models_path)
            .and_then(|m| m.modified())
            .ok();
        let stored = *self.models_json_mtime.read().await;
        match (stored, current_mtime) {
            (None, None) => false,   // No file yet — nothing to compare
            (None, Some(_)) => true, // First call with existing file — treat as changed so cache is rebuilt from disk
            (Some(_), None) => true, // File disappeared
            (Some(old), Some(new)) => new != old,
        }
    }

    /// Read models directly from ~/.pi/agent/models.json.
    ///
    /// This replaces the old ephemeral Pi spawn approach. The models.json file is
    /// provisioned by oqto via eavs and contains provider configs with full model
    /// lists. Reading it directly is instant, reliable, and avoids all the
    /// fragility of spawning a bun/Pi process (wrong HOME, missing env vars,
    /// permission errors, timeouts).
    async fn load_models_from_disk(&self) -> serde_json::Value {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let models_path = PathBuf::from(&home).join(".pi/agent/models.json");

        // Track mtime for cache invalidation
        if let Ok(meta) = std::fs::metadata(&models_path)
            && let Ok(mtime) = meta.modified()
        {
            *self.models_json_mtime.write().await = Some(mtime);
        }

        let content = match std::fs::read_to_string(&models_path) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "Could not read models.json at {}: {}",
                    models_path.display(),
                    e
                );
                return serde_json::Value::Array(vec![]);
            }
        };

        let config: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse models.json: {}", e);
                return serde_json::Value::Array(vec![]);
            }
        };

        // models.json has { "providers": { "name": { "api": ..., "models": [...] } } }
        // Flatten all provider models into a single array that the frontend expects.
        let mut all_models = Vec::new();

        if let Some(providers) = config.get("providers").and_then(|p| p.as_object()) {
            for (provider_name, provider_config) in providers {
                let api = provider_config
                    .get("api")
                    .and_then(|a| a.as_str())
                    .unwrap_or("openai-completions");
                let base_url = provider_config
                    .get("baseUrl")
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                let api_key_env = provider_config
                    .get("apiKey")
                    .and_then(|k| k.as_str())
                    .unwrap_or("");

                if let Some(models) = provider_config.get("models").and_then(|m| m.as_array()) {
                    for model in models {
                        // Each model in models.json already has id, name, reasoning,
                        // contextWindow, maxTokens, cost, etc. We add provider metadata
                        // so the frontend can display and route correctly.
                        let mut enriched = model.clone();
                        if let Some(obj) = enriched.as_object_mut() {
                            obj.entry("provider".to_string()).or_insert_with(|| {
                                serde_json::Value::String(provider_name.clone())
                            });
                            obj.entry("api".to_string())
                                .or_insert_with(|| serde_json::Value::String(api.to_string()));
                            obj.entry("baseUrl".to_string())
                                .or_insert_with(|| serde_json::Value::String(base_url.to_string()));
                            obj.entry("apiKeyEnv".to_string()).or_insert_with(|| {
                                serde_json::Value::String(api_key_env.to_string())
                            });
                        }
                        all_models.push(enriched);
                    }
                }
            }
        }

        if all_models.is_empty() {
            info!("No models found in {}", models_path.display());
        } else {
            info!(
                "Loaded {} models from {}",
                all_models.len(),
                models_path.display()
            );
        }

        serde_json::Value::Array(all_models)
    }

    /// Merge two model arrays.
    ///
    /// Models from `primary` take precedence; models from `secondary` are
    /// appended only if their `(provider, id)` identity is not already present.
    ///
    /// We intentionally key by provider+id (not id alone), because different
    /// providers can expose the same model id (e.g. Azure Foundry Codex and
    /// OpenAI Codex). Deduping by id alone incorrectly drops one provider.
    fn merge_model_lists(
        primary: &serde_json::Value,
        secondary: &serde_json::Value,
    ) -> serde_json::Value {
        let primary_arr = primary.as_array().cloned().unwrap_or_default();
        let secondary_arr = secondary.as_array().cloned().unwrap_or_default();

        if secondary_arr.is_empty() {
            return serde_json::Value::Array(primary_arr);
        }

        fn model_key(model: &serde_json::Value) -> Option<String> {
            let id = model.get("id").and_then(|v| v.as_str())?;
            let provider = model.get("provider").and_then(|v| v.as_str()).unwrap_or("");
            Some(format!("{provider}/{id}"))
        }

        let mut seen: std::collections::HashSet<String> =
            primary_arr.iter().filter_map(model_key).collect();

        let mut merged = primary_arr;
        for model in secondary_arr {
            if let Some(key) = model_key(&model)
                && seen.insert(key)
            {
                merged.push(model);
            }
        }
        serde_json::Value::Array(merged)
    }

    /// Spawn an ephemeral Pi RPC process to fetch the full model list.
    ///
    /// This picks up user-authenticated models (OAuth logins via `pi auth`,
    /// personal API keys) that are not in the admin-provisioned models.json.
    /// Called best-effort with a timeout — failure is non-fatal.
    async fn fetch_models_ephemeral(&self, workdir: &str) -> Result<serde_json::Value> {
        let workdir_path = std::path::Path::new(workdir);
        if !workdir_path.is_dir() {
            anyhow::bail!("Workdir '{}' does not exist", workdir);
        }

        debug!(
            "Spawning ephemeral Pi to fetch user-authenticated models for '{}'",
            workdir
        );

        let mut cmd = Command::new(&self.config.pi_binary);
        cmd.args(["--mode", "rpc", "--no-session"])
            .current_dir(workdir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Eavs virtual keys are now embedded directly in models.json,
        // so no env file loading is needed for ephemeral Pi spawns.

        let mut child = cmd
            .spawn()
            .context("Failed to spawn ephemeral Pi process")?;
        let mut stdin = child.stdin.take().context("No stdin on ephemeral Pi")?;
        let stdout = child.stdout.take().context("No stdout on ephemeral Pi")?;

        let mut reader = BufReader::new(stdout).lines();

        let result: Result<serde_json::Value> = async {
            let mut got_first_line = false;
            loop {
                let line = reader
                    .next_line()
                    .await
                    .context("Failed to read from ephemeral Pi")?;
                let Some(line) = line else {
                    anyhow::bail!("Ephemeral Pi stdout closed without response");
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if !got_first_line {
                    got_first_line = true;
                    let pi_cmd = PiCommand::GetAvailableModels {
                        id: Some("ephemeral_get_models".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await?;
                }

                for parse_result in PiMessage::parse_all(trimmed) {
                    match parse_result {
                        Ok(PiMessage::Response(resp)) => {
                            if resp.id.as_deref() == Some("ephemeral_get_models") {
                                if resp.success {
                                    if let Some(data) = resp.data {
                                        let models = if let Some(inner) = data.get("models") {
                                            inner.clone()
                                        } else if data.is_array() {
                                            data
                                        } else {
                                            serde_json::Value::Array(vec![])
                                        };
                                        return Ok(models);
                                    }
                                } else {
                                    let err_msg =
                                        resp.error.unwrap_or_else(|| "unknown error".to_string());
                                    anyhow::bail!("Ephemeral Pi get_models failed: {}", err_msg);
                                }
                            }
                        }
                        Ok(PiMessage::Event(_)) => {}
                        Err(e) => {
                            debug!("Ephemeral Pi: failed to parse line: {}", e);
                        }
                    }
                }
            }
        }
        .await;

        let _ = child.kill().await;
        result
    }

    /// Get session statistics.
    pub async fn get_session_stats(&self, session_id: &str) -> Result<SessionStats> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let request_id = "get_session_stats".to_string();

        // Get session and register response waiter
        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&resolved_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        // Register waiter before sending command
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send the command
        cmd_tx
            .send(PiSessionCommand::GetSessionStats)
            .await
            .context("Failed to send GetSessionStats command")?;

        // Wait for response with timeout
        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetSessionStats response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetSessionStats failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        let data = response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetSessionStats response missing data"))?;
        let stats: SessionStats =
            serde_json::from_value(data).context("Failed to parse SessionStats from response")?;

        Ok(stats)
    }

    /// Switch to a different session file.
    pub async fn switch_session(&self, session_id: &str, session_path: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SwitchSession(session_path.to_string()),
        )
        .await
    }

    /// Set the display name for a session.
    pub async fn set_session_name(&self, session_id: &str, name: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SetSessionName(name.to_string()),
        )
        .await
    }

    /// Export session to HTML.
    pub async fn export_html(
        &self,
        session_id: &str,
        output_path: Option<&str>,
    ) -> Result<serde_json::Value> {
        let request_id = "export_html".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::ExportHtml(output_path.map(String::from)))
            .await
            .context("Failed to send ExportHtml command")?;

        let response = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .context("Timeout waiting for ExportHtml response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "ExportHtml failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("ExportHtml response missing data"))
    }

    /// Get the last assistant message text.
    pub async fn get_last_assistant_text(&self, session_id: &str) -> Result<Option<String>> {
        let request_id = "get_last_assistant_text".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::GetLastAssistantText)
            .await
            .context("Failed to send GetLastAssistantText command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetLastAssistantText response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetLastAssistantText failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        // Parse the text from response data
        Ok(response.data.and_then(|d| d.as_str().map(String::from)))
    }

    /// Get available commands.
    pub async fn get_commands(&self, session_id: &str) -> Result<serde_json::Value> {
        let request_id = "get_commands".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::GetCommands)
            .await
            .context("Failed to send GetCommands command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetCommands response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetCommands failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetCommands response missing data"))
    }

    /// Cycle to the next model.
    pub async fn cycle_model(&self, session_id: &str) -> Result<PiResponse> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let request_id = "cycle_model".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&resolved_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::CycleModel)
            .await
            .context("Failed to send CycleModel command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for CycleModel response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "CycleModel failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        Ok(response)
    }

    /// Set the thinking level.
    pub async fn set_thinking_level(&self, session_id: &str, level: &str) -> Result<PiResponse> {
        let request_id = "set_thinking_level".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::SetThinkingLevel(level.to_string()))
            .await
            .context("Failed to send SetThinkingLevel command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for SetThinkingLevel response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "SetThinkingLevel failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        Ok(response)
    }

    /// Cycle through thinking levels.
    pub async fn cycle_thinking_level(&self, session_id: &str) -> Result<PiResponse> {
        let request_id = "cycle_thinking_level".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::CycleThinkingLevel)
            .await
            .context("Failed to send CycleThinkingLevel command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for CycleThinkingLevel response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "CycleThinkingLevel failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        Ok(response)
    }

    /// Set steering message delivery mode.
    pub async fn set_steering_mode(&self, session_id: &str, mode: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SetSteeringMode(mode.to_string()),
        )
        .await
    }

    /// Set follow-up message delivery mode.
    pub async fn set_follow_up_mode(&self, session_id: &str, mode: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SetFollowUpMode(mode.to_string()),
        )
        .await
    }

    /// Enable/disable auto-compaction.
    pub async fn set_auto_compaction(&self, session_id: &str, enabled: bool) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::SetAutoCompaction(enabled))
            .await
    }

    /// Enable/disable auto-retry.
    pub async fn set_auto_retry(&self, session_id: &str, enabled: bool) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::SetAutoRetry(enabled))
            .await
    }

    /// Abort an in-progress retry.
    pub async fn abort_retry(&self, session_id: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::AbortRetry)
            .await
    }

    /// Fork from a previous user message.
    /// Fork from a previous message.
    ///
    /// Flow:
    /// 1. Get old session file path via get_state
    /// 2. Tell Pi to fork (Pi creates new JSONL + switches internally)
    /// 3. Get new session file path via get_state
    /// 4. Tell Pi to switch back to the old session
    /// 5. Return fork result with the new session file path
    ///
    /// The caller (server.rs) is responsible for creating the new Oqto session
    /// by spawning a fresh Pi process that resumes from the forked JSONL.
    pub async fn fork(&self, session_id: &str, entry_id: &str) -> Result<ForkResult> {
        // 1. Get old session file path
        let old_state = self
            .get_state(session_id)
            .await
            .context("Failed to get state before fork")?;
        let old_session_file = old_state.session_file.clone();

        // 2. Send fork command to Pi
        let request_id = format!(
            "fork_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::Fork {
                entry_id: entry_id.to_string(),
                request_id: request_id.clone(),
            })
            .await
            .context("Failed to send Fork command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for Fork response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "Fork failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        let data = response
            .data
            .ok_or_else(|| anyhow::anyhow!("Fork response missing data"))?;

        let text = data
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let cancelled = data
            .get("cancelled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if cancelled {
            return Ok(ForkResult {
                text,
                cancelled: true,
                new_session_id: None,
                new_session_file: None,
            });
        }

        // 3. Get new session info (Pi has switched to the forked session).
        // Pi's fork response does not include the new session metadata, so we
        // must observe the session switch via get_state. Poll until either the
        // session id or session file differs from the pre-fork state.
        let old_pi_session_id = old_state.session_id.clone();
        let mut new_state = self
            .get_state(session_id)
            .await
            .context("Failed to get state after fork")?;

        let switched = |state: &PiState| {
            let id_changed = state.session_id != old_pi_session_id;
            let file_changed = match (&state.session_file, &old_session_file) {
                (Some(new_file), Some(old_file)) => new_file != old_file,
                (Some(_), None) => true,
                _ => false,
            };
            id_changed || file_changed
        };

        if !switched(&new_state) {
            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let candidate = self
                    .get_state(session_id)
                    .await
                    .context("Failed to poll state after fork")?;
                if switched(&candidate) {
                    new_state = candidate;
                    break;
                }
            }
        }

        if !switched(&new_state) {
            anyhow::bail!(
                "Fork completed but Pi did not switch to a distinct child session (entry_id='{}')",
                entry_id
            );
        }

        let new_session_file = new_state.session_file.clone();
        let new_pi_session_id = new_state.session_id.clone();

        // 4. Switch Pi back to the old session
        if let Some(ref old_file) = old_session_file {
            self.switch_session(session_id, old_file)
                .await
                .context("Failed to switch Pi back to original session after fork")?;
            info!(
                "Fork: Pi switched back to original session file: {}",
                old_file
            );
        } else {
            warn!("Fork: no old session file to switch back to");
        }

        Ok(ForkResult {
            text,
            cancelled: false,
            new_session_id: new_pi_session_id,
            new_session_file,
        })
    }

    /// Get messages available for forking.
    pub async fn get_fork_messages(&self, session_id: &str) -> Result<serde_json::Value> {
        let request_id = format!(
            "get_fork_messages_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&resolved_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::GetForkMessages {
                request_id: request_id.clone(),
            })
            .await
            .context("Failed to send GetForkMessages command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetForkMessages response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetForkMessages failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetForkMessages response missing data"))
    }

    /// Execute a bash command.
    pub async fn bash(&self, session_id: &str, command: &str) -> Result<serde_json::Value> {
        let request_id = "bash".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (
                Arc::clone(&session.pending_responses),
                session.cmd_tx.clone(),
            )
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::Bash(command.to_string()))
            .await
            .context("Failed to send Bash command")?;

        // Longer timeout for bash commands
        let response = tokio::time::timeout(Duration::from_secs(300), rx)
            .await
            .context("Timeout waiting for Bash response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "Bash failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("Bash response missing data"))
    }

    /// Abort a running bash command.
    pub async fn abort_bash(&self, session_id: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::AbortBash)
            .await
    }

    /// Respond to an extension UI prompt.
    pub async fn extension_ui_response(
        &self,
        session_id: &str,
        id: &str,
        value: Option<&str>,
        confirmed: Option<bool>,
        cancelled: Option<bool>,
    ) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::ExtensionUiResponse {
                id: id.to_string(),
                value: value.map(String::from),
                confirmed,
                cancelled,
            },
        )
        .await
    }

    /// Close a session.
    pub async fn close_session(&self, session_id: &str) -> Result<()> {
        let resolved_id = self.resolve_session_key(session_id).await;
        let resolved_id = resolved_id.as_deref().unwrap_or(session_id);
        info!("Closing session '{}'", resolved_id);

        // Resolve Pi native session ID before shutdown when we still have only
        // an Oqto ID. This avoids persisting final buffered messages under a
        // temporary identity.
        if resolved_id.starts_with("oqto-")
            && let Ok(Ok(state)) = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                self.get_state(resolved_id),
            )
            .await
            && let Some(pi_sid) = state.session_id
            && !pi_sid.trim().is_empty()
            && pi_sid != resolved_id
        {
            let external_id_ref = {
                let sessions = self.sessions.read().await;
                sessions
                    .get(resolved_id)
                    .map(|s| Arc::clone(&s.session_external_id))
            };
            if let Some(external_id_ref) = external_id_ref {
                let old = external_id_ref.read().await.clone();
                *external_id_ref.write().await = pi_sid.clone();
                info!(
                    "Pi[{}] close-session identity resolved: external_id {} -> {}",
                    resolved_id, old, pi_sid
                );
            }
        }

        // IMPORTANT: never hold the sessions RwLock guard across .await.
        // Doing so can deadlock concurrent session.create (writer waits behind
        // this read lock while we're blocked on cmd_tx backpressure).
        let cmd_tx = {
            let sessions = self.sessions.read().await;
            sessions
                .get(resolved_id)
                .map(|session| session.cmd_tx.clone())
        };

        // Best-effort close signal (can be dropped if receiver is gone).
        if let Some(cmd_tx) = cmd_tx {
            let _ = cmd_tx.send(PiSessionCommand::Close).await;
        }

        // Remove from sessions map
        let mut sessions = self.sessions.write().await;
        if let Some(mut session) = sessions.remove(resolved_id) {
            // Kill the process if still running
            let _ = session.process.kill().await;
            info!("Session '{}' closed", resolved_id);
        }

        let mut aliases = self.session_aliases.write().await;
        Self::drop_session_aliases_for_platform(&mut aliases, resolved_id, session_id);

        Ok(())
    }

    /// Run the idle cleanup loop.
    pub async fn cleanup_loop(self: Arc<Self>) {
        let interval = Duration::from_secs(self.config.cleanup_interval_secs);
        let idle_timeout = Duration::from_secs(self.config.idle_timeout_secs);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    self.cleanup_idle_sessions(idle_timeout).await;
                }
                _ = shutdown_rx.recv() => {
                    info!("Cleanup loop shutting down");
                    break;
                }
            }
        }
    }

    /// Shutdown all sessions and stop the manager.
    pub async fn shutdown(&self) {
        info!("Shutting down Pi session manager");

        // Signal shutdown
        let _ = self.shutdown_tx.send(());

        // Close all sessions
        let session_ids: Vec<String> = {
            let sessions = self.sessions.read().await;
            sessions.keys().cloned().collect()
        };

        for session_id in session_ids {
            if let Err(e) = self.close_session(&session_id).await {
                warn!("Error closing session '{}': {}", session_id, e);
            }
        }
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    /// Send a command to a session.
    async fn send_command(&self, session_id: &str, cmd: PiSessionCommand) -> Result<()> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
        let (cmd_tx, state) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&resolved_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (session.cmd_tx.clone(), Arc::clone(&session.state))
        };

        self.validate_command(&resolved_id, &state, &cmd).await?;

        cmd_tx
            .send(cmd)
            .await
            .context("Failed to send command to session")?;

        Ok(())
    }

    async fn resolve_session_key(&self, session_id: &str) -> Option<String> {
        {
            let sessions = self.sessions.read().await;
            if sessions.contains_key(session_id) {
                return Some(session_id.to_string());
            }
        }
        let aliases = self.session_aliases.read().await;
        aliases.get(session_id).cloned()
    }

    fn record_session_alias(
        aliases: &mut HashMap<String, String>,
        platform_id: &str,
        external_id: &str,
    ) {
        if external_id.trim().is_empty() || external_id == platform_id {
            return;
        }

        // Keep exactly one external_id alias per platform session key.
        aliases.retain(|key, value| value != platform_id || key == external_id);
        aliases.insert(external_id.to_string(), platform_id.to_string());
    }

    fn drop_session_aliases_for_platform(
        aliases: &mut HashMap<String, String>,
        platform_id: &str,
        requested_session_id: &str,
    ) {
        aliases.retain(|_, value| value != platform_id);
        if platform_id != requested_session_id {
            aliases.remove(requested_session_id);
        }
    }

    async fn validate_command(
        &self,
        session_id: &str,
        state: &Arc<RwLock<PiSessionState>>,
        cmd: &PiSessionCommand,
    ) -> Result<()> {
        let current_state = *state.read().await;
        let is_idle = current_state == PiSessionState::Idle;
        let is_starting = current_state == PiSessionState::Starting;
        let is_streaming = current_state == PiSessionState::Streaming;

        let is_stopping = current_state == PiSessionState::Stopping;

        match cmd {
            // Prompt, steer, and follow_up are accepted in any "live" state.
            // The command_processor handles state-aware routing:
            //   - steer/follow_up when idle → sent as prompt to Pi
            //   - prompt when streaming → sent with streamingBehavior=steer
            // If the process is dead (stopping), write will fail and state
            // resets to idle automatically.
            PiSessionCommand::Prompt { .. }
            | PiSessionCommand::Steer { .. }
            | PiSessionCommand::FollowUp { .. }
                if !(is_idle || is_starting || is_streaming || is_stopping) =>
            {
                anyhow::bail!(
                    "Session '{}' not ready (state={})",
                    session_id,
                    current_state
                );
            }
            PiSessionCommand::Compact(_) if !is_idle => {
                anyhow::bail!(
                    "Session '{}' not idle for compaction (state={})",
                    session_id,
                    current_state
                );
            }
            PiSessionCommand::NewSession(_) | PiSessionCommand::SwitchSession(_) if !is_idle => {
                anyhow::bail!(
                    "Session '{}' not idle for session switch (state={})",
                    session_id,
                    current_state
                );
            }
            _ => {}
        }

        Ok(())
    }

    /// Cleanup idle sessions that have no subscribers.
    async fn cleanup_idle_sessions(&self, idle_timeout: Duration) {
        let now = Instant::now();
        let mut to_close = Vec::new();

        // After this many seconds of no stdout in a non-idle state, we
        // send a GetState health check to Pi. If Pi is alive, it will
        // respond and update last_activity, buying more time. If the
        // process is dead or truly stuck, the next sweep will catch it.
        let health_check_after = Duration::from_secs(90);
        // After this many seconds, even with health checks, force-reset.
        // This is the absolute maximum for transient states.
        let hard_timeout_transient = Duration::from_secs(120);
        // For streaming/starting (waiting on LLM), be more patient but
        // still have a hard cap so users aren't stuck forever.
        let hard_timeout_streaming = Duration::from_secs(600); // 10 min

        let snapshots: Vec<(
            String,
            Arc<RwLock<PiSessionState>>,
            Arc<RwLock<Instant>>,
            usize,
            EventSubscribers,
            mpsc::Sender<PiSessionCommand>,
            Option<u32>,
        )> = {
            let sessions = self.sessions.read().await;
            let mut snaps = Vec::with_capacity(sessions.len());
            for (id, session) in sessions.iter() {
                snaps.push((
                    id.clone(),
                    Arc::clone(&session.state),
                    Arc::clone(&session.last_activity),
                    session.subscribers.subscriber_count().await,
                    session.subscribers.clone(),
                    session.cmd_tx.clone(),
                    session.child_pid(),
                ));
            }
            snaps
        };

        for (id, state, last_activity_arc, subscriber_count, session_event_tx, cmd_tx, child_pid) in
            snapshots
        {
            let current_state = *state.read().await;
            let last_activity = *last_activity_arc.read().await;
            let is_idle = current_state == PiSessionState::Idle;
            let no_subscribers = subscriber_count == 0;
            let timed_out = now.duration_since(last_activity) > idle_timeout;
            let elapsed = now.duration_since(last_activity);

            if !is_idle {
                // Step 1: Check if the Pi process is still alive.
                // If it crashed, the stdout reader should have caught it already,
                // but belt-and-suspenders: verify via kill(pid, 0).
                if let Some(pid) = child_pid {
                    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
                    if !alive {
                        warn!(
                            "Session '{}' in {:?} but Pi process {} is dead -- forcing to Idle + error",
                            id, current_state, pid,
                        );
                        *state.write().await = PiSessionState::Idle;
                        let error_event = CanonicalEvent {
                            session_id: id.clone(),
                            runner_id: self.config.runner_id.clone(),
                            ts: chrono::Utc::now().timestamp_millis(),
                            payload: EventPayload::AgentError {
                                error: "Agent process died unexpectedly".to_string(),
                                recoverable: false,
                                phase: Some(AgentPhase::Generating),
                            },
                        };
                        session_event_tx.publish(&error_event).await;
                        let idle_event = CanonicalEvent {
                            session_id: id.clone(),
                            runner_id: self.config.runner_id.clone(),
                            ts: chrono::Utc::now().timestamp_millis(),
                            payload: EventPayload::AgentIdle {
                                message_version: None,
                            },
                        };
                        session_event_tx.publish(&idle_event).await;
                        continue;
                    }
                }

                // Step 2: If no stdout for >90s, send a GetState health check.
                // Pi will respond with state data, which updates last_activity
                // via the stdout reader. This proves Pi is alive and responsive.
                if elapsed > health_check_after {
                    if current_state == PiSessionState::Stopping {
                        // Stopping sessions may legitimately have no further stdout
                        // (e.g. final event parse was dropped). Do not surface a
                        // misleading timeout error to users in this state.
                        if elapsed > hard_timeout_transient {
                            warn!(
                                "Session '{}' remained in Stopping for {:?} -- forcing to Idle without timeout error",
                                id, elapsed,
                            );
                            *state.write().await = PiSessionState::Idle;
                            let idle_event = CanonicalEvent {
                                session_id: id.clone(),
                                runner_id: self.config.runner_id.clone(),
                                ts: chrono::Utc::now().timestamp_millis(),
                                payload: EventPayload::AgentIdle {
                                    message_version: None,
                                },
                            };
                            session_event_tx.publish(&idle_event).await;
                        }
                        continue;
                    }

                    let hard_timeout = match current_state {
                        PiSessionState::Streaming | PiSessionState::Starting => {
                            hard_timeout_streaming
                        }
                        _ => hard_timeout_transient,
                    };

                    if elapsed > hard_timeout {
                        // Hard timeout exceeded even after health checks.
                        warn!(
                            "Session '{}' stuck in {:?} for {:?} -- hard timeout, forcing to Idle + error",
                            id, current_state, elapsed,
                        );
                        *state.write().await = PiSessionState::Idle;
                        let error_event = CanonicalEvent {
                            session_id: id.clone(),
                            runner_id: self.config.runner_id.clone(),
                            ts: chrono::Utc::now().timestamp_millis(),
                            payload: EventPayload::AgentError {
                                error: format!(
                                    "No response for {}s -- request timed out. The agent process was still alive but no data was received.",
                                    elapsed.as_secs()
                                ),
                                recoverable: true,
                                phase: Some(AgentPhase::Generating),
                            },
                        };
                        session_event_tx.publish(&error_event).await;
                        let idle_event = CanonicalEvent {
                            session_id: id.clone(),
                            runner_id: self.config.runner_id.clone(),
                            ts: chrono::Utc::now().timestamp_millis(),
                            payload: EventPayload::AgentIdle {
                                message_version: None,
                            },
                        };
                        session_event_tx.publish(&idle_event).await;
                    } else {
                        // Not yet at hard timeout -- send health check ping.
                        debug!(
                            "Session '{}' in {:?} for {:?} with no stdout -- sending health check",
                            id, current_state, elapsed,
                        );
                        let _ = cmd_tx.try_send(PiSessionCommand::GetState {
                            request_id: format!(
                                "get_state_probe_{}",
                                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
                            ),
                        });
                    }
                    continue;
                }
            }

            if is_idle && no_subscribers && timed_out {
                info!(
                    "Session '{}' idle for {:?} with no subscribers, marking for cleanup",
                    id,
                    now.duration_since(last_activity)
                );
                to_close.push(id);
            }
        }

        for session_id in to_close {
            if let Err(e) = self.close_session(&session_id).await {
                warn!(
                    "Error during idle cleanup of session '{}': {}",
                    session_id, e
                );
            }
        }
    }

    /// Background task that reads stdout and broadcasts events.
    async fn stdout_reader_task(
        session_id: String,
        stdout: tokio::process::ChildStdout,
        stderr: Option<tokio::process::ChildStderr>,
        event_tx: EventSubscribers,
        state: Arc<RwLock<PiSessionState>>,
        last_activity: Arc<RwLock<Instant>>,
        work_dir: PathBuf,
        pending_responses: PendingResponses,
        pending_client_id: PendingClientId,
        cmd_tx: mpsc::Sender<PiSessionCommand>,
        session_external_id: SessionExternalId,
        session_aliases: Arc<RwLock<HashMap<String, String>>>,
        runner_id: String,
        message_buffer: MessageBuffer,
        active_provider: Arc<RwLock<Option<String>>>,
        active_model: Arc<RwLock<Option<String>>>,
    ) {
        // Read stderr in a separate task, keeping last N lines in a ring buffer
        // so we can include them in the crash error event.
        let stderr_ring: Arc<tokio::sync::Mutex<std::collections::VecDeque<String>>> = Arc::new(
            tokio::sync::Mutex::new(std::collections::VecDeque::with_capacity(50)),
        );
        if let Some(stderr) = stderr {
            let session_id = session_id.clone();
            let ring = stderr_ring.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if !line.trim().is_empty() {
                        debug!("Pi[{}] stderr: {}", session_id, line);
                        let mut buf = ring.lock().await;
                        if buf.len() >= 50 {
                            buf.pop_front();
                        }
                        buf.push_back(line);
                    }
                }
            });
        }

        // Read stdout. Use read_until instead of lines() to avoid the
        // default line-length cap causing false "process exited" on large
        // tool payload events.
        let mut reader = BufReader::new(stdout);
        let mut raw_buf = Vec::new();
        let mut pending_json_fragment = String::new();
        let mut pending_messages: Vec<AgentMessage> = Vec::new();
        let mut pending_agent_end_retry_placeholder = false;
        let mut retry_cycle_user_persisted = false;
        let mut pending_error_text: Option<String> = None;
        let mut pending_error_recoverable: bool = true;
        let mut pending_error_persisted_oqto: bool = false;
        let mut pending_bound_client_ids: Vec<String> = Vec::new();
        let mut bridge_turn_bound_client_ids: VecDeque<String> = VecDeque::new();
        let mut retry_cycle_active = false;
        let mut translator = PiTranslator::new();
        let mut stream_trace_file = Self::open_stream_trace_file(&session_id).await;

        // Track the last session title broadcast to avoid redundant updates
        let mut last_synced_title = String::new();

        // Whether we've already resolved Pi's native session ID.
        // Sessions keyed directly by a non-oqto ID are already native.
        let mut pi_native_id_known = !session_id.starts_with("oqto-");

        // Mark as Idle after first successful read (Pi is ready)
        let mut first_event_seen = false;

        loop {
            raw_buf.clear();
            let bytes_read = match reader.read_until(b'\n', &mut raw_buf).await {
                Ok(n) => n,
                Err(e) => {
                    warn!("Pi[{}] stdout read error (continuing): {}", session_id, e);
                    continue;
                }
            };
            if bytes_read == 0 {
                break;
            }

            let raw_line = String::from_utf8_lossy(&raw_buf)
                .trim_end_matches(['\n', '\r'])
                .to_string();
            if raw_line.trim().is_empty() {
                continue;
            }

            Self::write_stream_trace(
                stream_trace_file.as_mut(),
                serde_json::json!({
                    "kind": "pi.raw_line",
                    "ts": chrono::Utc::now().timestamp_millis(),
                    "line": raw_line,
                }),
            )
            .await;

            // Update last activity
            *last_activity.write().await = Instant::now();

            // Pi can emit oversized JSON payloads that get split across multiple
            // newline chunks (not valid standalone JSON). Reassemble a pending
            // fragment before parsing.
            let line = if pending_json_fragment.is_empty() {
                raw_line
            } else {
                let mut combined = std::mem::take(&mut pending_json_fragment);
                combined.push_str(raw_line.trim());
                combined
            };

            // Parse the line. Pi may concatenate multiple JSON objects on a
            // single line when its output buffer fills mid-write (e.g. at
            // the 4096-byte boundary). parse_all handles this gracefully.
            let parsed_messages = PiMessage::parse_all(&line);
            if parsed_messages.is_empty() {
                continue;
            }

            // If we got a single parse error and the line looks like a
            // truncated JSON object, keep buffering until the rest arrives.
            // This prevents dropped events when Pi's stdout buffer splits a
            // large JSON line across multiple writes (common with thinking
            // models that produce very long `partial` fields in toolcall_delta
            // events, exceeding 4096/8192/16384 byte boundaries).
            //
            // Fragment indicators:
            //   - "EOF while parsing"       -> input ended mid-value
            //   - "unterminated string"      -> string literal split at boundary
            //   - "expected value at line 1 column 1" -> empty/whitespace line
            //   - "expected `,` or `}`"      -> object split mid-field
            //   - "expected `:`"             -> object split at key-value separator
            //   - "expected `]`"             -> array split at boundary
            //   - "control character..."     -> string with unescaped control char
            //     at boundary
            if parsed_messages.len() == 1
                && let Err(err) = &parsed_messages[0]
            {
                // Heuristic: if the line starts with `{` (looks like a JSON
                // object) and the parse failed, treat it as a fragment. This
                // is more robust than pattern-matching specific serde error
                // messages which vary across versions and edge cases.
                let looks_like_json_start =
                    line.trim_start().starts_with('{') || line.trim_start().starts_with('[');
                let is_known_fragment = err.contains("EOF while parsing")
                    || err.contains("unterminated string")
                    || err.contains("expected value at line 1 column 1");
                if (is_known_fragment || looks_like_json_start) && line.len() < 16 * 1024 * 1024 {
                    if !is_known_fragment {
                        debug!(
                            "Pi[{}] buffering likely truncated JSON ({} bytes): {}",
                            session_id,
                            line.len(),
                            err
                        );
                    }
                    pending_json_fragment = line;
                    continue;
                }
            }

            for parse_result in parsed_messages {
                let msg = match parse_result {
                    Ok(m) => {
                        Self::write_stream_trace(
                            stream_trace_file.as_mut(),
                            serde_json::json!({
                                "kind": "pi.parsed_message",
                                "ts": chrono::Utc::now().timestamp_millis(),
                                "message_debug": format!("{:?}", m),
                            }),
                        )
                        .await;
                        m
                    }
                    Err(e) => {
                        warn!(
                            "Pi[{}] failed to parse message: {} - line: {}",
                            session_id,
                            e,
                            &line[..line.len().min(200)]
                        );
                        continue;
                    }
                };

                // Handle responses vs events
                let pi_event = match msg {
                    PiMessage::Event(e) => *e,
                    PiMessage::Response(response) => {
                        debug!("Pi[{}] response: {:?}", session_id, response);

                        // Intercept get_state responses (including proactive/idle probes)
                        // to capture Pi's native session ID and sync session title.
                        if response
                            .id
                            .as_deref()
                            .is_some_and(|rid| rid.starts_with("get_state"))
                            && let Some(ref data) = response.data
                        {
                            // Capture Pi's native session ID from get_state.
                            // This is the authoritative Pi external_id.
                            if let Some(pi_sid) = data.get("sessionId").and_then(|v| v.as_str())
                                && !pi_sid.is_empty()
                                && !pi_native_id_known
                            {
                                pi_native_id_known = true;
                                let old_eid = session_external_id.read().await.clone();
                                *session_external_id.write().await = pi_sid.to_string();
                                info!(
                                    "Pi[{}] native session ID: {} (external_id: {} -> {})",
                                    session_id, pi_sid, old_eid, pi_sid
                                );

                                if pi_sid != session_id {
                                    let mut aliases = session_aliases.write().await;
                                    Self::record_session_alias(&mut aliases, &session_id, pi_sid);
                                }
                            }

                            // Update active model cache from get_state so list_sessions()
                            // reflects what Pi is actually using (source of truth).
                            let state_provider = data
                                .get("model")
                                .and_then(|m| m.get("provider"))
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string)
                                .or_else(|| {
                                    data.get("provider")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string)
                                });
                            let state_model = data
                                .get("model")
                                .and_then(|m| m.get("id"))
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string)
                                .or_else(|| {
                                    data.get("model")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string)
                                });
                            if let Some(provider) = state_provider {
                                *active_provider.write().await = Some(provider);
                            }
                            if let Some(model_id) = state_model {
                                *active_model.write().await = Some(model_id);
                            }

                            // Pi's auto-rename extension sets sessionName to:
                            //   "<workspace>: <title> [readable-id]"
                            // We parse it to extract the clean title and persist it.
                            if let Some(raw_name) = data.get("sessionName").and_then(|v| v.as_str())
                                && !raw_name.is_empty()
                            {
                                let parsed = oqto_pi::session_parser::ParsedTitle::parse(raw_name);
                                let clean_title = parsed.display_title().to_string();
                                if !clean_title.is_empty() && last_synced_title != clean_title {
                                    last_synced_title = clean_title.clone();

                                    let readable_id = parsed.readable_id.clone();

                                    // Broadcast title change to frontend immediately
                                    let title_event = CanonicalEvent {
                                    session_id: session_id.clone(),
                                    runner_id: runner_id.clone(),
                                    ts: chrono::Utc::now().timestamp_millis(),
                                    payload:
                                        oqto_protocol::events::EventPayload::SessionTitleChanged {
                                            title: clean_title.clone(),
                                            readable_id,
                                        },
                                };
                                    event_tx.publish(&title_event).await;
                                }
                            }
                        }

                        // Route response to waiting caller if there's a matching ID
                        if let Some(ref id) = response.id {
                            let mut pending = pending_responses.write().await;
                            if let Some(tx) = pending.remove(id) {
                                let _ = tx.send(response);
                            }
                        }
                        continue;
                    }
                };

                Self::write_stream_trace(
                    stream_trace_file.as_mut(),
                    serde_json::json!({
                        "kind": "pi.event",
                        "ts": chrono::Utc::now().timestamp_millis(),
                        "event": &pi_event,
                    }),
                )
                .await;

                match &pi_event {
                    PiEvent::AutoRetryStart { .. } => {
                        retry_cycle_active = true;
                    }
                    PiEvent::AutoRetryEnd { success, .. } => {
                        // Keep retry guard active through terminal failure handling.
                        // Some providers emit a final AgentEnd snapshot after
                        // retry.end(success=false), which must not be persisted as
                        // normal assistant text.
                        retry_cycle_active = !*success;
                    }
                    _ => {}
                }

                if let Some(bound_client_id) = Self::parse_oqto_turn_bound_client_id(&pi_event) {
                    bridge_turn_bound_client_ids.push_back(bound_client_id);
                }

                // Update internal state based on Pi event
                let new_state = match &pi_event {
                    PiEvent::AgentStart => {
                        debug!("Pi[{}] AgentStart", session_id);
                        Some(PiSessionState::Streaming)
                    }
                    PiEvent::MessageStart { message }
                        if message.role.eq_ignore_ascii_case("user") =>
                    {
                        // New user prompt boundary: allow exactly one pre-retry
                        // user snapshot persist for this turn.
                        retry_cycle_user_persisted = false;
                        None
                    }
                    PiEvent::AgentEnd { messages } => {
                        debug!(
                            "Pi[{}] AgentEnd with {} messages",
                            session_id,
                            messages.len()
                        );
                        pending_messages = messages
                            .iter()
                            .filter(|msg| !Self::is_empty_assistant_placeholder(msg))
                            .cloned()
                            .collect();
                        pending_agent_end_retry_placeholder =
                            messages.iter().any(Self::is_assistant_error_placeholder)
                                && pending_messages.len() == 1
                                && pending_messages
                                    .first()
                                    .is_some_and(|m| m.role.eq_ignore_ascii_case("user"));
                        Some(PiSessionState::Idle)
                    }
                    PiEvent::AutoCompactionStart { .. } => {
                        debug!("Pi[{}] AutoCompactionStart", session_id);
                        Some(PiSessionState::Compacting)
                    }
                    PiEvent::AutoCompactionEnd { .. } => {
                        debug!("Pi[{}] AutoCompactionEnd", session_id);
                        Some(PiSessionState::Idle)
                    }
                    _ => None,
                };

                if let Some(new_state) = new_state {
                    *state.write().await = new_state;
                }

                // Mark as ready after first event and proactively request get_state
                // to learn Pi's native session ID before any AgentEnd fires.
                if !first_event_seen {
                    first_event_seen = true;
                    let current_state = *state.read().await;
                    if current_state == PiSessionState::Starting {
                        *state.write().await = PiSessionState::Idle;
                    }
                    // Send get_state immediately so we learn Pi's native session ID
                    // before the first AgentEnd triggers oqto-log persistence.
                    if let Err(e) = cmd_tx
                        .send(PiSessionCommand::GetState {
                            request_id: format!(
                                "get_state_proactive_{}",
                                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
                            ),
                        })
                        .await
                    {
                        warn!(
                            "Pi[{}] failed to send proactive get_state: {}",
                            session_id, e
                        );
                    }
                }

                // For AgentEnd, bind client_ids deterministically from oqto-bridge
                // turn_bound events and attach them to user messages in order.
                if matches!(pi_event, PiEvent::AgentEnd { .. }) {
                    if retry_cycle_active {
                        debug!(
                            "Pi[{}] AgentEnd during retry cycle: preserving pending client_id bindings",
                            session_id
                        );
                    } else if pending_agent_end_retry_placeholder && retry_cycle_user_persisted {
                        debug!(
                            "Pi[{}] AgentEnd duplicate pre-retry placeholder: preserving pending client_id bindings",
                            session_id
                        );
                    } else {
                        let user_count = pending_messages
                            .iter()
                            .filter(|m| {
                                m.role.eq_ignore_ascii_case("user")
                                    || m.role.eq_ignore_ascii_case("human")
                            })
                            .count();

                        let mut bound_client_ids: Vec<String> = Vec::with_capacity(user_count);
                        for _ in 0..user_count {
                            if let Some(cid) = bridge_turn_bound_client_ids.pop_front() {
                                bound_client_ids.push(cid);
                            }
                        }

                        // Fallback queue for sessions without oqto-bridge turn_bound events.
                        if bound_client_ids.len() < user_count {
                            let missing = user_count - bound_client_ids.len();
                            let mut pending = pending_client_id.lock().await;
                            for _ in 0..missing {
                                if let Some(cid) = pending.pop_front() {
                                    bound_client_ids.push(cid);
                                }
                            }
                        }

                        if !bound_client_ids.is_empty() {
                            let assigned = Self::attach_client_ids_to_user_messages(
                                &mut pending_messages,
                                &bound_client_ids,
                            );
                            if assigned < bound_client_ids.len() {
                                warn!(
                                    "Pi[{}] client_id binding mismatch: assigned={} queued={} user_count={}",
                                    session_id,
                                    assigned,
                                    bound_client_ids.len(),
                                    user_count
                                );
                            }
                            pending_bound_client_ids = bound_client_ids;
                        } else {
                            pending_bound_client_ids.clear();
                        }

                        translator.set_pending_client_id(pending_bound_client_ids.last().cloned());
                    }
                }

                // Persist to oqto-log on AgentEnd BEFORE broadcasting canonical events.
                // This ensures oqto-log has the complete history before the frontend
                // receives agent.idle and potentially fetches/switches sessions.
                if matches!(pi_event, PiEvent::AgentEnd { .. }) && !pending_messages.is_empty() {
                    if pending_agent_end_retry_placeholder {
                        if retry_cycle_user_persisted {
                            debug!(
                                "Pi[{}] skipping duplicate pre-retry AgentEnd user snapshot",
                                session_id
                            );
                            pending_messages.clear();
                            pending_agent_end_retry_placeholder = false;
                            continue;
                        }
                        pending_messages.retain(|m| m.role.eq_ignore_ascii_case("user"));
                        retry_cycle_user_persisted = true;
                        pending_agent_end_retry_placeholder = false;
                    }

                    if retry_cycle_active {
                        debug!(
                            "Pi[{}] skipping oqto-log persist on AgentEnd during retry cycle ({} message(s))",
                            session_id,
                            pending_messages.len()
                        );
                        pending_messages.clear();
                        continue;
                    }

                    let duplicate_agent_end_delta = {
                        let buffer = message_buffer.read().await;
                        Self::is_duplicate_agent_end_tail(&buffer, &pending_messages)
                    };
                    if duplicate_agent_end_delta {
                        debug!(
                            "Pi[{}] skipping duplicate AgentEnd delta ({} message(s)) already at buffer tail",
                            session_id,
                            pending_messages.len()
                        );
                    }

                    if !duplicate_agent_end_delta && !pi_native_id_known {
                        debug!(
                            "Pi[{}] persisting AgentEnd before native session ID is known; using platform ID as temporary source",
                            session_id
                        );
                    }
                    // Persist to oqto-log (authoritative store).
                    if !duplicate_agent_end_delta && let Ok(home) = std::env::var("HOME") {
                        let pending_messages_for_oqto = pending_messages.clone();
                        let user_id =
                            std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
                        let workspace_id_buf = work_dir.to_string_lossy().to_string();
                        let workspace_id = if workspace_id_buf.trim().is_empty() {
                            "global"
                        } else {
                            workspace_id_buf.as_str()
                        };
                        let source_session_id = if pi_native_id_known {
                            let eid = session_external_id.read().await.clone();
                            if eid.trim().is_empty() {
                                session_id.clone()
                            } else {
                                eid
                            }
                        } else {
                            session_id.clone()
                        };

                        match oqto_history::oqto_log::store::append_agent_end_snapshot(
                            std::path::Path::new(&home),
                            &user_id,
                            workspace_id,
                            &session_id,
                            &session_id,
                            Some(&source_session_id),
                            &source_session_id,
                            &pending_messages_for_oqto,
                        )
                        .await
                        {
                            Ok(stats) => {
                                debug!(
                                    "Pi[{}] oqto-log persist ok: turns_written={} messages_written={} deduped={} snapshot_hash={}",
                                    session_id,
                                    stats.turns_written,
                                    stats.messages_written,
                                    stats.deduped,
                                    stats.snapshot_hash
                                );
                                if let Some(session_file) =
                                    oqto_pi::session_files::find_session_file_async(
                                        source_session_id.clone(),
                                        Some(work_dir.clone()),
                                    )
                                    .await
                                {
                                    let records = tokio::task::spawn_blocking(move || {
                                        read_pi_jsonl_message_records_for_import(&session_file)
                                    })
                                    .await
                                    .unwrap_or_default();
                                    if !records.is_empty() {
                                        match oqto_history::oqto_log::store::replace_session_with_pi_jsonl_records(
                                            std::path::Path::new(&home),
                                            &user_id,
                                            workspace_id,
                                            &session_id,
                                            &session_id,
                                            Some(&source_session_id),
                                            &source_session_id,
                                            &records,
                                        )
                                        .await
                                        {
                                            Ok(replace_stats) => debug!(
                                                "Pi[{}] oqto-log JSONL tail refresh ok: turns_written={} messages_written={}",
                                                session_id,
                                                replace_stats.turns_written,
                                                replace_stats.messages_written
                                            ),
                                            Err(e) => warn!(
                                                "Pi[{}] oqto-log JSONL tail refresh failed: {:?}",
                                                session_id, e
                                            ),
                                        }
                                    }
                                }

                                if let Ok(sess_stats) =
                                    oqto_history::oqto_log::store::read_session_stats(
                                        std::path::Path::new(&home),
                                        workspace_id,
                                        &session_id,
                                    )
                                    .await
                                {
                                    debug!(
                                        "Pi[{}] oqto-log telemetry: candidate_messages={} total_messages={} total_turns={}",
                                        session_id,
                                        pending_messages.len(),
                                        sess_stats.messages,
                                        sess_stats.turns
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("Pi[{}] failed to persist to oqto-log: {:?}", session_id, e);
                            }
                        }
                    }

                    // Update the authoritative message buffer from AgentEnd data.
                    // Pi commonly emits only the turn delta (user+assistant). Append
                    // these to preserve full active-session history; if a larger
                    // snapshot arrives, replace with that snapshot as ground truth.
                    if !duplicate_agent_end_delta {
                        let mut buffer = message_buffer.write().await;
                        if pending_messages.len() <= 2 && !buffer.is_empty() {
                            let start_idx = buffer.len();
                            let delta_msgs: Vec<ChatMessageProto> = pending_messages
                                .iter()
                                .enumerate()
                                .map(|(i, msg)| {
                                    agent_msg_to_chat_proto(msg, start_idx + i, &session_id)
                                })
                                .collect();
                            debug!(
                                "Pi[{}] appending {} AgentEnd delta message(s) to buffer (prev_len={})",
                                session_id,
                                delta_msgs.len(),
                                start_idx
                            );
                            buffer.extend(delta_msgs);
                        } else {
                            let snapshot_msgs: Vec<ChatMessageProto> = pending_messages
                                .iter()
                                .enumerate()
                                .map(|(idx, msg)| agent_msg_to_chat_proto(msg, idx, &session_id))
                                .collect();
                            debug!(
                                "Pi[{}] replacing message buffer with {} AgentEnd snapshot message(s)",
                                session_id,
                                snapshot_msgs.len()
                            );
                            *buffer = snapshot_msgs;
                        }
                    }
                    pending_messages.clear();
                }

                // Translate Pi event to canonical events and broadcast each one.
                // For AgentEnd, oqto-log is already persisted above so the frontend
                // can safely read history on agent.idle.
                let canonical_payloads = translator.translate(&pi_event);
                let ts = chrono::Utc::now().timestamp_millis();
                for payload in &canonical_payloads {
                    let mut enriched_payload = payload.clone();

                    // Persist only terminal (non-recoverable) errors here.
                    // Recoverable errors are represented by the AgentEnd snapshot,
                    // which avoids duplicate durable error rows.
                    if let oqto_protocol::events::EventPayload::AgentError {
                        ref error,
                        recoverable,
                        ..
                    } = enriched_payload
                    {
                        if !recoverable {
                            retry_cycle_active = false;
                        }
                        pending_error_text = if recoverable {
                            None
                        } else {
                            Some(error.clone())
                        };
                        pending_error_recoverable = recoverable;
                        pending_error_persisted_oqto = false;

                        // Persist terminal error to oqto-log so projected durable timeline
                        // includes non-recoverable failures (frontend error row parity).
                        if !recoverable
                            && !pending_error_persisted_oqto
                            && let Ok(home) = std::env::var("HOME")
                        {
                            let user_id =
                                std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
                            let workspace_id_buf = work_dir.to_string_lossy().to_string();
                            let workspace_id = if workspace_id_buf.trim().is_empty() {
                                "global"
                            } else {
                                workspace_id_buf.as_str()
                            };
                            let source_session_id = if pi_native_id_known {
                                let eid = session_external_id.read().await.clone();
                                if eid.trim().is_empty() {
                                    session_id.clone()
                                } else {
                                    eid
                                }
                            } else {
                                session_id.clone()
                            };
                            let error_msg = oqto_pi::AgentMessage {
                                role: "assistant".to_string(),
                                content: serde_json::json!([{
                                    "type": "text",
                                    "text": error.clone()
                                }]),
                                timestamp: Some(
                                    (chrono::Utc::now().timestamp_millis() / 1000) as u64,
                                ),
                                tool_call_id: None,
                                tool_name: None,
                                is_error: Some(true),
                                api: None,
                                provider: None,
                                model: None,
                                usage: None,
                                stop_reason: Some("error".to_string()),
                                extra: std::collections::HashMap::new(),
                            };
                            if let Err(e) =
                                oqto_history::oqto_log::store::append_agent_end_snapshot(
                                    std::path::Path::new(&home),
                                    &user_id,
                                    workspace_id,
                                    &session_id,
                                    &session_id,
                                    Some(&source_session_id),
                                    &source_session_id,
                                    &[error_msg],
                                )
                                .await
                            {
                                warn!(
                                    "Pi[{}] failed to persist terminal error to oqto-log on agent.error: {:?}",
                                    session_id, e
                                );
                            } else {
                                pending_error_persisted_oqto = true;
                            }
                        }
                    }

                    // Fallback persistence at AgentIdle for cases where we buffered
                    // an error before the native Pi session id was known.
                    if matches!(
                        enriched_payload,
                        oqto_protocol::events::EventPayload::AgentIdle { .. }
                    ) {
                        retry_cycle_active = false;
                    }
                    if matches!(
                        enriched_payload,
                        oqto_protocol::events::EventPayload::AgentIdle { .. }
                    ) && let Some(error_text) = pending_error_text.take()
                        && !pending_error_recoverable
                    {
                        if !pending_error_persisted_oqto && let Ok(home) = std::env::var("HOME") {
                            let user_id =
                                std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
                            let workspace_id_buf = work_dir.to_string_lossy().to_string();
                            let workspace_id = if workspace_id_buf.trim().is_empty() {
                                "global"
                            } else {
                                workspace_id_buf.as_str()
                            };
                            let source_session_id = {
                                let eid = session_external_id.read().await.clone();
                                if eid.trim().is_empty() {
                                    session_id.clone()
                                } else {
                                    eid
                                }
                            };
                            let error_msg = oqto_pi::AgentMessage {
                                role: "assistant".to_string(),
                                content: serde_json::json!([{
                                    "type": "text",
                                    "text": error_text.clone()
                                }]),
                                timestamp: Some(
                                    (chrono::Utc::now().timestamp_millis() / 1000) as u64,
                                ),
                                tool_call_id: None,
                                tool_name: None,
                                is_error: Some(true),
                                api: None,
                                provider: None,
                                model: None,
                                usage: None,
                                stop_reason: Some("error".to_string()),
                                extra: std::collections::HashMap::new(),
                            };
                            if let Err(e) =
                                oqto_history::oqto_log::store::append_agent_end_snapshot(
                                    std::path::Path::new(&home),
                                    &user_id,
                                    workspace_id,
                                    &session_id,
                                    &session_id,
                                    Some(&source_session_id),
                                    &source_session_id,
                                    &[error_msg],
                                )
                                .await
                            {
                                warn!(
                                    "Pi[{}] failed to persist buffered error to oqto-log on agent.idle: {:?}",
                                    session_id, e
                                );
                            } else {
                                pending_error_persisted_oqto = true;
                            }
                        }
                    }

                    if matches!(
                        enriched_payload,
                        oqto_protocol::events::EventPayload::AgentIdle { .. }
                    ) {
                        let message_version = if let Ok(home) = std::env::var("HOME") {
                            match oqto_history::oqto_log::projector::read_message_version_auto(
                                std::path::Path::new(&home),
                                &session_id,
                            )
                            .await
                            {
                                Ok(v @ Some(_)) => v,
                                _ => None,
                            }
                        } else {
                            None
                        };
                        enriched_payload =
                            oqto_protocol::events::EventPayload::AgentIdle { message_version };
                    }

                    // Now broadcast the event to subscribers
                    Self::write_stream_trace(
                        stream_trace_file.as_mut(),
                        serde_json::json!({
                            "kind": "runner.canonical_payload",
                            "ts": chrono::Utc::now().timestamp_millis(),
                            "payload": &enriched_payload,
                        }),
                    )
                    .await;

                    let canonical_event = CanonicalEvent {
                        session_id: session_id.clone(),
                        runner_id: runner_id.clone(),
                        ts,
                        payload: enriched_payload,
                    };
                    event_tx.publish(&canonical_event).await;

                    if let oqto_protocol::events::EventPayload::SessionTitleChanged {
                        title, ..
                    } = payload
                        && !title.is_empty()
                    {
                        last_synced_title = title.clone();
                    }
                }

                // Title updates primarily arrive via the auto-rename extension's
                // setStatus("oqto_title_changed", name) which the translator
                // converts to a SessionTitleChanged canonical event. As a
                // fallback for older extension versions or if the extension
                // event was missed, probe get_state after a delay to catch
                // the title from Pi's state.
                if matches!(pi_event, PiEvent::AgentEnd { .. }) {
                    let probe_cmd_tx = cmd_tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        let _ = probe_cmd_tx
                            .send(PiSessionCommand::GetState {
                                request_id: format!(
                                    "get_state_idle_probe_{}",
                                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
                                ),
                            })
                            .await;
                    });
                }
            } // end for parse_result in parsed_messages
        }

        // Process exited -- broadcast error event with stderr context
        info!("Pi[{}] stdout reader finished (process exited)", session_id);

        // Give stderr reader a moment to flush remaining lines
        tokio::time::sleep(Duration::from_millis(100)).await;

        let stderr_lines: Vec<String> = stderr_ring.lock().await.iter().cloned().collect();
        let error_msg = if stderr_lines.is_empty() {
            "Agent process exited".to_string()
        } else {
            // Include last stderr lines in the error for diagnosis
            let stderr_tail = if stderr_lines.len() > 20 {
                &stderr_lines[stderr_lines.len() - 20..]
            } else {
                &stderr_lines
            };
            format!(
                "Agent process exited. Last stderr output:\n{}",
                stderr_tail.join("\n")
            )
        };

        let exit_event = translator.state.on_process_exit(error_msg);
        let canonical_event = CanonicalEvent {
            session_id: session_id.clone(),
            runner_id,
            ts: chrono::Utc::now().timestamp_millis(),
            payload: exit_event,
        };
        event_tx.publish(&canonical_event).await;
        *state.write().await = PiSessionState::Stopping;
    }

    /// Background task that processes commands and writes to stdin.
    async fn command_processor_task(
        session_id: String,
        mut stdin: ChildStdin,
        mut cmd_rx: mpsc::Receiver<PiSessionCommand>,
        state: Arc<RwLock<PiSessionState>>,
        last_activity: Arc<RwLock<Instant>>,
        pending_client_id: PendingClientId,
    ) {
        while let Some(cmd) = cmd_rx.recv().await {
            let result = match cmd {
                PiSessionCommand::Prompt { message, client_id } => {
                    if let Some(cid) = client_id.clone().filter(|v| !v.trim().is_empty()) {
                        pending_client_id.lock().await.push_back(cid);
                    }

                    let current_state = *state.read().await;
                    let streaming_behavior = if current_state == PiSessionState::Streaming {
                        Some("steer".to_string())
                    } else {
                        *state.write().await = PiSessionState::Streaming;
                        None
                    };

                    let outbound_message = Self::append_oqto_meta(
                        message,
                        client_id.as_deref(),
                        streaming_behavior.as_deref().unwrap_or("default"),
                    );

                    let pi_cmd = PiCommand::Prompt {
                        id: None,
                        message: outbound_message,
                        images: None,
                        streaming_behavior,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::Steer {
                    message: msg,
                    client_id,
                } => {
                    debug!(
                        "Pi[{}] cmd_processor: Steer received, client_id = {:?}",
                        session_id, client_id
                    );
                    if let Some(cid) = client_id.clone().filter(|v| !v.trim().is_empty()) {
                        pending_client_id.lock().await.push_back(cid);
                    }

                    let current_state = *state.read().await;
                    if matches!(
                        current_state,
                        PiSessionState::Idle
                            | PiSessionState::Starting
                            | PiSessionState::Stopping
                            | PiSessionState::Aborting
                    ) {
                        *state.write().await = PiSessionState::Streaming;
                    }

                    let outbound_message =
                        Self::append_oqto_meta(msg, client_id.as_deref(), "steer");
                    let pi_cmd = PiCommand::Prompt {
                        id: None,
                        message: outbound_message,
                        images: None,
                        streaming_behavior: Some("steer".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::FollowUp {
                    message: msg,
                    client_id,
                } => {
                    if let Some(cid) = client_id.clone().filter(|v| !v.trim().is_empty()) {
                        pending_client_id.lock().await.push_back(cid);
                    }

                    let current_state = *state.read().await;
                    if matches!(
                        current_state,
                        PiSessionState::Idle
                            | PiSessionState::Starting
                            | PiSessionState::Stopping
                            | PiSessionState::Aborting
                    ) {
                        *state.write().await = PiSessionState::Streaming;
                    }

                    let outbound_message =
                        Self::append_oqto_meta(msg, client_id.as_deref(), "followUp");
                    let pi_cmd = PiCommand::Prompt {
                        id: None,
                        message: outbound_message,
                        images: None,
                        streaming_behavior: Some("followUp".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::Abort => {
                    let current_state = *state.read().await;
                    if current_state == PiSessionState::Streaming
                        || current_state == PiSessionState::Compacting
                    {
                        *state.write().await = PiSessionState::Aborting;
                    }
                    let pi_cmd = PiCommand::Abort { id: None };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::Compact(instructions) => {
                    let pi_cmd = PiCommand::Compact {
                        id: None,
                        custom_instructions: instructions,
                    };
                    *state.write().await = PiSessionState::Compacting;
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetState { request_id } => {
                    // Response coordination happens via pending_responses map
                    let pi_cmd = PiCommand::GetState {
                        id: Some(request_id),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::NewSession(parent) => {
                    let pi_cmd = PiCommand::NewSession {
                        id: Some("new_session".to_string()),
                        parent_session: parent,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetMessages => {
                    // Response coordination happens via pending_responses map
                    let pi_cmd = PiCommand::GetMessages {
                        id: Some("get_messages".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::SetModel { provider, model_id } => {
                    let pi_cmd = PiCommand::SetModel {
                        id: Some("set_model".to_string()),
                        provider,
                        model_id,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetAvailableModels => {
                    // Response coordination happens via pending_responses map
                    let pi_cmd = PiCommand::GetAvailableModels {
                        id: Some("get_available_models".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetSessionStats => {
                    let pi_cmd = PiCommand::GetSessionStats {
                        id: Some("get_session_stats".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Session management
                PiSessionCommand::SwitchSession(session_path) => {
                    let pi_cmd = PiCommand::SwitchSession {
                        id: Some("switch_session".to_string()),
                        session_path,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::SetSessionName(name) => {
                    let pi_cmd = PiCommand::SetSessionName {
                        id: Some("set_session_name".to_string()),
                        name,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::ExportHtml(output_path) => {
                    let pi_cmd = PiCommand::ExportHtml {
                        id: Some("export_html".to_string()),
                        output_path,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // State queries
                PiSessionCommand::GetLastAssistantText => {
                    let pi_cmd = PiCommand::GetLastAssistantText {
                        id: Some("get_last_assistant_text".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetCommands => {
                    let pi_cmd = PiCommand::GetCommands {
                        id: Some("get_commands".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Model configuration
                PiSessionCommand::CycleModel => {
                    let pi_cmd = PiCommand::CycleModel {
                        id: Some("cycle_model".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Thinking configuration
                PiSessionCommand::SetThinkingLevel(level) => {
                    let pi_cmd = PiCommand::SetThinkingLevel {
                        id: Some("set_thinking_level".to_string()),
                        level,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::CycleThinkingLevel => {
                    let pi_cmd = PiCommand::CycleThinkingLevel {
                        id: Some("cycle_thinking_level".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Queue modes
                PiSessionCommand::SetSteeringMode(mode) => {
                    let pi_cmd = PiCommand::SetSteeringMode {
                        id: Some("set_steering_mode".to_string()),
                        mode,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::SetFollowUpMode(mode) => {
                    let pi_cmd = PiCommand::SetFollowUpMode {
                        id: Some("set_follow_up_mode".to_string()),
                        mode,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Compaction
                PiSessionCommand::SetAutoCompaction(enabled) => {
                    let pi_cmd = PiCommand::SetAutoCompaction {
                        id: Some("set_auto_compaction".to_string()),
                        enabled,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Retry
                PiSessionCommand::SetAutoRetry(enabled) => {
                    let pi_cmd = PiCommand::SetAutoRetry {
                        id: Some("set_auto_retry".to_string()),
                        enabled,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::AbortRetry => {
                    let pi_cmd = PiCommand::AbortRetry {
                        id: Some("abort_retry".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Forking
                PiSessionCommand::Fork {
                    entry_id,
                    request_id,
                } => {
                    let pi_cmd = PiCommand::Fork {
                        id: Some(request_id),
                        entry_id,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetForkMessages { request_id } => {
                    let pi_cmd = PiCommand::GetForkMessages {
                        id: Some(request_id),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Bash execution
                PiSessionCommand::Bash(command) => {
                    let pi_cmd = PiCommand::Bash {
                        id: Some("bash".to_string()),
                        command,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::AbortBash => {
                    let pi_cmd = PiCommand::AbortBash {
                        id: Some("abort_bash".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Extension UI
                PiSessionCommand::ExtensionUiResponse {
                    id,
                    value,
                    confirmed,
                    cancelled,
                } => {
                    let pi_cmd = PiCommand::ExtensionUiResponse {
                        id,
                        value,
                        confirmed,
                        cancelled,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Lifecycle
                PiSessionCommand::Close => {
                    info!("Pi[{}] received Close command", session_id);
                    break;
                }
            };

            if let Err(e) = result {
                error!("Pi[{}] failed to write command: {}", session_id, e);
                // If write failed (e.g. broken pipe from dead process), the
                // state may have been set to Streaming/Compacting before the
                // write attempt. Reset to Idle so subsequent commands are not
                // permanently rejected with "not idle (state=streaming)".
                let current = *state.read().await;
                if current != PiSessionState::Idle {
                    warn!(
                        "Pi[{}] resetting state from {:?} to Idle after write failure",
                        session_id, current
                    );
                    *state.write().await = PiSessionState::Idle;
                }
            }

            *last_activity.write().await = Instant::now();
        }

        info!("Pi[{}] command processor finished", session_id);
    }

    /// Write a command to Pi's stdin.
    async fn write_command(stdin: &mut ChildStdin, cmd: &PiCommand) -> Result<()> {
        let json = serde_json::to_string(cmd).context("Failed to serialize command")?;
        stdin
            .write_all(json.as_bytes())
            .await
            .context("Failed to write to stdin")?;
        stdin
            .write_all(b"\n")
            .await
            .context("Failed to write newline")?;
        stdin.flush().await.context("Failed to flush stdin")?;
        Ok(())
    }

    fn is_empty_assistant_placeholder(msg: &AgentMessage) -> bool {
        let is_assistant =
            msg.role.eq_ignore_ascii_case("assistant") || msg.role.eq_ignore_ascii_case("agent");
        if !is_assistant {
            return false;
        }

        match &msg.content {
            serde_json::Value::Array(values) => values.is_empty(),
            serde_json::Value::String(text) => {
                let trimmed = text.trim();
                trimmed.is_empty() || trimmed == "[]"
            }
            serde_json::Value::Null => true,
            _ => false,
        }
    }

    fn is_assistant_error_placeholder(msg: &AgentMessage) -> bool {
        let is_assistant =
            msg.role.eq_ignore_ascii_case("assistant") || msg.role.eq_ignore_ascii_case("agent");
        if !is_assistant {
            return false;
        }
        if msg.stop_reason.as_deref() != Some("error") {
            return false;
        }
        Self::is_empty_assistant_placeholder(msg)
    }

    fn parse_oqto_turn_bound_client_id(pi_event: &PiEvent) -> Option<String> {
        let PiEvent::ExtensionUiRequest(req) = pi_event else {
            return None;
        };
        if req.method != "setStatus" || req.status_key.as_deref() != Some("oqto_queue_event") {
            return None;
        }
        let raw = req.status_text.as_deref()?;
        let parsed: OqtoQueueEvent = serde_json::from_str(raw).ok()?;
        if parsed.event_type != "turn_bound" {
            return None;
        }
        parsed
            .client_id
            .and_then(|cid| (!cid.trim().is_empty()).then_some(cid))
    }

    fn attach_client_ids_to_user_messages(
        messages: &mut [AgentMessage],
        ordered_client_ids: &[String],
    ) -> usize {
        let mut assigned = 0usize;
        let user_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter_map(|(idx, msg)| {
                (msg.role.eq_ignore_ascii_case("user") || msg.role.eq_ignore_ascii_case("human"))
                    .then_some(idx)
            })
            .collect();

        // Bind client_ids to the LAST N user messages (the most recent turn's
        // prompts), not the first N. The `ordered_client_ids` represents the
        // current turn's bindings; earlier user messages in the buffer are
        // historical and already persisted. Attaching to the tail ensures the
        // new user message gets its client_id, enabling frontend dedup on reload.
        let start = user_indices.len().saturating_sub(ordered_client_ids.len());
        for (i, msg_idx) in user_indices.iter().enumerate().skip(start) {
            let cid_idx = i - start;
            let Some(client_id) = ordered_client_ids.get(cid_idx) else {
                break;
            };
            if let Some(msg) = messages.get_mut(*msg_idx) {
                msg.extra.insert(
                    "client_id".to_string(),
                    serde_json::Value::String(client_id.clone()),
                );
                assigned += 1;
            }
        }

        assigned
    }

    fn build_oqto_meta_suffix(client_id: Option<&str>, intent: &str) -> Option<String> {
        let cid = client_id?.trim();
        if cid.is_empty() {
            return None;
        }

        let normalized_intent = match intent {
            "steer" => "steer",
            "followUp" => "followUp",
            _ => "default",
        };

        let meta = serde_json::json!({
            "clientId": cid,
            "intent": normalized_intent,
        });
        Some(format!(" [[oqto_meta:{}]]", meta))
    }

    fn append_oqto_meta(message: String, client_id: Option<&str>, intent: &str) -> String {
        let Some(suffix) = Self::build_oqto_meta_suffix(client_id, intent) else {
            return message;
        };
        if message.contains("[[oqto_meta:") {
            return message;
        }
        format!("{}{}", message, suffix)
    }

    fn stream_trace_enabled() -> bool {
        std::env::var("OQTO_TRACE_STREAMS")
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }

    fn sanitize_for_filename(input: &str) -> String {
        input
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    async fn open_stream_trace_file(session_id: &str) -> Option<tokio::fs::File> {
        if !Self::stream_trace_enabled() {
            return None;
        }

        let dir = std::env::var("OQTO_TRACE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp/oqto-stream-traces"));

        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            warn!("Failed to create stream trace dir {:?}: {}", dir, e);
            return None;
        }

        let filename = format!(
            "{}_{}.jsonl",
            Self::sanitize_for_filename(session_id),
            chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ")
        );
        let path = dir.join(filename);

        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
        {
            Ok(file) => {
                info!(
                    "Pi[{}] stream tracing enabled: {}",
                    session_id,
                    path.display()
                );
                Some(file)
            }
            Err(e) => {
                warn!("Failed to open stream trace file {:?}: {}", path, e);
                None
            }
        }
    }

    async fn write_stream_trace(file: Option<&mut tokio::fs::File>, value: serde_json::Value) {
        if let Some(file) = file {
            let mut line = match serde_json::to_vec(&value) {
                Ok(v) => v,
                Err(e) => {
                    warn!("Failed to serialize stream trace event: {}", e);
                    return;
                }
            };
            line.push(b'\n');
            if let Err(e) = file.write_all(&line).await {
                warn!("Failed to write stream trace event: {}", e);
            }
        }
    }

    fn chat_message_dedupe_signature(message: &ChatMessageProto) -> String {
        let mut parts = Vec::with_capacity(message.parts.len());
        for part in &message.parts {
            parts.push(serde_json::json!({
                "type": part.part_type,
                "text": part.text,
                "tool_name": part.tool_name,
                "tool_call_id": part.tool_call_id,
                "tool_input": part.tool_input,
                "tool_output": part.tool_output,
                "tool_status": part.tool_status,
            }));
        }

        serde_json::to_string(&serde_json::json!({
            "role": message.role.to_ascii_lowercase(),
            "client_id": message.client_id,
            "model_id": message.model_id,
            "provider_id": message.provider_id,
            "parts": parts,
        }))
        .unwrap_or_default()
    }

    fn agent_message_dedupe_signature(message: &AgentMessage) -> String {
        let proto = agent_msg_to_chat_proto(message, 0, "dedupe");
        Self::chat_message_dedupe_signature(&proto)
    }

    fn is_duplicate_agent_end_tail(buffer: &[ChatMessageProto], messages: &[AgentMessage]) -> bool {
        if buffer.is_empty() || messages.is_empty() || messages.len() > buffer.len() {
            return false;
        }

        let tail = &buffer[buffer.len() - messages.len()..];
        tail.iter()
            .zip(messages.iter())
            .all(|(existing, incoming)| {
                Self::chat_message_dedupe_signature(existing)
                    == Self::agent_message_dedupe_signature(incoming)
            })
    }

    async fn resolve_jsonl_session_title(session_id: &str, work_dir: &Path) -> Option<String> {
        let session_file = oqto_pi::session_files::find_session_file_async(
            session_id.to_string(),
            Some(work_dir.to_path_buf()),
        )
        .await?;

        tokio::task::spawn_blocking(move || read_jsonl_session_name(session_file))
            .await
            .ok()
            .and_then(|result| result.ok())?
    }

    fn compute_stats_delta(messages: &[AgentMessage]) -> Option<StatsDelta> {
        let mut delta = StatsDelta::default();
        let mut saw_usage = false;

        for msg in messages {
            if let Some(usage) = msg.usage.as_ref() {
                saw_usage = true;
                delta.tokens_in += usage.input as i64;
                delta.tokens_out += usage.output as i64;
                delta.cache_read += usage.cache_read as i64;
                delta.cache_write += usage.cache_write as i64;
                if let Some(cost) = usage.cost.as_ref() {
                    delta.cost_usd += cost.total;
                }
            }
        }

        if saw_usage { Some(delta) } else { None }
    }
}

#[derive(Debug, Deserialize)]
struct JsonlSessionInfoEntry {
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct PiJsonlMessageEntryForImport {
    #[serde(rename = "type")]
    entry_type: String,
    id: Option<String>,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    message: Option<AgentMessage>,
}

fn read_pi_jsonl_message_records_for_import(
    path: &std::path::Path,
) -> Vec<oqto_history::oqto_log::store::PiJsonlMessageRecord> {
    use std::io::BufRead;

    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let reader = std::io::BufReader::new(file);
    let mut records = Vec::new();
    for (line_idx, line) in reader.lines().map_while(Result::ok).enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<PiJsonlMessageEntryForImport>(trimmed) else {
            continue;
        };
        if entry.entry_type != "message" {
            continue;
        }
        if let Some(message) = entry.message {
            records.push(oqto_history::oqto_log::store::PiJsonlMessageRecord {
                source_entry_id: entry.id.unwrap_or_else(|| format!("line:{line_idx}")),
                parent_source_entry_id: entry.parent_id,
                source_sequence: line_idx as i64,
                message,
            });
        }
    }
    records
}

fn read_jsonl_session_name(path: PathBuf) -> Result<Option<String>> {
    use std::io::BufRead;

    let file = std::fs::File::open(&path)
        .with_context(|| format!("Failed to open Pi session file {}", path.display()))?;
    let reader = std::io::BufReader::new(file);
    let mut last_name: Option<String> = None;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let entry: JsonlSessionInfoEntry = match serde_json::from_str(&line) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };

        if entry.entry_type != "session_info" {
            continue;
        }

        if let Some(name) = entry.name {
            last_name = Some(name);
        }
    }

    Ok(last_name)
}

fn model_available_for_provider(
    models: &serde_json::Value,
    provider: &str,
    model_id: &str,
) -> bool {
    let Some(arr) = models.as_array() else {
        return false;
    };

    arr.iter().any(|model| {
        let id = model.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        let p = model
            .get("provider")
            .or_else(|| model.get("providerId"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        id == model_id && p == provider
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    enum PersistenceContractStep {
        PersistCommit,
        AgentIdleBroadcast,
    }

    fn idle_emitted_before_persist(steps: &[PersistenceContractStep]) -> bool {
        let mut persist_seen = false;
        for step in steps {
            match step {
                PersistenceContractStep::PersistCommit => persist_seen = true,
                PersistenceContractStep::AgentIdleBroadcast if !persist_seen => return true,
                PersistenceContractStep::AgentIdleBroadcast => return false,
            }
        }
        false
    }

    #[test]
    fn model_available_for_provider_matches_provider_and_model_id() {
        let models = serde_json::json!([
            {"id": "Kimi-K2.6", "provider": "Foundry_Kimi"},
            {"id": "gpt-5", "providerId": "openai"}
        ]);

        assert!(model_available_for_provider(
            &models,
            "Foundry_Kimi",
            "Kimi-K2.6"
        ));
        assert!(model_available_for_provider(&models, "openai", "gpt-5"));
        assert!(!model_available_for_provider(
            &models,
            "openai",
            "Kimi-K2.6"
        ));
    }

    #[test]
    fn model_available_for_provider_rejects_non_array() {
        let models = serde_json::json!({"models": []});
        assert!(!model_available_for_provider(&models, "openai", "gpt-5"));
    }

    #[test]
    fn test_session_state_display() {
        assert_eq!(PiSessionState::Starting.to_string(), "starting");
        assert_eq!(PiSessionState::Idle.to_string(), "idle");
        assert_eq!(PiSessionState::Streaming.to_string(), "streaming");
        assert_eq!(PiSessionState::Compacting.to_string(), "compacting");
        assert_eq!(PiSessionState::Aborting.to_string(), "aborting");
        assert_eq!(PiSessionState::Stopping.to_string(), "stopping");
    }

    #[test]
    fn test_default_config() {
        let config = PiManagerConfig::default();
        let pi = config.pi_binary.to_string_lossy().to_string();
        assert!(
            pi.ends_with("/pi"),
            "pi_binary should resolve to a pi executable, got: {pi}"
        );
        assert_eq!(config.idle_timeout_secs, 300);
        assert_eq!(config.cleanup_interval_secs, 60);
    }

    #[test]
    fn test_session_config_default() {
        let config = PiSessionConfig::default();
        assert!(config.provider.is_none());
        assert!(config.model.is_none());
        assert!(config.continue_session.is_none());
        assert!(config.env.is_empty());
    }

    #[test]
    fn test_session_state_serialization() {
        let state = PiSessionState::Streaming;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"streaming\"");

        let parsed: PiSessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PiSessionState::Streaming);
    }

    fn test_agent_message(role: &str, text: &str, client_id: Option<&str>) -> AgentMessage {
        let mut extra = std::collections::HashMap::new();
        if let Some(client_id) = client_id {
            extra.insert("client_id".to_string(), serde_json::json!(client_id));
        }
        AgentMessage {
            role: role.to_string(),
            content: serde_json::json!([{ "type": "text", "text": text }]),
            timestamp: Some(1_777_287_200),
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: None,
            provider: Some("provider-a".to_string()),
            model: Some("model-a".to_string()),
            usage: None,
            stop_reason: None,
            extra,
        }
    }

    #[test]
    fn duplicate_agent_end_tail_detects_replayed_assistant_delta() {
        let incoming = vec![test_agent_message("assistant", "same response", None)];
        let buffer = vec![agent_msg_to_chat_proto(&incoming[0], 0, "sess")];

        assert!(PiSessionManager::is_duplicate_agent_end_tail(
            &buffer, &incoming
        ));
    }

    #[test]
    fn duplicate_agent_end_tail_keeps_repeated_user_turn_with_new_client_id() {
        let prior = [
            test_agent_message("user", "repeat", Some("client-a")),
            test_agent_message("assistant", "same response", None),
        ];
        let incoming = vec![
            test_agent_message("user", "repeat", Some("client-b")),
            test_agent_message("assistant", "same response", None),
        ];
        let buffer = prior
            .iter()
            .enumerate()
            .map(|(idx, msg)| agent_msg_to_chat_proto(msg, idx, "sess"))
            .collect::<Vec<_>>();

        assert!(!PiSessionManager::is_duplicate_agent_end_tail(
            &buffer, &incoming
        ));
    }

    #[test]
    fn persistence_contracts_flags_idle_before_persist_commit() {
        let steps = [
            PersistenceContractStep::AgentIdleBroadcast,
            PersistenceContractStep::PersistCommit,
        ];
        assert!(idle_emitted_before_persist(&steps));
    }

    #[test]
    fn persistence_contracts_accepts_idle_after_persist_commit() {
        let steps = [
            PersistenceContractStep::PersistCommit,
            PersistenceContractStep::AgentIdleBroadcast,
        ];
        assert!(!idle_emitted_before_persist(&steps));
    }

    #[test]
    fn session_identity_isolation_alias_remap_replaces_stale_external_id() {
        let mut aliases = HashMap::new();
        PiSessionManager::record_session_alias(&mut aliases, "oqto-parent", "pi-old");
        PiSessionManager::record_session_alias(&mut aliases, "oqto-parent", "pi-new");

        assert_eq!(aliases.get("pi-new"), Some(&"oqto-parent".to_string()));
        assert!(!aliases.contains_key("pi-old"));
    }

    #[test]
    fn session_identity_isolation_child_alias_does_not_clobber_parent_alias() {
        let mut aliases = HashMap::new();
        PiSessionManager::record_session_alias(&mut aliases, "oqto-parent", "pi-parent");
        PiSessionManager::record_session_alias(&mut aliases, "oqto-child", "pi-child");

        assert_eq!(aliases.get("pi-parent"), Some(&"oqto-parent".to_string()));
        assert_eq!(aliases.get("pi-child"), Some(&"oqto-child".to_string()));
    }

    #[test]
    fn session_identity_isolation_drop_aliases_prunes_only_target_platform() {
        let mut aliases = HashMap::new();
        PiSessionManager::record_session_alias(&mut aliases, "oqto-a", "pi-a");
        PiSessionManager::record_session_alias(&mut aliases, "oqto-b", "pi-b");

        PiSessionManager::drop_session_aliases_for_platform(&mut aliases, "oqto-a", "pi-a");

        assert!(!aliases.contains_key("pi-a"));
        assert_eq!(aliases.get("pi-b"), Some(&"oqto-b".to_string()));
    }
}

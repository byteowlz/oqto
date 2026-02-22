//! Pi Session Manager for oqto-runner.
//!
//! Manages multiple Pi agent processes with:
//! - Process lifecycle (spawn, shutdown)
//! - Command routing (prompt, steer, follow_up, abort, compact)
//! - Event broadcasting to subscribers
//! - State tracking (Starting, Idle, Streaming, Compacting, Stopping)
//! - Idle session cleanup
//! - Persistence to hstry on AgentEnd

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result};
use futures::FutureExt;
use chrono::DateTime;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{RwLock, broadcast, mpsc, oneshot};

use crate::agent_browser::{agent_browser_session_dir, browser_session_name};
use crate::hstry::HstryClient;
use crate::local::SandboxConfig;
use crate::pi::{
    AgentMessage, PiCommand, PiEvent, PiMessage, PiResponse, PiState, SessionStats,
    session_parser::ParsedTitle,
};
use crate::runner::pi_translator::PiTranslator;
use crate::runner::protocol::{PiSessionInfo, PiSessionState};
use oqto_protocol::events::Event as CanonicalEvent;

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
    /// Path to hstry database (for direct writes).
    pub hstry_db_path: Option<PathBuf>,
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
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".local").join("share"));
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
            hstry_db_path: Some(data_dir.join("hstry").join("hstry.db")),
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
}

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
    Steer(String),
    /// Send a follow-up message (queue for after completion).
    FollowUp(String),
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
    GetState,
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
    // Internal: Incremental hstry persistence
    // ========================================================================
    /// Request Pi's current messages for incremental hstry persistence.
    /// Triggered on TurnEnd events so hstry stays current during streaming.
    /// Response is intercepted by the reader task (not routed to callers).
    IncrementalPersist,

    // ========================================================================
    // Forking (response via pending_responses)
    // ========================================================================
    /// Fork from a previous user message.
    Fork(String),
    /// Get messages available for forking.
    GetForkMessages,

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

/// Canonical event wrapper for the broadcast channel.
///
/// The pi_manager translates native Pi events into canonical events using
/// `PiTranslator` and broadcasts them. One native Pi event may produce
/// multiple canonical events, so each is broadcast individually.
pub type PiEventWrapper = CanonicalEvent;

// ============================================================================
// Internal Session Structure
// ============================================================================

/// Pending response waiters - maps request ID to oneshot sender.
type PendingResponses = Arc<RwLock<HashMap<String, oneshot::Sender<PiResponse>>>>;

/// Pending client_id for optimistic message matching.
/// Set by command_processor_task when a Prompt is sent, consumed by stdout_reader_task
/// when translating the agent_end messages.
type PendingClientId = Arc<RwLock<Option<String>>>;

/// The hstry external_id for a session.
///
/// Starts as the Oqto UUID (for optimistic session creation), then gets
/// updated to Pi's native session ID once `get_state` returns it. All hstry
/// reads/writes should use this value, not the Oqto session_id directly.
type HstryExternalId = Arc<RwLock<String>>;

/// Internal session state (held by the manager).
struct PiSession {
    /// Session ID (Oqto UUID -- the routing key used by frontend/API).
    id: String,
    /// Session configuration.
    #[allow(dead_code)]
    config: PiSessionConfig,
    /// Child process.
    process: Child,
    /// Current state.
    state: Arc<RwLock<PiSessionState>>,
    /// The external_id used in hstry for this session. Initially the Oqto UUID,
    /// updated to Pi's native session ID once known via `get_state`.
    hstry_external_id: HstryExternalId,
    /// Last activity timestamp.
    last_activity: Instant,
    /// Broadcast channel for events.
    event_tx: broadcast::Sender<PiEventWrapper>,
    /// Command sender to the session task.
    cmd_tx: mpsc::Sender<PiSessionCommand>,
    /// Pending response waiters (shared with reader task).
    pending_responses: PendingResponses,
    /// Pending client_id for the next prompt (shared between command and reader tasks).
    #[allow(dead_code)]
    pending_client_id: PendingClientId,
    /// Handle to the background reader task.
    _reader_handle: tokio::task::JoinHandle<()>,
    /// Handle to the command processor task.
    _cmd_handle: tokio::task::JoinHandle<()>,
}

impl PiSession {
    fn subscriber_count(&self) -> usize {
        self.event_tx.receiver_count()
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
/// - Persistence to hstry
pub struct PiSessionManager {
    /// Active sessions.
    sessions: RwLock<HashMap<String, PiSession>>,
    /// Manager configuration.
    config: PiManagerConfig,
    /// Shutdown signal sender.
    shutdown_tx: broadcast::Sender<()>,
    /// hstry gRPC client for persisting chat history.
    hstry_client: Option<HstryClient>,
    /// Sessions currently being created (guards against concurrent creation).
    /// Holds session IDs that are in the process of being spawned but not yet
    /// inserted into the `sessions` map. Prevents the TOQTOU race in
    /// `get_or_create_session` where two concurrent callers both pass the
    /// `contains_key` check and each spawn a separate Pi process.
    creating: tokio::sync::Mutex<std::collections::HashSet<String>>,
    /// Cached model lists per workdir (populated when any session in that workdir fetches models).
    /// Key: canonical workdir path, Value: list of available models as JSON.
    model_cache: RwLock<HashMap<String, serde_json::Value>>,
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

        // Always create the hstry gRPC client. The connection is established lazily.
        // Previously this was gated on hstry_db_path existing, but the DB file may not
        // exist yet at runner startup (race with hstry service). The gRPC client streams
        // data to the hstry service which creates the DB independently.
        let hstry_client = Some(HstryClient::new());

        // Load persisted model cache from disk
        let model_cache = Self::load_model_cache_from_disk(config.model_cache_dir.as_deref());

        Arc::new(Self {
            sessions: RwLock::new(HashMap::new()),
            config,
            shutdown_tx,
            hstry_client,
            creating: tokio::sync::Mutex::new(std::collections::HashSet::new()),
            model_cache: RwLock::new(model_cache),
            models_json_mtime: RwLock::new(None),
            session_aliases: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get a reference to the hstry client (if available).
    pub fn hstry_client(&self) -> Option<&HstryClient> {
        self.hstry_client.as_ref()
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
        // an Oqto ID (oqto-...). If direct lookup fails, resolve the Pi
        // native ID from hstry's external_id and retry.
        let continue_session = if config.continue_session.is_some() {
            config.continue_session.clone()
        } else {
            // Try direct match first (works for Pi-native IDs)
            let mut found =
                crate::pi::session_files::find_session_file(&session_id, Some(&config.cwd));
            // No match -- resolve Pi native ID via hstry and retry
            if found.is_none()
                && let Some(ref client) = self.hstry_client
            {
                match client.get_conversation(&session_id, None).await {
                    Ok(Some(conv)) => {
                        let pi_id = conv.external_id;
                        if !pi_id.is_empty() && pi_id != session_id {
                            debug!(
                                "Resolved Pi native ID for '{}' -> '{}' via hstry",
                                session_id, pi_id
                            );
                            found = crate::pi::session_files::find_session_file(
                                &pi_id,
                                Some(&config.cwd),
                            );
                        }
                    }
                    Ok(None) => {
                        debug!("No hstry conversation found for session '{}'", session_id);
                    }
                    Err(e) => {
                        warn!("Failed to resolve Pi native ID for '{}': {}", session_id, e);
                    }
                }
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
        // Per-session event broadcast. Each subscriber (browser tab) gets its
        // own receiver. If a subscriber falls behind (e.g., browser tab in
        // background, backend stalled on heavy query), its receiver gets
        // Lagged(n) and the runner emits stream.resync_required.
        // 1024 gives ~3-5 seconds of burst capacity during fast text_delta
        // streaming before lag occurs.
        let (event_tx, _) = broadcast::channel::<PiEventWrapper>(1024);
        let (cmd_tx, cmd_rx) = mpsc::channel::<PiSessionCommand>(32);

        // Shared state for the session
        let state = Arc::new(RwLock::new(PiSessionState::Starting));
        let last_activity = Arc::new(RwLock::new(Instant::now()));
        let pending_responses: PendingResponses = Arc::new(RwLock::new(HashMap::new()));
        // Pending client_id for optimistic message matching (shared between command and reader tasks)
        let pending_client_id: PendingClientId = Arc::new(RwLock::new(None));
        // hstry external_id -- starts as Oqto UUID, updated to Pi native ID by reader task
        let hstry_external_id: HstryExternalId = Arc::new(RwLock::new(session_id.clone()));

        // Spawn stdout reader task
        let reader_handle = {
            let session_id = session_id.clone();
            let event_tx = event_tx.clone();
            let state = Arc::clone(&state);
            let last_activity = Arc::clone(&last_activity);
            let hstry_client = self.hstry_client.clone();
            let work_dir = config.cwd.clone();
            let pending_responses = Arc::clone(&pending_responses);
            let pending_client_id = Arc::clone(&pending_client_id);
            let cmd_tx_for_reader = cmd_tx.clone();
            let hstry_eid = Arc::clone(&hstry_external_id);
            let session_aliases = Arc::clone(&self.session_aliases);

            let runner_id = self.config.runner_id.clone();
            tokio::spawn(async move {
                Self::stdout_reader_task(
                    session_id,
                    stdout,
                    stderr,
                    event_tx,
                    state,
                    last_activity,
                    hstry_client,
                    work_dir,
                    pending_responses,
                    pending_client_id,
                    cmd_tx_for_reader,
                    hstry_eid,
                    session_aliases,
                    runner_id,
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

        // Store the session (stdin is owned by the command processor task)
        let session = PiSession {
            id: session_id.clone(),
            config,
            process: child,
            state: Arc::clone(&state),
            hstry_external_id,
            last_activity: Instant::now(),
            event_tx,
            cmd_tx,
            pending_responses,
            pending_client_id,
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
        let result = self.create_session(session_id.to_string(), config).await;

        // Remove from creation set regardless of success/failure.
        {
            let mut creating = self.creating.lock().await;
            creating.remove(session_id);
        }

        result
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
        self.send_command(session_id, PiSessionCommand::Steer(message.to_string()))
            .await
    }

    /// Send a follow-up message to a session.
    pub async fn follow_up(&self, session_id: &str, message: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::FollowUp(message.to_string()))
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
    pub async fn subscribe(&self, session_id: &str) -> Result<broadcast::Receiver<PiEventWrapper>> {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&resolved_id)
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;

        Ok(session.event_tx.subscribe())
    }

    /// List all sessions.
    ///
    /// Returns the Oqto session ID (the key in the sessions map) as the
    /// session_id.  This is the same ID used in broadcast events and the
    /// one the frontend should use for all commands.  The Pi native ID is
    /// an internal detail stored in hstry.
    pub async fn list_sessions(&self) -> Vec<PiSessionInfo> {
        let snapshots: Vec<(
            String,
            Arc<RwLock<String>>,
            Arc<RwLock<PiSessionState>>,
            Instant,
            usize,
            PathBuf,
            Option<String>,
            Option<String>,
        )> = {
            let sessions = self.sessions.read().await;
            sessions
                .values()
                .map(|s| {
                    (
                        s.id.clone(),
                        Arc::clone(&s.hstry_external_id),
                        Arc::clone(&s.state),
                        s.last_activity,
                        s.subscriber_count(),
                        s.config.cwd.clone(),
                        s.config.provider.clone(),
                        s.config.model.clone(),
                    )
                })
                .collect()
        };

        let mut infos = Vec::with_capacity(snapshots.len());
        for (id, external_id, state, last_activity, subscriber_count, cwd, provider, model) in
            snapshots
        {
            let current_state = *state.read().await;
            let eid = external_id.read().await.clone();
            let hstry_id = if eid.is_empty() || eid == id {
                None
            } else {
                Some(eid)
            };
            infos.push(PiSessionInfo {
                session_id: id,
                hstry_id,
                state: current_state,
                last_activity: {
                    // Convert Instant to Unix timestamp in milliseconds.
                    // last_activity is an Instant of when the activity happened;
                    // elapsed() gives duration since then. Subtract from now.
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

    /// Resolve the hstry external_id for a session.
    ///
    /// Returns Pi's native session ID if known, otherwise the Oqto UUID.
    /// This should be used for all hstry lookups instead of the raw session_id.
    pub async fn hstry_external_id(&self, session_id: &str) -> String {
        let resolved_id = self
            .resolve_session_key(session_id)
            .await
            .unwrap_or_else(|| session_id.to_string());
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(&resolved_id) {
            session.hstry_external_id.read().await.clone()
        } else {
            // Session not running -- fall back to session_id (Oqto UUID).
            // hstry will try matching against external_id, readable_id, and id.
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

        let request_id = "get_state".to_string();

        // Register waiter before sending command
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send the command
        cmd_tx
            .send(PiSessionCommand::GetState)
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

        if let Some((pending_responses, cmd_tx, workdir)) = session_info {
            // Live session exists — ask Pi directly
            let (tx, rx) = oneshot::channel();
            {
                let mut pending = pending_responses.write().await;
                pending.insert(request_id.clone(), tx);
            }

            cmd_tx
                .send(PiSessionCommand::GetAvailableModels)
                .await
                .context("Failed to send GetAvailableModels command")?;

            let response = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .context("Timeout waiting for GetAvailableModels response")?
                .context("Response channel closed")?;

            if !response.success {
                anyhow::bail!(
                    "GetAvailableModels failed: {}",
                    response
                        .error
                        .unwrap_or_else(|| "unknown error".to_string())
                );
            }

            let data = response
                .data
                .ok_or_else(|| anyhow::anyhow!("GetAvailableModels response missing data"))?;

            // Pi returns { "models": [...] }, extract the inner array
            let models_array = if let Some(inner) = data.get("models") {
                inner.clone()
            } else if data.is_array() {
                data.clone()
            } else {
                warn!("GetAvailableModels: unexpected data shape, returning empty");
                serde_json::Value::Array(vec![])
            };

            // Cache by workdir so dead sessions can still list models, and persist to disk
            if models_array.as_array().is_some_and(|a| !a.is_empty()) {
                {
                    let mut cache = self.model_cache.write().await;
                    cache.insert(workdir.clone(), models_array.clone());
                }
                Self::persist_model_cache_to_disk(
                    self.config.model_cache_dir.as_deref(),
                    &workdir,
                    &models_array,
                );
            }

            Ok(models_array)
        } else {
            // No live session — check if models.json changed since last read,
            // then try cache, then read from disk.
            // Invalidate cache when the admin syncs models (file mtime changes).
            if self.models_json_changed().await {
                info!("models.json changed on disk, invalidating model cache");
                self.model_cache.write().await.clear();
            }

            let resolved_workdir = workdir
                .and_then(|value| {
                    let trimmed = value.trim();
                    (!trimmed.is_empty()).then_some(trimmed.to_string())
                })
                .or({
                    // Try to resolve workdir from the session via hstry (blocking-safe
                    // because we're already in an async context)
                    None
                });

            if let Some(ref wd) = resolved_workdir
                && let Some(models) = self.get_cached_models_for_workdir(wd).await
            {
                return Ok(models);
            }

            // Try hstry lookup to find workdir
            let hstry_workdir = self.resolve_workdir_from_hstry(session_id).await;
            if let Some(ref wd) = hstry_workdir
                && let Some(models) = self.get_cached_models_for_workdir(wd).await
            {
                return Ok(models);
            }

            // No cache hit — read models from two sources and merge:
            //  1. models.json on disk (eavs-provisioned, instant)
            //  2. Ephemeral Pi RPC (user OAuth/API key models, best-effort)
            let target_workdir = resolved_workdir
                .or(hstry_workdir)
                .unwrap_or_else(|| self.config.default_cwd.to_string_lossy().to_string());

            let disk_models = self.load_models_from_disk().await;

            // Best-effort: try ephemeral Pi to pick up user-authenticated models
            // (OAuth logins, personal API keys in auth.json). Short timeout — if
            // Pi fails, the eavs-provisioned models are still returned.
            let pi_models = match tokio::time::timeout(
                Duration::from_secs(15),
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

            // Merge: disk models first, then Pi models that aren't already present
            let models = Self::merge_model_lists(&disk_models, &pi_models);

            if models.as_array().is_some_and(|a| !a.is_empty()) {
                {
                    let mut cache = self.model_cache.write().await;
                    cache.insert(target_workdir.clone(), models.clone());
                }
                Self::persist_model_cache_to_disk(
                    self.config.model_cache_dir.as_deref(),
                    &target_workdir,
                    &models,
                );
            }
            Ok(models)
        }
    }

    /// Resolve the workdir for a session by looking it up in hstry.
    async fn resolve_workdir_from_hstry(&self, session_id: &str) -> Option<String> {
        let hstry = self.hstry_client.as_ref()?;
        let eid = self.hstry_external_id(session_id).await;
        if let Ok(Some(session)) =
            crate::history::repository::get_session_via_grpc(hstry, &eid).await
        {
            let wd = session.workspace_path;
            if !wd.is_empty() {
                return Some(wd);
            }
        }
        None
    }

    /// Get cached models for a specific workdir (called directly by runner for dead sessions).
    pub async fn get_cached_models_for_workdir(&self, workdir: &str) -> Option<serde_json::Value> {
        let cache = self.model_cache.read().await;
        cache.get(workdir).cloned()
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
    ) -> HashMap<String, serde_json::Value> {
        let Some(dir) = cache_dir else {
            return HashMap::new();
        };
        let mut map = HashMap::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return map;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match std::fs::read_to_string(&path) {
                    Ok(contents) => match serde_json::from_str::<ModelCacheEntry>(&contents) {
                        Ok(entry) => {
                            info!(
                                "Loaded cached models for workdir '{}' ({} models)",
                                entry.workdir,
                                entry.models.as_array().map(|a| a.len()).unwrap_or(0)
                            );
                            map.insert(entry.workdir, entry.models);
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
        let entry = ModelCacheEntry {
            workdir: workdir.to_string(),
            models: models.clone(),
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
            (None, _) => false, // First call — not a change, cache may be from disk
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
        if let Ok(meta) = std::fs::metadata(&models_path) {
            if let Ok(mtime) = meta.modified() {
                *self.models_json_mtime.write().await = Some(mtime);
            }
        }

        let content = match std::fs::read_to_string(&models_path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Could not read models.json at {}: {}", models_path.display(), e);
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
                let api = provider_config.get("api").and_then(|a| a.as_str()).unwrap_or("openai-completions");
                let base_url = provider_config.get("baseUrl").and_then(|u| u.as_str()).unwrap_or("");
                let api_key_env = provider_config.get("apiKey").and_then(|k| k.as_str()).unwrap_or("");

                if let Some(models) = provider_config.get("models").and_then(|m| m.as_array()) {
                    for model in models {
                        // Each model in models.json already has id, name, reasoning,
                        // contextWindow, maxTokens, cost, etc. We add provider metadata
                        // so the frontend can display and route correctly.
                        let mut enriched = model.clone();
                        if let Some(obj) = enriched.as_object_mut() {
                            obj.entry("provider".to_string())
                                .or_insert_with(|| serde_json::Value::String(provider_name.clone()));
                            obj.entry("api".to_string())
                                .or_insert_with(|| serde_json::Value::String(api.to_string()));
                            obj.entry("baseUrl".to_string())
                                .or_insert_with(|| serde_json::Value::String(base_url.to_string()));
                            obj.entry("apiKeyEnv".to_string())
                                .or_insert_with(|| serde_json::Value::String(api_key_env.to_string()));
                        }
                        all_models.push(enriched);
                    }
                }
            }
        }

        if all_models.is_empty() {
            info!("No models found in {}", models_path.display());
        } else {
            info!("Loaded {} models from {}", all_models.len(), models_path.display());
        }

        serde_json::Value::Array(all_models)
    }

    /// Merge two model arrays. Models from `primary` take precedence; models
    /// from `secondary` are appended only if their `id` is not already present.
    fn merge_model_lists(
        primary: &serde_json::Value,
        secondary: &serde_json::Value,
    ) -> serde_json::Value {
        let primary_arr = primary.as_array().cloned().unwrap_or_default();
        let secondary_arr = secondary.as_array().cloned().unwrap_or_default();

        if secondary_arr.is_empty() {
            return serde_json::Value::Array(primary_arr);
        }

        let seen: std::collections::HashSet<String> = primary_arr
            .iter()
            .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(String::from))
            .collect();

        let mut merged = primary_arr;
        for model in secondary_arr {
            if let Some(id) = model.get("id").and_then(|id| id.as_str()) {
                if !seen.contains(id) {
                    merged.push(model);
                }
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

        debug!("Spawning ephemeral Pi to fetch user-authenticated models for '{}'", workdir);

        let mut cmd = Command::new(&self.config.pi_binary);
        cmd.args(["--mode", "rpc", "--no-session"])
            .current_dir(workdir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Load eavs.env so Pi can resolve the eavs proxy key
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let eavs_env_path = format!("{}/.config/oqto/eavs.env", home);
        if let Ok(contents) = std::fs::read_to_string(&eavs_env_path) {
            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    cmd.env(key.trim(), value.trim());
                }
            }
        }

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

                match PiMessage::parse(trimmed) {
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
    pub async fn fork(&self, session_id: &str, entry_id: &str) -> Result<serde_json::Value> {
        let request_id = "fork".to_string();

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
            .send(PiSessionCommand::Fork(entry_id.to_string()))
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

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("Fork response missing data"))
    }

    /// Get messages available for forking.
    pub async fn get_fork_messages(&self, session_id: &str) -> Result<serde_json::Value> {
        let request_id = "get_fork_messages".to_string();

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
            .send(PiSessionCommand::GetForkMessages)
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

        // Send close command
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(resolved_id) {
                let _ = session.cmd_tx.send(PiSessionCommand::Close).await;
            }
        }

        // Remove from sessions map
        let mut sessions = self.sessions.write().await;
        if let Some(mut session) = sessions.remove(resolved_id) {
            // Kill the process if still running
            let _ = session.process.kill().await;
            info!("Session '{}' closed", resolved_id);
        }

        let mut aliases = self.session_aliases.write().await;
        aliases.retain(|_, value| value != resolved_id);
        if resolved_id != session_id {
            aliases.remove(session_id);
        }

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

        match cmd {
            PiSessionCommand::Prompt { .. } => {
                if !(is_idle || is_starting) {
                    anyhow::bail!(
                        "Session '{}' not idle (state={})",
                        session_id,
                        current_state
                    );
                }
            }
            PiSessionCommand::FollowUp(_) | PiSessionCommand::Steer(_) => {
                if !(is_idle || is_starting || is_streaming) {
                    anyhow::bail!(
                        "Session '{}' not ready for steer/follow_up (state={})",
                        session_id,
                        current_state
                    );
                }
            }
            PiSessionCommand::Compact(_) => {
                if !is_idle {
                    anyhow::bail!(
                        "Session '{}' not idle for compaction (state={})",
                        session_id,
                        current_state
                    );
                }
            }
            PiSessionCommand::NewSession(_) | PiSessionCommand::SwitchSession(_) => {
                if !is_idle {
                    anyhow::bail!(
                        "Session '{}' not idle for session switch (state={})",
                        session_id,
                        current_state
                    );
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Cleanup idle sessions that have no subscribers.
    async fn cleanup_idle_sessions(&self, idle_timeout: Duration) {
        let now = Instant::now();
        let mut to_close = Vec::new();

        let snapshots: Vec<(String, Arc<RwLock<PiSessionState>>, Instant, usize)> = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .map(|(id, session)| {
                    (
                        id.clone(),
                        Arc::clone(&session.state),
                        session.last_activity,
                        session.subscriber_count(),
                    )
                })
                .collect()
        };

        for (id, state, last_activity, subscriber_count) in snapshots {
            let current_state = *state.read().await;
            let is_idle = current_state == PiSessionState::Idle;
            let no_subscribers = subscriber_count == 0;
            let timed_out = now.duration_since(last_activity) > idle_timeout;

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
        event_tx: broadcast::Sender<PiEventWrapper>,
        state: Arc<RwLock<PiSessionState>>,
        last_activity: Arc<RwLock<Instant>>,
        hstry_client: Option<HstryClient>,
        work_dir: PathBuf,
        pending_responses: PendingResponses,
        pending_client_id: PendingClientId,
        cmd_tx: mpsc::Sender<PiSessionCommand>,
        hstry_external_id: HstryExternalId,
        session_aliases: Arc<RwLock<HashMap<String, String>>>,
        runner_id: String,
    ) {
        // Read stderr in a separate task (for debugging)
        if let Some(stderr) = stderr {
            let session_id = session_id.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if !line.trim().is_empty() {
                        debug!("Pi[{}] stderr: {}", session_id, line);
                    }
                }
            });
        }

        // Read stdout
        let mut reader = BufReader::new(stdout).lines();
        let mut pending_messages: Vec<AgentMessage> = Vec::new();
        let mut pending_hstry_client_id: Option<String> = None;
        let mut translator = PiTranslator::new();

        // Track the last session title synced to hstry to avoid redundant updates
        let mut last_synced_title = String::new();

        // Whether we've already resolved Pi's native session ID.
        let mut pi_native_id_known = false;

        // Mark as Idle after first successful read (Pi is ready)
        let mut first_event_seen = false;

        // Debounce incremental hstry persistence: persist at most every 5 seconds
        // during streaming (triggered on TurnEnd events).
        let mut last_incremental_persist = Instant::now();
        let incremental_persist_interval = Duration::from_secs(5);
        // Track whether an incremental persist is already in flight to avoid
        // queuing multiple get_messages requests.
        let mut incremental_persist_in_flight = false;

        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            // Update last activity
            *last_activity.write().await = Instant::now();

            // Parse the message
            let msg = match PiMessage::parse(&line) {
                Ok(m) => m,
                Err(e) => {
                    warn!(
                        "Pi[{}] failed to parse message: {} - line: {}",
                        session_id, e, line
                    );
                    continue;
                }
            };

            // Handle responses vs events
            let pi_event = match msg {
                PiMessage::Event(e) => e,
                PiMessage::Response(response) => {
                    debug!("Pi[{}] response: {:?}", session_id, response);

                    // Intercept get_state responses to capture Pi's native session
                    // ID and sync session title to hstry.
                    if response.id.as_deref() == Some("get_state")
                        && let Some(ref data) = response.data
                    {
                        // Capture Pi's native session ID from the JSONL header.
                        // This is the authoritative external_id for hstry -- the
                        // same ID that hstry's adapter sync will use when importing
                        // the JSONL file, so using it here prevents duplicates.
                        if let Some(pi_sid) = data.get("sessionId").and_then(|v| v.as_str())
                            && !pi_sid.is_empty()
                            && !pi_native_id_known
                        {
                            pi_native_id_known = true;
                            let old_eid = hstry_external_id.read().await.clone();
                            *hstry_external_id.write().await = pi_sid.to_string();
                            info!(
                                "Pi[{}] native session ID: {} (hstry external_id: {} -> {})",
                                session_id, pi_sid, old_eid, pi_sid
                            );

                            // If we already wrote to hstry under the old Oqto UUID
                            // (AgentEnd fired before get_state -- unlikely due to
                            // proactive get_state but defensive), delete the stale
                            // record so the next persist creates it correctly.
                            if old_eid != pi_sid
                                && let Some(ref client) = hstry_client
                            {
                                let client = client.clone();
                                let old = old_eid.clone();
                                let new_id = pi_sid.to_string();
                                let oqto_session_id = session_id.clone();
                                let runner_id = runner_id.clone();
                                let work_dir = work_dir.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = Self::migrate_hstry_conversation_on_rekey(
                                        &client,
                                        &old,
                                        &new_id,
                                        &oqto_session_id,
                                        &runner_id,
                                        &work_dir,
                                    )
                                    .await
                                    {
                                        warn!(
                                            "Failed to migrate hstry conversation {} -> {}: {}",
                                            old, new_id, e
                                        );
                                    }
                                });
                            }

                            if pi_sid != session_id {
                                session_aliases
                                    .write()
                                    .await
                                    .insert(pi_sid.to_string(), session_id.clone());
                            }

                            if let Some(ref client) = hstry_client {
                                let client = client.clone();
                                let eid = hstry_external_id.read().await.clone();
                                let oqto_session_id = session_id.clone();
                                let runner_id = runner_id.clone();
                                let work_dir = work_dir.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = Self::ensure_hstry_conversation(
                                        &client,
                                        &eid,
                                        &oqto_session_id,
                                        &runner_id,
                                        &work_dir,
                                    )
                                    .await
                                    {
                                        warn!(
                                            "Pi[{}] failed to ensure hstry conversation: {:?}",
                                            eid, e
                                        );
                                    }
                                });
                            }
                        }

                        // Pi's auto-rename extension sets sessionName to:
                        //   "<workspace>: <title> [readable-id]"
                        // We parse it to extract the clean title and persist it.
                        if let Some(ref client) = hstry_client
                            && let Some(raw_name) = data.get("sessionName").and_then(|v| v.as_str())
                            && !raw_name.is_empty()
                        {
                            let parsed = crate::pi::session_parser::ParsedTitle::parse(raw_name);
                            let clean_title = parsed.display_title().to_string();
                            if !clean_title.is_empty() && last_synced_title != clean_title {
                                last_synced_title = clean_title.clone();

                                // Broadcast title change to frontend immediately
                                let title_event = CanonicalEvent {
                                    session_id: session_id.clone(),
                                    runner_id: runner_id.clone(),
                                    ts: chrono::Utc::now().timestamp_millis(),
                                    payload:
                                        oqto_protocol::events::EventPayload::SessionTitleChanged {
                                            title: clean_title.clone(),
                                            readable_id: parsed.readable_id.map(|s| s.to_string()),
                                        },
                                };
                                let _ = event_tx.send(title_event);

                                let client = client.clone();
                                let eid = hstry_external_id.read().await.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = client
                                        .update_conversation(
                                            &eid,
                                            Some(clean_title.clone()),
                                            None,
                                            None,
                                            None,
                                            None,
                                            None,
                                            Some("pi".to_string()),
                                            None, // platform_id already set on creation
                                        )
                                        .await
                                    {
                                        warn!("Pi[{}] failed to sync title to hstry: {}", eid, e);
                                    } else {
                                        debug!(
                                            "Pi[{}] synced title to hstry: '{}'",
                                            eid, clean_title
                                        );
                                    }
                                });
                            }
                        }
                    }

                    // Intercept incremental persistence responses (internal use only).
                    // These are NOT routed to external callers.
                    if response.id.as_deref() == Some("_incremental_persist") {
                        incremental_persist_in_flight = false;
                        if response.success {
                            if let Some(ref data) = response.data
                                && let Some(msgs_val) = data.get("messages")
                            {
                                match serde_json::from_value::<Vec<AgentMessage>>(msgs_val.clone())
                                {
                                    Ok(messages) if !messages.is_empty() => {
                                        if let Some(ref client) = hstry_client {
                                            let client = client.clone();
                                            let eid = hstry_external_id.read().await.clone();
                                            let sid = session_id.clone();
                                            let rid = runner_id.clone();
                                            let wd = work_dir.clone();
                                            tokio::spawn(async move {
                                                if let Err(e) = Self::persist_to_hstry_grpc(
                                                    &client, &eid, &sid, &rid, &messages, &wd,
                                                    None, // no client_id for incremental
                                                )
                                                .await
                                                {
                                                    warn!(
                                                        "Pi[{}] incremental hstry persist failed: {:?}",
                                                        sid, e
                                                    );
                                                } else {
                                                    debug!(
                                                        "Pi[{}] incremental hstry persist: {} messages (eid={})",
                                                        sid,
                                                        messages.len(),
                                                        eid,
                                                    );
                                                }
                                            });
                                        }
                                    }
                                    Ok(_) => {} // empty messages, skip
                                    Err(e) => {
                                        warn!(
                                            "Pi[{}] failed to parse incremental persist messages: {}",
                                            session_id, e
                                        );
                                    }
                                }
                            }
                        } else {
                            debug!(
                                "Pi[{}] incremental persist get_messages failed: {:?}",
                                session_id, response.error
                            );
                        }
                        continue; // Don't route to external callers
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

            // Update internal state based on Pi event
            let new_state = match &pi_event {
                PiEvent::AgentStart => {
                    debug!("Pi[{}] AgentStart", session_id);
                    Some(PiSessionState::Streaming)
                }
                PiEvent::AgentEnd { messages } => {
                    debug!(
                        "Pi[{}] AgentEnd with {} messages",
                        session_id,
                        messages.len()
                    );
                    pending_messages = messages.clone();
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
                // before the first AgentEnd triggers hstry persistence.
                if let Err(e) = cmd_tx.send(PiSessionCommand::GetState).await {
                    warn!(
                        "Pi[{}] failed to send proactive get_state: {}",
                        session_id, e
                    );
                }
            }

            // For AgentEnd, transfer the pending client_id to the translator before translating.
            // This ensures the client_id is included in the user message for optimistic matching.
            if matches!(pi_event, PiEvent::AgentEnd { .. }) {
                let client_id = pending_client_id.write().await.take();
                translator.set_pending_client_id(client_id.clone());
                pending_hstry_client_id = client_id;
            }

            // Persist to hstry on AgentEnd BEFORE broadcasting canonical events.
            // This ensures hstry has the complete history before the frontend
            // receives agent.idle and potentially fetches/switches sessions.
            if matches!(pi_event, PiEvent::AgentEnd { .. }) && !pending_messages.is_empty() {
                if let Some(ref client) = hstry_client {
                    let eid = hstry_external_id.read().await.clone();
                    let client_id = pending_hstry_client_id.take();
                    if let Err(e) = Self::persist_to_hstry_grpc(
                        client,
                        &eid,
                        &session_id,
                        &runner_id,
                        &pending_messages,
                        &work_dir,
                        client_id,
                    )
                    .await
                    {
                        warn!("Pi[{}] failed to persist to hstry: {:?}", session_id, e);
                    } else {
                        debug!(
                            "Pi[{}] persisted {} messages to hstry (external_id={})",
                            session_id,
                            pending_messages.len(),
                            eid,
                        );
                    }
                }
                pending_messages.clear();
            }

            // Translate Pi event to canonical events and broadcast each one.
            // For AgentEnd, hstry is already persisted above so the frontend
            // can safely read history on agent.idle.
            let canonical_payloads = translator.translate(&pi_event);
            let ts = chrono::Utc::now().timestamp_millis();
            for payload in &canonical_payloads {
                let canonical_event = CanonicalEvent {
                    session_id: session_id.clone(),
                    runner_id: runner_id.clone(),
                    ts,
                    payload: payload.clone(),
                };
                let _ = event_tx.send(canonical_event);

                // Sync title changes to hstry immediately
                if let oqto_protocol::events::EventPayload::SessionTitleChanged { title, .. } =
                    payload
                    && !title.is_empty()
                    && last_synced_title != *title
                {
                    last_synced_title = title.clone();
                    if let Some(ref client) = hstry_client {
                        let client = client.clone();
                        let eid = hstry_external_id.read().await.clone();
                        let clean_title = title.clone();
                        tokio::spawn(async move {
                            if let Err(e) = client
                                .update_conversation(
                                    &eid,
                                    Some(clean_title),
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    Some("pi".to_string()),
                                    None,
                                )
                                .await
                            {
                                warn!("Failed to sync title to hstry: {}", e);
                            }
                        });
                    }
                }
            }

            // Incremental hstry persistence on TurnEnd events.
            // This ensures hstry stays reasonably current during multi-turn streaming,
            // so that a page reload mid-stream can recover messages from hstry.
            // Debounced to avoid hammering hstry on rapid turn sequences.
            if matches!(pi_event, PiEvent::TurnEnd { .. }) {
                let elapsed = last_incremental_persist.elapsed();
                if elapsed >= incremental_persist_interval
                    && hstry_client.is_some()
                    && !incremental_persist_in_flight
                {
                    last_incremental_persist = Instant::now();
                    incremental_persist_in_flight = true;
                    if let Err(e) = cmd_tx.send(PiSessionCommand::IncrementalPersist).await {
                        warn!(
                            "Pi[{}] failed to send IncrementalPersist command: {}",
                            session_id, e
                        );
                        incremental_persist_in_flight = false;
                    } else {
                        debug!(
                            "Pi[{}] triggered incremental hstry persist on TurnEnd",
                            session_id
                        );
                    }
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
                    let _ = probe_cmd_tx.send(PiSessionCommand::GetState).await;
                });
            }
        }

        // Process exited -- broadcast error event
        info!("Pi[{}] stdout reader finished (process exited)", session_id);
        let exit_event = translator
            .state
            .on_process_exit("Agent process exited".to_string());
        let canonical_event = CanonicalEvent {
            session_id: session_id.clone(),
            runner_id,
            ts: chrono::Utc::now().timestamp_millis(),
            payload: exit_event,
        };
        let _ = event_tx.send(canonical_event);
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
                    *state.write().await = PiSessionState::Streaming;
                    // Store client_id in shared state for the reader task's translator
                    // to include in the persisted messages when agent_end arrives.
                    *pending_client_id.write().await = client_id;
                    let pi_cmd = PiCommand::Prompt {
                        id: None,
                        message,
                        images: None,
                        streaming_behavior: None,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::Steer(msg) => {
                    // The runner decides how to deliver based on session state:
                    // - Streaming: send as steer (interrupt mid-run)
                    // - Idle/Starting/Stopping/Aborting: send as prompt (new turn or zombie session)
                    // - Other states: send as steer and let Pi handle it
                    let current_state = *state.read().await;
                    let pi_cmd = if matches!(
                        current_state,
                        PiSessionState::Idle
                            | PiSessionState::Starting
                            | PiSessionState::Stopping
                            | PiSessionState::Aborting
                    ) {
                        debug!(
                            "Session '{}' is in {:?}, routing steer as prompt",
                            session_id, current_state
                        );
                        *state.write().await = PiSessionState::Streaming;
                        PiCommand::Prompt {
                            id: None,
                            message: msg,
                            images: None,
                            streaming_behavior: None,
                        }
                    } else {
                        PiCommand::Steer {
                            id: None,
                            message: msg,
                        }
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::FollowUp(msg) => {
                    // The runner decides how to deliver based on session state:
                    // - Streaming: send as follow_up (queued until done)
                    // - Idle/Starting/Stopping/Aborting: send as prompt (new turn or zombie session)
                    // - Other states: send as follow_up and let Pi handle it
                    let current_state = *state.read().await;
                    let pi_cmd = if matches!(
                        current_state,
                        PiSessionState::Idle
                            | PiSessionState::Starting
                            | PiSessionState::Stopping
                            | PiSessionState::Aborting
                    ) {
                        debug!(
                            "Session '{}' is in {:?}, routing follow_up as prompt",
                            session_id, current_state
                        );
                        *state.write().await = PiSessionState::Streaming;
                        PiCommand::Prompt {
                            id: None,
                            message: msg,
                            images: None,
                            streaming_behavior: None,
                        }
                    } else {
                        PiCommand::FollowUp {
                            id: None,
                            message: msg,
                        }
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
                PiSessionCommand::GetState => {
                    // Response coordination happens via pending_responses map
                    let pi_cmd = PiCommand::GetState {
                        id: Some("get_state".to_string()),
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
                PiSessionCommand::IncrementalPersist => {
                    // Internal command: fetch messages for incremental hstry persistence.
                    // Uses a distinct request ID so the reader task can intercept it
                    // without interfering with external get_messages requests.
                    let pi_cmd = PiCommand::GetMessages {
                        id: Some("_incremental_persist".to_string()),
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
                PiSessionCommand::Fork(entry_id) => {
                    let pi_cmd = PiCommand::Fork {
                        id: Some("fork".to_string()),
                        entry_id,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetForkMessages => {
                    let pi_cmd = PiCommand::GetForkMessages {
                        id: Some("get_fork_messages".to_string()),
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

    /// Ensure a hstry conversation exists as soon as we know the real session ID.
    async fn ensure_hstry_conversation(
        client: &HstryClient,
        hstry_external_id: &str,
        oqto_session_id: &str,
        runner_id: &str,
        work_dir: &Path,
    ) -> Result<()> {
        if let Ok(Some(_)) = client.get_conversation(hstry_external_id, None).await {
            let metadata_json = build_metadata_json(
                client,
                hstry_external_id,
                oqto_session_id,
                runner_id,
                work_dir,
                None,
            )
            .await?;
            let _ = client
                .update_conversation(
                    hstry_external_id,
                    None,
                    Some(work_dir.to_string_lossy().to_string()),
                    None,
                    None,
                    Some(metadata_json),
                    None,
                    Some("pi".to_string()),
                    Some(oqto_session_id.to_string()),
                )
                .await;
            return Ok(());
        }

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let metadata_json = build_metadata_json(
            client,
            hstry_external_id,
            oqto_session_id,
            runner_id,
            work_dir,
            None,
        )
        .await?;
        let jsonl_title = resolve_jsonl_session_title(hstry_external_id, work_dir).await;
        let (_title, readable_id) = jsonl_title
            .as_deref()
            .map(|name| {
                let parsed = ParsedTitle::parse(name);
                (
                    Some(parsed.display_title().to_string()),
                    parsed.get_readable_id().map(|id| id.to_string()),
                )
            })
            .unwrap_or((None, None));

        client
            .write_conversation(
                hstry_external_id,
                None,
                Some(work_dir.to_string_lossy().to_string()),
                None,
                None,
                Some(metadata_json),
                Vec::new(),
                now_ms,
                Some(now_ms),
                Some("pi".to_string()),
                readable_id,
                Some(oqto_session_id.to_string()),
            )
            .await?;

        Ok(())
    }

    async fn migrate_hstry_conversation_on_rekey(
        client: &HstryClient,
        old_external_id: &str,
        new_external_id: &str,
        oqto_session_id: &str,
        runner_id: &str,
        work_dir: &Path,
    ) -> Result<()> {
        if old_external_id == new_external_id {
            return Ok(());
        }

        let Some(old_conv) = client.get_conversation(old_external_id, None).await? else {
            return Ok(());
        };

        let new_conv = client.get_conversation(new_external_id, None).await?;
        if new_conv.is_some() {
            let new_messages = client.get_messages(new_external_id, None, None).await?;
            if !new_messages.is_empty() {
                client.delete_conversation(old_external_id).await?;
                info!(
                    "Deleted stale hstry record under old external_id={}",
                    old_external_id
                );
                return Ok(());
            }
        }

        let old_messages = client.get_messages(old_external_id, None, None).await?;
        if old_messages.is_empty() {
            if new_conv.is_some() {
                client.delete_conversation(old_external_id).await?;
                info!(
                    "Deleted empty hstry record under old external_id={}",
                    old_external_id
                );
            }
            return Ok(());
        }

        let metadata_json = build_metadata_json(
            client,
            old_external_id,
            oqto_session_id,
            runner_id,
            work_dir,
            None,
        )
        .await?;
        let workspace = old_conv
            .workspace
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some(work_dir.to_string_lossy().to_string()));
        let readable_id = old_conv
            .readable_id
            .clone()
            .filter(|value| !value.trim().is_empty());

        client
            .write_conversation(
                new_external_id,
                old_conv.title.clone(),
                workspace,
                old_conv.model.clone(),
                old_conv.provider.clone(),
                Some(metadata_json),
                old_messages,
                old_conv.created_at_ms,
                old_conv.updated_at_ms,
                Some("pi".to_string()),
                readable_id,
                Some(oqto_session_id.to_string()),
            )
            .await?;

        client.delete_conversation(old_external_id).await?;
        info!(
            "Migrated hstry conversation {} -> {}",
            old_external_id, new_external_id
        );

        Ok(())
    }

    /// Persist messages to hstry via gRPC.
    ///
    /// `hstry_external_id` is the key used in hstry (Pi's native session ID when
    /// known, otherwise Oqto's UUID as a fallback). `oqto_session_id` is always
    /// Oqto's UUID, stored in metadata for reverse mapping.
    async fn persist_to_hstry_grpc(
        client: &HstryClient,
        hstry_external_id: &str,
        oqto_session_id: &str,
        runner_id: &str,
        messages: &[AgentMessage],
        work_dir: &Path,
        client_id: Option<String>,
    ) -> Result<()> {
        use crate::hstry::agent_message_to_proto_with_client_id;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let stats_delta = compute_stats_delta(messages);
        let metadata_json = build_metadata_json(
            client,
            hstry_external_id,
            oqto_session_id,
            runner_id,
            work_dir,
            stats_delta,
        )
        .await?;

        let jsonl_indices =
            resolve_jsonl_message_indices(hstry_external_id, work_dir, messages).await;
        let needs_fallback = jsonl_indices
            .as_ref()
            .map(|indices| indices.iter().any(|value| value.is_none()))
            .unwrap_or(true);
        let fallback_start_idx = if needs_fallback {
            fetch_last_hstry_idx(client, hstry_external_id)
                .await
                .map(|idx| idx + 1)
        } else {
            None
        };

        let jsonl_title = resolve_jsonl_session_title(hstry_external_id, work_dir).await;
        let (title, readable_id) = jsonl_title
            .as_deref()
            .map(|name| {
                let parsed = ParsedTitle::parse(name);
                (
                    Some(parsed.display_title().to_string()),
                    parsed.get_readable_id().map(|id| id.to_string()),
                )
            })
            .unwrap_or((None, None));

        // Convert messages to proto format.
        // Use AppendMessages if the conversation likely exists (most common case),
        // falling back to WriteConversation if not found.
        // Guard against compaction: if Pi's message count is fewer than what's
        // already in hstry, a context compaction happened. Don't overwrite the
        // complete history with the truncated context window — only update
        // metadata/stats.
        let existing_max_idx = fetch_last_hstry_idx(client, hstry_external_id).await;
        let skip_messages = if let Some(max_idx) = existing_max_idx {
            let existing_count = (max_idx + 1) as usize;
            if messages.len() < existing_count {
                info!(
                    "Skipping hstry message persist: Pi has {} messages but hstry already has {} \
                     (context compaction detected). Updating metadata only.",
                    messages.len(),
                    existing_count,
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        if skip_messages {
            // Update metadata (stats, title, platform_id, etc.), don't touch messages.
            // Always call update to ensure platform_id is set even when no other
            // metadata has changed.
            let _ = client
                .update_conversation(
                    hstry_external_id,
                    title,
                    Some(work_dir.to_string_lossy().to_string()),
                    None,
                    None,
                    Some(metadata_json),
                    readable_id,
                    Some("pi".to_string()),
                    Some(oqto_session_id.to_string()),
                )
                .await;
            return Ok(());
        }

        let last_user_idx = messages
            .iter()
            .rposition(|msg| matches!(msg.role.as_str(), "user" | "human"));

        let proto_messages: Vec<_> = messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                let client_id_for_msg = if Some(i) == last_user_idx {
                    client_id.clone()
                } else {
                    None
                };
                let jsonl_idx = jsonl_indices
                    .as_ref()
                    .and_then(|indices| indices.get(i))
                    .and_then(|value| *value);
                let idx = jsonl_idx
                    .or_else(|| fallback_start_idx.map(|base| base + i as i32))
                    .unwrap_or(i as i32);
                agent_message_to_proto_with_client_id(msg, idx, client_id_for_msg)
            })
            .collect();

        // Try append first (fast path -- conversation already exists)
        match client
            .append_messages(hstry_external_id, proto_messages.clone(), Some(now_ms))
            .await
        {
            Ok(_) => {
                // Always update metadata after appending messages to ensure
                // platform_id (oqto session ID) is set in hstry. Without this,
                // conversations created by hstry's own JSONL sync lack the
                // platform_id, causing duplicate sessions in the frontend.
                let _ = client
                    .update_conversation(
                        hstry_external_id,
                        title.clone(),
                        Some(work_dir.to_string_lossy().to_string()),
                        None,
                        None,
                        Some(metadata_json.clone()),
                        readable_id.clone(),
                        Some("pi".to_string()),
                        Some(oqto_session_id.to_string()),
                    )
                    .await;
            }
            Err(_) => {
                // Conversation doesn't exist yet -- create it with WriteConversation
                let model = messages.iter().rev().find_map(|m| m.model.clone());
                let provider = messages.iter().rev().find_map(|m| m.provider.clone());

                client
                    .write_conversation(
                        hstry_external_id,
                        title,
                        Some(work_dir.to_string_lossy().to_string()),
                        model,
                        provider,
                        Some(metadata_json.clone()),
                        proto_messages,
                        now_ms,
                        Some(now_ms),
                        Some("pi".to_string()),
                        readable_id,
                        Some(oqto_session_id.to_string()),
                    )
                    .await?;
            }
        }

        info!(
            "Persisted {} messages to hstry (external_id='{}', oqto_session='{}')",
            messages.len(),
            hstry_external_id,
            oqto_session_id,
        );

        Ok(())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct JsonlMessageKey {
    role: String,
    timestamp: Option<u64>,
    content: String,
    tool_call_id: Option<String>,
    tool_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonlEntry {
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(default)]
    message: Option<AgentMessage>,
    #[serde(default)]
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonlSessionInfoEntry {
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Clone)]
struct JsonlMessageEntry {
    idx: i32,
    key: JsonlMessageKey,
}

fn message_signature(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(text) => text.trim().to_string(),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block.as_object())
            .filter_map(|obj| {
                let block_type = obj.get("type").and_then(|t| t.as_str())?;
                match block_type {
                    "text" => obj.get("text").and_then(|t| t.as_str()),
                    "thinking" => obj.get("thinking").and_then(|t| t.as_str()),
                    _ => None,
                }
            })
            .map(|text| text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn build_jsonl_key(message: &AgentMessage, timestamp: Option<u64>) -> JsonlMessageKey {
    JsonlMessageKey {
        role: message.role.to_lowercase(),
        timestamp,
        content: message_signature(&message.content),
        tool_call_id: message.tool_call_id.clone(),
        tool_name: message.tool_name.clone(),
    }
}

fn parse_entry_timestamp(timestamp: Option<&str>) -> Option<u64> {
    let parsed = timestamp.and_then(|value| DateTime::parse_from_rfc3339(value).ok());
    parsed.and_then(|dt| {
        let millis = dt.timestamp_millis();
        if millis >= 0 {
            Some(millis as u64)
        } else {
            None
        }
    })
}

fn read_jsonl_message_entries(path: PathBuf) -> Result<Vec<JsonlMessageEntry>> {
    use std::io::BufRead;

    let file = std::fs::File::open(&path)
        .with_context(|| format!("Failed to open Pi session file {}", path.display()))?;
    let reader = std::io::BufReader::new(file);
    let mut entries = Vec::new();
    let mut idx: i32 = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let entry: JsonlEntry = match serde_json::from_str(&line) {
            Ok(parsed) => parsed,
            Err(err) => {
                warn!("Failed to parse Pi JSONL entry: {}", err);
                continue;
            }
        };

        if entry.entry_type != "message" {
            continue;
        }

        let Some(message) = entry.message else {
            continue;
        };

        let entry_ts = parse_entry_timestamp(entry.timestamp.as_deref());
        let timestamp = message.timestamp.or(entry_ts);
        let key = build_jsonl_key(&message, timestamp);

        entries.push(JsonlMessageEntry { idx, key });
        idx += 1;
    }

    Ok(entries)
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

async fn resolve_jsonl_message_indices(
    session_id: &str,
    work_dir: &Path,
    messages: &[AgentMessage],
) -> Option<Vec<Option<i32>>> {
    let session_file = crate::pi::session_files::find_session_file_async(
        session_id.to_string(),
        Some(work_dir.to_path_buf()),
    )
    .await?;

    let entries = tokio::task::spawn_blocking(move || read_jsonl_message_entries(session_file))
        .await
        .ok()
        .and_then(|result| result.ok())?;

    let mut keyed: HashMap<JsonlMessageKey, VecDeque<i32>> = HashMap::new();
    let mut keyed_no_ts: HashMap<JsonlMessageKey, VecDeque<i32>> = HashMap::new();

    for entry in entries {
        keyed
            .entry(entry.key.clone())
            .or_default()
            .push_back(entry.idx);
        let mut no_ts_key = entry.key.clone();
        no_ts_key.timestamp = None;
        keyed_no_ts
            .entry(no_ts_key)
            .or_default()
            .push_back(entry.idx);
    }

    let mut result = Vec::with_capacity(messages.len());
    for message in messages {
        let key = build_jsonl_key(message, message.timestamp);
        let mut idx = keyed.get_mut(&key).and_then(|values| values.pop_front());
        if idx.is_none() {
            let mut no_ts_key = key.clone();
            no_ts_key.timestamp = None;
            idx = keyed_no_ts
                .get_mut(&no_ts_key)
                .and_then(|values| values.pop_front());
        }
        result.push(idx);
    }

    Some(result)
}

async fn fetch_last_hstry_idx(client: &HstryClient, session_id: &str) -> Option<i32> {
    match client.get_messages(session_id, None, Some(1)).await {
        Ok(mut messages) => messages.pop().map(|msg| msg.idx),
        Err(_) => None,
    }
}

async fn resolve_jsonl_session_title(session_id: &str, work_dir: &Path) -> Option<String> {
    let session_file = crate::pi::session_files::find_session_file_async(
        session_id.to_string(),
        Some(work_dir.to_path_buf()),
    )
    .await?;

    tokio::task::spawn_blocking(move || read_jsonl_session_name(session_file))
        .await
        .ok()
        .and_then(|result| result.ok())?
}

#[derive(Debug, Clone, Copy, Default)]
struct StatsDelta {
    tokens_in: i64,
    tokens_out: i64,
    cache_read: i64,
    cache_write: i64,
    cost_usd: f64,
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

/// Build metadata JSON for hstry conversation.
///
/// `hstry_external_id` is the key in hstry (Pi native ID or Oqto UUID).
/// `oqto_session_id` is always Oqto's UUID -- stored in metadata so we can
/// always map between the two identifiers.
async fn build_metadata_json(
    client: &HstryClient,
    hstry_external_id: &str,
    oqto_session_id: &str,
    runner_id: &str,
    work_dir: &Path,
    delta: Option<StatsDelta>,
) -> Result<String> {
    let mut metadata = serde_json::Map::new();

    if let Ok(Some(conversation)) = client.get_conversation(hstry_external_id, None).await
        && !conversation.metadata_json.trim().is_empty()
        && let Ok(serde_json::Value::Object(existing)) =
            serde_json::from_str::<serde_json::Value>(&conversation.metadata_json)
    {
        metadata = existing;
    }

    // Always store the Oqto session ID so we can map back from Pi native ID
    metadata.insert(
        "oqto_session_id".to_string(),
        serde_json::Value::String(oqto_session_id.to_string()),
    );
    metadata.insert(
        "workdir".to_string(),
        serde_json::Value::String(work_dir.to_string_lossy().to_string()),
    );
    metadata.insert(
        "runner_id".to_string(),
        serde_json::Value::String(runner_id.to_string()),
    );

    if let Some(delta) = delta {
        let existing = metadata
            .get("stats")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let tokens_in = existing
            .get("tokens_in")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            + delta.tokens_in;
        let tokens_out = existing
            .get("tokens_out")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            + delta.tokens_out;
        let cache_read = existing
            .get("cache_read")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            + delta.cache_read;
        let cache_write = existing
            .get("cache_write")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            + delta.cache_write;
        let cost_usd = existing
            .get("cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            + delta.cost_usd;

        metadata.insert(
            "stats".to_string(),
            serde_json::json!({
                "tokens_in": tokens_in,
                "tokens_out": tokens_out,
                "cache_read": cache_read,
                "cache_write": cache_write,
                "cost_usd": cost_usd,
            }),
        );
    }

    Ok(serde_json::Value::Object(metadata).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(config.pi_binary.to_string_lossy().contains(".bun/bin/pi"));
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
}

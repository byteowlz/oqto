//! Pi Session Manager for octo-runner.
//!
//! Manages multiple Pi agent processes with:
//! - Process lifecycle (spawn, shutdown)
//! - Command routing (prompt, steer, follow_up, abort, compact)
//! - Event broadcasting to subscribers
//! - State tracking (Starting, Idle, Streaming, Compacting, Stopping)
//! - Idle session cleanup
//! - Persistence to hstry on AgentEnd

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{RwLock, broadcast, mpsc, oneshot};

use crate::hstry::HstryClient;
use crate::local::SandboxConfig;
use crate::pi::{AgentMessage, PiCommand, PiEvent, PiMessage, PiResponse, PiState, SessionStats};
use crate::runner::pi_translator::PiTranslator;
use octo_protocol::events::Event as CanonicalEvent;

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
}

impl Default for PiManagerConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".local").join("share"));

        Self {
            pi_binary: PathBuf::from(home.clone()).join(".bun/bin/pi"),
            default_cwd: PathBuf::from(&home).join("projects"),
            idle_timeout_secs: 300, // 5 minutes
            cleanup_interval_secs: 60,
            hstry_db_path: Some(data_dir.join("hstry").join("hstry.db")),
            sandbox_config: None,
        }
    }
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

/// Session lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiSessionState {
    /// Session is starting up.
    Starting,
    /// Session is idle, waiting for commands.
    Idle,
    /// Session is actively streaming a response.
    Streaming,
    /// Session is compacting conversation history.
    Compacting,
    /// Session is aborting an active run.
    Aborting,
    /// Session is shutting down.
    Stopping,
}

impl std::fmt::Display for PiSessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Idle => write!(f, "idle"),
            Self::Streaming => write!(f, "streaming"),
            Self::Compacting => write!(f, "compacting"),
            Self::Aborting => write!(f, "aborting"),
            Self::Stopping => write!(f, "stopping"),
        }
    }
}

/// Information about a Pi session (for external queries).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionInfo {
    /// Session ID.
    pub session_id: String,
    /// Current state.
    pub state: PiSessionState,
    /// Last activity timestamp (Unix ms).
    pub last_activity: i64,
    /// Number of active subscribers.
    pub subscriber_count: usize,
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
/// Starts as the Octo UUID (for optimistic session creation), then gets
/// updated to Pi's native session ID once `get_state` returns it. All hstry
/// reads/writes should use this value, not the Octo session_id directly.
type HstryExternalId = Arc<RwLock<String>>;

/// Internal session state (held by the manager).
struct PiSession {
    /// Session ID (Octo UUID -- the routing key used by frontend/API).
    id: String,
    /// Session configuration.
    #[allow(dead_code)]
    config: PiSessionConfig,
    /// Child process.
    process: Child,
    /// Current state.
    state: Arc<RwLock<PiSessionState>>,
    /// The external_id used in hstry for this session. Initially the Octo UUID,
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
    /// inserted into the `sessions` map. Prevents the TOCTOU race in
    /// `get_or_create_session` where two concurrent callers both pass the
    /// `contains_key` check and each spawn a separate Pi process.
    creating: tokio::sync::Mutex<std::collections::HashSet<String>>,
    /// Cached model lists per workdir (populated when any session in that workdir fetches models).
    /// Key: canonical workdir path, Value: list of available models as JSON.
    model_cache: RwLock<HashMap<String, serde_json::Value>>,
}

impl PiSessionManager {
    /// Create a new Pi session manager.
    pub fn new(config: PiManagerConfig) -> Arc<Self> {
        let (shutdown_tx, _) = broadcast::channel(1);

        // Create hstry client if not explicitly disabled via hstry_db_path=None.
        // The actual gRPC connection is established lazily on first use.
        let hstry_client = config.hstry_db_path.as_ref().map(|_| HstryClient::new());

        Arc::new(Self {
            sessions: RwLock::new(HashMap::new()),
            config,
            shutdown_tx,
            hstry_client,
            creating: tokio::sync::Mutex::new(std::collections::HashSet::new()),
            model_cache: RwLock::new(HashMap::new()),
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
        // not through Octo) so the agent has the full conversation context.
        let continue_session = config
            .continue_session
            .clone()
            .or_else(|| {
                crate::pi::session_files::find_session_file(
                    &session_id,
                    Some(&config.cwd),
                )
            });

        if continue_session.is_some() && config.continue_session.is_none() {
            info!(
                "Auto-discovered Pi session file for '{}': {:?}",
                session_id,
                continue_session.as_ref().unwrap()
            );
        }

        let session_file = config
            .session_file
            .as_ref()
            .or(continue_session.as_ref())
            .cloned();

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
                let effective_config = sandbox_config.with_workspace_config(&config.cwd);

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
        let (event_tx, _) = broadcast::channel::<PiEventWrapper>(256);
        let (cmd_tx, cmd_rx) = mpsc::channel::<PiSessionCommand>(32);

        // Shared state for the session
        let state = Arc::new(RwLock::new(PiSessionState::Starting));
        let last_activity = Arc::new(RwLock::new(Instant::now()));
        let pending_responses: PendingResponses = Arc::new(RwLock::new(HashMap::new()));
        // Pending client_id for optimistic message matching (shared between command and reader tasks)
        let pending_client_id: PendingClientId = Arc::new(RwLock::new(None));
        // hstry external_id -- starts as Octo UUID, updated to Pi native ID by reader task
        let hstry_external_id: HstryExternalId =
            Arc::new(RwLock::new(session_id.clone()));

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
    /// A creation-in-progress guard prevents the TOCTOU race where two callers
    /// both see the session as absent and each spawn a separate Pi process.
    pub async fn get_or_create_session(
        self: &Arc<Self>,
        session_id: &str,
        config: PiSessionConfig,
    ) -> Result<String> {
        // Fast path: session already exists.
        {
            let sessions = self.sessions.read().await;
            if sessions.contains_key(session_id) {
                debug!("Session '{}' already exists", session_id);
                return Ok(session_id.to_string());
            }
        }

        // Acquire creation lock to prevent concurrent spawns for the same ID.
        {
            let mut creating = self.creating.lock().await;
            // Re-check under lock: another caller may have finished creating
            // between our read above and acquiring this lock.
            {
                let sessions = self.sessions.read().await;
                if sessions.contains_key(session_id) {
                    debug!("Session '{}' created by concurrent caller", session_id);
                    return Ok(session_id.to_string());
                }
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
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;

        Ok(session.event_tx.subscribe())
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Vec<PiSessionInfo> {
        let snapshots: Vec<(String, Arc<RwLock<PiSessionState>>, Instant, usize)> = {
            let sessions = self.sessions.read().await;
            sessions
                .values()
                .map(|s| {
                    (
                        s.id.clone(),
                        Arc::clone(&s.state),
                        s.last_activity,
                        s.subscriber_count(),
                    )
                })
                .collect()
        };

        let mut infos = Vec::with_capacity(snapshots.len());
        for (id, state, last_activity, subscriber_count) in snapshots {
            let current_state = *state.read().await;
            infos.push(PiSessionInfo {
                session_id: id,
                state: current_state,
                last_activity: last_activity.elapsed().as_millis() as i64,
                subscriber_count,
            });
        }

        infos
    }

    /// Resolve the hstry external_id for a session.
    ///
    /// Returns Pi's native session ID if known, otherwise the Octo UUID.
    /// This should be used for all hstry lookups instead of the raw session_id.
    pub async fn hstry_external_id(&self, session_id: &str) -> String {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            session.hstry_external_id.read().await.clone()
        } else {
            // Session not running -- fall back to session_id (Octo UUID).
            // hstry will try matching against external_id, readable_id, and id.
            session_id.to_string()
        }
    }

    /// Get state of a specific session.
    pub async fn get_state(&self, session_id: &str) -> Result<PiState> {
        let (runner_state, pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
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
        let request_id = "get_messages".to_string();

        // Get session and register response waiter
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
        let request_id = "set_model".to_string();

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
        let request_id = "get_available_models".to_string();

        // Try to get the live session
        let session_info = {
            let sessions = self.sessions.read().await;
            sessions.get(session_id).map(|s| {
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
                warn!(
                    "GetAvailableModels: unexpected data shape, returning empty"
                );
                serde_json::Value::Array(vec![])
            };

            // Cache by workdir so dead sessions can still list models
            if models_array.as_array().is_some_and(|a| !a.is_empty()) {
                let mut cache = self.model_cache.write().await;
                cache.insert(workdir, models_array.clone());
            }

            Ok(models_array)
        } else {
            // No live session — try workdir cache first, then hstry lookup
            if let Some(workdir) = workdir.and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then_some(trimmed)
            }) {
                if let Some(models) = self.get_cached_models_for_workdir(workdir).await {
                    return Ok(models);
                }
            }
            self.get_cached_models_for_session(session_id).await
        }
    }

    /// Look up cached models for a session by finding its workdir.
    /// Falls back to returning the first available cache entry if we can't determine the workdir.
    async fn get_cached_models_for_session(&self, session_id: &str) -> Result<serde_json::Value> {
        // Try to determine workdir from hstry
        if let Some(ref hstry) = self.hstry_client {
            let eid = self.hstry_external_id(session_id).await;
            if let Ok(Some(session)) =
                crate::history::repository::get_session_via_grpc(hstry, &eid).await
            {
                let workdir = session.workspace_path;
                let cache = self.model_cache.read().await;
                if let Some(models) = cache.get(&workdir) {
                    return Ok(models.clone());
                }
            }
        }

        // Couldn't find a specific workdir match — return empty
        Ok(serde_json::Value::Array(vec![]))
    }

    /// Get cached models for a specific workdir (called directly by runner for dead sessions).
    pub async fn get_cached_models_for_workdir(
        &self,
        workdir: &str,
    ) -> Option<serde_json::Value> {
        let cache = self.model_cache.read().await;
        cache.get(workdir).cloned()
    }

    /// Get session statistics.
    pub async fn get_session_stats(&self, session_id: &str) -> Result<SessionStats> {
        let request_id = "get_session_stats".to_string();

        // Get session and register response waiter
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
        let request_id = "cycle_model".to_string();

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
        info!("Closing session '{}'", session_id);

        // Send close command
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(session_id) {
                let _ = session.cmd_tx.send(PiSessionCommand::Close).await;
            }
        }

        // Remove from sessions map
        let mut sessions = self.sessions.write().await;
        if let Some(mut session) = sessions.remove(session_id) {
            // Kill the process if still running
            let _ = session.process.kill().await;
            info!("Session '{}' closed", session_id);
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
        let (cmd_tx, state) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (session.cmd_tx.clone(), Arc::clone(&session.state))
        };

        self.validate_command(session_id, &state, &cmd).await?;

        cmd_tx
            .send(cmd)
            .await
            .context("Failed to send command to session")?;

        Ok(())
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
        let mut translator = PiTranslator::new();

        // Track the last session title synced to hstry to avoid redundant updates
        let mut last_synced_title = String::new();

        // Whether we've already resolved Pi's native session ID.
        let mut pi_native_id_known = false;

        // Mark as Idle after first successful read (Pi is ready)
        let mut first_event_seen = false;

        // Runner ID for canonical event envelopes.
        // TODO: pass actual runner_id from config
        let runner_id = "local".to_string();

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
                    if response.id.as_deref() == Some("get_state") {
                        if let Some(ref data) = response.data {
                            // Capture Pi's native session ID from the JSONL header.
                            // This is the authoritative external_id for hstry -- the
                            // same ID that hstry's adapter sync will use when importing
                            // the JSONL file, so using it here prevents duplicates.
                            if let Some(pi_sid) =
                                data.get("sessionId").and_then(|v| v.as_str())
                            {
                                if !pi_sid.is_empty() && !pi_native_id_known {
                                    pi_native_id_known = true;
                                    let old_eid = hstry_external_id.read().await.clone();
                                    *hstry_external_id.write().await = pi_sid.to_string();
                                    info!(
                                        "Pi[{}] native session ID: {} (hstry external_id: {} -> {})",
                                        session_id, pi_sid, old_eid, pi_sid
                                    );

                                    // If we already wrote to hstry under the old Octo UUID
                                    // (AgentEnd fired before get_state -- unlikely due to
                                    // proactive get_state but defensive), delete the stale
                                    // record so the next persist creates it correctly.
                                    if old_eid != pi_sid {
                                        if let Some(ref client) = hstry_client {
                                            let client = client.clone();
                                            let old = old_eid.clone();
                                            tokio::spawn(async move {
                                                if let Ok(Some(_)) =
                                                    client.get_conversation(&old, None).await
                                                {
                                                    if let Err(e) =
                                                        client.delete_conversation(&old).await
                                                    {
                                                        warn!(
                                                            "Failed to delete stale hstry record {}: {}",
                                                            old, e
                                                        );
                                                    } else {
                                                        info!(
                                                            "Deleted stale hstry record under old external_id={}",
                                                            old
                                                        );
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                            }

                            // Pi's auto-rename extension sets sessionName to:
                            //   "<workspace>: <title> [readable-id]"
                            // We parse it to extract the clean title and persist it.
                            if let Some(ref client) = hstry_client {
                                if let Some(raw_name) =
                                    data.get("sessionName").and_then(|v| v.as_str())
                                {
                                    if !raw_name.is_empty() {
                                        let parsed =
                                            crate::pi::session_parser::ParsedTitle::parse(
                                                raw_name,
                                            );
                                        let clean_title =
                                            parsed.display_title().to_string();
                                        if !clean_title.is_empty()
                                            && last_synced_title != clean_title
                                        {
                                            last_synced_title = clean_title.clone();
                                            let readable_id = parsed
                                                .get_readable_id()
                                                .map(String::from);
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
                                                        readable_id,
                                                        Some("pi".to_string()),
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                        "Pi[{}] failed to sync title to hstry: {}",
                                                        eid, e
                                                    );
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
                translator.set_pending_client_id(client_id);
            }

            // Translate Pi event to canonical events and broadcast each one
            let canonical_payloads = translator.translate(&pi_event);
            let ts = chrono::Utc::now().timestamp_millis();
            for payload in canonical_payloads {
                let canonical_event = CanonicalEvent {
                    session_id: session_id.clone(),
                    runner_id: runner_id.clone(),
                    ts,
                    payload,
                };
                let _ = event_tx.send(canonical_event);
            }

            // Persist to hstry on AgentEnd
            if matches!(pi_event, PiEvent::AgentEnd { .. }) && !pending_messages.is_empty() {
                if let Some(ref client) = hstry_client {
                    let eid = hstry_external_id.read().await.clone();
                    if let Err(e) = Self::persist_to_hstry_grpc(
                        client,
                        &eid,
                        &session_id,
                        &pending_messages,
                        &work_dir,
                    )
                    .await
                    {
                        warn!("Pi[{}] failed to persist to hstry: {}", session_id, e);
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
                    // - Idle: send as prompt (new turn)
                    // - Other states: send as steer and let Pi handle it
                    let current_state = *state.read().await;
                    let pi_cmd = if current_state == PiSessionState::Idle {
                        debug!("Session '{}' is idle, routing steer as prompt", session_id);
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
                    // - Idle: send as prompt (new turn)
                    // - Other states: send as follow_up and let Pi handle it
                    let current_state = *state.read().await;
                    let pi_cmd = if current_state == PiSessionState::Idle {
                        debug!(
                            "Session '{}' is idle, routing follow_up as prompt",
                            session_id
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

    /// Persist messages to hstry via gRPC.
    ///
    /// `hstry_external_id` is the key used in hstry (Pi's native session ID when
    /// known, otherwise Octo's UUID as a fallback). `octo_session_id` is always
    /// Octo's UUID, stored in metadata for reverse mapping.
    async fn persist_to_hstry_grpc(
        client: &HstryClient,
        hstry_external_id: &str,
        octo_session_id: &str,
        messages: &[AgentMessage],
        work_dir: &PathBuf,
    ) -> Result<()> {
        use crate::hstry::agent_message_to_proto;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let stats_delta = compute_stats_delta(messages);
        let metadata_json =
            build_metadata_json(client, hstry_external_id, octo_session_id, work_dir, stats_delta)
                .await?;

        // Convert messages to proto format.
        // Use AppendMessages if the conversation likely exists (most common case),
        // falling back to WriteConversation if not found.
        let proto_messages: Vec<_> = messages
            .iter()
            .enumerate()
            .map(|(i, msg)| agent_message_to_proto(msg, i as i32))
            .collect();

        // Try append first (fast path -- conversation already exists)
        match client
            .append_messages(hstry_external_id, proto_messages.clone(), Some(now_ms))
            .await
        {
            Ok(_) => {}
            Err(_) => {
                // Conversation doesn't exist yet -- create it with WriteConversation
                let model = messages.iter().rev().find_map(|m| m.model.clone());
                let provider = messages.iter().rev().find_map(|m| m.provider.clone());

                // Use Octo UUID as readable_id so hstry lookups by Octo
                // session_id still work (the query does external_id OR
                // readable_id OR id matching).
                let octo_readable_id = if octo_session_id != hstry_external_id {
                    Some(octo_session_id.to_string())
                } else {
                    None
                };

                client
                    .write_conversation(
                        hstry_external_id,
                        None, // title comes from Pi auto-rename extension via JSONL
                        Some(work_dir.to_string_lossy().to_string()),
                        model,
                        provider,
                        Some(metadata_json.clone()),
                        proto_messages,
                        now_ms,
                        Some(now_ms),
                        Some("pi".to_string()),
                        octo_readable_id,
                    )
                    .await?;
            }
        }

        if stats_delta.is_some() {
            let _ = client
                .update_conversation(
                    hstry_external_id,
                    None,
                    None,
                    None,
                    None,
                    Some(metadata_json),
                    None,
                    Some("pi".to_string()),
                )
                .await;
        }

        info!(
            "Persisted {} messages to hstry (external_id='{}', octo_session='{}')",
            messages.len(),
            hstry_external_id,
            octo_session_id,
        );

        Ok(())
    }
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
/// `hstry_external_id` is the key in hstry (Pi native ID or Octo UUID).
/// `octo_session_id` is always Octo's UUID -- stored in metadata so we can
/// always map between the two identifiers.
async fn build_metadata_json(
    client: &HstryClient,
    hstry_external_id: &str,
    octo_session_id: &str,
    work_dir: &PathBuf,
    delta: Option<StatsDelta>,
) -> Result<String> {
    let mut metadata = serde_json::Map::new();

    if let Ok(Some(conversation)) = client.get_conversation(hstry_external_id, None).await {
        if !conversation.metadata_json.trim().is_empty() {
            if let Ok(serde_json::Value::Object(existing)) =
                serde_json::from_str::<serde_json::Value>(&conversation.metadata_json)
            {
                metadata = existing;
            }
        }
    }

    // Always store the Octo session ID so we can map back from Pi native ID
    metadata.insert(
        "octo_session_id".to_string(),
        serde_json::Value::String(octo_session_id.to_string()),
    );
    metadata.insert(
        "workdir".to_string(),
        serde_json::Value::String(work_dir.to_string_lossy().to_string()),
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

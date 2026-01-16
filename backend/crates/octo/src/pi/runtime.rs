//! Pi runtime abstraction for different execution environments.
//!
//! This module provides a trait-based abstraction for running Pi processes
//! in different isolation modes:
//!
//! - **Local**: Direct subprocess on the host (single-user mode)
//! - **Runner**: Via octo-runner daemon for multi-user isolation
//! - **Container**: HTTP client to pi-bridge running inside a container
//!
//! The abstraction allows `MainChatPiService` to work uniformly across
//! all deployment modes while providing proper process isolation.

use anyhow::{Context, Result};
use async_trait::async_trait;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

use super::PiClientConfig;
use super::types::*;

/// Configuration for spawning a Pi process.
#[derive(Debug, Clone)]
pub struct PiSpawnConfig {
    /// Working directory for the Pi process.
    pub work_dir: PathBuf,
    /// Path to the Pi executable.
    pub pi_executable: String,
    /// Whether to continue the previous session.
    pub continue_session: bool,
    /// Specific session file to use (--session <path>).
    pub session_file: Option<PathBuf>,
    /// Provider to use (e.g., "anthropic", "openai").
    pub provider: Option<String>,
    /// Model to use.
    pub model: Option<String>,
    /// Extension files to load.
    pub extensions: Vec<String>,
    /// Additional files to append to system prompt.
    pub append_system_prompt: Vec<PathBuf>,
    /// Environment variables to set.
    pub env: HashMap<String, String>,
}

impl Default for PiSpawnConfig {
    fn default() -> Self {
        Self {
            work_dir: PathBuf::from("."),
            pi_executable: "pi".to_string(),
            continue_session: false,
            session_file: None,
            provider: None,
            model: None,
            extensions: Vec::new(),
            append_system_prompt: Vec::new(),
            env: HashMap::new(),
        }
    }
}

/// Abstract interface for a running Pi process.
///
/// Implementations handle the details of how commands are sent and
/// events are received, whether via direct stdin/stdout, Unix socket
/// RPC to a runner, or HTTP to a container bridge.
#[async_trait]
pub trait PiProcess: Send + Sync {
    /// Send a command to Pi and wait for the response.
    async fn send_command(&self, command: PiCommand) -> Result<PiResponse>;

    /// Subscribe to events from Pi.
    fn subscribe(&self) -> broadcast::Receiver<PiEvent>;
}

/// Runtime for spawning and managing Pi processes.
///
/// Different implementations provide isolation appropriate to the
/// deployment mode (single-user, multi-user, container).
#[async_trait]
pub trait PiRuntime: Send + Sync {
    /// Spawn a new Pi process with the given configuration.
    async fn spawn(&self, config: PiSpawnConfig) -> Result<Box<dyn PiProcess>>;
}

// ============================================================================
// Local Runtime - Direct subprocess on host
// ============================================================================

/// Runtime that spawns Pi as a direct subprocess.
///
/// This is used for single-user mode where no isolation is needed.
/// Pi runs as the same user as the Octo backend.
#[derive(Debug, Default)]
pub struct LocalPiRuntime {
    config: PiClientConfig,
}

impl LocalPiRuntime {
    pub fn new() -> Self {
        Self {
            config: PiClientConfig::default(),
        }
    }
}

#[async_trait]
impl PiRuntime for LocalPiRuntime {
    async fn spawn(&self, config: PiSpawnConfig) -> Result<Box<dyn PiProcess>> {
        info!(
            "Spawning local Pi process in {:?}, continue={}",
            config.work_dir, config.continue_session
        );

        // Build the command
        let mut cmd = Command::new(&config.pi_executable);
        cmd.arg("--mode").arg("rpc");

        // Session handling: specific file takes precedence over continue
        if let Some(ref session_file) = config.session_file {
            cmd.arg("--session").arg(session_file);
        } else if config.continue_session {
            cmd.arg("--continue");
        }

        cmd.current_dir(&config.work_dir);

        // Add provider/model if configured
        if let Some(ref provider) = config.provider {
            cmd.arg("--provider").arg(provider);
        }
        if let Some(ref model) = config.model {
            cmd.arg("--model").arg(model);
        }

        // Add extensions
        for extension in &config.extensions {
            cmd.arg("--extension").arg(extension);
        }

        // Add system prompt files
        for file in &config.append_system_prompt {
            if file.exists() {
                cmd.arg("--append-system-prompt").arg(file);
            }
        }

        // Set environment
        cmd.envs(&config.env);

        // Set up stdio
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Spawn the process
        let child = cmd.spawn().with_context(|| {
            format!(
                "Failed to spawn Pi process. Executable: {}, Working dir: {:?}",
                config.pi_executable, config.work_dir
            )
        })?;

        let process = LocalPiProcess::new(child, self.config.clone())?;
        Ok(Box::new(process))
    }
}

/// A Pi process running as a local subprocess.
pub struct LocalPiProcess {
    /// Channel to send commands to pi.
    command_tx: mpsc::Sender<String>,
    /// Broadcast channel for events from pi.
    event_tx: broadcast::Sender<PiEvent>,
    /// Pending response receivers (keyed by request ID).
    pending_responses: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<PiResponse>>>>,
    /// Counter for generating unique request IDs.
    request_counter: Arc<Mutex<u64>>,
    /// Handle to the background tasks (kept alive).
    _handles: Vec<tokio::task::JoinHandle<()>>,
}

impl LocalPiProcess {
    fn new(mut child: Child, config: PiClientConfig) -> Result<Self> {
        let stdin = child.stdin.take().context("pi process has no stdin")?;
        let stdout = child.stdout.take().context("pi process has no stdout")?;

        let (command_tx, command_rx) = mpsc::channel::<String>(config.command_buffer_size);
        let (event_tx, _) = broadcast::channel::<PiEvent>(config.event_buffer_size);
        let pending_responses = Arc::new(RwLock::new(HashMap::new()));
        // Spawn stdin writer task
        let stdin_handle = tokio::spawn(Self::stdin_writer_task(stdin, command_rx));

        // Spawn stdout reader task
        let stdout_handle = tokio::spawn(Self::stdout_reader_task(
            stdout,
            event_tx.clone(),
            Arc::clone(&pending_responses),
        ));

        // Spawn stderr reader task (just for logging)
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(Self::stderr_reader_task(stderr));
        }

        Ok(Self {
            command_tx,
            event_tx,
            pending_responses,
            request_counter: Arc::new(Mutex::new(0)),
            _handles: vec![stdin_handle, stdout_handle],
        })
    }

    async fn next_request_id(&self) -> String {
        let mut counter = self.request_counter.lock().await;
        *counter += 1;
        format!("req-{}", *counter)
    }

    fn serialize_command_with_id(&self, command: &PiCommand, id: &str) -> Result<String> {
        let mut value = serde_json::to_value(command).context("failed to serialize command")?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("id".to_string(), serde_json::Value::String(id.to_string()));
        }
        serde_json::to_string(&value).context("failed to stringify command")
    }

    async fn stdin_writer_task(
        mut stdin: tokio::process::ChildStdin,
        mut command_rx: mpsc::Receiver<String>,
    ) {
        info!("Pi stdin writer task started");
        while let Some(command) = command_rx.recv().await {
            let line = format!("{}\n", command);
            let display_cmd: String = command.chars().take(200).collect();
            debug!("Sending to pi: {}", display_cmd);
            if let Err(e) = stdin.write_all(line.as_bytes()).await {
                error!("Failed to write to pi stdin: {:?}", e);
                break;
            }
            if let Err(e) = stdin.flush().await {
                error!("Failed to flush pi stdin: {:?}", e);
                break;
            }
        }
        info!("Pi stdin writer task ended");
    }

    async fn stdout_reader_task(
        stdout: tokio::process::ChildStdout,
        event_tx: broadcast::Sender<PiEvent>,
        pending_responses: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<PiResponse>>>>,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        info!("Pi stdout reader task started");

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let display_line: String = line.chars().take(200).collect();
            debug!("Received from pi: {}", display_line);

            match PiMessage::parse(&line) {
                Ok(PiMessage::Response(response)) => {
                    debug!(
                        "Parsed as response, id={:?}, success={}",
                        response.id, response.success
                    );
                    if let Some(ref id) = response.id {
                        let mut pending = pending_responses.write().await;
                        if let Some(tx) = pending.remove(id) {
                            let _ = tx.send(response);
                        } else {
                            warn!("Received response for unknown request ID: {}", id);
                        }
                    } else {
                        warn!("Response has no ID: {:?}", response);
                    }
                }
                Ok(PiMessage::Event(event)) => {
                    let _ = event_tx.send(event);
                }
                Err(e) => {
                    let display_line: String = line.chars().take(200).collect();
                    warn!(
                        "Failed to parse pi message: {:?}, line: {}",
                        e, display_line
                    );
                }
            }
        }

        info!("Pi stdout reader task ended");
    }

    async fn stderr_reader_task(stderr: tokio::process::ChildStderr) {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if !line.trim().is_empty() {
                warn!("Pi stderr: {}", line);
            }
        }
        info!("Pi stderr reader task ended");
    }
}

#[async_trait]
impl PiProcess for LocalPiProcess {
    async fn send_command(&self, command: PiCommand) -> Result<PiResponse> {
        let request_id = self.next_request_id().await;
        let json = self.serialize_command_with_id(&command, &request_id)?;

        // Set up response channel
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending_responses.write().await;
            pending.insert(request_id.clone(), response_tx);
        }

        // Send command
        self.command_tx
            .send(json)
            .await
            .context("failed to send command to pi")?;

        // Wait for response with timeout
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), response_rx)
            .await
            .context("timeout waiting for pi response")?
            .context("response channel closed")?;

        Ok(response)
    }

    fn subscribe(&self) -> broadcast::Receiver<PiEvent> {
        self.event_tx.subscribe()
    }
}

// ============================================================================
// Runner Runtime - Via octo-runner for multi-user isolation
// ============================================================================

use crate::runner::client::RunnerClient;

/// Runtime that spawns Pi via the octo-runner daemon.
///
/// This is used for multi-user mode where Pi needs to run as a separate
/// Linux user for isolation. Communication happens via the runner's
/// Unix socket using WriteStdin/ReadStdout RPC.
pub struct RunnerPiRuntime {
    /// The runner client for communicating with the daemon.
    client: RunnerClient,
    /// Client config for buffer sizes.
    config: PiClientConfig,
}

impl RunnerPiRuntime {
    pub fn new(client: RunnerClient) -> Self {
        Self {
            client,
            config: PiClientConfig::default(),
        }
    }
}

#[async_trait]
impl PiRuntime for RunnerPiRuntime {
    async fn spawn(&self, config: PiSpawnConfig) -> Result<Box<dyn PiProcess>> {
        info!(
            "Spawning Pi process via runner in {:?}, continue={}",
            config.work_dir, config.continue_session
        );

        // Build arguments for pi command
        let mut args = vec!["--mode".to_string(), "rpc".to_string()];

        // Session handling: specific file takes precedence over continue
        if let Some(ref session_file) = config.session_file {
            args.push("--session".to_string());
            args.push(session_file.to_string_lossy().to_string());
        } else if config.continue_session {
            args.push("--continue".to_string());
        }

        if let Some(ref provider) = config.provider {
            args.push("--provider".to_string());
            args.push(provider.clone());
        }

        if let Some(ref model) = config.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }

        for extension in &config.extensions {
            args.push("--extension".to_string());
            args.push(extension.clone());
        }

        for file in &config.append_system_prompt {
            if file.exists() {
                args.push("--append-system-prompt".to_string());
                args.push(file.to_string_lossy().to_string());
            }
        }

        // Generate a unique process ID
        let process_id = format!("pi-{}", uuid::Uuid::new_v4());

        // Spawn via runner
        let pid = self
            .client
            .spawn_rpc_process(
                &process_id,
                &config.pi_executable,
                args,
                &config.work_dir,
                config.env.clone(),
            )
            .await
            .context("failed to spawn Pi via runner")?;

        info!(
            "Spawned Pi process via runner: id={}, pid={}",
            process_id, pid
        );

        let process = RunnerPiProcess::new(self.client.clone(), process_id, self.config.clone());

        Ok(Box::new(process))
    }
}

impl std::fmt::Debug for RunnerPiRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerPiRuntime")
            .field("client", &self.client)
            .finish()
    }
}

/// A Pi process running via the octo-runner daemon.
///
/// Communication happens through WriteStdin/ReadStdout RPC calls
/// to the runner, which forwards to the Pi process.
pub struct RunnerPiProcess {
    /// Channel to send commands to the writer task.
    command_tx: mpsc::Sender<String>,
    /// Broadcast channel for events from pi.
    event_tx: broadcast::Sender<PiEvent>,
    /// Pending response receivers (keyed by request ID).
    pending_responses: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<PiResponse>>>>,
    /// Counter for generating unique request IDs.
    request_counter: Arc<Mutex<u64>>,
    /// Handle to the background reader task.
    _reader_handle: tokio::task::JoinHandle<()>,
}

impl RunnerPiProcess {
    fn new(client: RunnerClient, process_id: String, config: PiClientConfig) -> Self {
        let (command_tx, mut command_rx) = mpsc::channel::<String>(config.command_buffer_size);
        let (event_tx, _) = broadcast::channel::<PiEvent>(config.event_buffer_size);
        let pending_responses = Arc::new(RwLock::new(HashMap::new()));
        let running = Arc::new(RwLock::new(true));

        // Spawn writer task that sends commands via runner
        let client_clone = client.clone();
        let process_id_clone = process_id.clone();
        let running_clone = Arc::clone(&running);
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                let data = format!("{}\n", command);
                if let Err(e) = client_clone.write_stdin(&process_id_clone, &data).await {
                    error!("Failed to write to Pi via runner: {:?}", e);
                    *running_clone.write().await = false;
                    break;
                }
            }
            info!("Runner Pi writer task ended");
        });

        // Spawn reader task that subscribes to stdout via runner (push-based, like local mode)
        let client_clone2 = client.clone();
        let process_id_clone2 = process_id.clone();
        let event_tx_clone = event_tx.clone();
        let pending_clone = Arc::clone(&pending_responses);
        let running_clone2 = Arc::clone(&running);
        let reader_handle = tokio::spawn(async move {
            // Subscribe to stdout stream
            match client_clone2.subscribe_stdout(&process_id_clone2).await {
                Ok(mut subscription) => {
                    // Read lines as they arrive (push-based, no polling)
                    while let Some(event) = subscription.next().await {
                        match event {
                            crate::runner::client::StdoutSubscriptionEvent::Line(line) => {
                                if line.trim().is_empty() {
                                    continue;
                                }
                                Self::process_line(&line, &event_tx_clone, &pending_clone).await;
                            }
                            crate::runner::client::StdoutSubscriptionEvent::End { .. } => {
                                info!("Pi process exited via runner subscription");
                                *running_clone2.write().await = false;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to subscribe to Pi stdout via runner: {:?}", e);
                    *running_clone2.write().await = false;
                }
            }

            info!("Runner Pi reader task ended");
        });

        Self {
            command_tx,
            event_tx,
            pending_responses,
            request_counter: Arc::new(Mutex::new(0)),
            _reader_handle: reader_handle,
        }
    }

    async fn process_line(
        line: &str,
        event_tx: &broadcast::Sender<PiEvent>,
        pending_responses: &Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<PiResponse>>>>,
    ) {
        let display_line: String = line.chars().take(200).collect();
        debug!("Received from pi via runner: {}", display_line);

        match PiMessage::parse(line) {
            Ok(PiMessage::Response(response)) => {
                if let Some(ref id) = response.id {
                    let mut pending = pending_responses.write().await;
                    if let Some(tx) = pending.remove(id) {
                        let _ = tx.send(response);
                    } else {
                        warn!("Received response for unknown request ID: {}", id);
                    }
                }
            }
            Ok(PiMessage::Event(event)) => {
                let _ = event_tx.send(event);
            }
            Err(e) => {
                warn!(
                    "Failed to parse pi message: {:?}, line: {}",
                    e, display_line
                );
            }
        }
    }

    async fn next_request_id(&self) -> String {
        let mut counter = self.request_counter.lock().await;
        *counter += 1;
        format!("req-{}", *counter)
    }

    fn serialize_command_with_id(&self, command: &PiCommand, id: &str) -> Result<String> {
        let mut value = serde_json::to_value(command).context("failed to serialize command")?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("id".to_string(), serde_json::Value::String(id.to_string()));
        }
        serde_json::to_string(&value).context("failed to stringify command")
    }
}

#[async_trait]
impl PiProcess for RunnerPiProcess {
    async fn send_command(&self, command: PiCommand) -> Result<PiResponse> {
        let request_id = self.next_request_id().await;
        let json = self.serialize_command_with_id(&command, &request_id)?;

        // Set up response channel
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending_responses.write().await;
            pending.insert(request_id.clone(), response_tx);
        }

        // Send command via the writer task
        self.command_tx
            .send(json)
            .await
            .context("failed to send command to runner pi writer")?;

        // Wait for response with timeout
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), response_rx)
            .await
            .context("timeout waiting for pi response via runner")?
            .context("response channel closed")?;

        Ok(response)
    }

    fn subscribe(&self) -> broadcast::Receiver<PiEvent> {
        self.event_tx.subscribe()
    }
}

// ============================================================================
// Container Runtime - HTTP client to pi-bridge in container
// ============================================================================

/// Runtime that connects to pi-bridge running inside a container.
///
/// This is used for container mode where Pi runs inside the same container
/// as opencode. Communication happens via HTTP to the pi-bridge service
/// which bridges to Pi's stdin/stdout.
pub struct ContainerPiRuntime {
    /// HTTP client for making requests.
    http_client: reqwest::Client,
    /// Client config for buffer sizes.
    config: PiClientConfig,
}

impl ContainerPiRuntime {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
            config: PiClientConfig::default(),
        }
    }
}

impl Default for ContainerPiRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PiRuntime for ContainerPiRuntime {
    async fn spawn(&self, config: PiSpawnConfig) -> Result<Box<dyn PiProcess>> {
        // For container mode, we expect the pi-bridge to already be running
        // in the container. The "spawn" here just creates the client connection.

        // Extract bridge URL from env if present, or construct from config
        let bridge_url = config
            .env
            .get("PI_BRIDGE_URL")
            .cloned()
            .unwrap_or_else(|| "http://localhost:41824".to_string());

        info!("Connecting to pi-bridge at {} (container mode)", bridge_url);

        // Check if pi-bridge is healthy
        let health_url = format!("{}/health", bridge_url);
        let response = self
            .http_client
            .get(&health_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .context("failed to connect to pi-bridge")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "pi-bridge health check failed with status: {}",
                response.status()
            );
        }

        let process =
            ContainerPiProcess::new(self.http_client.clone(), bridge_url, self.config.clone());

        Ok(Box::new(process))
    }
}

impl std::fmt::Debug for ContainerPiRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerPiRuntime").finish()
    }
}

/// A Pi process accessed via pi-bridge HTTP API in a container.
pub struct ContainerPiProcess {
    /// HTTP client for requests.
    http_client: reqwest::Client,
    /// Base URL for pi-bridge.
    bridge_url: String,
    /// Broadcast channel for events.
    event_tx: broadcast::Sender<PiEvent>,
    /// Counter for generating unique request IDs.
    request_counter: Arc<Mutex<u64>>,
    /// WebSocket connection handle (if connected).
    _ws_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ContainerPiProcess {
    fn new(http_client: reqwest::Client, bridge_url: String, config: PiClientConfig) -> Self {
        let (event_tx, _) = broadcast::channel::<PiEvent>(config.event_buffer_size);
        let running = Arc::new(RwLock::new(true));

        // Start WebSocket connection for events
        let ws_url = bridge_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        let ws_url = format!("{}/ws", ws_url);
        let event_tx_clone = event_tx.clone();
        let running_clone = Arc::clone(&running);

        let ws_handle = tokio::spawn(async move {
            loop {
                match Self::connect_websocket(&ws_url, &event_tx_clone, &running_clone).await {
                    Ok(()) => {
                        info!("WebSocket connection closed, reconnecting...");
                    }
                    Err(e) => {
                        warn!("WebSocket connection error: {:?}, reconnecting...", e);
                    }
                }

                if !*running_clone.read().await {
                    break;
                }

                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        Self {
            http_client,
            bridge_url,
            event_tx,
            request_counter: Arc::new(Mutex::new(0)),
            _ws_handle: Some(ws_handle),
        }
    }

    async fn connect_websocket(
        ws_url: &str,
        event_tx: &broadcast::Sender<PiEvent>,
        running: &Arc<RwLock<bool>>,
    ) -> Result<()> {
        use tokio_tungstenite::connect_async;
        use tokio_tungstenite::tungstenite::Message;

        let (ws_stream, _) = connect_async(ws_url)
            .await
            .context("WebSocket connect failed")?;
        let (_, mut read) = ws_stream.split();

        use futures::StreamExt;
        while let Some(msg) = read.next().await {
            if !*running.read().await {
                break;
            }

            match msg {
                Ok(Message::Text(text)) => {
                    // Try to parse as Pi event
                    match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(value) => {
                            // Check if it's an event (has "type" field but not "success")
                            if value.get("type").is_some() && value.get("success").is_none() {
                                if let Ok(event) = serde_json::from_value::<PiEvent>(value) {
                                    let _ = event_tx.send(event);
                                }
                            }
                        }
                        Err(e) => {
                            debug!("Failed to parse WebSocket message: {:?}", e);
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    warn!("WebSocket error: {:?}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn next_request_id(&self) -> String {
        let mut counter = self.request_counter.lock().await;
        *counter += 1;
        format!("container-req-{}", *counter)
    }
}

#[async_trait]
impl PiProcess for ContainerPiProcess {
    async fn send_command(&self, command: PiCommand) -> Result<PiResponse> {
        let request_id = self.next_request_id().await;

        // Serialize command with ID
        let mut value = serde_json::to_value(&command).context("failed to serialize command")?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "id".to_string(),
                serde_json::Value::String(request_id.clone()),
            );
        }

        // Send via HTTP
        let url = format!("{}/command", self.bridge_url);
        let response = self
            .http_client
            .post(&url)
            .json(&value)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("failed to send command to pi-bridge")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "pi-bridge command failed with status: {}",
                response.status()
            );
        }

        // Parse response
        let body: serde_json::Value = response.json().await.context("failed to parse response")?;

        let success = body
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let data = body.get("data").cloned();
        let error = body.get("error").and_then(|v| v.as_str()).map(String::from);

        Ok(PiResponse {
            success,
            id: Some(request_id),
            data,
            error,
        })
    }

    fn subscribe(&self) -> broadcast::Receiver<PiEvent> {
        self.event_tx.subscribe()
    }
}

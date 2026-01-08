//! Pi RPC client.
//!
//! Manages communication with a pi subprocess via stdin/stdout.

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};

use super::types::*;

/// Configuration for the Pi client.
#[derive(Debug, Clone)]
pub struct PiClientConfig {
    /// Buffer size for the event broadcast channel.
    pub event_buffer_size: usize,
    /// Buffer size for the command channel.
    pub command_buffer_size: usize,
}

impl Default for PiClientConfig {
    fn default() -> Self {
        Self {
            event_buffer_size: 256,
            command_buffer_size: 64,
        }
    }
}

/// Client for communicating with a pi subprocess.
pub struct PiClient {
    /// Channel to send commands to pi.
    command_tx: mpsc::Sender<String>,
    /// Broadcast channel for events from pi.
    event_tx: broadcast::Sender<PiEvent>,
    /// Pending response receivers (keyed by request ID).
    pending_responses: Arc<RwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<PiResponse>>>>,
    /// Counter for generating unique request IDs.
    request_counter: Arc<Mutex<u64>>,
    /// Handle to the background tasks.
    _handles: Vec<tokio::task::JoinHandle<()>>,
}

impl PiClient {
    /// Create a new Pi client from a child process.
    ///
    /// Takes ownership of the child's stdin/stdout for communication.
    pub fn new(mut child: Child, config: PiClientConfig) -> Result<Self> {
        let stdin = child.stdin.take().context("pi process has no stdin")?;
        let stdout = child.stdout.take().context("pi process has no stdout")?;

        let (command_tx, command_rx) = mpsc::channel::<String>(config.command_buffer_size);
        let (event_tx, _) = broadcast::channel::<PiEvent>(config.event_buffer_size);
        let pending_responses = Arc::new(RwLock::new(std::collections::HashMap::new()));

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

    /// Generate a unique request ID.
    async fn next_request_id(&self) -> String {
        let mut counter = self.request_counter.lock().await;
        *counter += 1;
        format!("req-{}", *counter)
    }

    /// Send a command to pi and wait for the response.
    pub async fn send_command(&self, command: PiCommand) -> Result<PiResponse> {
        // Generate request ID
        let request_id = self.next_request_id().await;

        // Serialize command with the ID
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

    /// Subscribe to events from pi.
    pub fn subscribe(&self) -> broadcast::Receiver<PiEvent> {
        self.event_tx.subscribe()
    }

    /// Send a prompt to pi.
    pub async fn prompt(&self, message: &str) -> Result<PiResponse> {
        self.send_command(PiCommand::Prompt {
            id: None,
            message: message.to_string(),
        })
        .await
    }

    /// Abort the current operation.
    pub async fn abort(&self) -> Result<PiResponse> {
        self.send_command(PiCommand::Abort { id: None }).await
    }

    /// Queue a steering message to interrupt the agent mid-run.
    pub async fn steer(&self, message: &str) -> Result<PiResponse> {
        self.send_command(PiCommand::Steer {
            id: None,
            message: message.to_string(),
        })
        .await
    }

    /// Queue a follow-up message for after the agent finishes.
    pub async fn follow_up(&self, message: &str) -> Result<PiResponse> {
        self.send_command(PiCommand::FollowUp {
            id: None,
            message: message.to_string(),
        })
        .await
    }

    /// Get current state.
    pub async fn get_state(&self) -> Result<PiState> {
        let response = self.send_command(PiCommand::GetState { id: None }).await?;
        if !response.success {
            anyhow::bail!("get_state failed: {:?}", response.error);
        }
        let data = response.data.context("get_state returned no data")?;
        serde_json::from_value(data).context("failed to parse state")
    }

    /// Get all messages.
    pub async fn get_messages(&self) -> Result<Vec<AgentMessage>> {
        let response = self
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

    /// Set model.
    pub async fn set_model(&self, provider: &str, model_id: &str) -> Result<PiResponse> {
        self.send_command(PiCommand::SetModel {
            id: None,
            provider: provider.to_string(),
            model_id: model_id.to_string(),
        })
        .await
    }

    /// List all available models.
    pub async fn get_available_models(&self) -> Result<Vec<PiModel>> {
        let response = self
            .send_command(PiCommand::GetAvailableModels { id: None })
            .await?;
        if !response.success {
            anyhow::bail!("get_available_models failed: {:?}", response.error);
        }
        let data = response
            .data
            .context("get_available_models returned no data")?;
        let models = data
            .get("models")
            .context("no models field in response")?;
        serde_json::from_value(models.clone()).context("failed to parse models")
    }

    /// Compact context.
    pub async fn compact(&self, custom_instructions: Option<&str>) -> Result<CompactionResult> {
        let response = self
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

    /// Start a new session.
    pub async fn new_session(&self) -> Result<PiResponse> {
        self.send_command(PiCommand::NewSession {
            id: None,
            parent_session: None,
        })
        .await
    }

    /// Get session stats.
    pub async fn get_session_stats(&self) -> Result<SessionStats> {
        let response = self
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

    // ========================================================================
    // Internal helper methods
    // ========================================================================

    fn serialize_command_with_id(&self, command: &PiCommand, id: &str) -> Result<String> {
        // Serialize to Value first, then inject ID
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
            // Safely truncate for logging, respecting Unicode char boundaries
            let display_cmd: String = command.chars().take(200).collect();
            info!("Sending to pi: {}", display_cmd);
            if let Err(e) = stdin.write_all(line.as_bytes()).await {
                error!("Failed to write to pi stdin: {:?}", e);
                break;
            }
            if let Err(e) = stdin.flush().await {
                error!("Failed to flush pi stdin: {:?}", e);
                break;
            }
            info!("Successfully sent command to pi");
        }
        info!("Pi stdin writer task ended");
    }

    async fn stdout_reader_task(
        stdout: tokio::process::ChildStdout,
        event_tx: broadcast::Sender<PiEvent>,
        pending_responses: Arc<RwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<PiResponse>>>>,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        info!("Pi stdout reader task started");

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            // Safely truncate for logging, respecting Unicode char boundaries
            let display_line: String = line.chars().take(200).collect();
            info!("Received from pi: {}", display_line);

            match PiMessage::parse(&line) {
                Ok(PiMessage::Response(response)) => {
                    info!("Parsed as response, id={:?}, success={}", response.id, response.success);
                    // If response has an ID, send to waiting receiver
                    if let Some(ref id) = response.id {
                        let mut pending = pending_responses.write().await;
                        let pending_count = pending.len();
                        info!("Looking for request ID {} in {} pending requests", id, pending_count);
                        if let Some(tx) = pending.remove(id) {
                            info!("Found pending request, sending response");
                            let _ = tx.send(response);
                        } else {
                            warn!("Received response for unknown request ID: {}", id);
                        }
                    } else {
                        warn!("Response has no ID: {:?}", response);
                    }
                }
                Ok(PiMessage::Event(event)) => {
                    // Broadcast event to subscribers
                    debug!("Parsed as event: {:?}", std::any::type_name_of_val(&event));
                    let _ = event_tx.send(event);
                }
                Err(e) => {
                    // Safely truncate for logging, respecting Unicode char boundaries
                    let display_line: String = line.chars().take(200).collect();
                    warn!("Failed to parse pi message: {:?}, line: {}", e, display_line);
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

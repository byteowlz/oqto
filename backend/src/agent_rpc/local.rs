//! Local backend implementation for AgentRPC.
//!
//! This backend runs opencode as native processes on the host, suitable for:
//! - Single-user local development
//! - Multi-user mode with systemd user services
//! - Lightweight deployments without containers

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use log::{debug, info, warn};
use tokio::sync::RwLock;

use super::{
    AgentBackend, AgentEvent, AgentEventStream, Conversation, HealthStatus, Message,
    SendMessageRequest, SessionHandle, StartSessionOpts,
};
use crate::history;
use crate::local::{LocalRuntime, LocalRuntimeConfig};

/// Configuration for the local agent backend.
#[derive(Debug, Clone)]
pub struct LocalBackendConfig {
    /// Local runtime configuration
    pub runtime: LocalRuntimeConfig,
    /// Base data directory for all users (e.g., /var/lib/octo/users)
    /// Each user gets {data_dir}/{user_id}/.local/share/opencode/
    pub data_dir: PathBuf,
    /// Base port for allocating session ports
    pub base_port: u16,
    /// Whether to use single-user mode (no user isolation)
    pub single_user: bool,
}

impl Default for LocalBackendConfig {
    fn default() -> Self {
        Self {
            runtime: LocalRuntimeConfig::default(),
            data_dir: dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("/var/lib/octo"))
                .join("octo/users"),
            base_port: 41820,
            single_user: true,
        }
    }
}

/// Active session tracking.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ActiveSession {
    session_id: String,
    user_id: String,
    workdir: PathBuf,
    opencode_port: u16,
    ttyd_port: u16,
    fileserver_port: u16,
}

/// Local backend for running opencode as native processes.
pub struct LocalBackend {
    config: LocalBackendConfig,
    runtime: LocalRuntime,
    /// Track active sessions by session_id
    sessions: Arc<RwLock<HashMap<String, ActiveSession>>>,
    /// Next available port
    next_port: Arc<RwLock<u16>>,
}

impl LocalBackend {
    /// Create a new local backend.
    pub fn new(config: LocalBackendConfig) -> Result<Self> {
        let mut runtime_config = config.runtime.clone();
        runtime_config.expand_paths();
        runtime_config.validate()?;

        let runtime = LocalRuntime::new(runtime_config);
        let base_port = config.base_port;

        Ok(Self {
            config,
            runtime,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            next_port: Arc::new(RwLock::new(base_port)),
        })
    }

    /// Allocate ports for a new session.
    async fn allocate_ports(&self) -> (u16, u16, u16) {
        let mut port = self.next_port.write().await;
        let opencode_port = *port;
        let ttyd_port = *port + 1;
        let fileserver_port = *port + 2;
        *port += 3;
        (opencode_port, ttyd_port, fileserver_port)
    }

    /// Get the opencode data directory for a user.
    fn opencode_data_dir(&self, user_id: &str) -> PathBuf {
        if self.config.single_user {
            // Single-user mode: use default XDG location
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("opencode")
        } else {
            // Multi-user mode: user-specific directory
            self.config
                .data_dir
                .join(user_id)
                .join(".local/share/opencode")
        }
    }

    /// Build environment variables for a session.
    fn build_env(&self, user_id: &str, opts: &StartSessionOpts) -> HashMap<String, String> {
        let mut env = opts.env.clone();

        // Set XDG_DATA_HOME for multi-user mode
        if !self.config.single_user {
            let user_data = self.config.data_dir.join(user_id).join(".local/share");
            env.insert(
                "XDG_DATA_HOME".to_string(),
                user_data.to_string_lossy().to_string(),
            );
        }

        env
    }

    /// Get session by ID.
    async fn get_session(&self, session_id: &str) -> Option<ActiveSession> {
        self.sessions.read().await.get(session_id).cloned()
    }
}

#[async_trait]
impl AgentBackend for LocalBackend {
    async fn list_conversations(&self, user_id: &str) -> Result<Vec<Conversation>> {
        let opencode_dir = self.opencode_data_dir(user_id);
        debug!("Listing conversations from {:?}", opencode_dir);

        let sessions = history::list_sessions_from_dir(&opencode_dir)
            .unwrap_or_else(|e| {
                warn!("Failed to list sessions from {:?}: {}", opencode_dir, e);
                Vec::new()
            });

        // Check which sessions are currently active
        let active_sessions = self.sessions.read().await;

        Ok(sessions
            .into_iter()
            .map(|s| Conversation {
                id: s.id.clone(),
                title: s.title,
                parent_id: s.parent_id,
                workspace_path: s.workspace_path,
                project_name: s.project_name,
                created_at: s.created_at,
                updated_at: s.updated_at,
                is_active: active_sessions.contains_key(&s.id),
                version: s.version,
            })
            .collect())
    }

    async fn get_conversation(
        &self,
        user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<Conversation>> {
        let conversations = self.list_conversations(user_id).await?;
        Ok(conversations.into_iter().find(|c| c.id == conversation_id))
    }

    async fn get_messages(&self, user_id: &str, conversation_id: &str) -> Result<Vec<Message>> {
        let opencode_dir = self.opencode_data_dir(user_id);

        let messages = history::get_session_messages_from_dir(conversation_id, &opencode_dir)
            .context("reading messages from disk")?;

        // Convert history::ChatMessage to our Message type
        Ok(messages
            .into_iter()
            .map(|m| Message {
                id: m.id,
                session_id: m.session_id,
                role: m.role,
                parts: m
                    .parts
                    .into_iter()
                    .map(|p| match p.part_type.as_str() {
                        "text" => super::MessagePart::Text {
                            text: p.text.unwrap_or_default(),
                        },
                        "tool" => super::MessagePart::Tool {
                            tool: p.tool_name.unwrap_or_default(),
                            call_id: None,
                            state: Some(super::ToolState {
                                status: p.tool_status,
                                input: p.tool_input,
                                output: p.tool_output,
                                title: p.tool_title,
                                metadata: None,
                            }),
                        },
                        "step-start" => super::MessagePart::StepStart,
                        "step-finish" => super::MessagePart::StepFinish {
                            reason: None,
                            cost: None,
                            tokens: None,
                        },
                        _ => super::MessagePart::Unknown,
                    })
                    .collect(),
                created_at: m.created_at,
                completed_at: m.completed_at,
                model: if m.provider_id.is_some() || m.model_id.is_some() {
                    Some(super::MessageModel {
                        provider_id: m.provider_id.unwrap_or_default(),
                        model_id: m.model_id.unwrap_or_default(),
                    })
                } else {
                    None
                },
                tokens: if m.tokens_input.is_some() || m.tokens_output.is_some() {
                    Some(super::TokenUsage {
                        input: m.tokens_input,
                        output: m.tokens_output,
                        reasoning: None,
                        cache: None,
                    })
                } else {
                    None
                },
            })
            .collect())
    }

    async fn start_session(
        &self,
        user_id: &str,
        workdir: &Path,
        opts: StartSessionOpts,
    ) -> Result<SessionHandle> {
        // Check if we already have a session for this workdir
        {
            let sessions = self.sessions.read().await;
            for (sid, session) in sessions.iter() {
                if session.user_id == user_id && session.workdir == workdir {
                    info!("Reusing existing session {} for workdir {:?}", sid, workdir);
                    return Ok(SessionHandle {
                        session_id: sid.clone(),
                        opencode_session_id: opts.resume_session_id.clone(),
                        api_url: format!("http://localhost:{}", session.opencode_port),
                        opencode_port: session.opencode_port,
                        ttyd_port: session.ttyd_port,
                        fileserver_port: session.fileserver_port,
                        workdir: workdir.to_string_lossy().to_string(),
                        is_new: false,
                    });
                }
            }
        }

        // Allocate new ports
        let (opencode_port, ttyd_port, fileserver_port) = self.allocate_ports().await;

        // Generate session ID
        let session_id = format!("local-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

        // Build environment
        let env = self.build_env(user_id, &opts);

        // Ensure workdir exists
        std::fs::create_dir_all(workdir)
            .with_context(|| format!("creating workdir {:?}", workdir))?;

        // Start the session via LocalRuntime
        let _pids = self
            .runtime
            .start_session(
                &session_id,
                user_id,
                workdir,
                opts.agent.as_deref(),
                opts.project_id.as_deref(),
                opencode_port,
                fileserver_port,
                ttyd_port,
                env,
            )
            .await
            .context("starting local session")?;

        // Track the session
        let active_session = ActiveSession {
            session_id: session_id.clone(),
            user_id: user_id.to_string(),
            workdir: workdir.to_path_buf(),
            opencode_port,
            ttyd_port,
            fileserver_port,
        };
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), active_session);

        info!(
            "Started local session {} on ports {}/{}/{}",
            session_id, opencode_port, ttyd_port, fileserver_port
        );

        Ok(SessionHandle {
            session_id,
            opencode_session_id: opts.resume_session_id,
            api_url: format!("http://localhost:{}", opencode_port),
            opencode_port,
            ttyd_port,
            fileserver_port,
            workdir: workdir.to_string_lossy().to_string(),
            is_new: true,
        })
    }

    async fn attach(&self, _user_id: &str, session_id: &str) -> Result<AgentEventStream> {
        let session = self
            .get_session(session_id)
            .await
            .context("session not found")?;

        // Connect to the opencode SSE endpoint
        let url = format!("http://localhost:{}/event", session.opencode_port);

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .context("connecting to opencode SSE")?;

        let stream = response.bytes_stream();

        // Convert to our AgentEventStream
        // This is a simplified implementation - a full one would parse SSE properly
        use futures::StreamExt;
        let event_stream = stream.map(|result| {
            result
                .map(|bytes| AgentEvent {
                    event_type: "message".to_string(),
                    data: String::from_utf8_lossy(&bytes).to_string(),
                })
                .map_err(|e| anyhow::anyhow!("stream error: {}", e))
        });

        Ok(Box::pin(event_stream))
    }

    async fn send_message(
        &self,
        _user_id: &str,
        session_id: &str,
        message: SendMessageRequest,
    ) -> Result<()> {
        let session = self
            .get_session(session_id)
            .await
            .context("session not found")?;

        let url = format!(
            "http://localhost:{}/session/{}/prompt_async",
            session.opencode_port, session_id
        );

        // Build the request body
        let parts: Vec<serde_json::Value> = message
            .parts
            .into_iter()
            .map(|p| match p {
                super::SendMessagePart::Text { text } => {
                    serde_json::json!({"type": "text", "text": text})
                }
                super::SendMessagePart::File {
                    mime,
                    url,
                    filename,
                } => {
                    serde_json::json!({"type": "file", "mime": mime, "url": url, "filename": filename})
                }
                super::SendMessagePart::Agent { name, id } => {
                    serde_json::json!({"type": "agent", "name": name, "id": id})
                }
            })
            .collect();

        let mut body = serde_json::json!({ "parts": parts });
        if let Some(model) = message.model {
            body["model"] = serde_json::json!({
                "providerID": model.provider_id,
                "modelID": model.model_id,
            });
        }

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("sending message to opencode")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("opencode returned {}: {}", status, text);
        }

        Ok(())
    }

    async fn stop_session(&self, _user_id: &str, session_id: &str) -> Result<()> {
        // Stop via runtime
        self.runtime.stop_session(session_id).await?;

        // Remove from tracking
        self.sessions.write().await.remove(session_id);

        info!("Stopped local session {}", session_id);
        Ok(())
    }

    async fn health(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            healthy: true,
            mode: "local".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            details: Some(format!(
                "opencode: {}, ttyd: {}, fileserver: {}",
                self.config.runtime.opencode_binary,
                self.config.runtime.ttyd_binary,
                self.config.runtime.fileserver_binary,
            )),
        })
    }

    async fn get_session_url(&self, _user_id: &str, session_id: &str) -> Result<Option<String>> {
        Ok(self
            .get_session(session_id)
            .await
            .map(|s| format!("http://localhost:{}", s.opencode_port)))
    }

    fn user_data_dir(&self, user_id: &str) -> PathBuf {
        self.opencode_data_dir(user_id)
    }
}

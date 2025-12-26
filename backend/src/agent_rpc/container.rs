//! Container backend implementation for AgentRPC.
//!
//! This backend runs opencode inside Docker/Podman containers, providing:
//! - Full isolation between users
//! - Consistent environment across deployments
//! - Easy resource limits and security boundaries

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use log::{debug, info};
use tokio::sync::RwLock;

use super::{
    AgentBackend, AgentEvent, AgentEventStream, Conversation, HealthStatus, Message,
    SendMessageRequest, SessionHandle, StartSessionOpts,
};
use crate::container::{ContainerConfig, ContainerRuntime, ContainerRuntimeApi};

/// Configuration for the container agent backend.
#[derive(Debug, Clone)]
pub struct ContainerBackendConfig {
    /// Default container image to use
    pub image: String,
    /// Base port for allocating session ports
    pub base_port: u16,
    /// Base directory for user data on the host
    pub data_dir: PathBuf,
    /// Whether to use host network mode
    pub host_network: bool,
    /// Environment variables to pass to all containers
    pub env: HashMap<String, String>,
}

impl Default for ContainerBackendConfig {
    fn default() -> Self {
        Self {
            image: "octo-dev:latest".to_string(),
            base_port: 41820,
            data_dir: PathBuf::from("/var/lib/octo/users"),
            host_network: false,
            env: HashMap::new(),
        }
    }
}

/// Active container session tracking.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ContainerSession {
    session_id: String,
    container_id: String,
    user_id: String,
    workdir: PathBuf,
    opencode_port: u16,
    ttyd_port: u16,
    fileserver_port: u16,
}

/// Container backend for running opencode in Docker/Podman containers.
pub struct ContainerBackend {
    config: ContainerBackendConfig,
    runtime: Arc<dyn ContainerRuntimeApi>,
    /// Track active sessions by session_id
    sessions: Arc<RwLock<HashMap<String, ContainerSession>>>,
    /// Next available port
    next_port: Arc<RwLock<u16>>,
}

impl ContainerBackend {
    /// Create a new container backend.
    pub fn new(config: ContainerBackendConfig, runtime: Arc<dyn ContainerRuntimeApi>) -> Self {
        let base_port = config.base_port;
        Self {
            config,
            runtime,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            next_port: Arc::new(RwLock::new(base_port)),
        }
    }

    /// Create with auto-detected runtime.
    pub fn with_auto_runtime(config: ContainerBackendConfig) -> Self {
        let runtime = Arc::new(ContainerRuntime::new());
        Self::new(config, runtime)
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

    /// Get the host data directory for a user.
    fn user_host_dir(&self, user_id: &str) -> PathBuf {
        self.config.data_dir.join(user_id)
    }

    /// Get session by ID.
    async fn get_session(&self, session_id: &str) -> Option<ContainerSession> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// Query opencode API inside the container.
    async fn query_opencode<T: serde::de::DeserializeOwned>(
        &self,
        session: &ContainerSession,
        path: &str,
    ) -> Result<T> {
        let url = format!("http://localhost:{}{}", session.opencode_port, path);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;

        let response = client.get(&url).send().await.context("querying opencode")?;

        if !response.status().is_success() {
            anyhow::bail!("opencode returned {}", response.status());
        }

        response.json().await.context("parsing opencode response")
    }
}

#[async_trait]
impl AgentBackend for ContainerBackend {
    async fn list_conversations(&self, user_id: &str) -> Result<Vec<Conversation>> {
        // Check if user has an active session
        let sessions = self.sessions.read().await;
        let user_session = sessions.values().find(|s| s.user_id == user_id);

        if let Some(session) = user_session {
            // Query opencode API
            #[derive(serde::Deserialize)]
            #[allow(dead_code)]
            struct OpenCodeSession {
                id: String,
                title: Option<String>,
                #[serde(rename = "parentID")]
                parent_id: Option<String>,
                directory: Option<String>,
                #[serde(rename = "projectID")]
                project_id: Option<String>,
                version: Option<String>,
                time: OpenCodeTime,
            }

            #[derive(serde::Deserialize)]
            struct OpenCodeTime {
                created: i64,
                updated: i64,
            }

            let opencode_sessions: Vec<OpenCodeSession> =
                self.query_opencode(session, "/session").await?;

            Ok(opencode_sessions
                .into_iter()
                .map(|s| {
                    let workspace_path = s.directory.clone().unwrap_or_default();
                    let project_name = crate::history::project_name_from_path(&workspace_path);
                    Conversation {
                        id: s.id.clone(),
                        title: s.title,
                        parent_id: s.parent_id,
                        workspace_path,
                        project_name,
                        created_at: s.time.created,
                        updated_at: s.time.updated,
                        is_active: true,
                        version: s.version,
                    }
                })
                .collect())
        } else {
            // No active session - return empty (could read from mounted volume)
            debug!("No active container session for user {}", user_id);
            Ok(Vec::new())
        }
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
        let sessions = self.sessions.read().await;
        let session = sessions
            .values()
            .find(|s| s.user_id == user_id)
            .context("no active session for user")?;

        // Query opencode messages API
        #[derive(serde::Deserialize)]
        struct OpenCodeMessageWrapper {
            info: OpenCodeMessageInfo,
            parts: Vec<OpenCodePart>,
        }

        #[derive(serde::Deserialize)]
        struct OpenCodeMessageInfo {
            id: String,
            #[serde(rename = "sessionID")]
            session_id: String,
            role: String,
            time: OpenCodeTime,
            #[serde(rename = "modelID")]
            model_id: Option<String>,
            #[serde(rename = "providerID")]
            provider_id: Option<String>,
            tokens: Option<OpenCodeTokens>,
        }

        #[derive(serde::Deserialize)]
        struct OpenCodeTime {
            created: i64,
            completed: Option<i64>,
        }

        #[derive(serde::Deserialize)]
        struct OpenCodeTokens {
            input: Option<i64>,
            output: Option<i64>,
            reasoning: Option<i64>,
            cache: Option<OpenCodeCache>,
        }

        #[derive(serde::Deserialize)]
        struct OpenCodeCache {
            read: Option<i64>,
            write: Option<i64>,
        }

        #[derive(serde::Deserialize)]
        struct OpenCodePart {
            #[serde(rename = "type")]
            part_type: String,
            text: Option<String>,
            tool: Option<String>,
            #[serde(rename = "callID")]
            call_id: Option<String>,
            state: Option<OpenCodeToolState>,
            reason: Option<String>,
            cost: Option<f64>,
            tokens: Option<OpenCodeTokens>,
        }

        #[derive(serde::Deserialize)]
        struct OpenCodeToolState {
            status: Option<String>,
            input: Option<serde_json::Value>,
            output: Option<String>,
            title: Option<String>,
            metadata: Option<serde_json::Value>,
        }

        let messages: Vec<OpenCodeMessageWrapper> = self
            .query_opencode(session, &format!("/session/{}/message", conversation_id))
            .await?;

        Ok(messages
            .into_iter()
            .map(|m| Message {
                id: m.info.id,
                session_id: m.info.session_id,
                role: m.info.role,
                parts: m
                    .parts
                    .into_iter()
                    .map(|p| match p.part_type.as_str() {
                        "text" => super::MessagePart::Text {
                            text: p.text.unwrap_or_default(),
                        },
                        "tool" => super::MessagePart::Tool {
                            tool: p.tool.unwrap_or_default(),
                            call_id: p.call_id,
                            state: p.state.map(|s| super::ToolState {
                                status: s.status,
                                input: s.input,
                                output: s.output,
                                title: s.title,
                                metadata: s.metadata,
                            }),
                        },
                        "step-start" => super::MessagePart::StepStart,
                        "step-finish" => super::MessagePart::StepFinish {
                            reason: p.reason,
                            cost: p.cost,
                            tokens: p.tokens.map(|t| super::TokenUsage {
                                input: t.input,
                                output: t.output,
                                reasoning: t.reasoning,
                                cache: t.cache.map(|c| super::TokenCache {
                                    read: c.read,
                                    write: c.write,
                                }),
                            }),
                        },
                        _ => super::MessagePart::Unknown,
                    })
                    .collect(),
                created_at: m.info.time.created,
                completed_at: m.info.time.completed,
                model: if m.info.provider_id.is_some() || m.info.model_id.is_some() {
                    Some(super::MessageModel {
                        provider_id: m.info.provider_id.unwrap_or_default(),
                        model_id: m.info.model_id.unwrap_or_default(),
                    })
                } else {
                    None
                },
                tokens: m.info.tokens.map(|t| super::TokenUsage {
                    input: t.input,
                    output: t.output,
                    reasoning: t.reasoning,
                    cache: t.cache.map(|c| super::TokenCache {
                        read: c.read,
                        write: c.write,
                    }),
                }),
            })
            .collect())
    }

    async fn start_session(
        &self,
        user_id: &str,
        workdir: &Path,
        opts: StartSessionOpts,
    ) -> Result<SessionHandle> {
        // Check if we already have a session for this user
        {
            let sessions = self.sessions.read().await;
            for (sid, session) in sessions.iter() {
                if session.user_id == user_id {
                    info!("Reusing existing container session {} for user {}", sid, user_id);
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

        // Allocate ports
        let (opencode_port, ttyd_port, fileserver_port) = self.allocate_ports().await;

        // Generate session/container name
        let session_id = format!(
            "octo-{}-{}",
            user_id,
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        // Setup user data directory
        let user_dir = self.user_host_dir(user_id);
        std::fs::create_dir_all(&user_dir)
            .with_context(|| format!("creating user dir {:?}", user_dir))?;

        // Build container config
        let mut env = self.config.env.clone();
        env.extend(opts.env);

        // Set XDG paths inside container
        env.insert("XDG_DATA_HOME".to_string(), "/home/dev/.local/share".to_string());

        let volumes = vec![
            (
                user_dir.to_string_lossy().to_string(),
                "/home/dev".to_string(),
            ),
            (
                workdir.to_string_lossy().to_string(),
                "/workspace".to_string(),
            ),
        ];

        // Port mappings
        let ports = if self.config.host_network {
            vec![]
        } else {
            vec![
                crate::container::PortMapping::new(opencode_port, 41820),
                crate::container::PortMapping::new(ttyd_port, 7681),
                crate::container::PortMapping::new(fileserver_port, 8080),
            ]
        };

        let config = ContainerConfig {
            image: self.config.image.clone(),
            name: Some(session_id.clone()),
            hostname: Some(session_id.clone()),
            ports,
            volumes,
            env,
            workdir: Some("/workspace".to_string()),
            command: vec![],
            network_mode: if self.config.host_network {
                Some("host".to_string())
            } else {
                None
            },
            labels: HashMap::new(),
        };

        // Create and start container
        let container_id = self
            .runtime
            .create_container(&config)
            .await
            .context("creating container")?;

        info!(
            "Started container {} ({}) on ports {}/{}/{}",
            session_id, container_id, opencode_port, ttyd_port, fileserver_port
        );

        // Track session
        let container_session = ContainerSession {
            session_id: session_id.clone(),
            container_id: container_id.clone(),
            user_id: user_id.to_string(),
            workdir: workdir.to_path_buf(),
            opencode_port,
            ttyd_port,
            fileserver_port,
        };
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), container_session);

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
        let session = self
            .get_session(session_id)
            .await
            .context("session not found")?;

        // Stop container
        self.runtime
            .stop_container(&session.container_id, Some(10))
            .await
            .context("stopping container")?;

        // Remove from tracking
        self.sessions.write().await.remove(session_id);

        info!("Stopped container session {}", session_id);
        Ok(())
    }

    async fn health(&self) -> Result<HealthStatus> {
        // Check container runtime
        match self.runtime.list_containers(false).await {
            Ok(_) => Ok(HealthStatus {
                healthy: true,
                mode: "container".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                details: Some(format!("image: {}", self.config.image)),
            }),
            Err(e) => Ok(HealthStatus {
                healthy: false,
                mode: "container".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                details: Some(format!("runtime error: {}", e)),
            }),
        }
    }

    async fn get_session_url(&self, _user_id: &str, session_id: &str) -> Result<Option<String>> {
        Ok(self
            .get_session(session_id)
            .await
            .map(|s| format!("http://localhost:{}", s.opencode_port)))
    }

    fn user_data_dir(&self, user_id: &str) -> PathBuf {
        self.user_host_dir(user_id)
            .join(".local/share/opencode")
    }
}

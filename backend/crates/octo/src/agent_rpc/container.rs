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
                    info!(
                        "Reusing existing container session {} for user {}",
                        sid, user_id
                    );
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
        if let Some(model) = opts.model {
            env.entry("OPENCODE_MODEL".to_string()).or_insert(model);
        }

        // Set XDG paths inside container
        env.insert(
            "XDG_DATA_HOME".to_string(),
            "/home/dev/.local/share".to_string(),
        );

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{Container, ContainerError, ContainerResult, ContainerStats};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use tempfile::tempdir;

    // =========================================================================
    // Mock Container Runtime for Testing
    // =========================================================================

    /// Mock container runtime that doesn't actually create containers.
    struct MockContainerRuntime {
        /// Track created containers
        containers: Arc<RwLock<HashMap<String, MockContainer>>>,
        /// Whether to simulate failures
        fail_create: bool,
        fail_stop: bool,
        fail_list: bool,
    }

    #[derive(Clone)]
    struct MockContainer {
        id: String,
        name: String,
        running: bool,
    }

    impl MockContainerRuntime {
        fn new() -> Self {
            Self {
                containers: Arc::new(RwLock::new(HashMap::new())),
                fail_create: false,
                fail_stop: false,
                fail_list: false,
            }
        }

        fn with_failures(fail_create: bool, fail_stop: bool, fail_list: bool) -> Self {
            Self {
                containers: Arc::new(RwLock::new(HashMap::new())),
                fail_create,
                fail_stop,
                fail_list,
            }
        }
    }

    #[async_trait]
    impl ContainerRuntimeApi for MockContainerRuntime {
        async fn create_container(&self, config: &ContainerConfig) -> ContainerResult<String> {
            if self.fail_create {
                return Err(ContainerError::CommandFailed {
                    command: "create".to_string(),
                    message: "Mock: container creation failed".to_string(),
                });
            }

            let id = format!("mock-{}", uuid::Uuid::new_v4());
            let container = MockContainer {
                id: id.clone(),
                name: config.name.clone().unwrap_or_default(),
                running: true,
            };

            self.containers.write().await.insert(id.clone(), container);
            Ok(id)
        }

        async fn start_container(&self, id: &str) -> ContainerResult<()> {
            let mut containers = self.containers.write().await;
            if let Some(container) = containers.get_mut(id) {
                container.running = true;
                Ok(())
            } else {
                Err(ContainerError::ContainerNotFound(id.to_string()))
            }
        }

        async fn stop_container(&self, id: &str, _timeout: Option<u32>) -> ContainerResult<()> {
            if self.fail_stop {
                return Err(ContainerError::CommandFailed {
                    command: "stop".to_string(),
                    message: "Mock: container stop failed".to_string(),
                });
            }

            let mut containers = self.containers.write().await;
            if let Some(container) = containers.get_mut(id) {
                container.running = false;
                Ok(())
            } else {
                Err(ContainerError::ContainerNotFound(id.to_string()))
            }
        }

        async fn remove_container(&self, id: &str, _force: bool) -> ContainerResult<()> {
            let mut containers = self.containers.write().await;
            if containers.remove(id).is_some() {
                Ok(())
            } else {
                Err(ContainerError::ContainerNotFound(id.to_string()))
            }
        }

        async fn list_containers(&self, _all: bool) -> ContainerResult<Vec<Container>> {
            if self.fail_list {
                return Err(ContainerError::CommandFailed {
                    command: "list".to_string(),
                    message: "Mock: list containers failed".to_string(),
                });
            }

            let containers = self.containers.read().await;
            Ok(containers
                .values()
                .map(|c| {
                    // Use serde to construct Container with proper enum value
                    let state_str = if c.running { "running" } else { "exited" };
                    let json = serde_json::json!({
                        "Id": c.id,
                        "Names": [c.name.clone()],
                        "Image": "mock-image",
                        "State": state_str,
                        "Status": if c.running { "Up 1 hour" } else { "Exited (0) 1 hour ago" },
                        "Created": "2024-01-01T00:00:00Z",
                        "Ports": []
                    });
                    serde_json::from_value(json).unwrap()
                })
                .collect())
        }

        async fn container_state_status(&self, id: &str) -> ContainerResult<Option<String>> {
            let containers = self.containers.read().await;
            if let Some(c) = containers.get(id) {
                Ok(Some(if c.running {
                    "running".to_string()
                } else {
                    "exited".to_string()
                }))
            } else {
                Ok(None)
            }
        }

        async fn get_image_digest(&self, _image: &str) -> ContainerResult<Option<String>> {
            Ok(Some("sha256:mock123".to_string()))
        }

        async fn get_stats(&self, id: &str) -> ContainerResult<ContainerStats> {
            Ok(ContainerStats {
                container_id: id.to_string(),
                name: "mock-container".to_string(),
                cpu_percent: "5.0%".to_string(),
                mem_usage: "100MiB / 1GiB".to_string(),
                mem_percent: "10.0%".to_string(),
                net_io: "1kB / 500B".to_string(),
                block_io: "0B / 0B".to_string(),
                pids: "10".to_string(),
            })
        }

        async fn exec_detached(&self, _id: &str, _cmd: &[&str]) -> ContainerResult<()> {
            Ok(())
        }

        async fn exec_output(&self, _id: &str, _cmd: &[&str]) -> ContainerResult<String> {
            Ok("mock output".to_string())
        }
    }

    // =========================================================================
    // ContainerBackendConfig Tests
    // =========================================================================

    #[test]
    fn test_container_backend_config_default() {
        let config = ContainerBackendConfig::default();

        assert_eq!(config.image, "octo-dev:latest");
        assert_eq!(config.base_port, 41820);
        assert_eq!(config.data_dir, PathBuf::from("/var/lib/octo/users"));
        assert!(!config.host_network);
        assert!(config.env.is_empty());
    }

    #[test]
    fn test_container_backend_config_custom() {
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "secret".to_string());

        let config = ContainerBackendConfig {
            image: "custom-image:v1.0".to_string(),
            base_port: 50000,
            data_dir: PathBuf::from("/custom/data"),
            host_network: true,
            env,
        };

        assert_eq!(config.image, "custom-image:v1.0");
        assert_eq!(config.base_port, 50000);
        assert!(config.host_network);
        assert_eq!(config.env.get("API_KEY").unwrap(), "secret");
    }

    #[test]
    fn test_container_backend_config_clone() {
        let config = ContainerBackendConfig {
            image: "test:latest".to_string(),
            base_port: 45000,
            ..Default::default()
        };

        let cloned = config.clone();
        assert_eq!(cloned.image, config.image);
        assert_eq!(cloned.base_port, config.base_port);
    }

    #[test]
    fn test_container_backend_config_debug() {
        let config = ContainerBackendConfig::default();
        let debug_str = format!("{:?}", config);

        assert!(debug_str.contains("ContainerBackendConfig"));
        assert!(debug_str.contains("image"));
        assert!(debug_str.contains("base_port"));
    }

    // =========================================================================
    // ContainerBackend Construction Tests
    // =========================================================================

    #[test]
    fn test_container_backend_new() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config.clone(), runtime);

        assert_eq!(backend.config.image, config.image);
        assert_eq!(backend.config.base_port, config.base_port);
    }

    // =========================================================================
    // Port Allocation Tests
    // =========================================================================

    #[tokio::test]
    async fn test_allocate_ports_sequential() {
        let config = ContainerBackendConfig {
            base_port: 55000,
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        // First allocation
        let (p1, p2, p3) = backend.allocate_ports().await;
        assert_eq!(p1, 55000);
        assert_eq!(p2, 55001);
        assert_eq!(p3, 55002);

        // Second allocation
        let (p4, p5, p6) = backend.allocate_ports().await;
        assert_eq!(p4, 55003);
        assert_eq!(p5, 55004);
        assert_eq!(p6, 55005);
    }

    #[tokio::test]
    async fn test_allocate_ports_concurrent_unique() {
        let config = ContainerBackendConfig {
            base_port: 56000,
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = Arc::new(ContainerBackend::new(config, runtime));

        let mut handles = vec![];
        for _ in 0..5 {
            let b = Arc::clone(&backend);
            handles.push(tokio::spawn(async move { b.allocate_ports().await }));
        }

        let mut all_ports = Vec::new();
        for handle in handles {
            let (p1, p2, p3) = handle.await.unwrap();
            all_ports.extend([p1, p2, p3]);
        }

        // All 15 ports should be unique
        let unique: std::collections::HashSet<_> = all_ports.iter().collect();
        assert_eq!(unique.len(), 15);
    }

    // =========================================================================
    // User Directory Tests
    // =========================================================================

    #[test]
    fn test_user_host_dir() {
        let config = ContainerBackendConfig {
            data_dir: PathBuf::from("/data/users"),
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        assert_eq!(
            backend.user_host_dir("alice"),
            PathBuf::from("/data/users/alice")
        );
        assert_eq!(
            backend.user_host_dir("bob"),
            PathBuf::from("/data/users/bob")
        );
    }

    #[test]
    fn test_user_host_dir_special_chars() {
        let config = ContainerBackendConfig {
            data_dir: PathBuf::from("/data/users"),
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        // User IDs with special chars (though ideally sanitized upstream)
        assert_eq!(
            backend.user_host_dir("user_123"),
            PathBuf::from("/data/users/user_123")
        );
    }

    // =========================================================================
    // Session Management Tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_session_nonexistent() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let session = backend.get_session("nonexistent").await;
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_session_tracking() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        // Manually add a session
        let session = ContainerSession {
            session_id: "ses_test".to_string(),
            container_id: "container_abc".to_string(),
            user_id: "alice".to_string(),
            workdir: PathBuf::from("/workspace"),
            opencode_port: 41820,
            ttyd_port: 41821,
            fileserver_port: 41822,
        };

        backend
            .sessions
            .write()
            .await
            .insert("ses_test".to_string(), session);

        // Should be retrievable
        let retrieved = backend.get_session("ses_test").await;
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.user_id, "alice");
        assert_eq!(retrieved.container_id, "container_abc");
    }

    // =========================================================================
    // AgentBackend Trait Implementation Tests
    // =========================================================================

    #[tokio::test]
    async fn test_health_healthy_runtime() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let health = backend.health().await.unwrap();

        assert!(health.healthy);
        assert_eq!(health.mode, "container");
        assert!(health.version.is_some());
        assert!(health.details.unwrap().contains("octo-dev:latest"));
    }

    #[tokio::test]
    async fn test_health_unhealthy_runtime() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::with_failures(false, false, true));
        let backend = ContainerBackend::new(config, runtime);

        let health = backend.health().await.unwrap();

        assert!(!health.healthy);
        assert_eq!(health.mode, "container");
        assert!(health.details.unwrap().contains("runtime error"));
    }

    #[tokio::test]
    async fn test_list_conversations_no_active_session() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        // User with no active session returns empty
        let conversations = backend.list_conversations("bob").await.unwrap();
        assert!(conversations.is_empty());
    }

    #[tokio::test]
    async fn test_get_conversation_no_active_session() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let conv = backend
            .get_conversation("alice", "some_conv")
            .await
            .unwrap();
        assert!(conv.is_none());
    }

    #[tokio::test]
    async fn test_get_messages_no_active_session() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let result = backend.get_messages("alice", "conv_123").await;

        // Should error because no active session
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_session_creates_container() {
        let temp_dir = tempdir().unwrap();
        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            base_port: 48000,
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime.clone());

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        let handle = backend
            .start_session("alice", &workdir, StartSessionOpts::default())
            .await
            .unwrap();

        assert!(handle.session_id.starts_with("octo-alice-"));
        assert!(handle.is_new);
        assert_eq!(handle.opencode_port, 48000);
        assert_eq!(handle.ttyd_port, 48001);
        assert_eq!(handle.fileserver_port, 48002);

        // Container should exist in mock runtime
        assert_eq!(runtime.containers.read().await.len(), 1);
    }

    #[tokio::test]
    async fn test_start_session_reuses_existing() {
        let temp_dir = tempdir().unwrap();
        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            base_port: 49000,
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        // First session
        let handle1 = backend
            .start_session("alice", &workdir, StartSessionOpts::default())
            .await
            .unwrap();
        assert!(handle1.is_new);

        // Second session for same user should reuse
        let handle2 = backend
            .start_session("alice", &workdir, StartSessionOpts::default())
            .await
            .unwrap();
        assert!(!handle2.is_new);
        assert_eq!(handle1.session_id, handle2.session_id);
    }

    #[tokio::test]
    async fn test_start_session_different_users_separate() {
        let temp_dir = tempdir().unwrap();
        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            base_port: 47000,
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        let handle1 = backend
            .start_session("alice", &workdir, StartSessionOpts::default())
            .await
            .unwrap();

        let handle2 = backend
            .start_session("bob", &workdir, StartSessionOpts::default())
            .await
            .unwrap();

        // Different users get different sessions
        assert_ne!(handle1.session_id, handle2.session_id);
        assert!(handle1.is_new);
        assert!(handle2.is_new);
    }

    #[tokio::test]
    async fn test_start_session_with_model() {
        let temp_dir = tempdir().unwrap();
        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        let opts = StartSessionOpts {
            model: Some("anthropic/claude-3-opus".to_string()),
            ..Default::default()
        };

        let handle = backend
            .start_session("alice", &workdir, opts)
            .await
            .unwrap();
        assert!(handle.is_new);
    }

    #[tokio::test]
    async fn test_start_session_with_env_vars() {
        let temp_dir = tempdir().unwrap();
        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        let mut env = HashMap::new();
        env.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());

        let opts = StartSessionOpts {
            env,
            ..Default::default()
        };

        let handle = backend
            .start_session("alice", &workdir, opts)
            .await
            .unwrap();
        assert!(handle.is_new);
    }

    #[tokio::test]
    async fn test_start_session_creates_user_dir() {
        let temp_dir = tempdir().unwrap();
        let user_dir = temp_dir.path().join("new_user");

        // Ensure it doesn't exist yet
        assert!(!user_dir.exists());

        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        backend
            .start_session("new_user", &workdir, StartSessionOpts::default())
            .await
            .unwrap();

        // User directory should now exist
        assert!(user_dir.exists());
    }

    #[tokio::test]
    async fn test_stop_session_stops_container() {
        let temp_dir = tempdir().unwrap();
        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime.clone());

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        let handle = backend
            .start_session("alice", &workdir, StartSessionOpts::default())
            .await
            .unwrap();

        // Container should be running
        {
            let containers = runtime.containers.read().await;
            assert!(containers.values().any(|c| c.running));
        }

        // Stop the session
        backend
            .stop_session("alice", &handle.session_id)
            .await
            .unwrap();

        // Container should be stopped (not removed, but stopped)
        {
            let containers = runtime.containers.read().await;
            assert!(containers.values().all(|c| !c.running));
        }

        // Session should be removed from tracking
        assert!(backend.get_session(&handle.session_id).await.is_none());
    }

    #[tokio::test]
    async fn test_stop_session_not_found() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let result = backend.stop_session("alice", "nonexistent").await;

        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("not found")),
            Ok(_) => panic!("Expected error"),
        }
    }

    #[tokio::test]
    async fn test_get_session_url_found() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        // Add a session
        let session = ContainerSession {
            session_id: "ses_url_test".to_string(),
            container_id: "container_xyz".to_string(),
            user_id: "alice".to_string(),
            workdir: PathBuf::from("/workspace"),
            opencode_port: 41820,
            ttyd_port: 41821,
            fileserver_port: 41822,
        };
        backend
            .sessions
            .write()
            .await
            .insert("ses_url_test".to_string(), session);

        let url = backend
            .get_session_url("alice", "ses_url_test")
            .await
            .unwrap();

        assert!(url.is_some());
        assert_eq!(url.unwrap(), "http://localhost:41820");
    }

    #[tokio::test]
    async fn test_get_session_url_not_found() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let url = backend
            .get_session_url("alice", "nonexistent")
            .await
            .unwrap();

        assert!(url.is_none());
    }

    #[tokio::test]
    async fn test_attach_session_not_found() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let result = backend.attach("alice", "nonexistent").await;

        assert!(result.is_err());
        // Use match instead of unwrap_err since AgentEventStream doesn't implement Debug
        match result {
            Err(e) => assert!(e.to_string().contains("not found")),
            Ok(_) => panic!("Expected error"),
        }
    }

    #[tokio::test]
    async fn test_send_message_session_not_found() {
        let config = ContainerBackendConfig::default();
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let message = SendMessageRequest {
            parts: vec![super::super::SendMessagePart::Text {
                text: "Hello".to_string(),
            }],
            model: None,
        };

        let result = backend.send_message("alice", "nonexistent", message).await;

        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("not found")),
            Ok(_) => panic!("Expected error"),
        }
    }

    // =========================================================================
    // ContainerSession Tests
    // =========================================================================

    #[test]
    fn test_container_session_clone() {
        let session = ContainerSession {
            session_id: "ses_123".to_string(),
            container_id: "cnt_456".to_string(),
            user_id: "bob".to_string(),
            workdir: PathBuf::from("/workspace"),
            opencode_port: 8080,
            ttyd_port: 8081,
            fileserver_port: 8082,
        };

        let cloned = session.clone();

        assert_eq!(cloned.session_id, session.session_id);
        assert_eq!(cloned.container_id, session.container_id);
        assert_eq!(cloned.user_id, session.user_id);
        assert_eq!(cloned.workdir, session.workdir);
    }

    #[test]
    fn test_container_session_debug() {
        let session = ContainerSession {
            session_id: "ses_debug".to_string(),
            container_id: "cnt_debug".to_string(),
            user_id: "alice".to_string(),
            workdir: PathBuf::from("/home/alice"),
            opencode_port: 9000,
            ttyd_port: 9001,
            fileserver_port: 9002,
        };

        let debug_str = format!("{:?}", session);

        assert!(debug_str.contains("ses_debug"));
        assert!(debug_str.contains("cnt_debug"));
        assert!(debug_str.contains("alice"));
    }

    // =========================================================================
    // Host Network Mode Tests
    // =========================================================================

    #[tokio::test]
    async fn test_start_session_host_network_no_port_mappings() {
        let temp_dir = tempdir().unwrap();
        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            host_network: true,
            ..Default::default()
        };
        let runtime = Arc::new(MockContainerRuntime::new());
        let backend = ContainerBackend::new(config, runtime);

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        // Should succeed even with host network
        let handle = backend
            .start_session("alice", &workdir, StartSessionOpts::default())
            .await
            .unwrap();

        assert!(handle.is_new);
    }

    // =========================================================================
    // Edge Cases and Error Handling
    // =========================================================================

    #[tokio::test]
    async fn test_start_session_container_creation_fails() {
        let temp_dir = tempdir().unwrap();
        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        // Runtime configured to fail on create
        let runtime = Arc::new(MockContainerRuntime::with_failures(true, false, false));
        let backend = ContainerBackend::new(config, runtime);

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        let result = backend
            .start_session("alice", &workdir, StartSessionOpts::default())
            .await;

        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("container")),
            Ok(_) => panic!("Expected error"),
        }
    }

    #[tokio::test]
    async fn test_stop_session_container_stop_fails() {
        let temp_dir = tempdir().unwrap();
        let config = ContainerBackendConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        // Create normally, but fail on stop
        let runtime = Arc::new(MockContainerRuntime::with_failures(false, true, false));
        let backend = ContainerBackend::new(config, runtime);

        let workdir = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workdir).unwrap();

        let handle = backend
            .start_session("alice", &workdir, StartSessionOpts::default())
            .await
            .unwrap();

        let result = backend.stop_session("alice", &handle.session_id).await;

        assert!(result.is_err());
    }
}

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

        if let Some(model) = &opts.model {
            env.entry("OPENCODE_MODEL".to_string())
                .or_insert_with(|| model.clone());
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

        let sessions = history::list_sessions_from_dir(&opencode_dir).unwrap_or_else(|e| {
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
        let session_id = format!(
            "local-{}",
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        );

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::tempdir;

    // =========================================================================
    // LocalBackendConfig Tests
    // =========================================================================

    #[test]
    fn test_local_backend_config_default() {
        let config = LocalBackendConfig::default();

        assert_eq!(config.base_port, 41820);
        assert!(config.single_user);
        assert!(config.data_dir.to_string_lossy().contains("octo"));
    }

    #[test]
    fn test_local_backend_config_custom() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig::default(),
            data_dir: PathBuf::from("/custom/data"),
            base_port: 50000,
            single_user: false,
        };

        assert_eq!(config.base_port, 50000);
        assert!(!config.single_user);
        assert_eq!(config.data_dir, PathBuf::from("/custom/data"));
    }

    // =========================================================================
    // LocalBackend Construction Tests
    // =========================================================================

    #[test]
    fn test_local_backend_new_validates_binaries() {
        // Using non-existent binaries should fail validation
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "nonexistent-opencode-12345".to_string(),
                fileserver_binary: "nonexistent-fileserver-12345".to_string(),
                ttyd_binary: "nonexistent-ttyd-12345".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let result = LocalBackend::new(config);
        assert!(result.is_err());
        // Use match instead of unwrap_err since LocalBackend doesn't implement Debug
        match result {
            Err(e) => {
                let err = e.to_string();
                assert!(err.contains("opencode") || err.contains("not found"));
            }
            Ok(_) => panic!("Expected error"),
        }
    }

    #[test]
    fn test_local_backend_new_with_valid_binaries() {
        // Use common system binaries that exist everywhere
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let result = LocalBackend::new(config);
        assert!(result.is_ok());
    }

    // =========================================================================
    // Port Allocation Tests
    // =========================================================================

    #[tokio::test]
    async fn test_allocate_ports_sequential() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            base_port: 50000,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();

        // First allocation
        let (p1, p2, p3) = backend.allocate_ports().await;
        assert_eq!(p1, 50000);
        assert_eq!(p2, 50001);
        assert_eq!(p3, 50002);

        // Second allocation should be sequential
        let (p4, p5, p6) = backend.allocate_ports().await;
        assert_eq!(p4, 50003);
        assert_eq!(p5, 50004);
        assert_eq!(p6, 50005);

        // Third allocation
        let (p7, p8, p9) = backend.allocate_ports().await;
        assert_eq!(p7, 50006);
        assert_eq!(p8, 50007);
        assert_eq!(p9, 50008);
    }

    #[tokio::test]
    async fn test_allocate_ports_concurrent() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            base_port: 60000,
            ..Default::default()
        };

        let backend = Arc::new(LocalBackend::new(config).unwrap());

        // Spawn multiple concurrent allocations
        let mut handles = vec![];
        for _ in 0..10 {
            let b = Arc::clone(&backend);
            handles.push(tokio::spawn(async move { b.allocate_ports().await }));
        }

        let mut all_ports = Vec::new();
        for handle in handles {
            let (p1, p2, p3) = handle.await.unwrap();
            all_ports.extend([p1, p2, p3]);
        }

        // All ports should be unique
        all_ports.sort();
        let unique_ports: std::collections::HashSet<_> = all_ports.iter().collect();
        assert_eq!(unique_ports.len(), 30, "All ports should be unique");

        // Ports should be in range
        assert!(all_ports.iter().all(|&p| (60000..60030).contains(&p)));
    }

    // =========================================================================
    // Data Directory Tests
    // =========================================================================

    #[test]
    fn test_opencode_data_dir_single_user() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            single_user: true,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();
        let data_dir = backend.opencode_data_dir("any_user");

        // Single-user mode should use XDG default, not user-specific path
        assert!(data_dir.to_string_lossy().contains("opencode"));
        assert!(!data_dir.to_string_lossy().contains("any_user"));
    }

    #[test]
    fn test_opencode_data_dir_multi_user() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            data_dir: PathBuf::from("/var/lib/octo/users"),
            single_user: false,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();

        // Each user should get their own directory
        let alice_dir = backend.opencode_data_dir("alice");
        let bob_dir = backend.opencode_data_dir("bob");

        assert_eq!(
            alice_dir,
            PathBuf::from("/var/lib/octo/users/alice/.local/share/opencode")
        );
        assert_eq!(
            bob_dir,
            PathBuf::from("/var/lib/octo/users/bob/.local/share/opencode")
        );
        assert_ne!(alice_dir, bob_dir);
    }

    // =========================================================================
    // Environment Building Tests
    // =========================================================================

    #[test]
    fn test_build_env_single_user_no_xdg() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            single_user: true,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();
        let opts = StartSessionOpts::default();
        let env = backend.build_env("test_user", &opts);

        // Single-user mode should NOT set XDG_DATA_HOME
        assert!(!env.contains_key("XDG_DATA_HOME"));
    }

    #[test]
    fn test_build_env_multi_user_sets_xdg() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            data_dir: PathBuf::from("/data/users"),
            single_user: false,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();
        let opts = StartSessionOpts::default();
        let env = backend.build_env("alice", &opts);

        // Multi-user mode should set XDG_DATA_HOME
        assert!(env.contains_key("XDG_DATA_HOME"));
        assert_eq!(
            env.get("XDG_DATA_HOME").unwrap(),
            "/data/users/alice/.local/share"
        );
    }

    #[test]
    fn test_build_env_with_model_override() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            single_user: true,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();
        let opts = StartSessionOpts {
            model: Some("anthropic/claude-3-5-sonnet".to_string()),
            ..Default::default()
        };
        let env = backend.build_env("user", &opts);

        assert_eq!(
            env.get("OPENCODE_MODEL").unwrap(),
            "anthropic/claude-3-5-sonnet"
        );
    }

    #[test]
    fn test_build_env_model_not_overwritten() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            single_user: true,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();

        // If OPENCODE_MODEL is already in opts.env, it should NOT be overwritten
        let mut preset_env = HashMap::new();
        preset_env.insert("OPENCODE_MODEL".to_string(), "preset/model".to_string());

        let opts = StartSessionOpts {
            model: Some("anthropic/claude-3-5-sonnet".to_string()),
            env: preset_env,
            ..Default::default()
        };
        let env = backend.build_env("user", &opts);

        // The preset value should be preserved (or_insert_with doesn't overwrite)
        assert_eq!(env.get("OPENCODE_MODEL").unwrap(), "preset/model");
    }

    #[test]
    fn test_build_env_preserves_custom_env() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            single_user: true,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();

        let mut custom_env = HashMap::new();
        custom_env.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());
        custom_env.insert("API_KEY".to_string(), "secret123".to_string());

        let opts = StartSessionOpts {
            env: custom_env,
            ..Default::default()
        };
        let env = backend.build_env("user", &opts);

        assert_eq!(env.get("CUSTOM_VAR").unwrap(), "custom_value");
        assert_eq!(env.get("API_KEY").unwrap(), "secret123");
    }

    // =========================================================================
    // Session Management Tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_session_nonexistent() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();
        let session = backend.get_session("nonexistent-session").await;

        assert!(session.is_none());
    }

    // =========================================================================
    // AgentBackend Trait Implementation Tests
    // =========================================================================

    #[tokio::test]
    async fn test_health_returns_healthy() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();
        let health = backend.health().await.unwrap();

        assert!(health.healthy);
        assert_eq!(health.mode, "local");
        assert!(health.version.is_some());
        assert!(health.details.is_some());

        let details = health.details.unwrap();
        assert!(details.contains("opencode: sh"));
        assert!(details.contains("ttyd: sh"));
        assert!(details.contains("fileserver: sh"));
    }

    #[tokio::test]
    async fn test_list_conversations_empty_dir() {
        let temp_dir = tempdir().unwrap();

        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            data_dir: temp_dir.path().to_path_buf(),
            single_user: false,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();

        // User with no sessions should return empty list (not error)
        let conversations = backend.list_conversations("new_user").await.unwrap();
        assert!(conversations.is_empty());
    }

    #[tokio::test]
    async fn test_get_conversation_not_found() {
        let temp_dir = tempdir().unwrap();

        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            data_dir: temp_dir.path().to_path_buf(),
            single_user: false,
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();

        let conversation = backend
            .get_conversation("user", "nonexistent_conv")
            .await
            .unwrap();
        assert!(conversation.is_none());
    }

    #[tokio::test]
    async fn test_get_session_url_not_found() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();
        let url = backend
            .get_session_url("user", "nonexistent")
            .await
            .unwrap();

        assert!(url.is_none());
    }

    #[tokio::test]
    async fn test_stop_session_not_found() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();

        // Stopping a nonexistent session should succeed (no-op in LocalRuntime)
        let result = backend.stop_session("user", "nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_attach_session_not_found() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();
        let result = backend.attach("user", "nonexistent").await;

        assert!(result.is_err());
        // Use match to avoid Debug requirement on AgentEventStream
        match result {
            Err(e) => assert!(e.to_string().contains("not found")),
            Ok(_) => panic!("Expected error"),
        }
    }

    #[tokio::test]
    async fn test_send_message_session_not_found() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();
        let message = SendMessageRequest {
            parts: vec![super::super::SendMessagePart::Text {
                text: "Hello".to_string(),
            }],
            model: None,
        };

        let result = backend.send_message("user", "nonexistent", message).await;

        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("not found")),
            Ok(_) => panic!("Expected error"),
        }
    }

    // =========================================================================
    // Session Reuse Tests
    // =========================================================================

    #[tokio::test]
    async fn test_session_tracks_active_conversations() {
        let config = LocalBackendConfig {
            runtime: LocalRuntimeConfig {
                opencode_binary: "sh".to_string(),
                fileserver_binary: "sh".to_string(),
                ttyd_binary: "sh".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let backend = LocalBackend::new(config).unwrap();

        // Initially no sessions
        assert!(backend.sessions.read().await.is_empty());

        // We can't fully test start_session without real binaries, but we can
        // verify the sessions map behavior
        let active_session = ActiveSession {
            session_id: "test-session".to_string(),
            user_id: "alice".to_string(),
            workdir: PathBuf::from("/home/alice/project"),
            opencode_port: 41820,
            ttyd_port: 41821,
            fileserver_port: 41822,
        };

        backend
            .sessions
            .write()
            .await
            .insert("test-session".to_string(), active_session);

        // Now should have one session
        assert_eq!(backend.sessions.read().await.len(), 1);

        // Get session should work
        let session = backend.get_session("test-session").await;
        assert!(session.is_some());
        let session = session.unwrap();
        assert_eq!(session.user_id, "alice");
        assert_eq!(session.opencode_port, 41820);
    }

    // =========================================================================
    // Clone and Debug Tests
    // =========================================================================

    #[test]
    fn test_local_backend_config_debug() {
        let config = LocalBackendConfig::default();
        let debug_str = format!("{:?}", config);

        assert!(debug_str.contains("LocalBackendConfig"));
        assert!(debug_str.contains("base_port"));
        assert!(debug_str.contains("single_user"));
    }

    #[test]
    fn test_active_session_debug() {
        let session = ActiveSession {
            session_id: "ses_123".to_string(),
            user_id: "bob".to_string(),
            workdir: PathBuf::from("/workspace"),
            opencode_port: 8080,
            ttyd_port: 8081,
            fileserver_port: 8082,
        };

        let debug_str = format!("{:?}", session);
        assert!(debug_str.contains("ses_123"));
        assert!(debug_str.contains("bob"));
    }

    #[test]
    fn test_active_session_clone() {
        let session = ActiveSession {
            session_id: "ses_456".to_string(),
            user_id: "carol".to_string(),
            workdir: PathBuf::from("/projects/app"),
            opencode_port: 9000,
            ttyd_port: 9001,
            fileserver_port: 9002,
        };

        let cloned = session.clone();
        assert_eq!(cloned.session_id, session.session_id);
        assert_eq!(cloned.user_id, session.user_id);
        assert_eq!(cloned.workdir, session.workdir);
        assert_eq!(cloned.opencode_port, session.opencode_port);
    }
}

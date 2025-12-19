//! Agent management service.
//!
//! Manages opencode agent instances within containers via docker exec.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::container::ContainerRuntimeApi;
use crate::session::{Session, SessionService};

use super::models::{AgentInfo, AgentStatus, StartAgentResponse, StopAgentResponse};

/// Main agent port (started by entrypoint).
pub const MAIN_AGENT_PORT: u16 = 41820;

/// Base port for sub-agents.
const SUB_AGENT_BASE_PORT: u16 = 4001;

/// Maximum number of sub-agents per container.
const MAX_SUB_AGENTS: u16 = 99;

/// Agent management service.
#[derive(Clone)]
pub struct AgentService {
    runtime: Arc<dyn ContainerRuntimeApi>,
    sessions: SessionService,
    /// In-memory tracking of agent ports per session.
    /// Map of session_id -> (agent_id -> port).
    agent_ports: Arc<RwLock<HashMap<String, HashMap<String, u16>>>>,
}

impl AgentService {
    /// Create a new agent service.
    pub fn new(runtime: Arc<dyn ContainerRuntimeApi>, sessions: SessionService) -> Self {
        Self {
            runtime,
            sessions,
            agent_ports: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// List all agents for a session (running + available directories).
    pub async fn list_agents(&self, session_id: &str) -> Result<Vec<AgentInfo>> {
        let session = self.get_session(session_id).await?;
        let container_id = session
            .container_id
            .as_ref()
            .context("session has no container")?;

        let mut agents = Vec::new();

        // 1. Add main agent (always exists)
        let main_status = self
            .check_agent_status(&session, MAIN_AGENT_PORT)
            .await;
        let (main_has_agents_md, main_has_git) = self
            .check_directory_files(container_id, "/home/dev/workspace")
            .await;
        agents.push(AgentInfo::main(
            MAIN_AGENT_PORT,
            main_status,
            main_has_agents_md,
            main_has_git,
        ));

        // 2. Scan for subdirectories that could be agents
        let subdirs = self.list_workspace_subdirs(container_id).await?;

        // 3. Check each subdir
        let ports = self.agent_ports.read().await;
        let session_ports = ports.get(session_id);

        for subdir in subdirs {
            let (has_agents_md, has_git) = self
                .check_directory_files(container_id, &format!("/home/dev/workspace/{}", subdir))
                .await;

            // Only include directories with AGENTS.md or .git
            if !has_agents_md && !has_git {
                continue;
            }

            // Check if we have a known port for this agent
            let port = session_ports.and_then(|p| p.get(&subdir).copied());

            let status = if let Some(p) = port {
                self.check_agent_status(&session, p).await
            } else {
                AgentStatus::Stopped
            };

            agents.push(AgentInfo::sub_agent(
                subdir,
                port,
                status,
                has_agents_md,
                has_git,
            ));
        }

        Ok(agents)
    }

    /// Start an agent in a subdirectory.
    pub async fn start_agent(
        &self,
        session_id: &str,
        directory: &str,
    ) -> Result<StartAgentResponse> {
        let session = self.get_session(session_id).await?;
        let container_id = session
            .container_id
            .as_ref()
            .context("session has no container")?;

        // Validate directory name
        let agent_id = self.validate_agent_directory(directory)?;

        // Check if already running
        {
            let ports = self.agent_ports.read().await;
            if let Some(session_ports) = ports.get(session_id) {
                if let Some(&existing_port) = session_ports.get(&agent_id) {
                    let status = self.check_agent_status(&session, existing_port).await;
                    if status == AgentStatus::Running {
                        info!(
                            "Agent {} already running on port {} for session {}",
                            agent_id, existing_port, session_id
                        );
                        return Ok(StartAgentResponse {
                            id: agent_id,
                            port: existing_port,
                            status: AgentStatus::Running,
                        });
                    }
                }
            }
        }

        // Allocate a port
        let port = self.allocate_port(session_id).await?;

        // Build the command to start opencode serve
        let workspace_path = format!("/home/dev/workspace/{}", agent_id);
        let cmd = format!(
            "cd {} && opencode serve --port {} --hostname 0.0.0.0 > /tmp/agent-{}.log 2>&1 &",
            workspace_path, port, agent_id
        );

        info!(
            "Starting agent {} on port {} in container {}",
            agent_id, port, container_id
        );

        // Execute in container
        self.runtime
            .exec_detached(container_id, &["bash", "-c", &cmd])
            .await
            .context("failed to start agent")?;

        // Track the port
        {
            let mut ports = self.agent_ports.write().await;
            ports
                .entry(session_id.to_string())
                .or_default()
                .insert(agent_id.clone(), port);
        }

        Ok(StartAgentResponse {
            id: agent_id,
            port,
            status: AgentStatus::Starting,
        })
    }

    /// Stop an agent.
    pub async fn stop_agent(&self, session_id: &str, agent_id: &str) -> Result<StopAgentResponse> {
        let session = self.get_session(session_id).await?;
        let container_id = session
            .container_id
            .as_ref()
            .context("session has no container")?;

        // Can't stop main agent
        if agent_id == "main" {
            anyhow::bail!("cannot stop main agent");
        }

        // Get the port
        let port = {
            let ports = self.agent_ports.read().await;
            ports
                .get(session_id)
                .and_then(|p| p.get(agent_id).copied())
        };

        let Some(port) = port else {
            warn!("Agent {} not found in session {}", agent_id, session_id);
            return Ok(StopAgentResponse { stopped: false });
        };

        // Kill the process
        let cmd = format!("pkill -f 'opencode serve.*port {}'", port);
        info!(
            "Stopping agent {} (port {}) in container {}",
            agent_id, port, container_id
        );

        let _ = self
            .runtime
            .exec_detached(container_id, &["bash", "-c", &cmd])
            .await;

        // Remove from tracking
        {
            let mut ports = self.agent_ports.write().await;
            if let Some(session_ports) = ports.get_mut(session_id) {
                session_ports.remove(agent_id);
            }
        }

        Ok(StopAgentResponse { stopped: true })
    }

    /// Get agent status.
    pub async fn get_agent(&self, session_id: &str, agent_id: &str) -> Result<Option<AgentInfo>> {
        let agents = self.list_agents(session_id).await?;
        Ok(agents.into_iter().find(|a| a.id == agent_id))
    }

    /// Create a new agent directory with AGENTS.md file.
    ///
    /// This creates the directory structure but does not start the agent.
    /// Call `start_agent` afterwards to start opencode serve.
    pub async fn create_agent(
        &self,
        session_id: &str,
        name: &str,
        description: &str,
    ) -> Result<super::models::CreateAgentResponse> {
        use super::models::{agent_color, CreateAgentResponse};

        let session = self.get_session(session_id).await?;
        let container_id = session
            .container_id
            .as_ref()
            .context("session has no container")?;

        // Validate and sanitize the name
        let agent_id = self.validate_agent_directory(name)?;

        // Check if directory already exists
        let workspace_path = format!("/home/dev/workspace/{}", agent_id);
        let check_exists = format!("test -d {}", workspace_path);
        let exists = self
            .runtime
            .exec_output(container_id, &["bash", "-c", &check_exists])
            .await
            .is_ok();

        if exists {
            anyhow::bail!("agent directory '{}' already exists", agent_id);
        }

        // Create directory
        let mkdir_cmd = format!("mkdir -p {}", workspace_path);
        self.runtime
            .exec_output(container_id, &["bash", "-c", &mkdir_cmd])
            .await
            .context("failed to create agent directory")?;

        // Create AGENTS.md with formatted content
        // Use base64 encoding to safely pass content through shell
        use base64::Engine;
        let agents_md_content = format!("# {}\n\n{}", agent_id, description);
        let encoded = base64::engine::general_purpose::STANDARD.encode(agents_md_content.as_bytes());
        let agents_md_path = format!("{}/AGENTS.md", workspace_path);
        
        let write_cmd = format!(
            "echo '{}' | base64 -d > {}",
            encoded,
            agents_md_path
        );
        self.runtime
            .exec_output(container_id, &["bash", "-c", &write_cmd])
            .await
            .context("failed to create AGENTS.md")?;

        info!(
            "Created agent '{}' in container {} at {}",
            agent_id, container_id, workspace_path
        );

        Ok(CreateAgentResponse {
            id: agent_id.clone(),
            directory: workspace_path,
            color: agent_color(&agent_id),
        })
    }

    /// Get the port for a specific agent.
    ///
    /// Returns `None` if the agent is not running or not found.
    /// For the "main" agent, returns the session's opencode_port.
    pub async fn get_agent_port(&self, session_id: &str, agent_id: &str) -> Result<Option<u16>> {
        // Main agent uses the session's opencode port
        if agent_id == "main" {
            let session = self.get_session(session_id).await?;
            return Ok(Some(session.opencode_port as u16));
        }

        // Sub-agents use tracked ports
        let ports = self.agent_ports.read().await;
        Ok(ports
            .get(session_id)
            .and_then(|session_ports| session_ports.get(agent_id).copied()))
    }

    /// Rediscover agents after control plane restart.
    ///
    /// Scans ports to find running opencode instances.
    pub async fn rediscover_agents(&self, session_id: &str) -> Result<()> {
        let session = self.get_session(session_id).await?;

        // Scan sub-agent ports
        for offset in 0..MAX_SUB_AGENTS {
            let port = SUB_AGENT_BASE_PORT + offset;
            let status = self.check_agent_status(&session, port).await;

            if status == AgentStatus::Running {
                // Try to figure out which agent this is by querying opencode
                if let Ok(Some(directory)) = self.get_agent_directory(&session, port).await {
                    let agent_id = directory
                        .strip_prefix("/home/dev/workspace/")
                        .unwrap_or(&directory)
                        .to_string();

                    if !agent_id.is_empty() && agent_id != "workspace" {
                        info!(
                            "Rediscovered agent {} on port {} for session {}",
                            agent_id, port, session_id
                        );
                        let mut ports = self.agent_ports.write().await;
                        ports
                            .entry(session_id.to_string())
                            .or_default()
                            .insert(agent_id, port);
                    }
                }
            }
        }

        Ok(())
    }

    // ========================================================================
    // Helper methods
    // ========================================================================

    async fn get_session(&self, session_id: &str) -> Result<Session> {
        self.sessions
            .get_session(session_id)
            .await?
            .context("session not found")
    }

    fn validate_agent_directory(&self, directory: &str) -> Result<String> {
        // Sanitize: only allow lowercase alphanumeric and hyphens
        let sanitized: String = directory
            .chars()
            .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
            .collect();

        if sanitized.is_empty() {
            anyhow::bail!("invalid agent directory name");
        }

        if sanitized.starts_with('-') || sanitized.ends_with('-') {
            anyhow::bail!("agent directory cannot start or end with hyphen");
        }

        if sanitized.contains("--") {
            anyhow::bail!("agent directory cannot contain consecutive hyphens");
        }

        Ok(sanitized)
    }

    async fn allocate_port(&self, session_id: &str) -> Result<u16> {
        let ports = self.agent_ports.read().await;
        let used_ports: Vec<u16> = ports
            .get(session_id)
            .map(|p| p.values().copied().collect())
            .unwrap_or_default();

        for offset in 0..MAX_SUB_AGENTS {
            let port = SUB_AGENT_BASE_PORT + offset;
            if !used_ports.contains(&port) {
                return Ok(port);
            }
        }

        anyhow::bail!("no available ports for sub-agents")
    }

    async fn check_agent_status(&self, session: &Session, port: u16) -> AgentStatus {
        // Try to reach the opencode API on this port
        let url = format!("http://localhost:{}/session", session.opencode_port);
        let _ = url; // We need to query internal container port

        // For now, use a simple HTTP check through the host
        // In production, we'd need to route through the container network
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .ok();

        let Some(client) = client else {
            return AgentStatus::Stopped;
        };

        // Query through the container's exposed port
        // For main agent, use session.opencode_port
        // For sub-agents, we need internal routing (not exposed)
        if port == MAIN_AGENT_PORT {
            let url = format!("http://localhost:{}/session", session.opencode_port);
            match client.get(&url).send().await {
                Ok(res) if res.status().is_success() => AgentStatus::Running,
                _ => AgentStatus::Stopped,
            }
        } else {
            // Sub-agents aren't exposed externally, so we can't check directly
            // For now, assume running if we have it tracked
            // TODO: Use docker exec to check
            AgentStatus::Running
        }
    }

    async fn check_directory_files(&self, container_id: &str, path: &str) -> (bool, bool) {
        // Check for AGENTS.md
        let has_agents_md = self
            .runtime
            .exec_detached(
                container_id,
                &["test", "-f", &format!("{}/AGENTS.md", path)],
            )
            .await
            .is_ok();

        // Check for .git
        let has_git = self
            .runtime
            .exec_detached(container_id, &["test", "-d", &format!("{}/.git", path)])
            .await
            .is_ok();

        (has_agents_md, has_git)
    }

    async fn list_workspace_subdirs(&self, container_id: &str) -> Result<Vec<String>> {
        // List directories in /home/dev/workspace
        let output = self
            .runtime
            .exec_output(
                container_id,
                &[
                    "find",
                    "/home/dev/workspace",
                    "-maxdepth",
                    "1",
                    "-mindepth",
                    "1",
                    "-type",
                    "d",
                    "-printf",
                    "%f\\n",
                ],
            )
            .await?;

        let subdirs: Vec<String> = output
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.starts_with('.'))
            .collect();

        debug!("Found workspace subdirs: {:?}", subdirs);
        Ok(subdirs)
    }

    async fn get_agent_directory(&self, session: &Session, port: u16) -> Result<Option<String>> {
        let _ = (session, port);
        // Query opencode's /path endpoint to get the working directory
        // This would require internal container networking
        // For now, return None
        Ok(None)
    }
}

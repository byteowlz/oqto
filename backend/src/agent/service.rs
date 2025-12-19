//! Agent management service.
//!
//! Manages opencode agent instances within containers via docker exec.

use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::container::ContainerRuntimeApi;
use crate::session::{Session, SessionService};

use super::models::{AgentInfo, AgentStatus, CreateAgentResponse, StartAgentResponse, StopAgentResponse, agent_color};
use super::repository::{AgentRecord, AgentRepository};

/// Main agent port (started by entrypoint).
pub const MAIN_AGENT_PORT: u16 = 41820;

/// Base port for sub-agents inside the container.
const INTERNAL_AGENT_BASE_PORT: u16 = 4001;

/// Agent management service.
#[derive(Clone)]
pub struct AgentService {
    runtime: Arc<dyn ContainerRuntimeApi>,
    sessions: SessionService,
    repo: AgentRepository,
}

impl AgentService {
    /// Create a new agent service.
    pub fn new(
        runtime: Arc<dyn ContainerRuntimeApi>,
        sessions: SessionService,
        repo: AgentRepository,
    ) -> Self {
        Self {
            runtime,
            sessions,
            repo,
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
            .check_agent_health(&session, MAIN_AGENT_PORT)
            .await;
        let (main_has_agents_md, main_has_git) = self
            .check_directory_files(container_id, "/home/dev/workspace")
            .await;
        agents.push(AgentInfo::main(
            MAIN_AGENT_PORT,
            session.opencode_port as u16,
            main_status,
            main_has_agents_md,
            main_has_git,
        ));

        // 2. Get persisted agents from database
        let db_agents = self.repo.list_by_session(session_id).await?;
        let db_agent_ids: std::collections::HashSet<_> = db_agents.iter().map(|a| a.agent_id.clone()).collect();

        // Add persisted agents
        for record in db_agents {
            let status = self.check_agent_health(&session, record.internal_port as u16).await;
            
            // Update status in DB if it changed
            if status != record.status {
                let _ = self.repo.update_status(&record.id, status).await;
            }

            agents.push(AgentInfo::sub_agent(
                record.agent_id,
                Some(record.internal_port as u16),
                Some(record.external_port as u16),
                status,
                record.has_agents_md,
                record.has_git,
            ));
        }

        // 3. Scan for new subdirectories that could be agents (not yet in DB)
        let subdirs = self.list_workspace_subdirs(container_id).await?;

        for subdir in subdirs {
            if db_agent_ids.contains(&subdir) {
                continue; // Already in the list
            }

            let (has_agents_md, has_git) = self
                .check_directory_files(container_id, &format!("/home/dev/workspace/{}", subdir))
                .await;

            // Only include directories with AGENTS.md or .git
            if !has_agents_md && !has_git {
                continue;
            }

            // This is a new directory that could be an agent but isn't started yet
            agents.push(AgentInfo::sub_agent(
                subdir,
                None,
                None,
                AgentStatus::Stopped,
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

        // Check if already running in DB
        if let Some(existing) = self.repo.get_by_session_and_agent(session_id, &agent_id).await? {
            if existing.status == AgentStatus::Running || existing.status == AgentStatus::Starting {
                info!(
                    "Agent {} already running on port {} for session {}",
                    agent_id, existing.internal_port, session_id
                );
                return Ok(StartAgentResponse {
                    id: agent_id,
                    port: existing.internal_port as u16,
                    external_port: existing.external_port as u16,
                    status: existing.status,
                });
            }
            
            // Agent exists but is stopped - restart it
            let internal_port = existing.internal_port as u16;
            let external_port = existing.external_port as u16;
            
            self.start_opencode_in_container(container_id, &agent_id, internal_port).await?;
            
            self.repo.update_status(&existing.id, AgentStatus::Starting).await?;
            
            return Ok(StartAgentResponse {
                id: agent_id,
                port: internal_port,
                external_port,
                status: AgentStatus::Starting,
            });
        }

        // Allocate ports
        let (internal_port, external_port) = self.allocate_agent_ports(&session).await?;

        // Start opencode serve in the container
        self.start_opencode_in_container(container_id, &agent_id, internal_port).await?;

        // Create the agent record
        let record_id = format!("{}:{}", session_id, agent_id);
        let (has_agents_md, has_git) = self
            .check_directory_files(container_id, &format!("/home/dev/workspace/{}", agent_id))
            .await;

        let record = AgentRecord {
            id: record_id,
            session_id: session_id.to_string(),
            agent_id: agent_id.clone(),
            name: format_agent_name(&agent_id),
            directory: format!("/home/dev/workspace/{}", agent_id),
            internal_port: internal_port as i64,
            external_port: external_port as i64,
            status: AgentStatus::Starting,
            has_agents_md,
            has_git,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: Some(chrono::Utc::now().to_rfc3339()),
            stopped_at: None,
        };

        self.repo.create(&record).await?;

        info!(
            "Started agent {} on internal port {} (external {}) in session {}",
            agent_id, internal_port, external_port, session_id
        );

        Ok(StartAgentResponse {
            id: agent_id,
            port: internal_port,
            external_port,
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

        // Get the agent from DB
        let record = self.repo.get_by_session_and_agent(session_id, agent_id).await?;
        
        let Some(record) = record else {
            warn!("Agent {} not found in session {}", agent_id, session_id);
            return Ok(StopAgentResponse { stopped: false });
        };

        // Kill the process
        let cmd = format!("pkill -f 'opencode serve.*port {}'", record.internal_port);
        info!(
            "Stopping agent {} (port {}) in container {}",
            agent_id, record.internal_port, container_id
        );

        let _ = self
            .runtime
            .exec_detached(container_id, &["bash", "-c", &cmd])
            .await;

        // Update status in DB
        self.repo.update_status(&record.id, AgentStatus::Stopped).await?;

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
    ) -> Result<CreateAgentResponse> {
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

    /// Get the external port for a specific agent.
    ///
    /// Returns `None` if the agent is not running or not found.
    /// For the "main" agent, returns the session's opencode_port.
    pub async fn get_agent_port(&self, session_id: &str, agent_id: &str) -> Result<Option<u16>> {
        // Main agent uses the session's opencode port
        if agent_id == "main" {
            let session = self.get_session(session_id).await?;
            return Ok(Some(session.opencode_port as u16));
        }

        // Sub-agents use tracked ports from DB
        let record = self.repo.get_by_session_and_agent(session_id, agent_id).await?;
        Ok(record.map(|r| r.external_port as u16))
    }

    /// Rediscover agents after control plane restart.
    ///
    /// Scans ports to find running opencode instances and syncs with DB.
    pub async fn rediscover_agents(&self, session_id: &str) -> Result<()> {
        let session = self.get_session(session_id).await?;
        
        let Some(agent_base_port) = session.agent_base_port else {
            debug!("Session {} has no agent port range configured", session_id);
            return Ok(());
        };
        
        let max_agents = session.max_agents.unwrap_or(10);

        // Scan internal ports to find running agents
        for i in 0..max_agents {
            let internal_port = INTERNAL_AGENT_BASE_PORT + i as u16;
            let external_port = (agent_base_port + i) as u16;
            
            let status = self.check_agent_health(&session, internal_port).await;

            if status == AgentStatus::Running {
                // Try to figure out which agent this is by querying opencode
                if let Ok(Some(directory)) = self.get_agent_directory(&session, external_port).await {
                    let agent_id = directory
                        .strip_prefix("/home/dev/workspace/")
                        .unwrap_or(&directory)
                        .to_string();

                    if !agent_id.is_empty() && agent_id != "workspace" {
                        // Check if already in DB
                        let existing = self.repo.get_by_session_and_agent(session_id, &agent_id).await?;
                        
                        if existing.is_none() {
                            info!(
                                "Rediscovered agent {} on port {} for session {}",
                                agent_id, internal_port, session_id
                            );
                            
                            let record_id = format!("{}:{}", session_id, agent_id);
                            let record = AgentRecord {
                                id: record_id,
                                session_id: session_id.to_string(),
                                agent_id: agent_id.clone(),
                                name: format_agent_name(&agent_id),
                                directory: format!("/home/dev/workspace/{}", agent_id),
                                internal_port: internal_port as i64,
                                external_port: external_port as i64,
                                status: AgentStatus::Running,
                                has_agents_md: false, // Will be updated on next list
                                has_git: false,
                                created_at: chrono::Utc::now().to_rfc3339(),
                                started_at: Some(chrono::Utc::now().to_rfc3339()),
                                stopped_at: None,
                            };
                            
                            self.repo.create(&record).await?;
                        } else if let Some(record) = existing {
                            // Update status if changed
                            if record.status != AgentStatus::Running {
                                self.repo.update_status(&record.id, AgentStatus::Running).await?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Mark all agents for a session as stopped.
    ///
    /// Called when stopping a session.
    pub async fn mark_all_stopped(&self, session_id: &str) -> Result<()> {
        self.repo.mark_all_stopped(session_id).await
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

    async fn allocate_agent_ports(&self, session: &Session) -> Result<(u16, u16)> {
        let agent_base_port = session.agent_base_port
            .context("session has no agent port range configured")?;
        let max_agents = session.max_agents.unwrap_or(10);

        // Find the next available port offset
        let used_offsets = self.repo.list_running_by_session(&session.id).await?;
        let used_internal_ports: std::collections::HashSet<_> = used_offsets
            .iter()
            .map(|r| r.internal_port as u16)
            .collect();

        for i in 0..max_agents {
            let internal_port = INTERNAL_AGENT_BASE_PORT + i as u16;
            if !used_internal_ports.contains(&internal_port) {
                let external_port = (agent_base_port + i) as u16;
                return Ok((internal_port, external_port));
            }
        }

        anyhow::bail!("no available ports for sub-agents (max {} reached)", max_agents)
    }

    async fn start_opencode_in_container(
        &self,
        container_id: &str,
        agent_id: &str,
        internal_port: u16,
    ) -> Result<()> {
        let workspace_path = format!("/home/dev/workspace/{}", agent_id);
        let cmd = format!(
            "cd {} && opencode serve --port {} --hostname 0.0.0.0 > /tmp/agent-{}.log 2>&1 &",
            workspace_path, internal_port, agent_id
        );

        self.runtime
            .exec_detached(container_id, &["bash", "-c", &cmd])
            .await
            .context("failed to start agent")?;

        Ok(())
    }

    async fn check_agent_health(&self, session: &Session, internal_port: u16) -> AgentStatus {
        // For main agent, use session's external opencode port
        // For sub-agents, calculate external port from session's agent_base_port
        let external_port = if internal_port == MAIN_AGENT_PORT {
            session.opencode_port as u16
        } else if let Some(agent_base) = session.agent_base_port {
            let offset = internal_port - INTERNAL_AGENT_BASE_PORT;
            (agent_base as u16) + offset
        } else {
            return AgentStatus::Stopped;
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
        {
            Ok(c) => c,
            Err(_) => return AgentStatus::Stopped,
        };

        let url = format!("http://localhost:{}/session", external_port);
        match client.get(&url).send().await {
            Ok(res) if res.status().is_success() => AgentStatus::Running,
            _ => AgentStatus::Stopped,
        }
    }

    async fn check_directory_files(&self, container_id: &str, path: &str) -> (bool, bool) {
        // Check for AGENTS.md
        let has_agents_md = self
            .runtime
            .exec_output(
                container_id,
                &["test", "-f", &format!("{}/AGENTS.md", path)],
            )
            .await
            .is_ok();

        // Check for .git
        let has_git = self
            .runtime
            .exec_output(container_id, &["test", "-d", &format!("{}/.git", path)])
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

    async fn get_agent_directory(&self, _session: &Session, external_port: u16) -> Result<Option<String>> {
        // Query opencode's API to get the working directory
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()?;

        let url = format!("http://localhost:{}/project/path", external_port);
        match client.get(&url).send().await {
            Ok(res) if res.status().is_success() => {
                let body = res.text().await?;
                // OpenCode returns the path as a JSON string
                let path: String = serde_json::from_str(&body).unwrap_or(body);
                Ok(Some(path))
            }
            _ => Ok(None),
        }
    }
}

/// Format agent name from ID (e.g., "doc-writer" -> "Doc Writer").
fn format_agent_name(id: &str) -> String {
    id.split('-')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

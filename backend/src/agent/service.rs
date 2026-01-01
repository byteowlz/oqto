//! Agent management service.
//!
//! Manages opencode agent instances within containers via docker exec.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::container::ContainerRuntimeApi;
use crate::session::{RuntimeMode, Session, SessionService};

use super::models::{
    AgentExecRequest, AgentExecResponse, AgentInfo, AgentRuntimeInfo, AgentScaffoldRequest,
    AgentStatus, CreateAgentResponse, OpenCodeContextInfo, OpenCodeSessionInfo,
    OpenCodeSessionStatus, OpenCodeTokenCache, OpenCodeTokenLimit, OpenCodeTokenTotals,
    StartAgentResponse, StopAgentResponse, agent_color,
};
use super::repository::{AgentRecord, AgentRepository};

/// Main agent port (started by entrypoint).
pub const MAIN_AGENT_PORT: u16 = 41820;

/// Base port for sub-agents inside the container.
const INTERNAL_AGENT_BASE_PORT: u16 = 4001;

/// Agent scaffolding configuration.
/// Defines the external command used to scaffold new agent directories.
#[derive(Debug, Clone, Default)]
pub struct ScaffoldConfig {
    /// Binary to use for scaffolding (e.g., "byt", "cookiecutter", custom script)
    pub binary: String,
    /// Subcommand to invoke (e.g., "new" for "byt new")
    pub subcommand: String,
    /// Argument format for template name (e.g., "--template" for "--template rust-cli")
    pub template_arg: String,
    /// Argument format for output directory
    pub output_arg: String,
    /// Argument to create GitHub repo
    pub github_arg: Option<String>,
    /// Argument to make repo private
    pub private_arg: Option<String>,
    /// Argument format for description
    pub description_arg: Option<String>,
}

/// Agent management service.
#[derive(Clone)]
pub struct AgentService {
    runtime: Arc<dyn ContainerRuntimeApi>,
    sessions: SessionService,
    repo: AgentRepository,
    scaffold_config: ScaffoldConfig,
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
            scaffold_config: ScaffoldConfig::default(),
        }
    }

    /// Create a new agent service with custom scaffold configuration.
    pub fn with_scaffold_config(
        runtime: Arc<dyn ContainerRuntimeApi>,
        sessions: SessionService,
        repo: AgentRepository,
        scaffold_config: ScaffoldConfig,
    ) -> Self {
        Self {
            runtime,
            sessions,
            repo,
            scaffold_config,
        }
    }

    /// List all agents for a session (running + available directories).
    pub async fn list_agents(
        &self,
        session_id: &str,
        include_context: bool,
    ) -> Result<Vec<AgentInfo>> {
        let session = self.get_session(session_id).await?;
        let container_id = session
            .container_id
            .as_ref()
            .context("session has no container")?;

        let mut agents = Vec::new();

        // 1. Add main agent (always exists)
        let main_status = self.check_agent_health(&session, MAIN_AGENT_PORT).await;
        let main_runtime = if include_context && main_status == AgentStatus::Running {
            self.fetch_opencode_runtime(session.opencode_port as u16)
                .await
        } else {
            None
        };
        let (main_has_agents_md, main_has_git) = self
            .check_directory_files(container_id, "/home/dev/workspace")
            .await;
        agents.push(AgentInfo::main(
            MAIN_AGENT_PORT,
            session.opencode_port as u16,
            main_status,
            main_has_agents_md,
            main_has_git,
            main_runtime,
        ));

        // 2. Get persisted agents from database
        let db_agents = self.repo.list_by_session(session_id).await?;
        let db_agent_ids: HashSet<_> = db_agents.iter().map(|a| a.agent_id.clone()).collect();

        // Add persisted agents
        for record in db_agents {
            let status = self
                .check_agent_health(&session, record.internal_port as u16)
                .await;

            // Update status in DB if it changed
            if status != record.status {
                let _ = self.repo.update_status(&record.id, status).await;
            }

            let runtime = if include_context && status == AgentStatus::Running {
                self.fetch_opencode_runtime(record.external_port as u16)
                    .await
            } else {
                None
            };

            agents.push(AgentInfo::sub_agent(
                record.agent_id,
                Some(record.internal_port as u16),
                Some(record.external_port as u16),
                status,
                record.has_agents_md,
                record.has_git,
                runtime,
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
                None,
            ));
        }

        // 4. Discover running opencode instances in the agent port range.
        let discovered = self
            .discover_running_agents(&session, &db_agent_ids, include_context)
            .await?;
        agents.extend(discovered);

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
        if let Some(existing) = self
            .repo
            .get_by_session_and_agent(session_id, &agent_id)
            .await?
        {
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

            self.start_opencode_in_container(container_id, &agent_id, internal_port)
                .await?;

            self.repo
                .update_status(&existing.id, AgentStatus::Starting)
                .await?;

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
        self.start_opencode_in_container(container_id, &agent_id, internal_port)
            .await?;

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
        let record = self
            .repo
            .get_by_session_and_agent(session_id, agent_id)
            .await?;

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
        self.repo
            .update_status(&record.id, AgentStatus::Stopped)
            .await?;

        Ok(StopAgentResponse { stopped: true })
    }

    /// Get agent status.
    pub async fn get_agent(
        &self,
        session_id: &str,
        agent_id: &str,
        include_context: bool,
    ) -> Result<Option<AgentInfo>> {
        let agents = self.list_agents(session_id, include_context).await?;
        Ok(agents.into_iter().find(|a| a.id == agent_id))
    }

    /// Create a new agent directory with optional scaffolding and AGENTS.md file.
    ///
    /// This creates the directory structure but does not start the agent.
    /// Call `start_agent` afterwards to start opencode serve.
    pub async fn create_agent(
        &self,
        session_id: &str,
        name: &str,
        description: &str,
        scaffold: Option<&AgentScaffoldRequest>,
    ) -> Result<CreateAgentResponse> {
        let session = self.get_session(session_id).await?;

        // Validate and sanitize the name
        let agent_id = self.validate_agent_directory(name)?;

        // Check if directory already exists
        let workspace_root = self.workspace_root(&session);
        let agent_path = workspace_root.join(&agent_id);
        match tokio::fs::metadata(&agent_path).await {
            Ok(_) => {
                anyhow::bail!("agent directory '{}' already exists", agent_id);
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err).context("failed to check agent directory")?;
            }
        }

        self.create_agent_directory(&session, &agent_id, description, scaffold)
            .await?;

        let container_path = format!("/home/dev/workspace/{}", agent_id);

        info!(
            "Created agent '{}' for session {} at {}",
            agent_id, session_id, container_path
        );

        Ok(CreateAgentResponse {
            id: agent_id.clone(),
            directory: container_path,
            color: agent_color(&agent_id),
        })
    }

    /// Execute a command in the session workspace.
    pub async fn exec_command(
        &self,
        session_id: &str,
        request: AgentExecRequest,
    ) -> Result<AgentExecResponse> {
        let session = self.get_session(session_id).await?;
        if session.runtime_mode == RuntimeMode::Local {
            let workspace_root = self.workspace_root(&session);
            let cwd = request
                .cwd
                .as_ref()
                .map(|path| self.resolve_workspace_path(&workspace_root, path));
            return self.exec_local_command(&request, cwd.as_deref()).await;
        }

        let container_id = session
            .container_id
            .as_ref()
            .context("session has no container")?;

        let container_root = PathBuf::from("/home/dev/workspace");
        let cwd = request
            .cwd
            .as_ref()
            .map(|path| self.resolve_workspace_path(&container_root, path));

        if request.shell {
            let mut command = request.command.clone();
            if let Some(ref cwd) = cwd {
                command = format!("cd {} && {}", cwd, command);
            }

            let args = ["bash", "-lc", command.as_str()];
            if request.detach {
                self.runtime
                    .exec_detached(container_id, &args)
                    .await
                    .context("failed to run command")?;
                return Ok(AgentExecResponse { output: None });
            }

            let output = self
                .runtime
                .exec_output(container_id, &args)
                .await
                .context("failed to run command")?;
            return Ok(AgentExecResponse {
                output: Some(output),
            });
        }

        if cwd.is_some() {
            anyhow::bail!("cwd requires shell=true for container execution");
        }

        let mut command: Vec<String> = Vec::with_capacity(1 + request.args.len());
        command.push(request.command);
        command.extend(request.args);
        let cmd_refs: Vec<&str> = command.iter().map(String::as_str).collect();

        if request.detach {
            self.runtime
                .exec_detached(container_id, &cmd_refs)
                .await
                .context("failed to run command")?;
            return Ok(AgentExecResponse { output: None });
        }

        let output = self
            .runtime
            .exec_output(container_id, &cmd_refs)
            .await
            .context("failed to run command")?;

        Ok(AgentExecResponse {
            output: Some(output),
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
        let record = self
            .repo
            .get_by_session_and_agent(session_id, agent_id)
            .await?;
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
                if let Some(directory) = self.fetch_opencode_directory(external_port).await {
                    let agent_id = directory
                        .strip_prefix("/home/dev/workspace/")
                        .unwrap_or(&directory)
                        .to_string();

                    if !agent_id.is_empty() && agent_id != "workspace" {
                        // Check if already in DB
                        let existing = self
                            .repo
                            .get_by_session_and_agent(session_id, &agent_id)
                            .await?;

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
                                self.repo
                                    .update_status(&record.id, AgentStatus::Running)
                                    .await?;
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
    #[allow(dead_code)]
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
        let agent_base_port = session
            .agent_base_port
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

        anyhow::bail!(
            "no available ports for sub-agents (max {} reached)",
            max_agents
        )
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

    async fn fetch_opencode_runtime(&self, external_port: u16) -> Option<AgentRuntimeInfo> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .ok()?;

        let base = format!("http://localhost:{}", external_port);
        let directory = fetch_json::<OpenCodePathResponse>(&client, &format!("{}/path", base))
            .await
            .and_then(|resp| resp.directory);

        let sessions =
            fetch_json::<Vec<OpenCodeSessionInfo>>(&client, &format!("{}/session", base)).await;

        let status_list =
            fetch_json::<Vec<OpenCodeSessionStatus>>(&client, &format!("{}/session/status", base))
                .await;

        let context = if let Some(ref sessions) = sessions {
            self.fetch_opencode_context(&client, &base, sessions).await
        } else {
            None
        };

        if directory.is_none() && sessions.is_none() && status_list.is_none() && context.is_none() {
            return None;
        }

        Some(AgentRuntimeInfo {
            directory,
            sessions,
            status_list,
            context,
        })
    }

    async fn fetch_opencode_directory(&self, external_port: u16) -> Option<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .ok()?;

        let base = format!("http://localhost:{}", external_port);
        fetch_json::<OpenCodePathResponse>(&client, &format!("{}/path", base))
            .await
            .and_then(|resp| resp.directory)
    }

    async fn fetch_opencode_context(
        &self,
        client: &reqwest::Client,
        base: &str,
        sessions: &[OpenCodeSessionInfo],
    ) -> Option<OpenCodeContextInfo> {
        let session = sessions.first()?;
        let messages: Vec<OpenCodeMessageWrapper> =
            fetch_json(client, &format!("{}/session/{}/message", base, session.id)).await?;

        let mut last_assistant: Option<OpenCodeMessageInfo> = None;
        let mut totals = OpenCodeTokenTotals {
            input: 0,
            output: 0,
            reasoning: 0,
            cache: OpenCodeTokenCache { read: 0, write: 0 },
        };

        for message in messages {
            if message.info.role != "assistant" {
                continue;
            }

            if let Some(tokens) = message.info.tokens.as_ref() {
                totals.input += tokens.input;
                totals.output += tokens.output;
                totals.reasoning += tokens.reasoning;
                totals.cache.read += tokens.cache.read;
                totals.cache.write += tokens.cache.write;
            }

            last_assistant = Some(message.info);
        }

        let last = last_assistant?;
        let model_id = last.model_id?;
        let provider_id = last.provider_id?;

        let providers: OpenCodeProvidersResponse =
            fetch_json(client, &format!("{}/provider", base)).await?;
        let provider = providers.all.iter().find(|item| item.id == provider_id)?;
        let model = provider.models.get(&model_id)?;

        let current_tokens = totals.input
            + totals.output
            + totals.reasoning
            + totals.cache.read
            + totals.cache.write;
        let usage = if model.limit.context > 0 {
            ((current_tokens as f64 / model.limit.context as f64) * 100.0).round() as u64
        } else {
            0
        };

        Some(OpenCodeContextInfo {
            session_id: session.id.clone(),
            session_title: session.title.clone(),
            model_id,
            provider_id,
            current_tokens,
            total_tokens: totals,
            limit: model.limit.clone(),
            usage,
        })
    }

    async fn check_directory_files(&self, container_id: &str, path: &str) -> (bool, bool) {
        // Check for AGENTS.md or SKILL.md
        let agent_marker_cmd = format!("test -f {}/AGENTS.md || test -f {}/SKILL.md", path, path);
        let has_agents_md = self
            .runtime
            .exec_output(container_id, &["bash", "-c", &agent_marker_cmd])
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

    fn workspace_root(&self, session: &Session) -> PathBuf {
        PathBuf::from(&session.workspace_path).join("workspace")
    }

    fn resolve_workspace_path(&self, workspace_root: &Path, path: &str) -> String {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_string_lossy().to_string()
        } else {
            workspace_root.join(path).to_string_lossy().to_string()
        }
    }

    async fn create_agent_directory(
        &self,
        session: &Session,
        agent_id: &str,
        description: &str,
        scaffold: Option<&AgentScaffoldRequest>,
    ) -> Result<()> {
        let workspace_root = self.workspace_root(session);
        tokio::fs::create_dir_all(&workspace_root)
            .await
            .context("failed to create workspace directory")?;

        let agent_path = workspace_root.join(agent_id);

        if let Some(scaffold) = scaffold {
            self.scaffold_agent_directory(agent_id, scaffold, &workspace_root)
                .await?;
            match tokio::fs::metadata(&agent_path).await {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => {
                    anyhow::bail!("scaffolded agent path is not a directory");
                }
                Err(err) => {
                    return Err(err).context("scaffolded agent directory not found")?;
                }
            }
        } else {
            tokio::fs::create_dir_all(&agent_path)
                .await
                .context("failed to create agent directory")?;
        }

        if !self
            .has_agent_marker_files(&agent_path)
            .await
            .context("failed to check agent markers")?
        {
            let agents_md_content = format!("# {}\n\n{}\n", agent_id, description);
            tokio::fs::write(agent_path.join("AGENTS.md"), agents_md_content)
                .await
                .context("failed to write AGENTS.md")?;
        }

        Ok(())
    }

    async fn scaffold_agent_directory(
        &self,
        agent_id: &str,
        scaffold: &AgentScaffoldRequest,
        workspace_root: &Path,
    ) -> Result<()> {
        let cfg = &self.scaffold_config;

        match scaffold {
            AgentScaffoldRequest::Template {
                template,
                github,
                private,
                description,
            } => {
                let mut command = Command::new(&cfg.binary);
                command.arg(&cfg.subcommand).arg(agent_id);
                command.arg(&cfg.template_arg).arg(template);
                command.arg(&cfg.output_arg).arg(workspace_root);

                if *github {
                    if let Some(ref arg) = cfg.github_arg {
                        command.arg(arg);
                    }
                }
                if *private {
                    if let Some(ref arg) = cfg.private_arg {
                        command.arg(arg);
                    }
                }
                if let Some(desc) = description {
                    if let Some(ref arg) = cfg.description_arg {
                        command.arg(arg).arg(desc);
                    }
                }

                debug!(
                    binary = %cfg.binary,
                    subcommand = %cfg.subcommand,
                    template = %template,
                    "Running scaffold command"
                );

                let output = command
                    .output()
                    .await
                    .with_context(|| format!("failed to run {} scaffold", cfg.binary))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("{} scaffold failed: {}", cfg.binary, stderr.trim());
                }
            }
        }

        Ok(())
    }

    async fn has_agent_marker_files(&self, agent_path: &Path) -> Result<bool> {
        let agents_md = agent_path.join("AGENTS.md");
        let skill_md = agent_path.join("SKILL.md");

        match tokio::fs::metadata(&agents_md).await {
            Ok(_) => return Ok(true),
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => return Err(err).context("failed to read AGENTS.md")?,
        }

        match tokio::fs::metadata(&skill_md).await {
            Ok(_) => Ok(true),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err).context("failed to read SKILL.md")?,
        }
    }

    async fn exec_local_command(
        &self,
        request: &AgentExecRequest,
        cwd: Option<&str>,
    ) -> Result<AgentExecResponse> {
        let mut command = if request.shell {
            let mut cmd = Command::new("bash");
            cmd.arg("-lc").arg(&request.command);
            cmd
        } else {
            let mut cmd = Command::new(&request.command);
            cmd.args(&request.args);
            cmd
        };

        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }

        if request.detach {
            command.stdout(Stdio::null()).stderr(Stdio::null());
            command.spawn().context("failed to spawn local command")?;
            return Ok(AgentExecResponse { output: None });
        }

        let output = command
            .output()
            .await
            .context("failed to run local command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("local command failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(AgentExecResponse {
            output: Some(stdout),
        })
    }

    fn extract_agent_id_from_path(&self, path: &str) -> Option<String> {
        let path = Path::new(path);
        let name = path.file_name()?.to_string_lossy().to_string();
        if self.validate_agent_directory(&name).is_ok() {
            Some(name)
        } else {
            None
        }
    }

    async fn discover_running_agents(
        &self,
        session: &Session,
        known_agents: &HashSet<String>,
        include_context: bool,
    ) -> Result<Vec<AgentInfo>> {
        let Some(agent_base_port) = session.agent_base_port else {
            return Ok(Vec::new());
        };

        let max_agents = session.max_agents.unwrap_or(10);
        let mut discovered = Vec::new();

        for i in 0..max_agents {
            let internal_port = INTERNAL_AGENT_BASE_PORT + i as u16;
            let external_port = (agent_base_port + i) as u16;

            if self.check_agent_health(session, internal_port).await != AgentStatus::Running {
                continue;
            }

            let directory = self.fetch_opencode_directory(external_port).await;
            let agent_id = directory
                .as_deref()
                .and_then(|dir| dir.strip_prefix("/home/dev/workspace/"))
                .and_then(|name| self.extract_agent_id_from_path(name))
                .unwrap_or_else(|| format!("unknown-{}", external_port));

            if known_agents.contains(&agent_id) {
                continue;
            }

            let runtime = if include_context {
                self.fetch_opencode_runtime(external_port).await
            } else {
                Some(AgentRuntimeInfo {
                    directory,
                    sessions: None,
                    status_list: None,
                    context: None,
                })
            };

            discovered.push(AgentInfo::sub_agent(
                agent_id,
                Some(internal_port),
                Some(external_port),
                AgentStatus::Running,
                false,
                false,
                runtime,
            ));
        }

        Ok(discovered)
    }
}

async fn fetch_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
) -> Option<T> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }

    response.json::<T>().await.ok()
}

#[derive(Debug, Deserialize)]
struct OpenCodePathResponse {
    directory: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeMessageWrapper {
    info: OpenCodeMessageInfo,
}

#[derive(Debug, Deserialize)]
struct OpenCodeMessageInfo {
    role: String,
    #[serde(rename = "modelID")]
    model_id: Option<String>,
    #[serde(rename = "providerID")]
    provider_id: Option<String>,
    tokens: Option<OpenCodeTokenInfo>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeTokenInfo {
    input: u64,
    output: u64,
    reasoning: u64,
    cache: OpenCodeTokenCache,
}

#[derive(Debug, Deserialize)]
struct OpenCodeProvidersResponse {
    all: Vec<OpenCodeProvider>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeProvider {
    id: String,
    models: std::collections::HashMap<String, OpenCodeModelInfo>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeModelInfo {
    limit: OpenCodeTokenLimit,
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

//! Agent data models.

use serde::{Deserialize, Serialize};

/// Agent status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    /// Agent is running (opencode serve is responding).
    Running,
    /// Agent is starting up.
    Starting,
    /// Agent is stopped (directory exists but opencode not running).
    Stopped,
    /// Agent failed to start or crashed.
    Failed,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Running => write!(f, "running"),
            AgentStatus::Starting => write!(f, "starting"),
            AgentStatus::Stopped => write!(f, "stopped"),
            AgentStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for AgentStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "running" => Ok(AgentStatus::Running),
            "starting" => Ok(AgentStatus::Starting),
            "stopped" => Ok(AgentStatus::Stopped),
            "failed" => Ok(AgentStatus::Failed),
            _ => Err(format!("unknown agent status: {}", s)),
        }
    }
}

impl TryFrom<String> for AgentStatus {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

/// Information about an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent ID (directory name, or "main" for workspace root).
    pub id: String,
    /// Human-readable name (derived from id).
    pub name: String,
    /// Directory path inside the container.
    pub directory: String,
    /// Internal port (inside container, None if stopped).
    pub port: Option<u16>,
    /// External port (mapped to host, for clients to connect to).
    pub external_port: Option<u16>,
    /// Current status.
    pub status: AgentStatus,
    /// Whether the directory has an AGENTS.md file.
    pub has_agents_md: bool,
    /// Whether the directory is a git repository.
    pub has_git: bool,
    /// Color for UI (derived from name hash).
    pub color: String,
    /// Optional runtime details from the agent engine (opencode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<AgentRuntimeInfo>,
}

impl AgentInfo {
    /// Create a new agent info for the main workspace.
    ///
    /// The main agent uses the session's opencode port, which is already mapped externally.
    pub fn main(
        internal_port: u16,
        external_port: u16,
        status: AgentStatus,
        has_agents_md: bool,
        has_git: bool,
        runtime: Option<AgentRuntimeInfo>,
    ) -> Self {
        Self {
            id: "main".to_string(),
            name: "Main Workspace".to_string(),
            directory: "/home/dev/workspace".to_string(),
            port: Some(internal_port),
            external_port: Some(external_port),
            status,
            has_agents_md,
            has_git,
            color: agent_color("main"),
            runtime,
        }
    }

    /// Create a new agent info for a sub-agent.
    pub fn sub_agent(
        id: String,
        internal_port: Option<u16>,
        external_port: Option<u16>,
        status: AgentStatus,
        has_agents_md: bool,
        has_git: bool,
        runtime: Option<AgentRuntimeInfo>,
    ) -> Self {
        let color = agent_color(&id);
        let name = id
            .split('-')
            .map(|s| {
                let mut c = s.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        Self {
            directory: format!("/home/dev/workspace/{}", id),
            id,
            name,
            port: internal_port,
            external_port,
            status,
            has_agents_md,
            has_git,
            color,
            runtime,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRuntimeInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions: Option<Vec<OpenCodeSessionInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_list: Option<Vec<OpenCodeSessionStatus>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<OpenCodeContextInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeSessionInfo {
    pub id: String,
    pub title: String,
    pub time: OpenCodeSessionTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeSessionTime {
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeSessionStatus {
    #[serde(rename = "type")]
    pub status_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeContextInfo {
    pub session_id: String,
    pub session_title: String,
    pub model_id: String,
    pub provider_id: String,
    pub current_tokens: u64,
    pub total_tokens: OpenCodeTokenTotals,
    pub limit: OpenCodeTokenLimit,
    pub usage: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeTokenTotals {
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cache: OpenCodeTokenCache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeTokenCache {
    pub read: u64,
    pub write: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeTokenLimit {
    pub context: u64,
    pub output: u64,
}

/// Derive a color from an agent name (deterministic).
pub fn agent_color(name: &str) -> String {
    const COLORS: &[&str] = &[
        "#3B82F6", // blue
        "#10B981", // green
        "#F59E0B", // amber
        "#EF4444", // red
        "#8B5CF6", // purple
        "#EC4899", // pink
        "#06B6D4", // cyan
        "#84CC16", // lime
    ];

    let hash: usize = name.bytes().map(|b| b as usize).sum();
    COLORS[hash % COLORS.len()].to_string()
}

/// Request to start a new agent.
#[derive(Debug, Deserialize)]
pub struct StartAgentRequest {
    /// Directory name (relative to ~/workspace/).
    pub directory: String,
}

/// Request to create a new agent.
#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    /// Agent name (becomes directory name).
    pub name: String,
    /// Agent description (becomes AGENTS.md content).
    pub description: String,
    /// Optional scaffolding source for the agent directory.
    #[serde(default)]
    pub scaffold: Option<AgentScaffoldRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentScaffoldRequest {
    /// Use configured scaffold tool to create project from template.
    /// The scaffold binary and arguments are configured in config.toml [scaffold] section.
    #[serde(alias = "byt_template")]
    Template {
        template: String,
        #[serde(default)]
        github: bool,
        #[serde(default)]
        private: bool,
        #[serde(default)]
        description: Option<String>,
    },
}

/// Execute a command in a session workspace.
#[derive(Debug, Deserialize)]
pub struct AgentExecRequest {
    /// Command to execute (binary or shell command when shell=true).
    pub command: String,
    /// Optional arguments for the command when shell=false.
    #[serde(default)]
    pub args: Vec<String>,
    /// Optional working directory (absolute or relative to workspace root).
    #[serde(default)]
    pub cwd: Option<String>,
    /// Run command via `bash -lc`.
    #[serde(default)]
    pub shell: bool,
    /// Run command without waiting for output.
    #[serde(default)]
    pub detach: bool,
}

/// Response when starting an agent.
#[derive(Debug, Serialize)]
pub struct StartAgentResponse {
    pub id: String,
    /// Internal port (inside container).
    pub port: u16,
    /// External port (mapped to host, for clients to connect to).
    pub external_port: u16,
    pub status: AgentStatus,
}

/// Response when stopping an agent.
#[derive(Debug, Serialize)]
pub struct StopAgentResponse {
    pub stopped: bool,
}

/// Response when creating a new agent.
#[derive(Debug, Serialize)]
pub struct CreateAgentResponse {
    /// Agent ID (directory name).
    pub id: String,
    /// Directory path inside the container.
    pub directory: String,
    /// Color for UI (derived from name hash).
    pub color: String,
}

/// Response for a command execution.
#[derive(Debug, Serialize)]
pub struct AgentExecResponse {
    pub output: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_color_deterministic() {
        assert_eq!(agent_color("main"), agent_color("main"));
        assert_eq!(agent_color("doc-writer"), agent_color("doc-writer"));
    }

    #[test]
    fn test_agent_color_different() {
        // Different names should (usually) get different colors
        let c1 = agent_color("main");
        let c2 = agent_color("research");
        // They might be the same by chance, but let's at least verify they're valid
        assert!(c1.starts_with('#'));
        assert!(c2.starts_with('#'));
    }

    #[test]
    fn test_sub_agent_name_formatting() {
        let agent = AgentInfo::sub_agent(
            "doc-writer".to_string(),
            Some(4001),
            Some(41824),
            AgentStatus::Running,
            true,
            false,
            None,
        );
        assert_eq!(agent.name, "Doc Writer");
        assert_eq!(agent.directory, "/home/dev/workspace/doc-writer");
        assert_eq!(agent.port, Some(4001));
        assert_eq!(agent.external_port, Some(41824));
    }

    #[test]
    fn test_agent_status_from_str() {
        assert_eq!(
            "running".parse::<AgentStatus>().unwrap(),
            AgentStatus::Running
        );
        assert_eq!(
            "starting".parse::<AgentStatus>().unwrap(),
            AgentStatus::Starting
        );
        assert_eq!(
            "stopped".parse::<AgentStatus>().unwrap(),
            AgentStatus::Stopped
        );
        assert_eq!(
            "failed".parse::<AgentStatus>().unwrap(),
            AgentStatus::Failed
        );
        assert!("invalid".parse::<AgentStatus>().is_err());
    }

    #[test]
    fn test_main_agent() {
        let agent = AgentInfo::main(41820, 41820, AgentStatus::Running, true, true, None);
        assert_eq!(agent.id, "main");
        assert_eq!(agent.name, "Main Workspace");
        assert_eq!(agent.port, Some(41820));
        assert_eq!(agent.external_port, Some(41820));
    }

    #[test]
    fn test_scaffold_request_deserialize() {
        // Test with new "template" type
        let json = r#"{
            "name": "example",
            "description": "test",
            "scaffold": {
                "type": "template",
                "template": "rust-cli",
                "github": true,
                "private": true,
                "description": "demo"
            }
        }"#;

        let request: CreateAgentRequest = serde_json::from_str(json).unwrap();
        let scaffold = request.scaffold.unwrap();
        match scaffold {
            AgentScaffoldRequest::Template {
                template,
                github,
                private,
                description,
            } => {
                assert_eq!(template, "rust-cli");
                assert!(github);
                assert!(private);
                assert_eq!(description, Some("demo".to_string()));
            }
        }
    }

    #[test]
    fn test_scaffold_request_deserialize_legacy_byt_template() {
        // Test backward compatibility with "byt_template" type
        let json = r#"{
            "name": "example",
            "description": "test",
            "scaffold": {
                "type": "byt_template",
                "template": "rust-cli",
                "github": false,
                "private": false
            }
        }"#;

        let request: CreateAgentRequest = serde_json::from_str(json).unwrap();
        let scaffold = request.scaffold.unwrap();
        match scaffold {
            AgentScaffoldRequest::Template { template, .. } => {
                assert_eq!(template, "rust-cli");
            }
        }
    }

    #[test]
    fn test_exec_request_defaults() {
        let json = r#"{"command":"ls"}"#;
        let request: AgentExecRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.command, "ls");
        assert!(request.args.is_empty());
        assert!(request.cwd.is_none());
        assert!(!request.shell);
        assert!(!request.detach);
    }
}

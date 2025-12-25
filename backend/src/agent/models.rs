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
        }
    }
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
        );
        assert_eq!(agent.name, "Doc Writer");
        assert_eq!(agent.directory, "/home/dev/workspace/doc-writer");
        assert_eq!(agent.port, Some(4001));
        assert_eq!(agent.external_port, Some(41824));
    }

    #[test]
    fn test_agent_status_from_str() {
        assert_eq!("running".parse::<AgentStatus>().unwrap(), AgentStatus::Running);
        assert_eq!("starting".parse::<AgentStatus>().unwrap(), AgentStatus::Starting);
        assert_eq!("stopped".parse::<AgentStatus>().unwrap(), AgentStatus::Stopped);
        assert_eq!("failed".parse::<AgentStatus>().unwrap(), AgentStatus::Failed);
        assert!("invalid".parse::<AgentStatus>().is_err());
    }

    #[test]
    fn test_main_agent() {
        let agent = AgentInfo::main(41820, 41820, AgentStatus::Running, true, true);
        assert_eq!(agent.id, "main");
        assert_eq!(agent.name, "Main Workspace");
        assert_eq!(agent.port, Some(41820));
        assert_eq!(agent.external_port, Some(41820));
    }
}

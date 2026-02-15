//! Session data models.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use ts_rs::TS;

/// Session status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum SessionStatus {
    /// Session is being set up.
    Pending,
    /// Container is starting.
    Starting,
    /// Container is running.
    Running,
    /// Container is being stopped.
    Stopping,
    /// Container has stopped.
    Stopped,
    /// Session failed to start or crashed.
    Failed,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Pending => write!(f, "pending"),
            SessionStatus::Starting => write!(f, "starting"),
            SessionStatus::Running => write!(f, "running"),
            SessionStatus::Stopping => write!(f, "stopping"),
            SessionStatus::Stopped => write!(f, "stopped"),
            SessionStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for SessionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(SessionStatus::Pending),
            "starting" => Ok(SessionStatus::Starting),
            "running" => Ok(SessionStatus::Running),
            "stopping" => Ok(SessionStatus::Stopping),
            "stopped" => Ok(SessionStatus::Stopped),
            "failed" => Ok(SessionStatus::Failed),
            _ => Err(format!("unknown session status: {}", s)),
        }
    }
}

/// Runtime mode for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum RuntimeMode {
    /// Container-based runtime (Docker/Podman).
    #[default]
    Container,
    /// Local runtime (native processes).
    Local,
}

impl std::fmt::Display for RuntimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeMode::Container => write!(f, "container"),
            RuntimeMode::Local => write!(f, "local"),
        }
    }
}

impl std::str::FromStr for RuntimeMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "container" => Ok(RuntimeMode::Container),
            "local" => Ok(RuntimeMode::Local),
            _ => Err(format!("unknown runtime mode: {}", s)),
        }
    }
}

impl TryFrom<String> for RuntimeMode {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

/// A container session.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct Session {
    /// Unique session ID.
    pub id: String,
    /// Human-readable session ID (e.g., "cold-lamp-bird").
    pub readable_id: Option<String>,
    /// Container ID (once started) or comma-separated PIDs for local mode.
    pub container_id: Option<String>,
    /// Container name (or session identifier for local mode).
    pub container_name: String,
    /// User ID who owns this session.
    pub user_id: String,
    /// Path to the workspace directory.
    pub workspace_path: String,
    /// Agent name for the session.
    /// If not set, the default agent is used.
    pub agent: Option<String>,
    /// Container image to use (ignored in local mode).
    pub image: String,
    /// Image digest (sha256) when the container was created.
    pub image_digest: Option<String>,
    /// Reserved agent port.
    pub agent_port: i64,
    /// Port for file server.
    pub fileserver_port: i64,
    /// Port for ttyd terminal.
    pub ttyd_port: i64,
    /// Port for EAVS LLM proxy.
    pub eavs_port: Option<i64>,
    /// Base external port for sub-agents. Sub-agents use ports agent_base_port to agent_base_port + max_agents - 1.
    pub agent_base_port: Option<i64>,
    /// Maximum number of sub-agents allowed for this session.
    #[serde(default = "default_max_agents")]
    pub max_agents: Option<i64>,
    /// EAVS virtual key ID (human-readable, e.g., "cold-lamp").
    pub eavs_key_id: Option<String>,
    /// EAVS virtual key hash (for API lookups).
    pub eavs_key_hash: Option<String>,
    /// EAVS virtual key value (only set during container creation).
    #[serde(skip_serializing)]
    pub eavs_virtual_key: Option<String>,
    /// Port for mmry memory service.
    pub mmry_port: Option<i64>,
    /// Current session status.
    #[sqlx(try_from = "String")]
    pub status: SessionStatus,
    /// Runtime mode (container or local).
    #[sqlx(try_from = "String", default)]
    #[serde(default)]
    pub runtime_mode: RuntimeMode,
    /// When the session was created.
    pub created_at: String,
    /// When the container started.
    pub started_at: Option<String>,
    /// When the container stopped.
    pub stopped_at: Option<String>,
    /// Last activity timestamp (for idle timeout).
    pub last_activity_at: Option<String>,
    /// Error message if failed.
    pub error_message: Option<String>,
}

fn default_max_agents() -> Option<i64> {
    Some(10)
}

/// Configuration for creating a new session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SessionConfig {
    /// Path to the workspace directory to mount.
    pub workspace_path: String,
    /// Container image to use.
    pub image: String,
    /// Base port for services (will allocate agent, fileserver, ttyd sequentially).
    pub base_port: u16,
    /// Environment variables to inject.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            workspace_path: "/tmp/workspace".to_string(),
            image: "octo-dev:latest".to_string(),
            base_port: 41820,
            env: Default::default(),
        }
    }
}

/// Request to create a new session.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct CreateSessionRequest {
    /// Path to the workspace directory.
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// Container image to use (optional, defaults to octo-dev).
    #[serde(default)]
    pub image: Option<String>,
    /// Agent name for the session.
    /// Agents are defined in Pi's config or the workspace's
    /// agents directory.
    #[serde(default)]
    pub agent: Option<String>,
    /// Environment variables to inject.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// Response from session creation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[allow(dead_code)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct SessionResponse {
    /// Session information.
    #[serde(flatten)]
    pub session: Session,
    /// URLs for accessing the session.
    pub urls: SessionUrls,
}

/// URLs for accessing session services.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[allow(dead_code)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct SessionUrls {
    /// URL for agent runtime API (reserved).
    pub agent: String,
    /// URL for file server.
    pub fileserver: String,
    /// URL for terminal WebSocket.
    pub terminal: String,
}

impl Session {
    /// Check if the session is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, SessionStatus::Stopped | SessionStatus::Failed)
    }

    /// Check if the session is active (running or starting).
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            SessionStatus::Starting | SessionStatus::Running
        )
    }

    /// Get the URLs for this session.
    #[allow(dead_code)]
    pub fn urls(&self, host: &str) -> SessionUrls {
        SessionUrls {
            agent: format!("http://{}:{}", host, self.agent_port),
            fileserver: format!("http://{}:{}", host, self.fileserver_port),
            terminal: format!("ws://{}:{}", host, self.ttyd_port),
        }
    }
}

// Implement conversion from String for SQLx
impl TryFrom<String> for SessionStatus {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

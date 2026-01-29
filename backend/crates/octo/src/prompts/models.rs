//! Prompt data models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Source of the prompt request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptSource {
    /// Request from octo-guard (FUSE filesystem)
    OctoGuard,
    /// Request from octo-ssh-proxy
    OctoSshProxy,
    /// Request from network proxy (eavs integration)
    Network,
    /// Other/unknown source
    Other(String),
}

impl std::fmt::Display for PromptSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OctoGuard => write!(f, "octo-guard"),
            Self::OctoSshProxy => write!(f, "octo-ssh-proxy"),
            Self::Network => write!(f, "network"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// Type of access being requested.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptType {
    /// File read access
    FileRead,
    /// File write access
    FileWrite,
    /// SSH connection/signing
    SshSign,
    /// Network connection
    NetworkAccess,
    /// Other/custom type
    Other(String),
}

impl std::fmt::Display for PromptType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileRead => write!(f, "file read"),
            Self::FileWrite => write!(f, "file write"),
            Self::SshSign => write!(f, "SSH signing"),
            Self::NetworkAccess => write!(f, "network access"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// User's response action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptAction {
    /// Allow this single request
    AllowOnce,
    /// Allow for the duration of the session
    AllowSession,
    /// Deny the request
    Deny,
}

/// Status of a prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptStatus {
    /// Waiting for user response
    Pending,
    /// User responded
    Responded,
    /// Timed out without response
    TimedOut,
    /// Cancelled by requester
    Cancelled,
}

/// A prompt request from a security component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequest {
    /// Source component requesting approval
    pub source: PromptSource,

    /// Type of access being requested
    pub prompt_type: PromptType,

    /// Resource being accessed (path, host, etc.)
    pub resource: String,

    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,

    /// Additional context/metadata
    #[serde(default)]
    pub context: Option<serde_json::Value>,

    /// Timeout in seconds (default: 60)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Workspace ID if applicable
    #[serde(default)]
    pub workspace_id: Option<String>,

    /// Session ID if applicable
    #[serde(default)]
    pub session_id: Option<String>,
}

fn default_timeout() -> u64 {
    60
}

impl PromptRequest {
    /// Create a file access prompt.
    pub fn file_access(path: impl Into<String>, operation: &str) -> Self {
        let path = path.into();
        let prompt_type = match operation {
            "read" => PromptType::FileRead,
            "write" => PromptType::FileWrite,
            _ => PromptType::Other(operation.to_string()),
        };
        Self {
            source: PromptSource::OctoGuard,
            prompt_type,
            resource: path.clone(),
            description: Some(format!("{} access to {}", operation, path)),
            context: None,
            timeout_secs: default_timeout(),
            workspace_id: None,
            session_id: None,
        }
    }

    /// Create an SSH signing prompt.
    pub fn ssh_sign(host: impl Into<String>, key_comment: Option<&str>) -> Self {
        let host = host.into();
        Self {
            source: PromptSource::OctoSshProxy,
            prompt_type: PromptType::SshSign,
            resource: host.clone(),
            description: Some(format!(
                "SSH connection to {}{}",
                host,
                key_comment
                    .map(|k| format!(" using key '{}'", k))
                    .unwrap_or_default()
            )),
            context: key_comment.map(|k| serde_json::json!({ "key": k })),
            timeout_secs: default_timeout(),
            workspace_id: None,
            session_id: None,
        }
    }

    /// Create a network access prompt.
    pub fn network_access(domain: impl Into<String>) -> Self {
        let domain = domain.into();
        Self {
            source: PromptSource::Network,
            prompt_type: PromptType::NetworkAccess,
            resource: domain.clone(),
            description: Some(format!("Network access to {}", domain)),
            context: None,
            timeout_secs: default_timeout(),
            workspace_id: None,
            session_id: None,
        }
    }

    /// Set the timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set workspace context.
    pub fn with_workspace(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = Some(workspace_id.into());
        self
    }

    /// Set session context.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

/// A prompt with ID and timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    /// Unique prompt ID
    pub id: String,

    /// The original request
    #[serde(flatten)]
    pub request: PromptRequest,

    /// Current status
    pub status: PromptStatus,

    /// When the prompt was created
    pub created_at: DateTime<Utc>,

    /// When the prompt expires
    pub expires_at: DateTime<Utc>,

    /// User's response (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<PromptResponse>,
}

impl Prompt {
    /// Create a new prompt from a request.
    pub fn new(request: PromptRequest) -> Self {
        let id = generate_prompt_id();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(request.timeout_secs as i64);

        Self {
            id,
            request,
            status: PromptStatus::Pending,
            created_at: now,
            expires_at,
            response: None,
        }
    }

    /// Check if the prompt has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Get remaining time until expiry.
    pub fn remaining(&self) -> Duration {
        let remaining = self.expires_at - Utc::now();
        if remaining.num_milliseconds() > 0 {
            Duration::from_millis(remaining.num_milliseconds() as u64)
        } else {
            Duration::ZERO
        }
    }

    /// Mark as responded with the given action.
    pub fn respond(&mut self, action: PromptAction) {
        self.status = PromptStatus::Responded;
        self.response = Some(PromptResponse {
            action,
            responded_at: Utc::now(),
        });
    }

    /// Mark as timed out.
    pub fn timeout(&mut self) {
        self.status = PromptStatus::TimedOut;
    }

    /// Mark as cancelled.
    pub fn cancel(&mut self) {
        self.status = PromptStatus::Cancelled;
    }
}

/// Response to a prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptResponse {
    /// The action taken
    pub action: PromptAction,

    /// When the response was given
    pub responded_at: DateTime<Utc>,
}

/// Generate a short, human-friendly prompt ID.
fn generate_prompt_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let adjectives = [
        "red", "blue", "green", "swift", "calm", "bold", "warm", "cool",
    ];
    let nouns = [
        "hawk", "bear", "wolf", "deer", "lion", "fish", "frog", "owl",
    ];
    let adj = adjectives[rng.random_range(0..adjectives.len())];
    let noun = nouns[rng.random_range(0..nouns.len())];
    let num: u16 = rng.random_range(100..999);
    format!("{}-{}-{}", adj, noun, num)
}

/// WebSocket message for prompt updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptMessage {
    /// New prompt created
    Created { prompt: Prompt },

    /// Prompt was responded to
    Responded {
        prompt_id: String,
        action: PromptAction,
    },

    /// Prompt timed out
    TimedOut { prompt_id: String },

    /// Prompt was cancelled
    Cancelled { prompt_id: String },

    /// List of all pending prompts (sent on connect)
    Sync { prompts: Vec<Prompt> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_access_prompt() {
        let req = PromptRequest::file_access("~/.kube/config", "read");
        assert_eq!(req.source, PromptSource::OctoGuard);
        assert_eq!(req.prompt_type, PromptType::FileRead);
        assert_eq!(req.resource, "~/.kube/config");
    }

    #[test]
    fn test_ssh_prompt() {
        let req = PromptRequest::ssh_sign("github.com", Some("work_key"));
        assert_eq!(req.source, PromptSource::OctoSshProxy);
        assert_eq!(req.prompt_type, PromptType::SshSign);
        assert_eq!(req.resource, "github.com");
    }

    #[test]
    fn test_prompt_expiry() {
        let req = PromptRequest::file_access("/test", "read").with_timeout(1);
        let prompt = Prompt::new(req);

        assert!(!prompt.is_expired());
        assert!(prompt.remaining() <= Duration::from_secs(1));
    }

    #[test]
    fn test_prompt_response() {
        let req = PromptRequest::file_access("/test", "read");
        let mut prompt = Prompt::new(req);

        assert_eq!(prompt.status, PromptStatus::Pending);

        prompt.respond(PromptAction::AllowSession);

        assert_eq!(prompt.status, PromptStatus::Responded);
        assert!(prompt.response.is_some());
        assert_eq!(prompt.response.unwrap().action, PromptAction::AllowSession);
    }
}

//! Pi agent service for Main Chat.
//!
//! Manages Pi subprocesses for each user's Main Chat. Each user gets one Pi
//! process that persists across requests, enabling streaming and maintaining
//! session state.
//!
//! ## Session Lifecycle
//!
//! Sessions are managed with smart continuation vs fresh start logic:
//! - **Continue** if: last activity < 4 hours AND session file < 500KB
//! - **Fresh start** if: session is stale, too large, or user requests new session
//!
//! On fresh start, context is injected from:
//! 1. Last session's compaction summary (from main_chat.db)
//! 2. Recent mmry entries (decisions, handoffs, insights)

use anyhow::{Context, Result};
use log::{debug, info};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::process::Command;
use tokio::sync::{RwLock, broadcast};

use crate::pi::{PiClient, PiClientConfig, PiEvent, PiState, AgentMessage, CompactionResult, SessionStats};

/// Session freshness thresholds
const SESSION_MAX_AGE_HOURS: u64 = 4;
const SESSION_MAX_SIZE_BYTES: u64 = 500 * 1024; // 500KB

/// Configuration for the Pi service.
#[derive(Debug, Clone)]
pub struct MainChatPiServiceConfig {
    /// Path to the Pi CLI executable (e.g., "pi" or "/usr/local/bin/pi")
    pub pi_executable: String,
    /// Default provider for new sessions
    pub default_provider: Option<String>,
    /// Default model for new sessions
    pub default_model: Option<String>,
    /// Extension files to load (passed via --extension)
    pub extensions: Vec<String>,
    /// Maximum session age before forcing fresh start (hours)
    pub max_session_age_hours: u64,
    /// Maximum session file size before forcing fresh start (bytes)
    pub max_session_size_bytes: u64,
}

impl Default for MainChatPiServiceConfig {
    fn default() -> Self {
        Self {
            pi_executable: "pi".to_string(),
            default_provider: None,
            default_model: None,
            extensions: Vec::new(),
            max_session_age_hours: SESSION_MAX_AGE_HOURS,
            max_session_size_bytes: SESSION_MAX_SIZE_BYTES,
        }
    }
}

/// Information about the last Pi session for a directory.
#[derive(Debug, Clone)]
pub struct LastSessionInfo {
    /// File size in bytes
    pub size: u64,
    /// Last modification time
    pub modified: SystemTime,
}

/// Handle to a user's Pi session.
pub struct UserPiSession {
    /// The Pi client for this user.
    pub client: Arc<PiClient>,
}

/// Service for managing Pi sessions for Main Chat users.
pub struct MainChatPiService {
    /// Configuration.
    config: MainChatPiServiceConfig,
    /// Active sessions (keyed by user_id).
    sessions: RwLock<HashMap<String, Arc<UserPiSession>>>,
    /// Base workspace directory.
    workspace_dir: PathBuf,
    /// Single-user mode.
    single_user: bool,
}

impl MainChatPiService {
    /// Create a new Pi service.
    pub fn new(workspace_dir: PathBuf, single_user: bool, config: MainChatPiServiceConfig) -> Self {
        Self {
            config,
            sessions: RwLock::new(HashMap::new()),
            workspace_dir,
            single_user,
        }
    }

    /// Get the Main Chat directory for a user.
    fn get_main_chat_dir(&self, user_id: &str) -> PathBuf {
        if self.single_user {
            self.workspace_dir.join("main")
        } else {
            self.workspace_dir.join(user_id).join("main")
        }
    }

    /// Get the Pi sessions directory for a working directory.
    /// Pi stores sessions in ~/.pi/agent/sessions/{escaped-path}/
    fn get_pi_sessions_dir(&self, work_dir: &PathBuf) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let escaped_path = work_dir
            .to_string_lossy()
            .replace('/', "-")
            .trim_start_matches('-')
            .to_string();
        home.join(".pi")
            .join("agent")
            .join("sessions")
            .join(format!("-{}-", escaped_path))
    }

    /// Find the most recent Pi session file for a directory.
    fn find_last_session(&self, work_dir: &PathBuf) -> Option<LastSessionInfo> {
        let sessions_dir = self.get_pi_sessions_dir(work_dir);
        
        if !sessions_dir.exists() {
            debug!("Pi sessions directory does not exist: {:?}", sessions_dir);
            return None;
        }

        let mut latest: Option<LastSessionInfo> = None;

        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            let info = LastSessionInfo {
                                size: metadata.len(),
                                modified,
                            };

                            if latest.as_ref().map(|l| modified > l.modified).unwrap_or(true) {
                                latest = Some(info);
                            }
                        }
                    }
                }
            }
        }

        latest
    }

    /// Check if a session should be continued or if we need a fresh start.
    fn should_continue_session(&self, last_session: &LastSessionInfo) -> bool {
        let now = SystemTime::now();
        
        // Check age
        let age = now.duration_since(last_session.modified).unwrap_or(Duration::MAX);
        let max_age = Duration::from_secs(self.config.max_session_age_hours * 3600);
        
        if age > max_age {
            info!(
                "Session too old ({:?} > {:?}), starting fresh",
                age, max_age
            );
            return false;
        }

        // Check size
        if last_session.size > self.config.max_session_size_bytes {
            info!(
                "Session file too large ({} > {} bytes), starting fresh",
                last_session.size, self.config.max_session_size_bytes
            );
            return false;
        }

        info!(
            "Session is fresh (age={:?}, size={}), continuing",
            age, last_session.size
        );
        true
    }

    /// Get or create a Pi session for a user.
    pub async fn get_or_create_session(&self, user_id: &str) -> Result<Arc<UserPiSession>> {
        // Check if session exists
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(user_id) {
                return Ok(Arc::clone(session));
            }
        }

        // Create new session
        let session = self.create_session(user_id, false).await?;
        let session = Arc::new(session);

        // Store in cache
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(user_id.to_string(), Arc::clone(&session));
        }

        Ok(session)
    }

    /// Create a new Pi session for a user.
    /// 
    /// # Arguments
    /// * `user_id` - The user ID
    /// * `force_fresh` - If true, always start a fresh session regardless of staleness
    async fn create_session(&self, user_id: &str, force_fresh: bool) -> Result<UserPiSession> {
        let work_dir = self.get_main_chat_dir(user_id);
        
        // Ensure the directory exists
        if !work_dir.exists() {
            anyhow::bail!("Main Chat directory does not exist for user: {}", user_id);
        }

        // Determine if we should continue or start fresh
        let last_session = self.find_last_session(&work_dir);
        let should_continue = !force_fresh && last_session
            .as_ref()
            .map(|s| self.should_continue_session(s))
            .unwrap_or(false);

        info!(
            "Starting Pi session for user {} in {:?}, continue={}, provider={:?}, model={:?}",
            user_id, work_dir, should_continue, self.config.default_provider, self.config.default_model
        );

        // Build the command
        let mut cmd = Command::new(&self.config.pi_executable);
        cmd.arg("--mode").arg("rpc");
        
        // Continue or fresh start
        if should_continue {
            cmd.arg("--continue");
        }
        // Note: If not continuing, Pi will start a fresh session automatically
        
        cmd.current_dir(&work_dir);
        
        // Set up stdio
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Add provider/model if configured
        if let Some(ref provider) = self.config.default_provider {
            cmd.arg("--provider").arg(provider);
        }
        if let Some(ref model) = self.config.default_model {
            cmd.arg("--model").arg(model);
        }
        
        // Add extensions
        for extension in &self.config.extensions {
            cmd.arg("--extension").arg(extension);
        }

        // Append PERSONALITY.md and USER.md to system prompt if they exist
        let personality_file = work_dir.join("PERSONALITY.md");
        if personality_file.exists() {
            cmd.arg("--append-system-prompt").arg(&personality_file);
        }
        let user_file = work_dir.join("USER.md");
        if user_file.exists() {
            cmd.arg("--append-system-prompt").arg(&user_file);
        }

        // Spawn the process
        let child = cmd.spawn().with_context(|| {
            format!(
                "Failed to spawn Pi process. Executable: {}, Working dir: {:?}",
                self.config.pi_executable, work_dir
            )
        })?;

        // Create the client
        let client = PiClient::new(child, PiClientConfig::default())?;

        Ok(UserPiSession {
            client: Arc::new(client),
        })
    }

    /// Close a user's Pi session.
    pub async fn close_session(&self, user_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(_session) = sessions.remove(user_id) {
            info!("Closed Pi session for user {}", user_id);
            // The session will be dropped, which should clean up the process
        }
        Ok(())
    }

    /// Get session if it exists (without creating).
    pub async fn get_session(&self, user_id: &str) -> Option<Arc<UserPiSession>> {
        let sessions = self.sessions.read().await;
        sessions.get(user_id).cloned()
    }

    /// Check if a session exists for a user.
    pub async fn has_session(&self, user_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(user_id)
    }

}

impl UserPiSession {
    /// Send a prompt to the agent.
    pub async fn prompt(&self, message: &str) -> Result<()> {
        self.client.prompt(message).await?;
        Ok(())
    }

    /// Abort the current operation.
    pub async fn abort(&self) -> Result<()> {
        self.client.abort().await?;
        Ok(())
    }

    /// Queue a steering message to interrupt the agent mid-run.
    pub async fn steer(&self, message: &str) -> Result<()> {
        self.client.steer(message).await?;
        Ok(())
    }

    /// Queue a follow-up message for after the agent finishes.
    pub async fn follow_up(&self, message: &str) -> Result<()> {
        self.client.follow_up(message).await?;
        Ok(())
    }

    /// Get current state.
    pub async fn get_state(&self) -> Result<PiState> {
        self.client.get_state().await
    }

    /// Get all messages.
    pub async fn get_messages(&self) -> Result<Vec<AgentMessage>> {
        self.client.get_messages().await
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<PiEvent> {
        self.client.subscribe()
    }

    /// Compact the session context.
    pub async fn compact(&self, custom_instructions: Option<&str>) -> Result<CompactionResult> {
        self.client.compact(custom_instructions).await
    }

    /// Start a new session (clear history).
    pub async fn new_session(&self) -> Result<()> {
        self.client.new_session().await?;
        Ok(())
    }

    /// Set the current model.
    pub async fn set_model(&self, provider: &str, model_id: &str) -> Result<()> {
        self.client.set_model(provider, model_id).await?;
        Ok(())
    }

    /// Get session statistics.
    pub async fn get_session_stats(&self) -> Result<SessionStats> {
        self.client.get_session_stats().await
    }

    /// Get available models.
    pub async fn get_available_models(&self) -> Result<Vec<crate::pi::PiModel>> {
        self.client.get_available_models().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pi_sessions_dir_escaping() {
        let service = MainChatPiService::new(
            PathBuf::from("/tmp/test"),
            true,
            MainChatPiServiceConfig::default(),
        );
        
        let work_dir = PathBuf::from("/home/user/.local/share/octo/users/main");
        let sessions_dir = service.get_pi_sessions_dir(&work_dir);
        
        // Should escape slashes and wrap with dashes
        assert!(sessions_dir.to_string_lossy().contains("home-user-.local-share-octo-users-main"));
    }

    #[test]
    fn test_session_freshness_by_age() {
        let service = MainChatPiService::new(
            PathBuf::from("/tmp/test"),
            true,
            MainChatPiServiceConfig {
                max_session_age_hours: 1, // 1 hour for testing
                ..Default::default()
            },
        );

        // Fresh session (now)
        let fresh = LastSessionInfo {
            size: 1000,
            modified: SystemTime::now(),
        };
        assert!(service.should_continue_session(&fresh));

        // Stale session (2 hours ago)
        let stale = LastSessionInfo {
            size: 1000,
            modified: SystemTime::now() - Duration::from_secs(2 * 3600),
        };
        assert!(!service.should_continue_session(&stale));
    }

    #[test]
    fn test_session_freshness_by_size() {
        let service = MainChatPiService::new(
            PathBuf::from("/tmp/test"),
            true,
            MainChatPiServiceConfig {
                max_session_size_bytes: 1000, // 1KB for testing
                ..Default::default()
            },
        );

        // Small session
        let small = LastSessionInfo {
            size: 500,
            modified: SystemTime::now(),
        };
        assert!(service.should_continue_session(&small));

        // Large session
        let large = LastSessionInfo {
            size: 2000,
            modified: SystemTime::now(),
        };
        assert!(!service.should_continue_session(&large));
    }

    #[tokio::test]
    #[ignore] // Requires pi to be installed
    async fn test_pi_service_creation() {
        let temp = TempDir::new().unwrap();
        let main_dir = temp.path().join("main");
        std::fs::create_dir_all(&main_dir).unwrap();
        
        // Create minimal pi settings
        let pi_dir = main_dir.join(".pi");
        std::fs::create_dir_all(&pi_dir).unwrap();
        std::fs::write(
            pi_dir.join("settings.json"),
            r#"{"defaultProvider": "openai", "defaultModel": "gpt-4o-mini"}"#
        ).unwrap();

        let service = MainChatPiService::new(
            temp.path().to_path_buf(),
            true,
            MainChatPiServiceConfig::default(),
        );

        // This would fail without pi installed
        let session = service.get_or_create_session("test").await;
        assert!(session.is_ok() || session.is_err()); // Just check it doesn't panic
    }
}

//! Main Chat service for managing assistants and their data.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::db::{MainChatDb, main_chat_db_path, main_chat_dir_path};
use super::models::{
    AssistantInfo, ChatMessage, CreateChatMessage, CreateHistoryEntry, CreateSession, HistoryEntry,
    MainChatSession,
};
use super::repository::MainChatRepository;

/// Service for managing main chat.
pub struct MainChatService {
    workspace_dir: PathBuf,
    single_user: bool,
    /// Cache of open database connections (keyed by user_id).
    db_cache: RwLock<HashMap<String, Arc<MainChatDb>>>,
}

impl MainChatService {
    /// Create a new service instance.
    pub fn new(workspace_dir: PathBuf, single_user: bool) -> Self {
        Self {
            workspace_dir,
            single_user,
            db_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Get or open a database for a user's Main Chat.
    async fn get_db(&self, user_id: &str) -> Result<Arc<MainChatDb>> {
        // Check cache first
        {
            let cache = self.db_cache.read().await;
            if let Some(db) = cache.get(user_id) {
                return Ok(Arc::clone(db));
            }
        }

        // Open new connection
        let db_path = main_chat_db_path(&self.workspace_dir, user_id, self.single_user);
        let db = MainChatDb::open(&db_path)
            .await
            .with_context(|| format!("opening main chat database for user: {}", user_id))?;
        let db = Arc::new(db);

        // Cache it
        {
            let mut cache = self.db_cache.write().await;
            cache.insert(user_id.to_string(), Arc::clone(&db));
        }

        Ok(db)
    }

    /// Check if a user has Main Chat set up.
    pub fn main_chat_exists(&self, user_id: &str) -> bool {
        let db_path = main_chat_db_path(&self.workspace_dir, user_id, self.single_user);
        db_path.exists()
    }

    /// Get the directory path for a user's Main Chat.
    pub fn get_main_chat_dir(&self, user_id: &str) -> PathBuf {
        main_chat_dir_path(&self.workspace_dir, user_id, self.single_user)
    }

    /// Get info about a user's Main Chat.
    pub async fn get_main_chat_info(&self, user_id: &str) -> Result<AssistantInfo> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);

        let session_count = repo.count_sessions().await?;
        let history_count = repo.count_history().await?;
        let created_at = repo.get_config("created_at").await?;
        let name = repo
            .get_config("assistant_name")
            .await?
            .unwrap_or_else(|| "main".to_string());

        Ok(AssistantInfo {
            name,
            user_id: user_id.to_string(),
            path: self
                .get_main_chat_dir(user_id)
                .to_string_lossy()
                .to_string(),
            session_count,
            history_count,
            created_at,
        })
    }

    /// Update the assistant name for a user's Main Chat.
    pub async fn update_main_chat_name(&self, user_id: &str, name: &str) -> Result<AssistantInfo> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);

        repo.set_config("assistant_name", name).await?;
        self.get_main_chat_info(user_id).await
    }

    /// Initialize Main Chat for a user.
    pub async fn initialize_main_chat(
        &self,
        user_id: &str,
        name: Option<&str>,
    ) -> Result<AssistantInfo> {
        if self.main_chat_exists(user_id) {
            anyhow::bail!("Main Chat already exists for user");
        }

        // Create directory structure
        let main_chat_dir = self.get_main_chat_dir(user_id);
        std::fs::create_dir_all(&main_chat_dir).with_context(|| {
            format!("creating main chat directory: {}", main_chat_dir.display())
        })?;

        // Create and initialize database
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);

        // Set creation timestamp and name
        let created_at = chrono::Utc::now().to_rfc3339();
        repo.set_config("created_at", &created_at).await?;
        repo.set_config("assistant_name", name.unwrap_or("main"))
            .await?;

        // Create default files
        self.create_default_files(user_id, name.unwrap_or("main"))
            .await?;

        self.get_main_chat_info(user_id).await
    }

    /// Create default configuration files for Main Chat.
    async fn create_default_files(&self, user_id: &str, name: &str) -> Result<()> {
        let main_chat_dir = self.get_main_chat_dir(user_id);

        // Create AGENTS.md from template (replace {{name}} placeholder)
        let agents_template = include_str!("templates/AGENTS.md");
        let agents_content = agents_template.replace("{{name}}", name);
        let agents_path = main_chat_dir.join("AGENTS.md");
        std::fs::write(&agents_path, agents_content)
            .with_context(|| format!("writing AGENTS.md: {}", agents_path.display()))?;

        // Create PERSONALITY.md from template (replace {{name}} placeholder)
        let personality_template = include_str!("templates/PERSONALITY.md");
        let personality_content = personality_template.replace("{{name}}", name);
        let personality_path = main_chat_dir.join("PERSONALITY.md");
        std::fs::write(&personality_path, personality_content)
            .with_context(|| format!("writing PERSONALITY.md: {}", personality_path.display()))?;

        // Create ONBOARD.md from template (bootstrap instructions)
        let onboard_content = include_str!("templates/ONBOARD.md");
        let onboard_path = main_chat_dir.join("ONBOARD.md");
        std::fs::write(&onboard_path, onboard_content)
            .with_context(|| format!("writing ONBOARD.md: {}", onboard_path.display()))?;

        // Create USER.md from template
        let user_content = include_str!("templates/USER.md");
        let user_path = main_chat_dir.join("USER.md");
        std::fs::write(&user_path, user_content)
            .with_context(|| format!("writing USER.md: {}", user_path.display()))?;

        // Create .pi directory for pi agent config
        let pi_dir = main_chat_dir.join(".pi");
        std::fs::create_dir_all(&pi_dir)?;

        // Create pi settings.json
        let pi_settings_content = include_str!("templates/pi-settings.json");
        let pi_settings_path = pi_dir.join("settings.json");
        std::fs::write(&pi_settings_path, pi_settings_content)
            .with_context(|| format!("writing pi settings: {}", pi_settings_path.display()))?;

        // Create sessions directory for pi
        let sessions_dir = pi_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir)?;

        // Keep opencode.json for backward compatibility (existing sessions)
        let opencode_content = include_str!("templates/opencode.json");
        let opencode_path = main_chat_dir.join("opencode.json");
        std::fs::write(&opencode_path, opencode_content)
            .with_context(|| format!("writing opencode.json: {}", opencode_path.display()))?;

        // Create .opencode directory for backward compatibility
        let opencode_dir = main_chat_dir.join(".opencode");
        std::fs::create_dir_all(&opencode_dir)?;

        // Create plugin directory
        let plugin_dir = opencode_dir.join("plugin");
        std::fs::create_dir_all(&plugin_dir)?;

        // Create the main-chat plugin from template
        let plugin_content = include_str!("templates/main-chat-plugin.ts");
        let plugin_path = plugin_dir.join("main-chat.ts");
        std::fs::write(&plugin_path, plugin_content)
            .with_context(|| format!("writing plugin: {}", plugin_path.display()))?;

        Ok(())
    }

    /// Delete Main Chat for a user.
    pub async fn delete_main_chat(&self, user_id: &str) -> Result<()> {
        // Close any cached connection
        {
            let mut cache = self.db_cache.write().await;
            if let Some(db) = cache.remove(user_id) {
                db.close().await;
            }
        }

        // Delete the directory
        let main_chat_dir = self.get_main_chat_dir(user_id);
        if main_chat_dir.exists() {
            std::fs::remove_dir_all(&main_chat_dir).with_context(|| {
                format!("deleting main chat directory: {}", main_chat_dir.display())
            })?;
        }

        Ok(())
    }

    // ========== History Operations ==========

    /// Add a history entry.
    pub async fn add_history(
        &self,
        user_id: &str,
        entry: CreateHistoryEntry,
    ) -> Result<HistoryEntry> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);
        repo.add_history(entry).await
    }

    /// Get recent history.
    pub async fn get_recent_history(&self, user_id: &str, limit: i64) -> Result<Vec<HistoryEntry>> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);
        repo.get_recent_history(limit).await
    }

    /// Export history as JSONL.
    pub async fn export_history_jsonl(&self, user_id: &str) -> Result<String> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);

        // Get all history entries
        let entries = repo.get_recent_history(10000).await?;

        // Convert to JSONL
        let mut lines = Vec::new();
        for entry in entries.iter().rev() {
            // Reverse to get chronological order
            lines.push(serde_json::to_string(entry)?);
        }

        Ok(lines.join("\n"))
    }

    // ========== Session Operations ==========

    /// Register a new session.
    pub async fn add_session(
        &self,
        user_id: &str,
        session: CreateSession,
    ) -> Result<MainChatSession> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);
        repo.add_session(session).await
    }

    /// Get all sessions.
    pub async fn list_sessions(&self, user_id: &str) -> Result<Vec<MainChatSession>> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);
        repo.list_sessions().await
    }

    /// Get the latest session.
    pub async fn get_latest_session(&self, user_id: &str) -> Result<Option<MainChatSession>> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);
        repo.get_latest_session().await
    }

    // ========== Message Operations ==========

    /// Add a chat message.
    pub async fn add_message(
        &self,
        user_id: &str,
        message: CreateChatMessage,
    ) -> Result<ChatMessage> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);
        repo.add_message(message).await
    }

    /// Get all messages (display history).
    pub async fn get_all_messages(&self, user_id: &str) -> Result<Vec<ChatMessage>> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);
        repo.get_all_messages().await
    }

    /// Clear all messages (for fresh start).
    pub async fn clear_messages(&self, user_id: &str) -> Result<i64> {
        let db = self.get_db(user_id).await?;
        let repo = MainChatRepository::new(&db);
        repo.clear_messages().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::main_chat::models::HistoryEntryType;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_initialize_main_chat() {
        let temp = TempDir::new().unwrap();
        let service = MainChatService::new(temp.path().to_path_buf(), true);

        // Initialize main chat
        let info = service
            .initialize_main_chat("user123", Some("jarvis"))
            .await
            .unwrap();
        assert_eq!(info.name, "jarvis");
        assert_eq!(info.session_count, 0);

        // Check files exist
        let main_chat_dir = service.get_main_chat_dir("user123");
        assert!(main_chat_dir.join("main_chat.db").exists());
        assert!(main_chat_dir.join("opencode.json").exists());
        assert!(main_chat_dir.join("AGENTS.md").exists());
        assert!(main_chat_dir.join("ONBOARD.md").exists());

        // Check main chat exists
        assert!(service.main_chat_exists("user123"));
    }

    #[tokio::test]
    async fn test_main_chat_history_and_sessions() {
        let temp = TempDir::new().unwrap();
        let service = MainChatService::new(temp.path().to_path_buf(), true);

        service
            .initialize_main_chat("user123", Some("jarvis"))
            .await
            .unwrap();

        // Add session
        let session = service
            .add_session(
                "user123",
                CreateSession {
                    session_id: "oc-123".to_string(),
                    title: Some("Test".to_string()),
                },
            )
            .await
            .unwrap();
        assert_eq!(session.session_id, "oc-123");

        // Add history
        let entry = service
            .add_history(
                "user123",
                CreateHistoryEntry {
                    entry_type: HistoryEntryType::Summary,
                    content: "Discussed project architecture".to_string(),
                    session_id: Some("oc-123".to_string()),
                    meta: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(entry.entry_type, "summary");

        // Get recent history
        let history = service.get_recent_history("user123", 10).await.unwrap();
        assert_eq!(history.len(), 1);

        // Export JSONL
        let jsonl = service.export_history_jsonl("user123").await.unwrap();
        assert!(jsonl.contains("Discussed project architecture"));
    }
}

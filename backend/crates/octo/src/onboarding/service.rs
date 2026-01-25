//! Onboarding service for managing user onboarding state.

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tracing::{debug, instrument};

use super::models::{OnboardingState, UnlockComponentRequest, UpdateOnboardingRequest};

/// Service for managing onboarding state.
///
/// Onboarding state is stored as JSON in the user's `settings` field,
/// under the `onboarding` key. This allows us to avoid schema changes
/// while keeping onboarding data with the user record.
#[derive(Debug, Clone)]
pub struct OnboardingService {
    pool: SqlitePool,
}

/// User settings wrapper that includes onboarding.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct UserSettings {
    #[serde(default)]
    onboarding: OnboardingState,
    #[serde(flatten)]
    other: serde_json::Value,
}

impl OnboardingService {
    /// Create a new onboarding service.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get onboarding state for a user.
    ///
    /// If the user has no onboarding state, returns a fresh state.
    #[instrument(skip(self))]
    pub async fn get(&self, user_id: &str) -> Result<OnboardingState> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT settings FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .context("fetching user settings")?;

        let Some((settings_json,)) = row else {
            // User doesn't exist - return fresh state
            debug!(
                "User {} not found, returning fresh onboarding state",
                user_id
            );
            return Ok(OnboardingState::new());
        };

        let settings: UserSettings = settings_json
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| serde_json::from_str(s).unwrap_or_default())
            .unwrap_or_default();

        Ok(settings.onboarding)
    }

    /// Update onboarding state for a user.
    #[instrument(skip(self, request))]
    pub async fn update(
        &self,
        user_id: &str,
        request: UpdateOnboardingRequest,
    ) -> Result<OnboardingState> {
        let mut state = self.get(user_id).await?;

        if let Some(stage) = request.stage {
            state.stage = stage;
        }

        if let Some(language) = request.language {
            state.language = Some(language);
        }

        if let Some(languages) = request.languages {
            state.languages = languages;
        }

        if let Some(user_level) = request.user_level {
            state.user_level = user_level;
        }

        if let Some(tutorial_step) = request.tutorial_step {
            state.tutorial_step = tutorial_step;
        }

        if request.complete == Some(true) {
            state.complete();
        }

        self.save(user_id, &state).await?;
        Ok(state)
    }

    /// Unlock a UI component for a user.
    #[instrument(skip(self))]
    pub async fn unlock_component(
        &self,
        user_id: &str,
        request: UnlockComponentRequest,
    ) -> Result<OnboardingState> {
        let mut state = self.get(user_id).await?;

        if !state.unlocked.unlock(&request.component) {
            anyhow::bail!("Unknown component: {}", request.component);
        }

        self.save(user_id, &state).await?;
        Ok(state)
    }

    /// Activate godmode for a user (skip onboarding).
    #[instrument(skip(self))]
    pub async fn godmode(&self, user_id: &str) -> Result<OnboardingState> {
        let state = OnboardingState::godmode();
        self.save(user_id, &state).await?;
        Ok(state)
    }

    /// Complete onboarding for a user.
    #[instrument(skip(self))]
    pub async fn complete(&self, user_id: &str) -> Result<OnboardingState> {
        let mut state = self.get(user_id).await?;
        state.complete();
        self.save(user_id, &state).await?;
        Ok(state)
    }

    /// Reset onboarding state for a user.
    #[instrument(skip(self))]
    pub async fn reset(&self, user_id: &str) -> Result<OnboardingState> {
        let state = OnboardingState::new();
        self.save(user_id, &state).await?;
        Ok(state)
    }

    /// Advance to the next onboarding stage.
    #[instrument(skip(self))]
    pub async fn advance_stage(&self, user_id: &str) -> Result<OnboardingState> {
        let mut state = self.get(user_id).await?;
        state.advance_stage();
        self.save(user_id, &state).await?;
        Ok(state)
    }

    /// Save onboarding state to the user's settings.
    async fn save(&self, user_id: &str, state: &OnboardingState) -> Result<()> {
        // First, get existing settings
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT settings FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .context("fetching user settings for save")?;

        let Some(_) = row else {
            anyhow::bail!("User {} not found", user_id);
        };

        // Parse existing settings or create new
        let existing_json = row.and_then(|r| r.0).unwrap_or_else(|| "{}".to_string());
        let mut settings: serde_json::Value =
            serde_json::from_str(&existing_json).unwrap_or_else(|_| serde_json::json!({}));

        // Update the onboarding field
        settings["onboarding"] =
            serde_json::to_value(state).context("serializing onboarding state")?;

        let settings_json = serde_json::to_string(&settings).context("serializing settings")?;

        sqlx::query("UPDATE users SET settings = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(&settings_json)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .context("updating user settings")?;

        debug!("Saved onboarding state for user {}", user_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onboarding::{OnboardingStage, UserLevel};

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        sqlx::query(
            r#"
            CREATE TABLE users (
                id TEXT PRIMARY KEY NOT NULL,
                username TEXT UNIQUE NOT NULL,
                email TEXT UNIQUE NOT NULL,
                display_name TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'user',
                is_active BOOLEAN NOT NULL DEFAULT TRUE,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                settings TEXT DEFAULT '{}'
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert test user
        sqlx::query("INSERT INTO users (id, username, email, display_name) VALUES (?, ?, ?, ?)")
            .bind("test-user")
            .bind("testuser")
            .bind("test@example.com")
            .bind("Test User")
            .execute(&pool)
            .await
            .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_get_fresh_state() {
        let pool = setup_test_db().await;
        let service = OnboardingService::new(pool);

        let state = service.get("test-user").await.unwrap();
        assert_eq!(state.stage, OnboardingStage::Language);
        assert!(!state.completed);
        assert!(state.needs_onboarding());
    }

    #[tokio::test]
    async fn test_update_state() {
        let pool = setup_test_db().await;
        let service = OnboardingService::new(pool);

        let state = service
            .update(
                "test-user",
                UpdateOnboardingRequest {
                    stage: Some(OnboardingStage::Provider),
                    language: Some("de".to_string()),
                    languages: None,
                    user_level: None,
                    tutorial_step: None,
                    complete: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(state.stage, OnboardingStage::Provider);
        assert_eq!(state.language, Some("de".to_string()));

        // Verify persistence
        let loaded = service.get("test-user").await.unwrap();
        assert_eq!(loaded.stage, OnboardingStage::Provider);
        assert_eq!(loaded.language, Some("de".to_string()));
    }

    #[tokio::test]
    async fn test_godmode() {
        let pool = setup_test_db().await;
        let service = OnboardingService::new(pool);

        let state = service.godmode("test-user").await.unwrap();
        assert!(state.completed);
        assert!(state.godmode);
        assert!(!state.needs_onboarding());
        assert_eq!(state.user_level, UserLevel::Technical);

        // Verify persistence
        let loaded = service.get("test-user").await.unwrap();
        assert!(loaded.godmode);
    }

    #[tokio::test]
    async fn test_unlock_component() {
        let pool = setup_test_db().await;
        let service = OnboardingService::new(pool);

        let state = service
            .unlock_component(
                "test-user",
                UnlockComponentRequest {
                    component: "terminal".to_string(),
                },
            )
            .await
            .unwrap();

        assert!(state.unlocked.terminal);

        // Verify persistence
        let loaded = service.get("test-user").await.unwrap();
        assert!(loaded.unlocked.terminal);
    }

    #[tokio::test]
    async fn test_advance_stage() {
        let pool = setup_test_db().await;
        let service = OnboardingService::new(pool);

        let state = service.advance_stage("test-user").await.unwrap();
        assert_eq!(state.stage, OnboardingStage::Provider);

        let state = service.advance_stage("test-user").await.unwrap();
        assert_eq!(state.stage, OnboardingStage::Profile);
    }

    #[tokio::test]
    async fn test_complete() {
        let pool = setup_test_db().await;
        let service = OnboardingService::new(pool);

        let state = service.complete("test-user").await.unwrap();
        assert!(state.completed);
        assert_eq!(state.stage, OnboardingStage::Complete);
        assert!(!state.needs_onboarding());
    }

    #[tokio::test]
    async fn test_reset() {
        let pool = setup_test_db().await;
        let service = OnboardingService::new(pool);

        // First complete
        service.complete("test-user").await.unwrap();

        // Then reset
        let state = service.reset("test-user").await.unwrap();
        assert!(!state.completed);
        assert_eq!(state.stage, OnboardingStage::Language);
        assert!(state.needs_onboarding());
    }
}

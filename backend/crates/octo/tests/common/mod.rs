//! Test utilities and common setup.
#![allow(clippy::field_reassign_with_default)]

use anyhow::Result;
use async_trait::async_trait;
use axum::Router;
use octo::agent::{AgentRepository, AgentService, ScaffoldConfig};
use octo::agent_rpc::{
    AgentBackend, AgentEventStream, Conversation, HealthStatus, Message, SendMessageRequest,
    SessionHandle, StartSessionOpts,
};
use octo::api;
use octo::auth::{AuthConfig, AuthState, DevUser, Role};
use octo::container::ContainerRuntime;
use octo::db::Database;
use octo::invite::InviteCodeRepository;
use octo::local::LocalRuntimeConfig;
use octo::session::{RuntimeMode, SessionRepository, SessionService, SessionServiceConfig};
use octo::settings::SettingsService;
use octo::user::{UserRepository, UserService};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

fn make_dev_user(id: &str, name: &str, email: &str, password: &str, role: Role) -> DevUser {
    let password_hash =
        bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("Failed to hash password");

    DevUser {
        id: id.to_string(),
        name: name.to_string(),
        email: email.to_string(),
        password_hash,
        role,
    }
}

/// Create a test AuthConfig with a JWT secret for testing.
fn test_auth_config() -> AuthConfig {
    let mut config = AuthConfig::default();
    config.dev_mode = true;
    config.dev_users = vec![
        make_dev_user(
            "dev",
            "Developer",
            "dev@localhost",
            "devpassword123",
            Role::Admin,
        ),
        make_dev_user(
            "user",
            "Test User",
            "user@localhost",
            "userpassword123",
            Role::User,
        ),
    ];
    // Set a JWT secret for tests (required for token generation)
    config.jwt_secret = Some("test-secret-for-integration-tests-minimum-32-chars".to_string());
    config
}

fn create_pi_settings_services() -> (SettingsService, SettingsService) {
    let base_dir = std::env::temp_dir()
        .join("octo-tests")
        .join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&base_dir).expect("create test settings dir");

    let pi_settings_schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Pi Agent Settings",
        "type": "object",
        "properties": {
            "defaultProvider": { "type": "string" },
            "defaultModel": { "type": "string" },
            "defaultThinkingLevel": { "type": "string" }
        }
    });
    let pi_models_schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Pi Agent Models",
        "type": "object",
        "properties": {
            "providers": { "type": "object" }
        }
    });

    let settings = SettingsService::new_json(
        pi_settings_schema,
        PathBuf::from(&base_dir),
        "settings.json",
    )
    .expect("create pi settings service");
    let models =
        SettingsService::new_json(pi_models_schema, PathBuf::from(&base_dir), "models.json")
            .expect("create pi models service");

    (settings, models)
}

fn test_session_service_config() -> SessionServiceConfig {
    let workspace_root = std::env::temp_dir().join("octo-tests-workspaces");
    std::fs::create_dir_all(&workspace_root).expect("create test workspace dir");

    let local_config = LocalRuntimeConfig {
        workspace_dir: format!("{}/{{user_id}}", workspace_root.display()),
        ..LocalRuntimeConfig::default()
    };

    SessionServiceConfig {
        runtime_mode: RuntimeMode::Local,
        local_config: Some(local_config),
        user_data_path: workspace_root.to_string_lossy().to_string(),
        ..SessionServiceConfig::default()
    }
}

/// Create a test application with all services initialized.
pub async fn test_app() -> Router {
    // Use in-memory database for tests
    let db = Database::in_memory().await.unwrap();

    // Create auth state in dev mode with JWT secret
    let auth_config = test_auth_config();
    let auth_state = AuthState::new(auth_config);

    // Create container runtime (won't actually be used in unit tests)
    let runtime = Arc::new(ContainerRuntime::new());

    // Create session service
    let session_config = test_session_service_config();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, runtime.clone(), session_config);

    // Create agent service
    let agent_repo = AgentRepository::new(db.pool().clone());
    let agent_service = AgentService::with_scaffold_config(
        runtime,
        session_service.clone(),
        agent_repo,
        ScaffoldConfig::default(),
    );

    // Create user service
    let user_repo = UserRepository::new(db.pool().clone());
    let user_service = UserService::new(user_repo);

    // Create invite code repository
    let invite_repo = InviteCodeRepository::new(db.pool().clone());

    // Create app state and router
    let max_proxy_body_bytes = 10 * 1024 * 1024;
    let state = api::AppState::new(
        session_service,
        agent_service,
        user_service,
        invite_repo,
        auth_state,
        api::MmryState::default(),
        api::VoiceState::default(),
        api::SessionUiState::default(),
        api::TemplatesState::default(),
        max_proxy_body_bytes,
    );
    let (pi_settings, pi_models) = create_pi_settings_services();
    let state = state
        .with_settings_pi_agent(pi_settings)
        .with_settings_pi_models(pi_models);
    api::create_router_with_config(state, 100)
}

// ============================================================================
// Mock AgentBackend for testing
// ============================================================================

/// Mock implementation of AgentBackend for testing.
pub struct MockAgentBackend {
    /// Predefined conversations to return
    pub conversations: Vec<Conversation>,
    /// Predefined messages to return
    pub messages: Vec<Message>,
    /// Whether to simulate healthy status
    pub healthy: bool,
}

impl MockAgentBackend {
    /// Create a new mock backend with some default test data.
    pub fn new() -> Self {
        Self {
            conversations: vec![
                Conversation {
                    id: "conv_test1".to_string(),
                    title: Some("Test Conversation 1".to_string()),
                    parent_id: None,
                    workspace_path: "/home/test/project1".to_string(),
                    project_name: "project1".to_string(),
                    created_at: 1700000000000,
                    updated_at: 1700000001000,
                    is_active: false,
                    version: Some("1.0.0".to_string()),
                },
                Conversation {
                    id: "conv_test2".to_string(),
                    title: Some("Test Conversation 2".to_string()),
                    parent_id: None,
                    workspace_path: "/home/test/project2".to_string(),
                    project_name: "project2".to_string(),
                    created_at: 1700000002000,
                    updated_at: 1700000003000,
                    is_active: true,
                    version: Some("1.0.0".to_string()),
                },
            ],
            messages: vec![],
            healthy: true,
        }
    }
}

#[async_trait]
impl AgentBackend for MockAgentBackend {
    async fn list_conversations(&self, _user_id: &str) -> Result<Vec<Conversation>> {
        Ok(self.conversations.clone())
    }

    async fn get_conversation(
        &self,
        _user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<Conversation>> {
        Ok(self
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .cloned())
    }

    async fn get_messages(&self, _user_id: &str, _conversation_id: &str) -> Result<Vec<Message>> {
        Ok(self.messages.clone())
    }

    async fn start_session(
        &self,
        _user_id: &str,
        workdir: &Path,
        _opts: StartSessionOpts,
    ) -> Result<SessionHandle> {
        Ok(SessionHandle {
            session_id: "ses_mock123".to_string(),
            opencode_session_id: Some("ses_mock123".to_string()),
            api_url: "http://localhost:41820".to_string(),
            opencode_port: 41820,
            ttyd_port: 41821,
            fileserver_port: 41822,
            workdir: workdir.to_string_lossy().to_string(),
            is_new: true,
        })
    }

    async fn attach(&self, _user_id: &str, _session_id: &str) -> Result<AgentEventStream> {
        // Return an empty stream for testing
        use futures::stream;
        Ok(Box::pin(stream::empty()))
    }

    async fn send_message(
        &self,
        _user_id: &str,
        _session_id: &str,
        _message: SendMessageRequest,
    ) -> Result<()> {
        Ok(())
    }

    async fn stop_session(&self, _user_id: &str, _session_id: &str) -> Result<()> {
        Ok(())
    }

    async fn health(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            healthy: self.healthy,
            mode: "mock".to_string(),
            version: Some("1.0.0-test".to_string()),
            details: Some("Mock backend for testing".to_string()),
        })
    }

    async fn get_session_url(&self, _user_id: &str, session_id: &str) -> Result<Option<String>> {
        Ok(Some(format!(
            "http://localhost:41820/session/{}",
            session_id
        )))
    }
}

/// Create a test application with AgentBackend enabled.
pub async fn test_app_with_agent_backend() -> Router {
    let db = Database::in_memory().await.unwrap();

    let auth_config = test_auth_config();
    let auth_state = AuthState::new(auth_config);

    let runtime = Arc::new(ContainerRuntime::new());
    let session_config = test_session_service_config();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, runtime.clone(), session_config);

    let agent_repo = AgentRepository::new(db.pool().clone());
    let agent_service = AgentService::with_scaffold_config(
        runtime,
        session_service.clone(),
        agent_repo,
        ScaffoldConfig::default(),
    );

    let user_repo = UserRepository::new(db.pool().clone());
    let user_service = UserService::new(user_repo);

    let invite_repo = InviteCodeRepository::new(db.pool().clone());

    let mock_backend = Arc::new(MockAgentBackend::new());

    let max_proxy_body_bytes = 10 * 1024 * 1024;
    let state = api::AppState::with_agent_backend(
        session_service,
        agent_service,
        user_service,
        invite_repo,
        auth_state,
        mock_backend,
        api::MmryState::default(),
        api::VoiceState::default(),
        api::SessionUiState::default(),
        api::TemplatesState::default(),
        max_proxy_body_bytes,
    );
    let (pi_settings, pi_models) = create_pi_settings_services();
    let state = state
        .with_settings_pi_agent(pi_settings)
        .with_settings_pi_models(pi_models);
    api::create_router_with_config(state, 100)
}

/// Create a test application with AgentBackend and return a valid token.
/// Create a test application and return a valid token for the admin dev user.
pub async fn test_app_with_token() -> (Router, String) {
    let db = Database::in_memory().await.unwrap();

    let auth_config = test_auth_config();
    let auth_state = AuthState::new(auth_config);

    // Generate token for dev user
    let token = auth_state
        .generate_dev_token(&auth_state.dev_users()[0])
        .unwrap();

    let runtime = Arc::new(ContainerRuntime::new());
    let session_config = test_session_service_config();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, runtime.clone(), session_config);

    // Create agent service
    let agent_repo = AgentRepository::new(db.pool().clone());
    let agent_service = AgentService::with_scaffold_config(
        runtime,
        session_service.clone(),
        agent_repo,
        ScaffoldConfig::default(),
    );

    // Create user service
    let user_repo = UserRepository::new(db.pool().clone());
    let user_service = UserService::new(user_repo);

    // Create invite code repository
    let invite_repo = InviteCodeRepository::new(db.pool().clone());

    let max_proxy_body_bytes = 10 * 1024 * 1024;
    let state = api::AppState::new(
        session_service,
        agent_service,
        user_service,
        invite_repo,
        auth_state,
        api::MmryState::default(),
        api::VoiceState::default(),
        api::SessionUiState::default(),
        api::TemplatesState::default(),
        max_proxy_body_bytes,
    );
    let (pi_settings, pi_models) = create_pi_settings_services();
    let state = state
        .with_settings_pi_agent(pi_settings)
        .with_settings_pi_models(pi_models);
    (api::create_router_with_config(state, 100), token)
}

/// Create a test application and return a valid token for a regular user.
pub async fn test_app_with_user_token() -> (Router, String) {
    let db = Database::in_memory().await.unwrap();

    let auth_config = test_auth_config();
    let auth_state = AuthState::new(auth_config);

    // Generate token for regular user (second dev user)
    let token = auth_state
        .generate_dev_token(&auth_state.dev_users()[1])
        .unwrap();

    let runtime = Arc::new(ContainerRuntime::new());
    let session_config = test_session_service_config();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, runtime.clone(), session_config);

    // Create agent service
    let agent_repo = AgentRepository::new(db.pool().clone());
    let agent_service = AgentService::with_scaffold_config(
        runtime,
        session_service.clone(),
        agent_repo,
        ScaffoldConfig::default(),
    );

    // Create user service
    let user_repo = UserRepository::new(db.pool().clone());
    let user_service = UserService::new(user_repo);

    // Create invite code repository
    let invite_repo = InviteCodeRepository::new(db.pool().clone());

    let max_proxy_body_bytes = 10 * 1024 * 1024;
    let state = api::AppState::new(
        session_service,
        agent_service,
        user_service,
        invite_repo,
        auth_state,
        api::MmryState::default(),
        api::VoiceState::default(),
        api::SessionUiState::default(),
        api::TemplatesState::default(),
        max_proxy_body_bytes,
    );
    let (pi_settings, pi_models) = create_pi_settings_services();
    let state = state
        .with_settings_pi_agent(pi_settings)
        .with_settings_pi_models(pi_models);
    (api::create_router_with_config(state, 100), token)
}

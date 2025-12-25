//! Test utilities and common setup.

use axum::Router;
use std::sync::Arc;
use octo::agent::{AgentRepository, AgentService};
use octo::api;
use octo::auth::{AuthConfig, AuthState};
use octo::container::ContainerRuntime;
use octo::db::Database;
use octo::invite::InviteCodeRepository;
use octo::session::{SessionRepository, SessionService, SessionServiceConfig};
use octo::user::{UserRepository, UserService};

/// Create a test AuthConfig with a JWT secret for testing.
fn test_auth_config() -> AuthConfig {
    let mut config = AuthConfig::default();
    // Set a JWT secret for tests (required for token generation)
    config.jwt_secret = Some("test-secret-for-integration-tests-minimum-32-chars".to_string());
    config
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
    let session_config = SessionServiceConfig::default();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, runtime.clone(), session_config);

    // Create agent service
    let agent_repo = AgentRepository::new(db.pool().clone());
    let agent_service = AgentService::new(runtime, session_service.clone(), agent_repo);

    // Create user service
    let user_repo = UserRepository::new(db.pool().clone());
    let user_service = UserService::new(user_repo);

    // Create invite code repository
    let invite_repo = InviteCodeRepository::new(db.pool().clone());

    // Create app state and router
    let state = api::AppState::new(session_service, agent_service, user_service, invite_repo, auth_state);
    api::create_router(state)
}

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
    let session_config = SessionServiceConfig::default();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, runtime.clone(), session_config);

    // Create agent service
    let agent_repo = AgentRepository::new(db.pool().clone());
    let agent_service = AgentService::new(runtime, session_service.clone(), agent_repo);

    // Create user service
    let user_repo = UserRepository::new(db.pool().clone());
    let user_service = UserService::new(user_repo);

    // Create invite code repository
    let invite_repo = InviteCodeRepository::new(db.pool().clone());

    let state = api::AppState::new(session_service, agent_service, user_service, invite_repo, auth_state);
    (api::create_router(state), token)
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
    let session_config = SessionServiceConfig::default();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, runtime.clone(), session_config);

    // Create agent service
    let agent_repo = AgentRepository::new(db.pool().clone());
    let agent_service = AgentService::new(runtime, session_service.clone(), agent_repo);

    // Create user service
    let user_repo = UserRepository::new(db.pool().clone());
    let user_service = UserService::new(user_repo);

    // Create invite code repository
    let invite_repo = InviteCodeRepository::new(db.pool().clone());

    let state = api::AppState::new(session_service, agent_service, user_service, invite_repo, auth_state);
    (api::create_router(state), token)
}

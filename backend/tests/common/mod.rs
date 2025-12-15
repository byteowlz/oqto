//! Test utilities and common setup.

use axum::Router;
use workspace_backend::api;
use workspace_backend::auth::{AuthConfig, AuthState};
use workspace_backend::db::Database;
use workspace_backend::podman::Podman;
use workspace_backend::session::{SessionRepository, SessionService, SessionServiceConfig};

/// Create a test application with all services initialized.
pub async fn test_app() -> Router {
    // Use in-memory database for tests
    let db = Database::in_memory().await.unwrap();
    
    // Create auth state in dev mode
    let auth_config = AuthConfig::default();
    let auth_state = AuthState::new(auth_config);
    
    // Create podman client (won't actually be used in unit tests)
    let podman = Podman::new();
    
    // Create session service
    let session_config = SessionServiceConfig::default();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, podman, session_config);
    
    // Create app state and router
    let state = api::AppState::new(session_service, auth_state);
    api::create_router(state)
}

/// Create a test application and return a valid token for the admin dev user.
pub async fn test_app_with_token() -> (Router, String) {
    let db = Database::in_memory().await.unwrap();
    
    let auth_config = AuthConfig::default();
    let auth_state = AuthState::new(auth_config);
    
    // Generate token for dev user
    let token = auth_state
        .generate_dev_token(&auth_state.dev_users()[0])
        .unwrap();
    
    let podman = Podman::new();
    let session_config = SessionServiceConfig::default();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, podman, session_config);
    
    let state = api::AppState::new(session_service, auth_state);
    (api::create_router(state), token)
}

/// Create a test application and return a valid token for a regular user.
pub async fn test_app_with_user_token() -> (Router, String) {
    let db = Database::in_memory().await.unwrap();
    
    let auth_config = AuthConfig::default();
    let auth_state = AuthState::new(auth_config);
    
    // Generate token for regular user (second dev user)
    let token = auth_state
        .generate_dev_token(&auth_state.dev_users()[1])
        .unwrap();
    
    let podman = Podman::new();
    let session_config = SessionServiceConfig::default();
    let session_repo = SessionRepository::new(db.pool().clone());
    let session_service = SessionService::new(session_repo, podman, session_config);
    
    let state = api::AppState::new(session_service, auth_state);
    (api::create_router(state), token)
}

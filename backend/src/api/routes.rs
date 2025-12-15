//! API route definitions.

use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::auth::auth_middleware;

use super::handlers;
use super::proxy;
use super::state::AppState;

/// Create the application router.
pub fn create_router(state: AppState) -> Router {
    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    
    // Clone auth state for middleware
    let auth_state = state.auth.clone();

    // Protected routes (require authentication)
    let protected_routes = Router::new()
        // Session management
        .route("/sessions", get(handlers::list_sessions))
        .route("/sessions", post(handlers::create_session))
        .route("/sessions/{session_id}", get(handlers::get_session))
        .route("/sessions/{session_id}", delete(handlers::delete_session))
        .route("/sessions/{session_id}/stop", post(handlers::stop_session))
        // Proxy routes
        .route(
            "/sessions/{session_id}/opencode/{*path}",
            get(proxy::proxy_opencode)
                .post(proxy::proxy_opencode)
                .put(proxy::proxy_opencode)
                .delete(proxy::proxy_opencode),
        )
        .route(
            "/sessions/{session_id}/files/{*path}",
            get(proxy::proxy_fileserver),
        )
        .route(
            "/sessions/{session_id}/terminal",
            get(proxy::proxy_terminal_ws),
        )
        // Admin routes
        .route("/admin/sessions", get(handlers::admin_list_sessions))
        .route("/admin/sessions/{session_id}", delete(handlers::admin_force_stop_session))
        .layer(middleware::from_fn_with_state(auth_state.clone(), auth_middleware))
        .with_state(state.clone());

    // Public routes (no authentication)
    let public_routes = Router::new()
        .route("/health", get(handlers::health))
        .route("/auth/login", post(handlers::dev_login))
        .with_state(state);

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

//! API route definitions.

use axum::http::{HeaderValue, Method, header};
use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::auth::auth_middleware;

use super::handlers;
use super::proxy;
use super::state::AppState;

/// Create the application router.
pub fn create_router(state: AppState) -> Router {
    // CORS configuration - use specific origins from config
    let cors = build_cors_layer(&state);

    // Tracing layer with request IDs and timing
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

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
        .route(
            "/sessions/{session_id}/update",
            get(handlers::check_session_update),
        )
        .route(
            "/sessions/{session_id}/upgrade",
            post(handlers::upgrade_session),
        )
        .route("/sessions/updates", get(handlers::check_all_updates))
        // Opencode events (legacy global endpoint)
        .route("/opencode/event", get(proxy::opencode_events))
        // SSE events proxy for specific session
        .route(
            "/session/{session_id}/code/event",
            get(proxy::proxy_opencode_events),
        )
        // Proxy routes
        .route(
            "/sessions/{session_id}/opencode/{*path}",
            get(proxy::proxy_opencode)
                .post(proxy::proxy_opencode)
                .put(proxy::proxy_opencode)
                .delete(proxy::proxy_opencode),
        )
        // PRD-compatible proxy routes
        .route(
            "/session/{session_id}/code/{*path}",
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
            "/session/{session_id}/files/{*path}",
            get(proxy::proxy_fileserver),
        )
        .route(
            "/sessions/{session_id}/terminal",
            get(proxy::proxy_terminal_ws),
        )
        .route("/session/{session_id}/term", get(proxy::proxy_terminal_ws))
        // User profile routes (authenticated users)
        .route("/me", get(handlers::get_me))
        .route("/me", put(handlers::update_me))
        // Admin routes - sessions
        .route("/admin/sessions", get(handlers::admin_list_sessions))
        .route(
            "/admin/sessions/{session_id}",
            delete(handlers::admin_force_stop_session),
        )
        // Admin routes - user management
        .route("/admin/users", get(handlers::list_users))
        .route("/admin/users", post(handlers::create_user))
        .route("/admin/users/stats", get(handlers::get_user_stats))
        .route("/admin/users/{user_id}", get(handlers::get_user))
        .route("/admin/users/{user_id}", put(handlers::update_user))
        .route("/admin/users/{user_id}", delete(handlers::delete_user))
        .route(
            "/admin/users/{user_id}/deactivate",
            post(handlers::deactivate_user),
        )
        .route(
            "/admin/users/{user_id}/activate",
            post(handlers::activate_user),
        )
        // Admin routes - invite code management
        .route("/admin/invite-codes", get(handlers::list_invite_codes))
        .route("/admin/invite-codes", post(handlers::create_invite_code))
        .route(
            "/admin/invite-codes/batch",
            post(handlers::create_invite_codes_batch),
        )
        .route(
            "/admin/invite-codes/stats",
            get(handlers::get_invite_code_stats),
        )
        .route(
            "/admin/invite-codes/{code_id}",
            get(handlers::get_invite_code),
        )
        .route(
            "/admin/invite-codes/{code_id}",
            delete(handlers::delete_invite_code),
        )
        .route(
            "/admin/invite-codes/{code_id}/revoke",
            post(handlers::revoke_invite_code),
        )
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // Public routes (no authentication)
    let public_routes = Router::new()
        .route("/health", get(handlers::health))
        .route("/auth/login", post(handlers::login))
        .route("/auth/register", post(handlers::register))
        .route("/auth/logout", post(handlers::logout))
        // Keep dev_login for backwards compatibility
        .route("/auth/dev-login", post(handlers::dev_login))
        .with_state(state);

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(cors)
        .layer(trace_layer)
}

/// Build the CORS layer based on configuration.
///
/// In dev mode with no configured origins, allows localhost origins.
/// In production mode, requires explicit origin configuration.
fn build_cors_layer(state: &AppState) -> CorsLayer {
    let allowed_origins = state.auth.allowed_origins();
    let dev_mode = state.auth.is_dev_mode();

    // Define allowed methods
    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::PATCH,
        Method::OPTIONS,
    ];

    // Define allowed headers
    let headers = [
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::ORIGIN,
        header::COOKIE,
    ];

    if allowed_origins.is_empty() {
        if dev_mode {
            // In dev mode with no configured origins, allow common local origins
            tracing::warn!(
                "CORS: No origins configured, using default localhost origins for dev mode"
            );
            CorsLayer::new()
                .allow_origin([
                    "http://localhost:3000".parse::<HeaderValue>().unwrap(),
                    "http://localhost:3001".parse::<HeaderValue>().unwrap(),
                    "http://localhost:8080".parse::<HeaderValue>().unwrap(),
                    "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
                    "http://127.0.0.1:3001".parse::<HeaderValue>().unwrap(),
                    "http://127.0.0.1:8080".parse::<HeaderValue>().unwrap(),
                ])
                .allow_methods(methods)
                .allow_headers(headers)
                .allow_credentials(true)
        } else {
            // In production with no configured origins, deny all cross-origin requests
            tracing::warn!(
                "CORS: No origins configured in production mode, denying all cross-origin requests"
            );
            CorsLayer::new().allow_origin(AllowOrigin::exact(
                HeaderValue::from_static("null"), // This effectively denies all CORS
            ))
        }
    } else {
        // Use configured origins
        let mut origins: Vec<HeaderValue> = allowed_origins
            .iter()
            .filter_map(|origin| {
                origin.parse::<HeaderValue>().ok().or_else(|| {
                    tracing::warn!("CORS: Invalid origin in config: {}", origin);
                    None
                })
            })
            .collect();

        // In dev mode, always allow common localhost origins in addition to configured origins.
        if dev_mode {
            for origin in [
                "http://localhost:3000",
                "http://localhost:3001",
                "http://127.0.0.1:3000",
                "http://127.0.0.1:3001",
            ] {
                if let Ok(value) = origin.parse::<HeaderValue>() {
                    if !origins.contains(&value) {
                        origins.push(value);
                    }
                }
            }
        }

        if origins.is_empty() {
            tracing::error!("CORS: All configured origins are invalid!");
            CorsLayer::new().allow_origin(AllowOrigin::exact(HeaderValue::from_static("null")))
        } else {
            tracing::info!("CORS: Allowing {} origin(s)", origins.len());
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods(methods)
                .allow_headers(headers)
                .allow_credentials(true)
        }
    }
}

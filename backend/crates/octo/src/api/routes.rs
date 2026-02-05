//! API route definitions.

use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, header};
use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::auth::auth_middleware;

use super::a2ui as a2ui_handlers;
use super::delegate as delegate_handlers;
use super::handlers;
use super::main_chat as main_chat_handlers;
use super::main_chat_pi as main_chat_pi_handlers;
use super::onboarding_handlers;
use super::proxy;
use super::state::AppState;
use super::ui_control as ui_control_handlers;
use super::ws_multiplexed;

// Note: handlers module now provides all public handlers via re-exports in handlers/mod.rs
// Routes continue to use `handlers::function_name` - no changes needed

/// Create the application router with configurable max upload size.
pub fn create_router_with_config(state: AppState, max_upload_size_mb: usize) -> Router {
    // CORS configuration - use specific origins from config
    let cors = build_cors_layer(&state);
    let max_body_size = max_upload_size_mb * 1024 * 1024;

    // Tracing layer with request IDs and timing
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    // Clone auth state for middleware
    let auth_state = state.auth.clone();

    // Protected routes (require authentication)
    let protected_routes = Router::new()
        // Multiplexed WebSocket endpoint for Pi, files, terminal, hstry channels
        .route("/ws/mux", get(ws_multiplexed::ws_multiplexed_handler))
        // sldr routes
        .route(
            "/sldr",
            get(proxy::proxy_sldr_root)
                .post(proxy::proxy_sldr_root)
                .put(proxy::proxy_sldr_root)
                .delete(proxy::proxy_sldr_root)
                .patch(proxy::proxy_sldr_root),
        )
        .route(
            "/sldr/{*path}",
            get(proxy::proxy_sldr)
                .post(proxy::proxy_sldr)
                .put(proxy::proxy_sldr)
                .delete(proxy::proxy_sldr)
                .patch(proxy::proxy_sldr),
        )
        // Project management
        .route("/projects", get(handlers::list_workspace_dirs))
        .route("/projects/logo/{*path}", get(handlers::get_project_logo))
        .route(
            "/projects/locations",
            get(handlers::list_workspace_locations).post(handlers::upsert_workspace_location),
        )
        .route(
            "/projects/locations/active",
            post(handlers::set_active_workspace_location),
        )
        .route(
            "/projects/templates",
            get(handlers::list_project_templates).post(handlers::create_project_from_template),
        )
        .route("/feedback", post(handlers::create_feedback))
        // Session management
        .route("/sessions", get(handlers::list_sessions))
        .route("/sessions", post(handlers::create_session))
        .route(
            "/sessions/get-or-create",
            post(handlers::get_or_create_session),
        )
        .route(
            "/sessions/get-or-create-for-workspace",
            post(handlers::get_or_create_session_for_workspace),
        )
        .route("/sessions/{session_id}", get(handlers::get_session))
        .route(
            "/sessions/{session_id}/activity",
            post(handlers::touch_session_activity),
        )
        .route("/sessions/{session_id}", delete(handlers::delete_session))
        .route("/sessions/{session_id}/stop", post(handlers::stop_session))
        .route(
            "/sessions/{session_id}/resume",
            post(handlers::resume_session),
        )
        .route(
            "/sessions/{session_id}/update",
            get(handlers::check_session_update),
        )
        .route(
            "/sessions/{session_id}/upgrade",
            post(handlers::upgrade_session),
        )
        .route("/sessions/updates", get(handlers::check_all_updates))
        // Voice mode WebSocket proxies
        .route("/voice/stt", get(proxy::proxy_voice_stt_ws))
        .route("/voice/tts", get(proxy::proxy_voice_tts_ws))
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
            "/sessions/{session_id}/browser/stream",
            get(proxy::proxy_browser_stream_ws),
        )
        .route(
            "/session/{session_id}/browser/stream",
            get(proxy::proxy_browser_stream_ws),
        )
        // Workspace-based mmry routes (single-user mode)
        .route(
            "/workspace/memories",
            get(proxy::proxy_mmry_list_for_workspace).post(proxy::proxy_mmry_add_for_workspace),
        )
        .route(
            "/workspace/memories/search",
            post(proxy::proxy_mmry_search_for_workspace),
        )
        .route(
            "/workspace/memories/{memory_id}",
            get(proxy::proxy_mmry_memory_for_workspace)
                .put(proxy::proxy_mmry_memory_for_workspace)
                .delete(proxy::proxy_mmry_memory_for_workspace),
        )
        // Sub-agent proxy routes
        .route(
            "/session/{session_id}/agent/{agent_id}/code/event",
            get(proxy::proxy_opencode_agent_events),
        )
        .route(
            "/session/{session_id}/agent/{agent_id}/code/{*path}",
            get(proxy::proxy_opencode_agent)
                .post(proxy::proxy_opencode_agent)
                .put(proxy::proxy_opencode_agent)
                .delete(proxy::proxy_opencode_agent),
        )
        // User profile routes (authenticated users)
        .route("/me", get(handlers::get_me))
        .route("/me", put(handlers::update_me))
        .route("/auth/change-password", post(handlers::change_password))
        // UI control routes (agent-driven UI control)
        .route("/ui/navigate", post(ui_control_handlers::navigate))
        .route("/ui/session", post(ui_control_handlers::session))
        .route("/ui/view", post(ui_control_handlers::view))
        .route("/ui/palette", post(ui_control_handlers::palette))
        .route("/ui/palette/exec", post(ui_control_handlers::palette_exec))
        .route("/ui/spotlight", post(ui_control_handlers::spotlight))
        .route("/ui/tour", post(ui_control_handlers::tour))
        .route("/ui/sidebar", post(ui_control_handlers::sidebar))
        .route("/ui/panel", post(ui_control_handlers::panel))
        .route("/ui/theme", post(ui_control_handlers::theme))
        // Onboarding routes
        .route(
            "/onboarding",
            get(onboarding_handlers::get_onboarding).put(onboarding_handlers::update_onboarding),
        )
        .route(
            "/onboarding/check",
            get(onboarding_handlers::needs_onboarding),
        )
        .route(
            "/onboarding/advance",
            post(onboarding_handlers::advance_stage),
        )
        .route(
            "/onboarding/unlock/{component}",
            post(onboarding_handlers::unlock_component),
        )
        .route("/onboarding/godmode", post(onboarding_handlers::godmode))
        .route(
            "/onboarding/complete",
            post(onboarding_handlers::complete_onboarding),
        )
        .route(
            "/onboarding/reset",
            post(onboarding_handlers::reset_onboarding),
        )
        // Admin routes - sessions
        .route("/admin/sessions", get(handlers::admin_list_sessions))
        .route(
            "/admin/sessions/{session_id}",
            delete(handlers::admin_force_stop_session),
        )
        .route(
            "/admin/local/cleanup",
            post(handlers::admin_cleanup_local_sessions),
        )
        // Admin routes - stats
        .route("/admin/stats", get(handlers::get_admin_stats))
        // Admin routes - user management
        .route("/admin/users", get(handlers::list_users))
        .route("/admin/users", post(handlers::create_user))
        .route("/admin/users/stats", get(handlers::get_user_stats))
        .route("/admin/metrics", get(handlers::admin_metrics_stream))
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
        // Agent management routes
        .route(
            "/session/{session_id}/agents",
            get(handlers::list_agents).post(handlers::start_agent),
        )
        .route(
            "/session/{session_id}/agents/create",
            post(handlers::create_agent),
        )
        .route(
            "/session/{session_id}/agents/exec",
            post(handlers::exec_agent_command),
        )
        .route(
            "/session/{session_id}/agents/{agent_id}",
            get(handlers::get_agent).delete(handlers::stop_agent),
        )
        .route(
            "/session/{session_id}/agents/rediscover",
            post(handlers::rediscover_agents),
        )
        // Chat history routes (reads from disk, no running opencode needed)
        .route("/chat-history", get(handlers::list_chat_history))
        .route(
            "/chat-history/grouped",
            get(handlers::list_chat_history_grouped),
        )
        .route(
            "/chat-history/{session_id}",
            get(handlers::get_chat_session).patch(handlers::update_chat_session),
        )
        .route(
            "/chat-history/{session_id}/messages",
            get(handlers::get_chat_messages),
        )
        // Mmry (memory service) proxy routes
        .route(
            "/session/{session_id}/memories",
            get(proxy::proxy_mmry_list).post(proxy::proxy_mmry_add),
        )
        .route(
            "/session/{session_id}/memories/search",
            post(proxy::proxy_mmry_search),
        )
        .route(
            "/session/{session_id}/memories/stores",
            get(proxy::proxy_mmry_stores),
        )
        .route(
            "/session/{session_id}/memories/{memory_id}",
            get(proxy::proxy_mmry_memory)
                .put(proxy::proxy_mmry_memory)
                .delete(proxy::proxy_mmry_memory),
        )
        // Settings routes
        .route("/settings/schema", get(handlers::get_settings_schema))
        .route(
            "/settings",
            get(handlers::get_settings_values).patch(handlers::update_settings_values),
        )
        .route("/settings/reload", post(handlers::reload_settings))
        // OpenCode global config
        .route(
            "/opencode/config",
            get(handlers::get_global_opencode_config),
        )
        // Main Chat routes (single persistent cross-project assistant per user)
        .route(
            "/main",
            get(main_chat_handlers::get_main_chat)
                .post(main_chat_handlers::initialize_main_chat)
                .patch(main_chat_handlers::update_main_chat)
                .delete(main_chat_handlers::delete_main_chat),
        )
        .route(
            "/main/history",
            get(main_chat_handlers::get_history).post(main_chat_handlers::add_history),
        )
        .route("/main/export", get(main_chat_handlers::export_history))
        .route(
            "/main/sessions",
            get(main_chat_handlers::list_sessions).post(main_chat_handlers::register_session),
        )
        .route(
            "/main/sessions/latest",
            get(main_chat_handlers::get_latest_session),
        )
        // Main Chat Pi routes (Pi agent runtime for Main Chat)
        .route("/main/pi/status", get(main_chat_pi_handlers::get_pi_status))
        .route(
            "/main/pi/session",
            post(main_chat_pi_handlers::start_pi_session)
                .delete(main_chat_pi_handlers::close_session),
        )
        .route("/main/pi/state", get(main_chat_pi_handlers::get_pi_state))
        .route("/main/pi/prompt", post(main_chat_pi_handlers::send_prompt))
        .route("/main/pi/abort", post(main_chat_pi_handlers::abort_pi))
        .route(
            "/main/pi/messages",
            get(main_chat_pi_handlers::get_messages),
        )
        .route(
            "/main/pi/compact",
            post(main_chat_pi_handlers::compact_session),
        )
        .route("/main/pi/model", post(main_chat_pi_handlers::set_model))
        .route(
            "/main/pi/models",
            get(main_chat_pi_handlers::get_available_models),
        )
        .route(
            "/main/pi/commands",
            get(main_chat_pi_handlers::get_prompt_commands),
        )
        .route("/main/pi/new", post(main_chat_pi_handlers::new_session))
        .route("/main/pi/reset", post(main_chat_pi_handlers::reset_session))
        .route(
            "/main/pi/stats",
            get(main_chat_pi_handlers::get_session_stats),
        )
        .route("/main/pi/history", get(main_chat_pi_handlers::get_history))
        .route(
            "/main/pi/sessions",
            get(main_chat_pi_handlers::list_pi_sessions)
                .post(main_chat_pi_handlers::new_pi_session),
        )
        .route(
            "/main/pi/sessions/search",
            get(main_chat_pi_handlers::search_pi_sessions),
        )
        .route(
            "/main/pi/sessions/{session_id}",
            get(main_chat_pi_handlers::get_pi_session_messages)
                .post(main_chat_pi_handlers::resume_pi_session)
                .patch(main_chat_pi_handlers::update_pi_session)
                .delete(main_chat_pi_handlers::delete_pi_session),
        )
        // Workspace Pi routes (per-workspace Pi sessions)
        .route(
            "/pi/workspace/sessions",
            post(crate::api::workspace_pi::new_workspace_session),
        )
        .route(
            "/pi/workspace/sessions/{session_id}",
            delete(crate::api::workspace_pi::delete_workspace_session),
        )
        .route(
            "/pi/workspace/sessions/{session_id}/resume",
            post(crate::api::workspace_pi::resume_workspace_session),
        )
        .route(
            "/pi/workspace/sessions/{session_id}/messages",
            get(crate::api::workspace_pi::get_workspace_session_messages),
        )
        .route(
            "/pi/workspace/sessions/{session_id}/abort",
            post(crate::api::workspace_pi::abort_workspace_session),
        )
        .route(
            "/pi/workspace/state",
            get(crate::api::workspace_pi::get_workspace_state),
        )
        .route(
            "/pi/workspace/models",
            get(crate::api::workspace_pi::get_workspace_models),
        )
        .route(
            "/pi/workspace/model",
            post(crate::api::workspace_pi::set_workspace_model),
        )
        // Main Chat file access routes now use mux-only file channel
        // HSTRY (chat history) search routes
        .route("/search", get(handlers::search_sessions))
        // Scheduler (skdlr) overview
        .route("/scheduler/overview", get(handlers::scheduler_overview))
        // RSS/Atom feed fetch proxy
        .route("/feeds/fetch", get(handlers::fetch_feed))
        // CodexBar usage (optional, requires codexbar on PATH)
        .route("/codexbar/usage", get(handlers::codexbar_usage))
        // TRX (issue tracking) now uses mux-only channel
        // AgentRPC routes (unified backend API)
        .route("/agent/health", get(handlers::agent_health))
        .route(
            "/agent/conversations",
            get(handlers::agent_list_conversations),
        )
        .route(
            "/agent/conversations/{conversation_id}",
            get(handlers::agent_get_conversation),
        )
        .route(
            "/agent/conversations/{conversation_id}/messages",
            get(handlers::agent_get_messages),
        )
        .route("/agent/sessions", post(handlers::agent_start_session))
        .route(
            "/agent/sessions/{session_id}/messages",
            post(handlers::agent_send_message),
        )
        .route(
            "/agent/sessions/{session_id}",
            delete(handlers::agent_stop_session),
        )
        .route(
            "/agent/sessions/{session_id}/url",
            get(handlers::agent_get_session_url),
        )
        .route(
            "/agent/sessions/{session_id}/events",
            get(handlers::agent_attach),
        )
        // Agent ask endpoint - ask any agent a question and get response
        .route("/agents/ask", post(handlers::agents_ask))
        // Agent sessions search endpoint - find sessions by query
        .route("/agents/sessions", get(handlers::agents_search_sessions))
        // In-session search - search within a specific session using hstry
        .route(
            "/agents/sessions/{session_id}/search",
            get(handlers::agents_session_search),
        )
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // Public routes (no authentication)
    let public_routes = Router::new()
        .route("/health", get(handlers::health))
        .route("/ws/debug", get(handlers::ws_debug))
        .route("/features", get(handlers::features))
        .route("/auth/login", post(handlers::login))
        .route("/auth/register", post(handlers::register))
        .route("/auth/logout", post(handlers::logout))
        // Keep dev_login for backwards compatibility
        .route("/auth/dev-login", post(handlers::dev_login))
        .with_state(state.clone());

    // Delegation routes (localhost-only, no auth - used by Pi extension)
    // These routes check for localhost in the handler and reject non-local requests
    let delegate_routes = Router::new()
        .route("/delegate/start", post(delegate_handlers::start_session))
        .route(
            "/delegate/prompt/{session_id}",
            post(delegate_handlers::send_prompt),
        )
        .route(
            "/delegate/status/{session_id}",
            get(delegate_handlers::get_status),
        )
        .route(
            "/delegate/messages/{session_id}",
            get(delegate_handlers::get_messages),
        )
        .route(
            "/delegate/stop/{session_id}",
            post(delegate_handlers::stop_session),
        )
        .route("/delegate/sessions", get(delegate_handlers::list_sessions))
        .with_state(state.clone());

    // Test harness routes (dev mode only, no auth)
    // These routes allow sending mock events to test frontend features
    let test_routes = Router::new()
        .route("/test/event", post(super::test_harness::send_mock_event))
        .route("/test/a2ui", post(super::test_harness::send_mock_a2ui))
        .route(
            "/test/a2ui/sample",
            post(super::test_harness::send_sample_a2ui),
        )
        .with_state(state.clone());

    // A2UI routes (for agents to send UI surfaces)
    // These routes allow agents to display interactive UI in the frontend
    let a2ui_routes = Router::new()
        .route("/a2ui/surface", post(a2ui_handlers::send_surface))
        .route(
            "/a2ui/surface/{session_id}/{surface_id}",
            delete(a2ui_handlers::delete_surface),
        )
        .with_state(state);

    let permissions_policy = HeaderValue::from_static(
        "geolocation=(), microphone=(self), camera=()",
    );

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .merge(delegate_routes)
        .merge(test_routes)
        .merge(a2ui_routes)
        .layer(DefaultBodyLimit::max(max_body_size))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("permissions-policy"),
            permissions_policy,
        ))
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
        header::HeaderName::from_static("x-opencode-directory"),
    ];

    if allowed_origins.is_empty() {
        if dev_mode {
            // In dev mode with no configured origins, allow any origin
            tracing::warn!("CORS: No origins configured in dev mode, allowing any origin");
            CorsLayer::new()
                .allow_origin(AllowOrigin::any())
                .allow_methods(methods)
                .allow_headers(headers)
            // Note: allow_credentials(true) is incompatible with allow_origin(any())
            // For dev mode this is acceptable
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
                if let Ok(value) = origin.parse::<HeaderValue>()
                    && !origins.contains(&value)
                {
                    origins.push(value);
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

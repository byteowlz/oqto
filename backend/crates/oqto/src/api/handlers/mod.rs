//! API request handlers.
//!
//! This module contains all HTTP request handlers, organized by domain:
//! - `sessions`: Session CRUD operations
//! - `chat`: Chat history operations
//! - `projects`: Project/workspace management
//! - `admin`: Admin-only operations
//! - `settings`: Settings management
//! - `auth`: Authentication handlers
//! - `agents`: Agent management
//! - `agent_rpc`: Agent unified backend API
//! - `invites`: Invite code management
//! - `trx`: TRX issue tracking
//! - `misc`: Health checks, features, and utilities

mod admin;
mod auth;
mod chat;
mod feedback;
mod invites;
mod misc;
mod projects;
mod sessions;
mod settings;
pub mod trx;

// Re-export all public types and handlers

// Session handlers and types
pub use sessions::{
    browser_action, check_all_updates, check_session_update, create_session, delete_session,
    get_or_create_session, get_or_create_session_for_workspace, get_session, list_sessions,
    resume_session, start_browser, stop_session, touch_session_activity, upgrade_session,
};

// Chat history handlers and types
pub use chat::{
    delete_chat_session, get_chat_messages, get_chat_session, list_chat_history,
    list_chat_history_grouped, update_chat_session,
};
pub use feedback::create_feedback;

// Project handlers and types
pub use projects::{
    apply_workspace_pi_resources, create_project_from_template, get_project_logo,
    get_workspace_meta, get_workspace_pi_resources, get_workspace_sandbox, list_project_templates,
    list_workspace_dirs, list_workspace_locations, set_active_workspace_location,
    update_workspace_meta, update_workspace_sandbox, upsert_workspace_location,
};

// Admin handlers and types
pub use admin::{
    admin_cleanup_local_sessions, admin_force_stop_session, admin_list_sessions,
    admin_metrics_stream, get_admin_stats,
};

// User management (admin)
pub use admin::{
    activate_user, catalog_lookup, create_user, deactivate_user, delete_eavs_provider, delete_user,
    get_user, get_user_stats, list_eavs_providers, list_users, sync_all_models, sync_user_configs,
    update_user, upsert_eavs_provider,
};

// Auth handlers and types
pub use auth::{change_password, dev_login, get_me, login, logout, register, update_me};

// Settings handlers and types
pub use settings::{
    get_settings_schema, get_settings_values, reload_settings, update_settings_values,
};

// Invite code handlers and types
pub use invites::{
    create_invite_code, create_invite_codes_batch, delete_invite_code, get_invite_code,
    get_invite_code_stats, list_invite_codes, revoke_invite_code,
};

// TRX handlers and types
pub use trx::{
    close_trx_issue, create_trx_issue, get_trx_issue, list_trx_issues, sync_trx, update_trx_issue,
};

// Misc handlers and types
pub use misc::{
    codexbar_usage, features, fetch_feed, health, scheduler_delete, scheduler_overview,
    search_sessions, ws_debug,
};

// Internal helpers used by other modules

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    pub use super::misc::hstry_search_tests;
    #[allow(unused_imports)]
    pub use super::projects::tests as project_tests;
}

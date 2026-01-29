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
//! - `agent_rpc`: AgentRPC unified backend API
//! - `invites`: Invite code management
//! - `trx`: TRX issue tracking
//! - `misc`: Health checks, features, and utilities

mod admin;
mod agent_ask;
mod agent_rpc;
mod agents;
mod auth;
mod chat;
mod invites;
mod misc;
mod projects;
mod sessions;
mod settings;
mod trx;

// Re-export all public types and handlers

// Common types used across multiple modules
pub use misc::{FeaturesResponse, HealthResponse, VisualizerVoice, VoiceConfig, WsDebugResponse};

// Session handlers and types
pub use sessions::{
    GetOrCreateForWorkspaceRequest, SessionUpdateStatus, SessionUrls, SessionWithUrls,
    check_all_updates, check_session_update, create_session, delete_session, get_or_create_session,
    get_or_create_session_for_workspace, get_session, list_sessions, resume_session, stop_session,
    touch_session_activity, upgrade_session,
};

// Chat history handlers and types
pub use chat::{
    ChatHistoryQuery, ChatMessagesQuery, GroupedChatHistory, UpdateChatSessionRequest,
    get_chat_messages, get_chat_session, list_chat_history, list_chat_history_grouped,
    update_chat_session,
};

// Project handlers and types
pub use projects::{
    CreateProjectFromTemplateRequest, ListProjectTemplatesResponse, ProjectLogo,
    ProjectTemplateEntry, WorkspaceDirEntry, WorkspaceDirQuery, create_project_from_template,
    get_project_logo, list_project_templates, list_workspace_dirs,
};

// Admin handlers and types
pub use admin::{
    AdminMetricsSnapshot, AdminStatsResponse, LocalCleanupResponse, admin_cleanup_local_sessions,
    admin_force_stop_session, admin_list_sessions, admin_metrics_stream, get_admin_stats,
};

// User management (admin)
pub use admin::{
    activate_user, create_user, deactivate_user, delete_user, get_user, get_user_stats, list_users,
    update_user,
};

// Auth handlers and types
pub use auth::{
    LoginRequest, LoginResponse, RegisterRequest, RegisterResponse, UpdateMeRequest, UserInfo,
    dev_login, get_current_user, get_me, login, logout, register, update_me,
};

// Settings handlers and types
pub use settings::{
    SettingsQuery, get_global_opencode_config, get_settings_schema, get_settings_values,
    reload_settings, update_settings_values,
};

// Agent handlers and types
pub use agents::{
    AgentListQuery, create_agent, exec_agent_command, get_agent, list_agents, rediscover_agents,
    start_agent, stop_agent,
};

// AgentRPC handlers and types
pub use agent_rpc::{
    AgentAskAmbiguousResponse, AgentAskRequest, AgentAskResponse, AgentSessionsQuery,
    InSessionSearchQuery, InSessionSearchResult, SendAgentAgentPart, SendAgentFilePart,
    SendAgentMessageRequest, SessionMatch, SessionUrlResponse, StartAgentSessionRequest,
    agent_attach, agent_get_conversation, agent_get_messages, agent_get_session_url, agent_health,
    agent_list_conversations, agent_send_message, agent_start_session, agent_stop_session,
    agents_ask, agents_search_sessions, agents_session_search,
};

// Invite code handlers and types
pub use invites::{
    InviteCodeStats, create_invite_code, create_invite_codes_batch, delete_invite_code,
    get_invite_code, get_invite_code_stats, list_invite_codes, revoke_invite_code,
};

// TRX handlers and types
pub use trx::{
    CloseTrxIssueRequest, CreateTrxIssueRequest, TrxIssue, TrxWorkspaceQuery,
    UpdateTrxIssueRequest, close_trx_issue, create_trx_issue, get_trx_issue, list_trx_issues,
    sync_trx, update_trx_issue,
};

// Misc handlers and types
pub use misc::{
    FeedFetchQuery, FeedFetchResponse, SchedulerEntry, SchedulerOverview, SchedulerStats,
    SearchHit, SearchQuery, SearchResponse, codexbar_usage, features, fetch_feed, health,
    scheduler_overview, search_sessions, ws_debug,
};

// Internal helpers used by other modules
pub(crate) use chat::get_runner_for_user;
pub(crate) use trx::validate_workspace_path;

#[cfg(test)]
mod tests {
    // Re-export tests from submodules
    pub use super::misc::hstry_search_tests;
    pub use super::projects::tests as project_tests;
}

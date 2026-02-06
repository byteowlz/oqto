//! Chat history module - reads OpenCode session history from disk.
//!
//! This module provides read-only access to OpenCode chat sessions stored on disk,
//! without requiring a running OpenCode instance.
//!
//! OpenCode stores sessions in: ~/.local/share/opencode/storage/session/{projectID}/ses_*.json
//! where projectID is a hash of the workspace directory path.

pub mod models;
pub mod repository;
pub mod service;

// Re-export commonly used types and functions for backwards compatibility
#[allow(unused_imports)]
pub use models::{
    ChatMessage, ChatMessagePart, ChatSession, ChatSessionStats, HstrySearchHit, MessageInfo,
    MessageSummary, MessageTime, PartInfo, SessionInfo, SessionTime, TokenUsage, ToolState,
};

#[allow(unused_imports)]
pub use repository::{
    get_session, get_session_from_dir, get_session_from_hstry, get_session_messages_from_dir,
    hstry_db_path, list_sessions, list_sessions_from_dir, list_sessions_from_hstry,
    list_sessions_grouped, project_name_from_path, update_session_title,
    update_session_title_in_dir,
};

#[allow(unused_imports)]
pub use service::{
    get_session_messages_async, get_session_messages_rendered,
    get_session_messages_rendered_from_dir, get_session_messages_rendered_via_grpc,
    get_session_messages_via_grpc_cached, search_hstry,
};

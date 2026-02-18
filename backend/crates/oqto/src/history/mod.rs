//! Chat history module - reads chat sessions from hstry.
//!
//! This module provides access to chat sessions and messages stored in hstry,
//! either via gRPC (preferred) or direct SQLite access (runner-side).

pub mod models;
pub mod repository;
pub mod service;

// Re-export commonly used types and functions
#[allow(unused_imports)]
pub use models::{
    ChatMessage, ChatMessagePart, ChatSession, ChatSessionStats, HstrySearchHit, MessageInfo,
    MessageSummary, MessageTime, PartInfo, SessionInfo, SessionTime, TokenUsage, ToolState,
};

#[allow(unused_imports)]
pub use repository::{
    get_session, get_session_from_dir, get_session_from_hstry, get_session_messages_from_dir,
    hstry_db_path, list_sessions_from_dir, list_sessions_from_hstry, project_name_from_path,
    update_session_title, update_session_title_in_dir,
};

#[allow(unused_imports)]
pub use service::{
    get_session_messages_rendered_from_dir, get_session_messages_rendered_via_grpc,
    get_session_messages_via_grpc_cached, search_hstry,
};

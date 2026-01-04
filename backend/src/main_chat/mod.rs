//! Main Chat module for persistent cross-project AI assistant.
//!
//! Each user can have multiple named assistants (e.g., "jarvis", "govnr").
//! Each assistant has its own SQLite database stored at:
//! `~/.config/octo/users/<user_id>/main/<name>/assistant.db`
//!
//! The database stores:
//! - `history`: Conversation summaries, decisions, handoffs
//! - `sessions`: OpenCode session IDs linked to this assistant
//! - `config`: Assistant-specific configuration

mod db;
mod models;
mod repository;
mod service;

pub use models::{
    AssistantInfo, CreateHistoryEntry, CreateSession, HistoryEntry, HistoryEntryType,
    MainChatSession,
};
pub use service::MainChatService;

//! Main Chat module for persistent cross-project AI assistant.
//!
//! Each user has one Main Chat assistant with its own SQLite database stored at:
//! `{workspace}/main/main_chat.db` (single-user) or
//! `{workspace}/{user_id}/main/main_chat.db` (multi-user)
//!
//! The database stores:
//! - `history`: Conversation summaries, decisions, handoffs
//! - `sessions`: Session IDs linked to this assistant
//! - `config`: Assistant-specific configuration
//!
//! ## Pi Integration
//!
//! Main Chat uses Pi (pi-mono) as the agent runtime instead of OpenCode.
//! Pi provides:
//! - Block-based streaming
//! - Built-in compaction with LLM summarization
//! - JSON RPC protocol over stdin/stdout
//!
//! The `MainChatPiService` manages one Pi subprocess per user for real-time
//! interaction, while `MainChatService` handles persistent storage.

mod db;
mod models;
mod pi_service;
mod repository;
mod service;

pub use models::{
    AssistantInfo, ChatMessage, CreateChatMessage, CreateHistoryEntry, CreateSession,
    HistoryEntry, HistoryEntryType, MainChatSession, MessageRole,
};
pub use pi_service::{MainChatPiService, MainChatPiServiceConfig, UserPiSession};
pub use service::MainChatService;

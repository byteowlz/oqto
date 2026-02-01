//! TypeScript type generation tests.
//!
//! Run with: cargo test export_typescript_bindings -- --nocapture
//! Or use: just gen-types

use ts_rs::TS;

// Import all the types we want to export (using re-exported types from each module)
use octo::agent::{
    AgentInfo, AgentRuntimeInfo, AgentStatus, OpenCodeContextInfo, OpenCodeSessionInfo,
    OpenCodeSessionStatus, OpenCodeSessionTime, OpenCodeTokenCache, OpenCodeTokenLimit,
    OpenCodeTokenTotals,
};
use octo::canon::MessageRole;
use octo::history::{ChatMessage, ChatMessagePart, ChatSession};
use octo::main_chat::{AssistantInfo, HistoryEntryType};
use octo::session::{
    CreateSessionRequest, RuntimeMode, Session, SessionResponse, SessionStatus, SessionUrls,
};
use octo::user::{UserInfo, UserRole};

#[test]
fn export_typescript_bindings() {
    // Session types
    SessionStatus::export_all().expect("Failed to export SessionStatus");
    RuntimeMode::export_all().expect("Failed to export RuntimeMode");
    Session::export_all().expect("Failed to export Session");
    CreateSessionRequest::export_all().expect("Failed to export CreateSessionRequest");
    SessionResponse::export_all().expect("Failed to export SessionResponse");
    SessionUrls::export_all().expect("Failed to export SessionUrls");

    // User types
    UserRole::export_all().expect("Failed to export UserRole");
    UserInfo::export_all().expect("Failed to export UserInfo");

    // History types
    ChatSession::export_all().expect("Failed to export ChatSession");
    ChatMessage::export_all().expect("Failed to export ChatMessage");
    ChatMessagePart::export_all().expect("Failed to export ChatMessagePart");

    // Agent types
    AgentStatus::export_all().expect("Failed to export AgentStatus");
    AgentInfo::export_all().expect("Failed to export AgentInfo");
    AgentRuntimeInfo::export_all().expect("Failed to export AgentRuntimeInfo");
    OpenCodeSessionInfo::export_all().expect("Failed to export OpenCodeSessionInfo");
    OpenCodeSessionTime::export_all().expect("Failed to export OpenCodeSessionTime");
    OpenCodeSessionStatus::export_all().expect("Failed to export OpenCodeSessionStatus");
    OpenCodeContextInfo::export_all().expect("Failed to export OpenCodeContextInfo");
    OpenCodeTokenTotals::export_all().expect("Failed to export OpenCodeTokenTotals");
    OpenCodeTokenCache::export_all().expect("Failed to export OpenCodeTokenCache");
    OpenCodeTokenLimit::export_all().expect("Failed to export OpenCodeTokenLimit");

    // Main chat types
    HistoryEntryType::export_all().expect("Failed to export HistoryEntryType");
    MessageRole::export_all().expect("Failed to export MessageRole");
    AssistantInfo::export_all().expect("Failed to export AssistantInfo");

    println!("TypeScript bindings exported to frontend/src/generated/");
}

//! Legacy chat history compatibility module.
//!
//! New runtime history features should use `oqto_history::oqto_log` instead.

pub mod canon;
pub mod hstry;
pub mod models;
pub mod repository;
pub mod service;

// Re-export commonly used types and functions
#[allow(unused_imports)]
pub use canon::{legacy_message_to_canon, legacy_messages_to_canon};
pub use models::{ChatMessage, ChatMessagePart, ChatSession};
#[allow(unused_imports)]
pub use repository::{
    get_session, get_session_from_dir, get_session_from_hstry, get_session_messages_from_dir,
    hstry_db_path, list_sessions_from_dir, list_sessions_from_hstry, project_name_from_path,
    update_session_title, update_session_title_in_dir,
};

#[allow(unused_imports)]
pub use service::{
    get_session_messages_rendered_from_dir, get_session_messages_rendered_via_grpc,
    get_session_messages_via_grpc_cached,
};

#[allow(unused_imports)]
pub use hstry::{
    HstryClient, HstryEndpoint, HstryServiceConfig, HstryServiceManager,
    agent_message_to_proto_with_client_id,
};

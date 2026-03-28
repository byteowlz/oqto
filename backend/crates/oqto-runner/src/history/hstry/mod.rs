//! Unified hstry integration under the history namespace.

mod client;
mod convert;
mod service;

pub use client::HstryClient;
pub use convert::{
    SerializableMessage, agent_message_to_proto, agent_message_to_proto_with_client_id,
    proto_messages_to_serializable,
};
pub use service::{HstryServiceConfig, HstryServiceManager};

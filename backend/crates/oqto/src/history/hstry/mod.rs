//! Unified hstry integration under the history namespace.

mod client;
mod convert;
mod service;

pub use client::{HstryClient, HstryEndpoint};
pub use convert::agent_message_to_proto_with_client_id;
pub use service::{HstryServiceConfig, HstryServiceManager};

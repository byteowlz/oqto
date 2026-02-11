//! hstry integration for unified chat history storage.
//!
//! This module provides a client to communicate with the hstry daemon's
//! gRPC WriteService. Octo writes completed messages to hstry instead of
//! maintaining its own message database.
//!
//! ## Architecture
//!
//! ```text
//! Agent (Pi) <--stream--> Octo <--gRPC--> hstry daemon --> hstry.db
//! ```
//!
//! - Octo streams messages from Pi in real-time (low latency UI updates)
//! - When a message completes, Octo calls hstry's WriteService
//! - hstry daemon is the single writer to the database
//! - Deduplication via source_id="pi" + external_id=session_id
//!
//! ## Service Management
//!
//! In single-user mode, Octo auto-starts the hstry daemon if not running.
//! The daemon persists across Octo restarts.

mod client;
mod convert;
mod service;

pub use client::HstryClient;
pub use convert::{SerializableMessage, agent_message_to_proto, agent_message_to_proto_with_client_id, proto_messages_to_serializable};
pub use service::{HstryServiceConfig, HstryServiceManager};

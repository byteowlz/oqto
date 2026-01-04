//! Pi agent integration module.
//!
//! Provides types and handlers for communicating with the pi coding agent
//! via its RPC protocol (JSON over stdin/stdout).

mod client;
mod types;

pub use client::{PiClient, PiClientConfig};
pub use types::*;

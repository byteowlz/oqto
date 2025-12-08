//! EAVS (LLM Proxy) client module.
//!
//! Provides an async client for managing virtual API keys in EAVS.

mod client;
mod error;
mod types;

pub use client::EavsClient;
pub use error::{EavsError, EavsResult};
pub use types::*;

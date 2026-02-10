//! Pi agent integration module.
//!
//! Provides types and handlers for communicating with the pi coding agent
//! via its RPC protocol (JSON over stdin/stdout).
//!
//! ## Runtime Abstraction
//!
//! The `runtime` module provides a trait-based abstraction for running Pi
//! in different isolation modes:
//!
//! - **Local**: Direct subprocess (single-user mode)
//! - **Runner**: Via octo-runner daemon (multi-user isolation)
//! - **Container**: HTTP client to pi-bridge in container
//!
//! This allows `MainChatPiService` to work uniformly across all modes.

mod client;
pub mod runtime;
pub mod session_files;
pub mod session_parser;
mod types;

pub use client::PiClientConfig;
pub use runtime::{
    ContainerPiRuntime, LocalPiRuntime, PiProcess, PiRuntime, PiSpawnConfig, RunnerPiRuntime,
};
pub use types::*;

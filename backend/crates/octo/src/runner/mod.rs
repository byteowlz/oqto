//! Runner module for multi-user process isolation.
//!
//! This module provides the infrastructure for running processes as specific
//! Linux users without requiring the main octo process to have elevated privileges.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        octo-server                              │
//! │                    (runs as 'octo' user)                        │
//! └──────────────┬──────────────────────┬───────────────────────────┘
//!                │                      │
//!     Unix Socket│           Unix Socket│
//!                ▼                      ▼
//! ┌──────────────────────┐  ┌──────────────────────┐
//! │    octo-runner       │  │    octo-runner       │
//! │  (runs as 'alice')   │  │   (runs as 'bob')    │
//! │                      │  │                      │
//! │ /run/octo/runner-    │  │ /run/octo/runner-    │
//! │     alice.sock       │  │      bob.sock        │
//! └──────────┬───────────┘  └──────────┬───────────┘
//!            │                         │
//!            ▼                         ▼
//!    ┌───────────────┐         ┌───────────────┐
//!    │   opencode    │         │      pi       │
//!    │  (as alice)   │         │   (as bob)    │
//!    └───────────────┘         └───────────────┘
//! ```
//!
//! ## Components
//!
//! - **protocol**: JSON-RPC protocol types for runner communication
//! - **client**: Client library for octo to communicate with runners
//! - **daemon**: The runner daemon binary (see `bin/octo-runner.rs`)
//!
//! ## Usage
//!
//! The runner is started as a systemd user service for each Linux user.
//! Octo communicates with the appropriate runner via Unix socket to
//! spawn processes that run with that user's privileges.

pub mod client;
pub mod pi_manager;
pub mod pi_translator;
pub mod protocol;

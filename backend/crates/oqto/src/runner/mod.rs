//! Runner module for multi-user process isolation.
//!
//! This module provides the infrastructure for running processes as specific
//! Linux users without requiring the main oqto process to have elevated privileges.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        oqto-server                              │
//! │                    (runs as 'oqto' user)                        │
//! └──────────────┬──────────────────────┬───────────────────────────┘
//!                │                      │
//!     Unix Socket│           Unix Socket│
//!                ▼                      ▼
//! ┌──────────────────────┐  ┌──────────────────────┐
//! │    oqto-runner       │  │    oqto-runner       │
//! │  (runs as 'alice')   │  │   (runs as 'bob')    │
//! │                      │  │                      │
//! │ /run/oqto/runner-    │  │ /run/oqto/runner-    │
//! │     alice.sock       │  │      bob.sock        │
//! └──────────┬───────────┘  └──────────┬───────────┘
//!            │                         │
//!            ▼                         ▼
//!    ┌───────────────┐         ┌───────────────┐
//!    │   pi    │         │      pi       │
//!    │  (as alice)   │         │   (as bob)    │
//!    └───────────────┘         └───────────────┘
//! ```
//!
//! ## Components
//!
//! - **protocol**: JSON-RPC protocol types for runner communication
//! - **client**: Client library for oqto to communicate with runners
//! - **daemon**: The runner daemon binary (see `bin/oqto-runner.rs`)
//!
//! ## Usage
//!
//! The runner is started as a systemd user service for each Linux user.
//! Oqto communicates with the appropriate runner via Unix socket to
//! spawn processes that run with that user's privileges.

pub mod client;
pub mod pi_manager;
pub mod pi_translator;
pub mod protocol;

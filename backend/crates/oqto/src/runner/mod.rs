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
//! - **protocol**: shared runner RPC types re-exported from `oqto-runner-protocol`
//! - **client**: client library for oqto to communicate with per-user runners
//! - **router**: target/affinity routing helpers to choose the correct runner endpoint
//!
//! ## Usage
//!
//! The runner is started as a systemd user service for each Linux user.
//! Oqto communicates with the appropriate runner via Unix socket to
//! spawn processes that run with that user's privileges.

pub mod client;
pub mod protocol;
pub mod router;

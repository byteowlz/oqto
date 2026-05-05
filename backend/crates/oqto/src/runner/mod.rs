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
//! The runner daemon/runtime and socket client live in the `oqto-runner` crate.
//! This server-side module only keeps backend-specific target routing because it
//! depends on `AppState` and shared-workspace services.

pub mod router;

//! Paired, guard-owned remote hands link (ADR-0086).
//!
//! Shared by the `oqt` bridge (origin adapter `oqt_link`, standalone
//! destination `oqto-hands-host`) and, later, Runner's Durable core:
//!
//! - [`wire`]: `oqto.hands.v1` control vocabulary and generation-fenced data
//!   frames with byte credit.
//! - [`envframe`]: destination filter for the pi-env request stream.
//! - [`lease`], [`clock`]: fixed, suspend-aware lease deadlines.
//! - [`journal`]: immutable dispatch claims, controller CAS, fences.
//! - [`host`], [`launcher`], [`server`]: the destination enforcer.
//! - [`client`]: the origin side (pairing state, control, data streams).
//! - [`config`]: validated owner configuration with JSON schemas.
#![cfg(unix)]

pub mod client;
pub mod clock;
pub mod config;
pub mod envframe;
pub mod host;
pub mod journal;
pub mod launcher;
pub mod lease;
pub mod server;
pub mod wire;

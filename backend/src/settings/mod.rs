//! Settings module - schema-driven configuration management.
//!
//! Provides:
//! - Schema loading with x-scope filtering based on user role
//! - Config file reading/writing (TOML)
//! - Value comparison against defaults
//! - Hot-reload support

mod schema;
mod service;

pub use schema::{filter_schema_by_scope, load_schema, SettingsScope};
pub use service::{ConfigUpdate, SettingsService, SettingsValue};

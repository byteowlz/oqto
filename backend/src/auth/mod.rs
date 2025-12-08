//! Authentication module.
//!
//! Provides JWT validation middleware with support for:
//! - OIDC token validation (production)
//! - Dev bypass mode with configurable test users

mod claims;
mod config;
mod error;
mod middleware;

pub use claims::{Claims, Role};
pub use config::{AuthConfig, ConfigValidationError, DevUser};
pub use error::AuthError;
pub use middleware::{AuthState, CurrentUser, RequireAdmin, auth_middleware};

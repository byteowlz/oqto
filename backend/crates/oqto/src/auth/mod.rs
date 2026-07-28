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
#[allow(unused_imports)]
pub use config::{AuthConfig, ConfigValidationError, DevUser, resolve_secret_value};
pub use error::AuthError;
pub use middleware::{AuthMiddlewareState, AuthState, CurrentUser, RequireAdmin, auth_middleware};

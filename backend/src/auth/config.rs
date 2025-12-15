//! Authentication configuration.

use super::Role;
use serde::{Deserialize, Serialize};

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Enable development mode (bypass JWT validation).
    pub dev_mode: bool,

    /// JWT secret for HS256 (used in dev mode or simple setups).
    pub jwt_secret: Option<String>,

    /// OIDC issuer URL (for RS256 with JWKS).
    pub oidc_issuer: Option<String>,

    /// OIDC audience.
    pub oidc_audience: Option<String>,

    /// Development users (only used in dev mode).
    pub dev_users: Vec<DevUser>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            dev_mode: true,
            jwt_secret: Some("dev-secret-change-in-production".to_string()),
            oidc_issuer: None,
            oidc_audience: None,
            dev_users: vec![
                DevUser {
                    id: "dev".to_string(),
                    name: "Developer".to_string(),
                    email: "dev@localhost".to_string(),
                    password: "dev".to_string(),
                    role: Role::Admin,
                },
                DevUser {
                    id: "user".to_string(),
                    name: "Test User".to_string(),
                    email: "user@localhost".to_string(),
                    password: "user".to_string(),
                    role: Role::User,
                },
            ],
        }
    }
}

/// Development user configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevUser {
    /// User ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
    /// Password (plain text in dev mode only).
    pub password: String,
    /// Role.
    pub role: Role,
}

impl DevUser {
    /// Create a new dev user.
    #[allow(dead_code)]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        email: impl Into<String>,
        password: impl Into<String>,
        role: Role,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            email: email.into(),
            password: password.into(),
            role,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert!(config.dev_mode);
        assert!(config.jwt_secret.is_some());
        assert_eq!(config.dev_users.len(), 2);
    }

    #[test]
    fn test_dev_user_new() {
        let user = DevUser::new("test", "Test", "test@example.com", "pass", Role::Admin);
        assert_eq!(user.id, "test");
        assert_eq!(user.role, Role::Admin);
    }
}

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
    /// REQUIRED when dev_mode is false.
    pub jwt_secret: Option<String>,

    /// OIDC issuer URL (for RS256 with JWKS).
    pub oidc_issuer: Option<String>,

    /// OIDC audience.
    pub oidc_audience: Option<String>,

    /// Development users (only used in dev mode).
    /// Passwords are stored as bcrypt hashes for security.
    pub dev_users: Vec<DevUser>,

    /// Allowed CORS origins. If empty in production, CORS is disabled.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            dev_mode: true,
            // No default JWT secret - must be explicitly configured
            jwt_secret: None,
            oidc_issuer: None,
            oidc_audience: None,
            dev_users: vec![
                DevUser::new_with_plaintext(
                    "dev",
                    "Developer",
                    "dev@localhost",
                    "devpassword123",
                    Role::Admin,
                ),
                DevUser::new_with_plaintext(
                    "user",
                    "Test User",
                    "user@localhost",
                    "userpassword123",
                    Role::User,
                ),
            ],
            allowed_origins: vec![
                "http://localhost:3000".to_string(),
                "http://localhost:8080".to_string(),
            ],
        }
    }
}

impl AuthConfig {
    /// Validate the configuration.
    /// Returns an error if the configuration is invalid for the current mode.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if !self.dev_mode {
            // In production mode, JWT secret is required
            if self.jwt_secret.is_none() {
                return Err(ConfigValidationError::MissingJwtSecret);
            }

            // Check that the JWT secret is not the old insecure default
            if let Some(ref secret) = self.jwt_secret {
                if secret == "dev-secret-change-in-production" {
                    return Err(ConfigValidationError::InsecureJwtSecret);
                }
                // Ensure minimum secret length for security
                if secret.len() < 32 {
                    return Err(ConfigValidationError::JwtSecretTooShort);
                }
            }
        }

        Ok(())
    }

    /// Generate a secure random JWT secret.
    #[allow(dead_code)]
    pub fn generate_jwt_secret() -> String {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        
        let mut result = String::with_capacity(64);
        let chars: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        
        for _ in 0..64 {
            let hasher = RandomState::new();
            let mut h = hasher.build_hasher();
            h.write_u64(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64);
            let idx = (h.finish() as usize) % chars.len();
            result.push(chars[idx] as char);
        }
        result
    }
}

/// Configuration validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigValidationError {
    /// JWT secret is required in production mode.
    MissingJwtSecret,
    /// JWT secret is the insecure default value.
    InsecureJwtSecret,
    /// JWT secret is too short (minimum 32 characters).
    JwtSecretTooShort,
}

impl std::fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingJwtSecret => {
                write!(f, "JWT secret is required when dev_mode is false. Set AUTH_JWT_SECRET environment variable or jwt_secret in config.")
            }
            Self::InsecureJwtSecret => {
                write!(f, "JWT secret cannot be the default insecure value in production. Please configure a secure secret.")
            }
            Self::JwtSecretTooShort => {
                write!(f, "JWT secret must be at least 32 characters long for security.")
            }
        }
    }
}

impl std::error::Error for ConfigValidationError {}

/// Development user configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevUser {
    /// User ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
    /// Password hash (bcrypt).
    /// Use `new_with_plaintext` to create a user with automatic hashing.
    pub password_hash: String,
    /// Role.
    pub role: Role,
}

impl DevUser {
    /// Create a new dev user with a pre-hashed password.
    #[allow(dead_code)]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        email: impl Into<String>,
        password_hash: impl Into<String>,
        role: Role,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            email: email.into(),
            password_hash: password_hash.into(),
            role,
        }
    }

    /// Create a new dev user with a plaintext password (will be hashed).
    /// This is a convenience method for creating users programmatically.
    pub fn new_with_plaintext(
        id: impl Into<String>,
        name: impl Into<String>,
        email: impl Into<String>,
        password: &str,
        role: Role,
    ) -> Self {
        let password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
            .expect("Failed to hash password");
        
        Self {
            id: id.into(),
            name: name.into(),
            email: email.into(),
            password_hash,
            role,
        }
    }

    /// Verify a password against this user's hash.
    pub fn verify_password(&self, password: &str) -> bool {
        bcrypt::verify(password, &self.password_hash).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert!(config.dev_mode);
        // No default JWT secret for security
        assert!(config.jwt_secret.is_none());
        assert_eq!(config.dev_users.len(), 2);
    }

    #[test]
    fn test_dev_user_new_with_plaintext() {
        let user = DevUser::new_with_plaintext("test", "Test", "test@example.com", "testpass123", Role::Admin);
        assert_eq!(user.id, "test");
        assert_eq!(user.role, Role::Admin);
        // Password should be hashed, not plaintext
        assert_ne!(user.password_hash, "testpass123");
        assert!(user.password_hash.starts_with("$2"));
    }

    #[test]
    fn test_dev_user_password_verification() {
        let user = DevUser::new_with_plaintext("test", "Test", "test@example.com", "correctpassword", Role::User);
        
        // Correct password should verify
        assert!(user.verify_password("correctpassword"));
        
        // Wrong password should not verify
        assert!(!user.verify_password("wrongpassword"));
        assert!(!user.verify_password(""));
    }

    #[test]
    fn test_config_validation_dev_mode() {
        let config = AuthConfig::default();
        // Dev mode should be valid without JWT secret
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_production_mode_no_secret() {
        let mut config = AuthConfig::default();
        config.dev_mode = false;
        config.jwt_secret = None;
        
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigValidationError::MissingJwtSecret
        );
    }

    #[test]
    fn test_config_validation_production_mode_insecure_secret() {
        let mut config = AuthConfig::default();
        config.dev_mode = false;
        config.jwt_secret = Some("dev-secret-change-in-production".to_string());
        
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigValidationError::InsecureJwtSecret
        );
    }

    #[test]
    fn test_config_validation_production_mode_short_secret() {
        let mut config = AuthConfig::default();
        config.dev_mode = false;
        config.jwt_secret = Some("tooshort".to_string());
        
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigValidationError::JwtSecretTooShort
        );
    }

    #[test]
    fn test_config_validation_production_mode_valid() {
        let mut config = AuthConfig::default();
        config.dev_mode = false;
        config.jwt_secret = Some("a-very-long-and-secure-jwt-secret-that-is-at-least-32-chars".to_string());
        
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_generate_jwt_secret() {
        let secret = AuthConfig::generate_jwt_secret();
        assert_eq!(secret.len(), 64);
        // Should be alphanumeric
        assert!(secret.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}

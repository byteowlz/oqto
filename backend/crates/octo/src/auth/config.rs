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
            dev_mode: false,
            // No default JWT secret - must be explicitly configured
            jwt_secret: None,
            oidc_issuer: None,
            oidc_audience: None,
            dev_users: Vec::new(),
            allowed_origins: vec![
                "http://localhost:3000".to_string(),
                "http://localhost:8080".to_string(),
            ],
        }
    }
}

impl AuthConfig {
    /// Resolve the JWT secret, expanding `env:VAR_NAME` syntax.
    /// Returns the resolved secret or None if not configured.
    pub fn resolve_jwt_secret(&self) -> Result<Option<String>, ConfigValidationError> {
        match &self.jwt_secret {
            None => Ok(None),
            Some(value) => {
                if let Some(var_name) = value.strip_prefix("env:") {
                    match std::env::var(var_name) {
                        Ok(secret) if !secret.is_empty() => Ok(Some(secret)),
                        Ok(_) => Err(ConfigValidationError::EnvVarEmpty(var_name.to_string())),
                        Err(_) => Err(ConfigValidationError::EnvVarNotFound(var_name.to_string())),
                    }
                } else {
                    Ok(Some(value.clone()))
                }
            }
        }
    }

    /// Validate the configuration.
    /// Returns an error if the configuration is invalid for the current mode.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if !self.dev_mode {
            // In production mode, JWT secret is required
            let secret = self.resolve_jwt_secret()?;

            if secret.is_none() {
                return Err(ConfigValidationError::MissingJwtSecret);
            }

            // Check that the JWT secret is not the old insecure default
            if let Some(ref secret) = secret {
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

    /// Generate a secure random JWT secret using cryptographically secure RNG.
    ///
    /// Uses the `rand` crate with `ThreadRng` which is backed by the OS's
    /// cryptographically secure random number generator (via `getrandom`).
    #[allow(dead_code)]
    pub fn generate_jwt_secret() -> String {
        use rand::Rng;

        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        const SECRET_LENGTH: usize = 64;

        let mut rng = rand::rng();
        (0..SECRET_LENGTH)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
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
    /// Environment variable not found (for `env:VAR_NAME` syntax).
    EnvVarNotFound(String),
    /// Environment variable is empty (for `env:VAR_NAME` syntax).
    EnvVarEmpty(String),
}

impl std::fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingJwtSecret => {
                write!(
                    f,
                    "JWT secret is required when dev_mode is false. Set AUTH_JWT_SECRET environment variable or jwt_secret in config."
                )
            }
            Self::InsecureJwtSecret => {
                write!(
                    f,
                    "JWT secret cannot be the default insecure value in production. Please configure a secure secret."
                )
            }
            Self::JwtSecretTooShort => {
                write!(
                    f,
                    "JWT secret must be at least 32 characters long for security."
                )
            }
            Self::EnvVarNotFound(var) => {
                write!(
                    f,
                    "Environment variable '{}' not found (referenced via env:{} in config).",
                    var, var
                )
            }
            Self::EnvVarEmpty(var) => {
                write!(
                    f,
                    "Environment variable '{}' is empty (referenced via env:{} in config).",
                    var, var
                )
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
    pub password_hash: String,
    /// Role.
    pub role: Role,
}

impl DevUser {
    /// Verify a password against this user's hash.
    pub fn verify_password(&self, password: &str) -> bool {
        bcrypt::verify(password, &self.password_hash).unwrap_or(false)
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    fn make_dev_user(id: &str, name: &str, email: &str, password: &str, role: Role) -> DevUser {
        let password_hash =
            bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("Failed to hash password");

        DevUser {
            id: id.to_string(),
            name: name.to_string(),
            email: email.to_string(),
            password_hash,
            role,
        }
    }

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert!(!config.dev_mode);
        // No default JWT secret for security
        assert!(config.jwt_secret.is_none());
        assert!(config.dev_users.is_empty());
    }

    #[test]
    fn test_dev_user_password_hashing() {
        let user = make_dev_user(
            "test",
            "Test",
            "test@example.com",
            "testpass123",
            Role::Admin,
        );
        assert_eq!(user.id, "test");
        assert_eq!(user.role, Role::Admin);
        // Password should be hashed, not plaintext
        assert_ne!(user.password_hash, "testpass123");
        assert!(user.password_hash.starts_with("$2"));
    }

    #[test]
    fn test_dev_user_password_verification() {
        let user = make_dev_user(
            "test",
            "Test",
            "test@example.com",
            "correctpassword",
            Role::User,
        );

        // Correct password should verify
        assert!(user.verify_password("correctpassword"));

        // Wrong password should not verify
        assert!(!user.verify_password("wrongpassword"));
        assert!(!user.verify_password(""));
    }

    #[test]
    #[ignore] // Only run manually to generate hashes
    fn test_generate_dev_user_hashes() {
        // Generate and print bcrypt hashes for default dev users
        // These can be used as pre-computed hashes to avoid hashing at startup
        let dev_hash = bcrypt::hash("devpassword123", bcrypt::DEFAULT_COST).unwrap();
        let user_hash = bcrypt::hash("userpassword123", bcrypt::DEFAULT_COST).unwrap();
        println!("DEV_USER_HASH: {}", dev_hash);
        println!("USER_USER_HASH: {}", user_hash);
        // Verify they work
        assert!(bcrypt::verify("devpassword123", &dev_hash).unwrap());
        assert!(bcrypt::verify("userpassword123", &user_hash).unwrap());
    }

    #[test]
    fn test_config_validation_dev_mode() {
        let mut config = AuthConfig::default();
        config.dev_mode = true;
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
        config.jwt_secret =
            Some("a-very-long-and-secure-jwt-secret-that-is-at-least-32-chars".to_string());

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_generate_jwt_secret_length_and_charset() {
        let secret = AuthConfig::generate_jwt_secret();
        assert_eq!(secret.len(), 64, "Secret should be 64 characters long");
        // Should be alphanumeric
        assert!(
            secret.chars().all(|c| c.is_ascii_alphanumeric()),
            "Secret should only contain alphanumeric characters"
        );
    }

    #[test]
    fn test_generate_jwt_secret_uniqueness() {
        // Generate multiple secrets and ensure they're all different
        let secrets: Vec<String> = (0..100)
            .map(|_| AuthConfig::generate_jwt_secret())
            .collect();

        // Check that all secrets are unique
        let mut unique_secrets = secrets.clone();
        unique_secrets.sort();
        unique_secrets.dedup();
        assert_eq!(
            unique_secrets.len(),
            secrets.len(),
            "All generated secrets should be unique"
        );
    }

    #[test]
    fn test_generate_jwt_secret_entropy() {
        // Generate a secret and check that it has reasonable character distribution
        // This is a basic sanity check, not a formal entropy test
        let secret = AuthConfig::generate_jwt_secret();

        // Count character types
        let uppercase_count = secret.chars().filter(|c| c.is_ascii_uppercase()).count();
        let lowercase_count = secret.chars().filter(|c| c.is_ascii_lowercase()).count();
        let digit_count = secret.chars().filter(|c| c.is_ascii_digit()).count();

        // With 64 chars from a 62-char alphabet, we expect roughly:
        // - 26/62 * 64 ≈ 27 uppercase
        // - 26/62 * 64 ≈ 27 lowercase
        // - 10/62 * 64 ≈ 10 digits
        // Allow for variance but ensure we have at least some of each type
        // (probability of having 0 of any type in 64 chars is astronomically low)
        assert!(
            uppercase_count > 0,
            "Secret should contain at least one uppercase letter"
        );
        assert!(
            lowercase_count > 0,
            "Secret should contain at least one lowercase letter"
        );
        assert!(digit_count > 0, "Secret should contain at least one digit");
    }

    #[test]
    fn test_generate_jwt_secret_sufficient_for_hmac() {
        // HMAC-SHA256 requires at least 32 bytes of entropy
        // Our 64-character alphanumeric secret provides ~370 bits of entropy
        // (log2(62^64) ≈ 381 bits), which is more than sufficient
        let secret = AuthConfig::generate_jwt_secret();

        // Verify the secret meets minimum length requirements
        assert!(
            secret.len() >= 32,
            "Secret should be at least 32 characters for HMAC-SHA256"
        );

        // Verify it passes our own validation
        let mut config = AuthConfig::default();
        config.dev_mode = false;
        config.jwt_secret = Some(secret);
        assert!(
            config.validate().is_ok(),
            "Generated secret should pass validation"
        );
    }

    #[test]
    fn test_resolve_jwt_secret_literal() {
        let mut config = AuthConfig::default();
        config.jwt_secret = Some("my-literal-secret".to_string());

        let resolved = config.resolve_jwt_secret().unwrap();
        assert_eq!(resolved, Some("my-literal-secret".to_string()));
    }

    #[test]
    fn test_resolve_jwt_secret_env_var() {
        // Set a test env var
        // SAFETY: This is a test-only environment variable with a unique name
        unsafe {
            std::env::set_var(
                "TEST_JWT_SECRET_12345",
                "secret-from-env-var-at-least-32-chars",
            );
        }

        let mut config = AuthConfig::default();
        config.jwt_secret = Some("env:TEST_JWT_SECRET_12345".to_string());

        let resolved = config.resolve_jwt_secret().unwrap();
        assert_eq!(
            resolved,
            Some("secret-from-env-var-at-least-32-chars".to_string())
        );

        // Clean up
        // SAFETY: Cleaning up test environment variable
        unsafe {
            std::env::remove_var("TEST_JWT_SECRET_12345");
        }
    }

    #[test]
    fn test_resolve_jwt_secret_env_var_not_found() {
        let mut config = AuthConfig::default();
        config.jwt_secret = Some("env:NONEXISTENT_VAR_12345".to_string());

        let result = config.resolve_jwt_secret();
        assert_eq!(
            result.unwrap_err(),
            ConfigValidationError::EnvVarNotFound("NONEXISTENT_VAR_12345".to_string())
        );
    }

    #[test]
    fn test_resolve_jwt_secret_env_var_empty() {
        // SAFETY: This is a test-only environment variable with a unique name
        unsafe {
            std::env::set_var("TEST_EMPTY_JWT_SECRET", "");
        }

        let mut config = AuthConfig::default();
        config.jwt_secret = Some("env:TEST_EMPTY_JWT_SECRET".to_string());

        let result = config.resolve_jwt_secret();
        assert_eq!(
            result.unwrap_err(),
            ConfigValidationError::EnvVarEmpty("TEST_EMPTY_JWT_SECRET".to_string())
        );

        // SAFETY: Cleaning up test environment variable
        unsafe {
            std::env::remove_var("TEST_EMPTY_JWT_SECRET");
        }
    }

    #[test]
    fn test_resolve_jwt_secret_none() {
        let config = AuthConfig::default();
        let resolved = config.resolve_jwt_secret().unwrap();
        assert_eq!(resolved, None);
    }
}

//! User service for business logic.

use anyhow::{Context, Result, bail};
use tracing::{info, instrument, warn};

use super::models::{CreateUserRequest, UpdateUserRequest, User, UserListQuery, UserRole};
use super::repository::UserRepository;

/// Service for user management operations.
#[derive(Debug, Clone)]
pub struct UserService {
    repo: UserRepository,
}

impl UserService {
    /// Create a new user service.
    pub fn new(repo: UserRepository) -> Self {
        Self { repo }
    }

    /// Create a new user with validation.
    #[instrument(skip(self, request), fields(username = %request.username))]
    pub async fn create_user(&self, request: CreateUserRequest) -> Result<User> {
        // Validate username format
        if !is_valid_username(&request.username) {
            bail!("Invalid username format. Must be 3-50 alphanumeric characters, underscores, or hyphens.");
        }

        // Validate email format
        if !is_valid_email(&request.email) {
            bail!("Invalid email format.");
        }

        // Check username availability
        if !self.repo.is_username_available(&request.username).await? {
            bail!("Username '{}' is already taken.", request.username);
        }

        // Check email availability
        if !self.repo.is_email_available(&request.email).await? {
            bail!("Email '{}' is already registered.", request.email);
        }

        // Hash password if provided
        let mut processed_request = request;
        if let Some(password) = &processed_request.password {
            if password.len() < 6 {
                bail!("Password must be at least 6 characters.");
            }
            processed_request.password = Some(hash_password(password)?);
        }

        let user = self.repo.create(processed_request).await?;
        info!(user_id = %user.id, username = %user.username, "Created new user");

        Ok(user)
    }

    /// Get a user by ID.
    #[instrument(skip(self))]
    pub async fn get_user(&self, id: &str) -> Result<Option<User>> {
        self.repo.get(id).await
    }

    /// Get a user by username.
    #[instrument(skip(self))]
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        self.repo.get_by_username(username).await
    }

    /// Get a user by email.
    #[instrument(skip(self))]
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
        self.repo.get_by_email(email).await
    }

    /// Get a user by external ID (OIDC).
    #[instrument(skip(self))]
    pub async fn get_user_by_external_id(&self, external_id: &str) -> Result<Option<User>> {
        self.repo.get_by_external_id(external_id).await
    }

    /// List users with optional filters.
    #[instrument(skip(self))]
    pub async fn list_users(&self, query: UserListQuery) -> Result<Vec<User>> {
        self.repo.list(query).await
    }

    /// Update a user.
    #[instrument(skip(self, request))]
    pub async fn update_user(&self, id: &str, request: UpdateUserRequest) -> Result<User> {
        // Validate username if being updated
        if let Some(username) = &request.username {
            if !is_valid_username(username) {
                bail!("Invalid username format.");
            }
            // Check if new username is available (excluding current user)
            if let Some(existing) = self.repo.get_by_username(username).await? {
                if existing.id != id {
                    bail!("Username '{}' is already taken.", username);
                }
            }
        }

        // Validate email if being updated
        if let Some(email) = &request.email {
            if !is_valid_email(email) {
                bail!("Invalid email format.");
            }
            // Check if new email is available (excluding current user)
            if let Some(existing) = self.repo.get_by_email(email).await? {
                if existing.id != id {
                    bail!("Email '{}' is already registered.", email);
                }
            }
        }

        // Hash password if being updated
        let mut processed_request = request;
        if let Some(password) = &processed_request.password {
            if password.len() < 6 {
                bail!("Password must be at least 6 characters.");
            }
            processed_request.password = Some(hash_password(password)?);
        }

        let user = self.repo.update(id, processed_request).await?;
        info!(user_id = %user.id, "Updated user");

        Ok(user)
    }

    /// Delete a user.
    #[instrument(skip(self))]
    pub async fn delete_user(&self, id: &str) -> Result<()> {
        // Check if user exists
        let user = self.repo.get(id).await?;
        if user.is_none() {
            bail!("User not found: {}", id);
        }

        self.repo.delete(id).await?;
        info!(user_id = %id, "Deleted user");

        Ok(())
    }

    /// Deactivate a user (soft delete).
    #[instrument(skip(self))]
    pub async fn deactivate_user(&self, id: &str) -> Result<User> {
        let update = UpdateUserRequest {
            is_active: Some(false),
            ..Default::default()
        };

        let user = self.repo.update(id, update).await?;
        warn!(user_id = %id, "Deactivated user");

        Ok(user)
    }

    /// Activate a user.
    #[instrument(skip(self))]
    pub async fn activate_user(&self, id: &str) -> Result<User> {
        let update = UpdateUserRequest {
            is_active: Some(true),
            ..Default::default()
        };

        let user = self.repo.update(id, update).await?;
        info!(user_id = %id, "Activated user");

        Ok(user)
    }

    /// Verify user credentials (for local auth).
    #[instrument(skip(self, password))]
    pub async fn verify_credentials(&self, username: &str, password: &str) -> Result<Option<User>> {
        let user = self.repo.get_by_username(username).await?;

        match user {
            Some(user) if user.is_active => {
                if let Some(hash) = &user.password_hash {
                    if verify_password(password, hash)? {
                        // Update last login
                        self.repo.update_last_login(&user.id).await?;
                        return Ok(Some(user));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Get or create a user from OIDC claims.
    #[instrument(skip(self))]
    pub async fn get_or_create_from_oidc(
        &self,
        external_id: &str,
        email: &str,
        name: &str,
    ) -> Result<User> {
        // Try to find existing user by external_id
        if let Some(user) = self.repo.get_by_external_id(external_id).await? {
            // Update last login
            self.repo.update_last_login(&user.id).await?;
            return Ok(user);
        }

        // Try to find by email and link
        if let Some(existing) = self.repo.get_by_email(email).await? {
            // Link the external_id to existing user
            let update = UpdateUserRequest {
                display_name: Some(name.to_string()),
                ..Default::default()
            };
            // Note: We'd need to add external_id to UpdateUserRequest to do this properly
            let user = self.repo.update(&existing.id, update).await?;
            return Ok(user);
        }

        // Create new user
        let username = generate_username_from_email(email);
        let request = CreateUserRequest {
            username,
            email: email.to_string(),
            password: None,
            display_name: Some(name.to_string()),
            role: Some(UserRole::User),
            external_id: Some(external_id.to_string()),
        };

        self.repo.create(request).await
    }

    /// Get user statistics.
    #[instrument(skip(self))]
    pub async fn get_stats(&self) -> Result<UserStats> {
        let total = self.repo.count().await?;
        let admins = self.repo.count_by_role(UserRole::Admin).await?;
        let users = self.repo.count_by_role(UserRole::User).await?;
        let services = self.repo.count_by_role(UserRole::Service).await?;

        Ok(UserStats {
            total,
            admins,
            users,
            services,
        })
    }
}

/// User statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UserStats {
    pub total: i64,
    pub admins: i64,
    pub users: i64,
    pub services: i64,
}

/// Validate username format.
fn is_valid_username(username: &str) -> bool {
    let len = username.len();
    if !(3..=50).contains(&len) {
        return false;
    }

    username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Basic email validation.
fn is_valid_email(email: &str) -> bool {
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }
    !parts[0].is_empty() && parts[1].contains('.')
}

/// Hash a password using bcrypt.
fn hash_password(password: &str) -> Result<String> {
    // Use a lower cost factor for development speed
    let cost = if cfg!(debug_assertions) { 4 } else { 10 };
    bcrypt::hash(password, cost).context("Failed to hash password")
}

/// Verify a password against a bcrypt hash.
fn verify_password(password: &str, hash: &str) -> Result<bool> {
    bcrypt::verify(password, hash).context("Failed to verify password")
}

/// Generate a username from an email address.
fn generate_username_from_email(email: &str) -> String {
    let local = email.split('@').next().unwrap_or("user");
    let base: String = local
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();

    if base.len() >= 3 {
        base
    } else {
        format!("user_{}", nanoid::nanoid!(8))
    }
}

impl Default for UpdateUserRequest {
    fn default() -> Self {
        Self {
            username: None,
            email: None,
            password: None,
            display_name: None,
            avatar_url: None,
            role: None,
            is_active: None,
            settings: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_username() {
        assert!(is_valid_username("user"));
        assert!(is_valid_username("user_name"));
        assert!(is_valid_username("user-name"));
        assert!(is_valid_username("user123"));
        assert!(!is_valid_username("ab")); // too short
        assert!(!is_valid_username("user@name")); // invalid char
        assert!(!is_valid_username("user name")); // space
    }

    #[test]
    fn test_is_valid_email() {
        assert!(is_valid_email("user@example.com"));
        assert!(is_valid_email("user.name@sub.domain.com"));
        assert!(!is_valid_email("userexample.com"));
        assert!(!is_valid_email("user@"));
        assert!(!is_valid_email("@example.com"));
    }

    #[test]
    fn test_generate_username_from_email() {
        assert_eq!(generate_username_from_email("john@example.com"), "john");
        assert_eq!(
            generate_username_from_email("john.doe@example.com"),
            "johndoe"
        );
        // Short emails get random suffix
        let short = generate_username_from_email("ab@example.com");
        assert!(short.starts_with("user_"));
    }

    #[test]
    fn test_password_hashing() {
        let password = "test_password";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }
}

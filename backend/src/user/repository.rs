//! User repository for database operations.

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tracing::{debug, instrument};

use super::models::{CreateUserRequest, UpdateUserRequest, User, UserListQuery, UserRole};

/// Repository for user database operations.
#[derive(Debug, Clone)]
pub struct UserRepository {
    pool: SqlitePool,
}

impl UserRepository {
    /// Create a new user repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Generate a new user ID.
    fn generate_id() -> String {
        format!("usr_{}", nanoid::nanoid!(12))
    }

    /// Create a new user.
    #[instrument(skip(self, request), fields(username = %request.username))]
    pub async fn create(&self, request: CreateUserRequest) -> Result<User> {
        let id = Self::generate_id();
        let display_name = request
            .display_name
            .unwrap_or_else(|| request.username.clone());
        let role = request.role.unwrap_or(UserRole::User);

        debug!("Creating user: {} ({})", request.username, id);

        sqlx::query(
            r#"
            INSERT INTO users (id, external_id, username, email, password_hash, display_name, role)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.external_id)
        .bind(&request.username)
        .bind(&request.email)
        .bind(&request.password)
        .bind(&display_name)
        .bind(role.to_string())
        .execute(&self.pool)
        .await
        .context("Failed to insert user")?;

        self.get(&id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found after creation"))
    }

    /// Get a user by ID.
    #[instrument(skip(self))]
    pub async fn get(&self, id: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, external_id, username, email, password_hash, display_name, 
                   avatar_url, role, is_active, created_at, updated_at, last_login_at, settings
            FROM users
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch user")?;

        Ok(user)
    }

    /// Get a user by username.
    #[instrument(skip(self))]
    pub async fn get_by_username(&self, username: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, external_id, username, email, password_hash, display_name,
                   avatar_url, role, is_active, created_at, updated_at, last_login_at, settings
            FROM users
            WHERE username = ?
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch user by username")?;

        Ok(user)
    }

    /// Get a user by email.
    #[instrument(skip(self))]
    pub async fn get_by_email(&self, email: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, external_id, username, email, password_hash, display_name,
                   avatar_url, role, is_active, created_at, updated_at, last_login_at, settings
            FROM users
            WHERE email = ?
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch user by email")?;

        Ok(user)
    }

    /// Get a user by external ID (OIDC subject).
    #[instrument(skip(self))]
    pub async fn get_by_external_id(&self, external_id: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, external_id, username, email, password_hash, display_name,
                   avatar_url, role, is_active, created_at, updated_at, last_login_at, settings
            FROM users
            WHERE external_id = ?
            "#,
        )
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch user by external_id")?;

        Ok(user)
    }

    /// List users with optional filters.
    #[instrument(skip(self))]
    pub async fn list(&self, query: UserListQuery) -> Result<Vec<User>> {
        let limit = query.limit.unwrap_or(100);
        let offset = query.offset.unwrap_or(0);

        // Build dynamic query based on filters
        let mut sql = String::from(
            r#"
            SELECT id, external_id, username, email, password_hash, display_name,
                   avatar_url, role, is_active, created_at, updated_at, last_login_at, settings
            FROM users
            WHERE 1=1
            "#,
        );

        let mut bind_values: Vec<String> = Vec::new();

        if let Some(role) = &query.role {
            sql.push_str(" AND role = ?");
            bind_values.push(role.to_string());
        }

        if let Some(is_active) = query.is_active {
            sql.push_str(" AND is_active = ?");
            bind_values.push(if is_active { "1" } else { "0" }.to_string());
        }

        if let Some(search) = &query.search {
            sql.push_str(" AND (username LIKE ? OR email LIKE ? OR display_name LIKE ?)");
            let pattern = format!("%{}%", search);
            bind_values.push(pattern.clone());
            bind_values.push(pattern.clone());
            bind_values.push(pattern);
        }

        sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

        // Execute with dynamic bindings
        let mut query_builder = sqlx::query_as::<_, User>(&sql);

        for value in &bind_values {
            query_builder = query_builder.bind(value);
        }

        query_builder = query_builder.bind(limit).bind(offset);

        let users = query_builder
            .fetch_all(&self.pool)
            .await
            .context("Failed to list users")?;

        Ok(users)
    }

    /// Update a user.
    #[instrument(skip(self, request))]
    pub async fn update(&self, id: &str, request: UpdateUserRequest) -> Result<User> {
        // First check if user exists
        let existing = self
            .get(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found: {}", id))?;

        // Build update query dynamically
        let mut updates = Vec::new();
        let mut values: Vec<String> = Vec::new();

        if let Some(username) = &request.username {
            updates.push("username = ?");
            values.push(username.clone());
        }

        if let Some(email) = &request.email {
            updates.push("email = ?");
            values.push(email.clone());
        }

        if let Some(password) = &request.password {
            updates.push("password_hash = ?");
            values.push(password.clone());
        }

        if let Some(display_name) = &request.display_name {
            updates.push("display_name = ?");
            values.push(display_name.clone());
        }

        if let Some(avatar_url) = &request.avatar_url {
            updates.push("avatar_url = ?");
            values.push(avatar_url.clone());
        }

        if let Some(role) = &request.role {
            updates.push("role = ?");
            values.push(role.to_string());
        }

        if let Some(is_active) = request.is_active {
            updates.push("is_active = ?");
            values.push(if is_active { "1" } else { "0" }.to_string());
        }

        if let Some(settings) = &request.settings {
            updates.push("settings = ?");
            values.push(settings.clone());
        }

        if updates.is_empty() {
            return Ok(existing);
        }

        updates.push("updated_at = datetime('now')");

        let sql = format!("UPDATE users SET {} WHERE id = ?", updates.join(", "));

        let mut query_builder = sqlx::query(&sql);
        for value in &values {
            query_builder = query_builder.bind(value);
        }
        query_builder = query_builder.bind(id);

        query_builder
            .execute(&self.pool)
            .await
            .context("Failed to update user")?;

        self.get(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found after update"))
    }

    /// Delete a user.
    #[instrument(skip(self))]
    pub async fn delete(&self, id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete user")?;

        if result.rows_affected() == 0 {
            return Err(anyhow::anyhow!("User not found: {}", id));
        }

        Ok(())
    }

    /// Update last login timestamp.
    #[instrument(skip(self))]
    pub async fn update_last_login(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE users SET last_login_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to update last login")?;

        Ok(())
    }

    /// Check if a username is available.
    #[instrument(skip(self))]
    pub async fn is_username_available(&self, username: &str) -> Result<bool> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE username = ?")
            .bind(username)
            .fetch_one(&self.pool)
            .await
            .context("Failed to check username availability")?;

        Ok(count.0 == 0)
    }

    /// Check if an email is available.
    #[instrument(skip(self))]
    pub async fn is_email_available(&self, email: &str) -> Result<bool> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE email = ?")
            .bind(email)
            .fetch_one(&self.pool)
            .await
            .context("Failed to check email availability")?;

        Ok(count.0 == 0)
    }

    /// Count total users.
    #[instrument(skip(self))]
    pub async fn count(&self) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count users")?;

        Ok(count.0)
    }

    /// Count users by role.
    #[instrument(skip(self))]
    pub async fn count_by_role(&self, role: UserRole) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = ?")
            .bind(role.to_string())
            .fetch_one(&self.pool)
            .await
            .context("Failed to count users by role")?;

        Ok(count.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Run migrations
        sqlx::query(
            r#"
            CREATE TABLE users (
                id TEXT PRIMARY KEY NOT NULL,
                external_id TEXT UNIQUE,
                username TEXT UNIQUE NOT NULL,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT,
                display_name TEXT NOT NULL,
                avatar_url TEXT,
                role TEXT NOT NULL DEFAULT 'user',
                is_active BOOLEAN NOT NULL DEFAULT TRUE,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_login_at TEXT,
                settings TEXT DEFAULT '{}'
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_user() {
        let pool = setup_test_db().await;
        let repo = UserRepository::new(pool);

        let request = CreateUserRequest {
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
            password: Some("hashed_password".to_string()),
            display_name: Some("Test User".to_string()),
            role: None,
            external_id: None,
        };

        let user = repo.create(request).await.unwrap();
        assert_eq!(user.username, "testuser");
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.display_name, "Test User");
        assert_eq!(user.role, UserRole::User);
        assert!(user.is_active);

        // Fetch by ID
        let fetched = repo.get(&user.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, user.id);

        // Fetch by username
        let by_username = repo.get_by_username("testuser").await.unwrap().unwrap();
        assert_eq!(by_username.id, user.id);

        // Fetch by email
        let by_email = repo
            .get_by_email("test@example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_email.id, user.id);
    }

    #[tokio::test]
    async fn test_update_user() {
        let pool = setup_test_db().await;
        let repo = UserRepository::new(pool);

        let request = CreateUserRequest {
            username: "updateuser".to_string(),
            email: "update@example.com".to_string(),
            password: None,
            display_name: None,
            role: None,
            external_id: None,
        };

        let user = repo.create(request).await.unwrap();

        let update = UpdateUserRequest {
            username: None,
            email: None,
            password: None,
            display_name: Some("Updated Name".to_string()),
            avatar_url: None,
            role: Some(UserRole::Admin),
            is_active: None,
            settings: None,
        };

        let updated = repo.update(&user.id, update).await.unwrap();
        assert_eq!(updated.display_name, "Updated Name");
        assert_eq!(updated.role, UserRole::Admin);
    }

    #[tokio::test]
    async fn test_delete_user() {
        let pool = setup_test_db().await;
        let repo = UserRepository::new(pool);

        let request = CreateUserRequest {
            username: "deleteuser".to_string(),
            email: "delete@example.com".to_string(),
            password: None,
            display_name: None,
            role: None,
            external_id: None,
        };

        let user = repo.create(request).await.unwrap();
        repo.delete(&user.id).await.unwrap();

        let fetched = repo.get(&user.id).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_list_users() {
        let pool = setup_test_db().await;
        let repo = UserRepository::new(pool);

        // Create multiple users
        for i in 0..5 {
            let request = CreateUserRequest {
                username: format!("user{}", i),
                email: format!("user{}@example.com", i),
                password: None,
                display_name: None,
                role: if i == 0 {
                    Some(UserRole::Admin)
                } else {
                    None
                },
                external_id: None,
            };
            repo.create(request).await.unwrap();
        }

        // List all
        let all = repo.list(UserListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 5);

        // List admins only
        let admins = repo
            .list(UserListQuery {
                role: Some(UserRole::Admin),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(admins.len(), 1);

        // Search
        let search = repo
            .list(UserListQuery {
                search: Some("user2".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(search.len(), 1);
    }
}

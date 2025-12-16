//! Invite code repository for database operations.

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tracing::{debug, instrument};

use super::models::{CreateInviteCodeRequest, InviteCode, InviteCodeListQuery};

/// Repository for invite code database operations.
#[derive(Debug, Clone)]
pub struct InviteCodeRepository {
    pool: SqlitePool,
}

impl InviteCodeRepository {
    /// Create a new invite code repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Generate a new invite code ID.
    fn generate_id() -> String {
        format!("inv_{}", nanoid::nanoid!(12))
    }

    /// Generate a random invite code string.
    pub fn generate_code(prefix: Option<&str>) -> String {
        let random_part = nanoid::nanoid!(8, &nanoid::alphabet::SAFE);
        match prefix {
            Some(p) => format!("{}-{}", p, random_part),
            None => random_part,
        }
    }

    /// Create a new invite code.
    #[instrument(skip(self, request, created_by))]
    pub async fn create(
        &self,
        request: CreateInviteCodeRequest,
        created_by: &str,
    ) -> Result<InviteCode> {
        let id = Self::generate_id();
        let code = request.code.unwrap_or_else(|| Self::generate_code(None));

        let expires_at = request.expires_in_secs.map(|secs| {
            let expiry = chrono::Utc::now() + chrono::Duration::seconds(secs);
            expiry.format("%Y-%m-%d %H:%M:%S").to_string()
        });

        debug!("Creating invite code: {} ({})", code, id);

        sqlx::query(
            r#"
            INSERT INTO invite_codes (id, code, created_by, uses_remaining, max_uses, expires_at, note)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&code)
        .bind(created_by)
        .bind(request.max_uses)
        .bind(request.max_uses)
        .bind(&expires_at)
        .bind(&request.note)
        .execute(&self.pool)
        .await
        .context("Failed to insert invite code")?;

        self.get(&id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Invite code not found after creation"))
    }

    /// Create multiple invite codes at once.
    #[instrument(skip(self, created_by))]
    pub async fn create_batch(
        &self,
        count: u32,
        uses_per_code: i32,
        expires_in_secs: Option<i64>,
        prefix: Option<&str>,
        note: Option<&str>,
        created_by: &str,
    ) -> Result<Vec<InviteCode>> {
        let expires_at = expires_in_secs.map(|secs| {
            let expiry = chrono::Utc::now() + chrono::Duration::seconds(secs);
            expiry.format("%Y-%m-%d %H:%M:%S").to_string()
        });

        let mut codes = Vec::with_capacity(count as usize);

        for _ in 0..count {
            let id = Self::generate_id();
            let code = Self::generate_code(prefix);

            sqlx::query(
                r#"
                INSERT INTO invite_codes (id, code, created_by, uses_remaining, max_uses, expires_at, note)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&code)
            .bind(created_by)
            .bind(uses_per_code)
            .bind(uses_per_code)
            .bind(&expires_at)
            .bind(note)
            .execute(&self.pool)
            .await
            .context("Failed to insert invite code")?;

            if let Some(invite_code) = self.get(&id).await? {
                codes.push(invite_code);
            }
        }

        debug!("Created {} invite codes", codes.len());
        Ok(codes)
    }

    /// Get an invite code by ID.
    #[instrument(skip(self))]
    pub async fn get(&self, id: &str) -> Result<Option<InviteCode>> {
        let code = sqlx::query_as::<_, InviteCode>(
            r#"
            SELECT id, code, created_by, used_by, uses_remaining, max_uses,
                   expires_at, created_at, last_used_at, note
            FROM invite_codes
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch invite code")?;

        Ok(code)
    }

    /// Get an invite code by the code string.
    #[instrument(skip(self))]
    pub async fn get_by_code(&self, code: &str) -> Result<Option<InviteCode>> {
        let invite = sqlx::query_as::<_, InviteCode>(
            r#"
            SELECT id, code, created_by, used_by, uses_remaining, max_uses,
                   expires_at, created_at, last_used_at, note
            FROM invite_codes
            WHERE code = ?
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch invite code by code")?;

        Ok(invite)
    }

    /// List invite codes with optional filters.
    #[instrument(skip(self))]
    pub async fn list(&self, query: InviteCodeListQuery) -> Result<Vec<InviteCode>> {
        let limit = query.limit.unwrap_or(100);
        let offset = query.offset.unwrap_or(0);

        let mut sql = String::from(
            r#"
            SELECT id, code, created_by, used_by, uses_remaining, max_uses,
                   expires_at, created_at, last_used_at, note
            FROM invite_codes
            WHERE 1=1
            "#,
        );

        let mut bind_values: Vec<String> = Vec::new();

        if let Some(created_by) = &query.created_by {
            sql.push_str(" AND created_by = ?");
            bind_values.push(created_by.clone());
        }

        if let Some(valid) = query.valid {
            if valid {
                // Valid = has uses remaining AND (no expiry OR expiry in future)
                sql.push_str(" AND uses_remaining > 0 AND (expires_at IS NULL OR expires_at > datetime('now'))");
            } else {
                // Invalid = exhausted OR expired
                sql.push_str(" AND (uses_remaining <= 0 OR (expires_at IS NOT NULL AND expires_at <= datetime('now')))");
            }
        }

        sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

        let mut query_builder = sqlx::query_as::<_, InviteCode>(&sql);

        for value in &bind_values {
            query_builder = query_builder.bind(value);
        }

        query_builder = query_builder.bind(limit).bind(offset);

        let codes = query_builder
            .fetch_all(&self.pool)
            .await
            .context("Failed to list invite codes")?;

        Ok(codes)
    }

    /// Consume an invite code (decrement uses, record user).
    /// Returns the updated invite code if successful.
    #[instrument(skip(self))]
    pub async fn consume(&self, code: &str, user_id: &str) -> Result<InviteCode> {
        // First, get the code and validate it
        let invite = self
            .get_by_code(code)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Invite code not found"))?;

        if !invite.is_valid() {
            if invite.is_exhausted() {
                return Err(anyhow::anyhow!("Invite code has been fully used"));
            }
            if invite.is_expired() {
                return Err(anyhow::anyhow!("Invite code has expired"));
            }
            return Err(anyhow::anyhow!("Invite code is invalid"));
        }

        // Decrement uses and record user
        sqlx::query(
            r#"
            UPDATE invite_codes
            SET uses_remaining = uses_remaining - 1,
                used_by = ?,
                last_used_at = datetime('now')
            WHERE id = ? AND uses_remaining > 0
            "#,
        )
        .bind(user_id)
        .bind(&invite.id)
        .execute(&self.pool)
        .await
        .context("Failed to consume invite code")?;

        self.get(&invite.id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Invite code not found after consumption"))
    }

    /// Validate an invite code without consuming it.
    #[instrument(skip(self))]
    pub async fn validate(&self, code: &str) -> Result<bool> {
        let invite = self.get_by_code(code).await?;
        Ok(invite.map(|i| i.is_valid()).unwrap_or(false))
    }

    /// Revoke an invite code (set uses_remaining to 0).
    #[instrument(skip(self))]
    pub async fn revoke(&self, id: &str) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE invite_codes
            SET uses_remaining = 0
            WHERE id = ?
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to revoke invite code")?;

        if result.rows_affected() == 0 {
            return Err(anyhow::anyhow!("Invite code not found: {}", id));
        }

        Ok(())
    }

    /// Delete an invite code.
    #[instrument(skip(self))]
    pub async fn delete(&self, id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM invite_codes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete invite code")?;

        if result.rows_affected() == 0 {
            return Err(anyhow::anyhow!("Invite code not found: {}", id));
        }

        Ok(())
    }

    /// Count total invite codes.
    #[instrument(skip(self))]
    pub async fn count(&self) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM invite_codes")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count invite codes")?;

        Ok(count.0)
    }

    /// Count valid (still usable) invite codes.
    #[instrument(skip(self))]
    pub async fn count_valid(&self) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM invite_codes
            WHERE uses_remaining > 0
            AND (expires_at IS NULL OR expires_at > datetime('now'))
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to count valid invite codes")?;

        Ok(count.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create users table first (for foreign key)
        sqlx::query(
            r#"
            CREATE TABLE users (
                id TEXT PRIMARY KEY NOT NULL,
                username TEXT UNIQUE NOT NULL,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT,
                display_name TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'user',
                is_active BOOLEAN NOT NULL DEFAULT TRUE,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert a test admin user
        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, display_name, role)
            VALUES ('usr_admin', 'admin', 'admin@test.com', 'Admin', 'admin')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Create invite_codes table
        sqlx::query(
            r#"
            CREATE TABLE invite_codes (
                id TEXT PRIMARY KEY NOT NULL,
                code TEXT UNIQUE NOT NULL,
                created_by TEXT NOT NULL REFERENCES users(id),
                used_by TEXT REFERENCES users(id),
                uses_remaining INTEGER NOT NULL DEFAULT 1,
                max_uses INTEGER NOT NULL DEFAULT 1,
                expires_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_used_at TEXT,
                note TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_invite_code() {
        let pool = setup_test_db().await;
        let repo = InviteCodeRepository::new(pool);

        let request = CreateInviteCodeRequest {
            code: Some("TEST123".to_string()),
            max_uses: 5,
            expires_in_secs: None,
            note: Some("Test code".to_string()),
        };

        let code = repo.create(request, "usr_admin").await.unwrap();
        assert_eq!(code.code, "TEST123");
        assert_eq!(code.max_uses, 5);
        assert_eq!(code.uses_remaining, 5);
        assert!(code.is_valid());

        // Fetch by ID
        let fetched = repo.get(&code.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, code.id);

        // Fetch by code
        let by_code = repo.get_by_code("TEST123").await.unwrap().unwrap();
        assert_eq!(by_code.id, code.id);
    }

    #[tokio::test]
    async fn test_consume_invite_code() {
        let pool = setup_test_db().await;
        let repo = InviteCodeRepository::new(pool.clone());

        // Create a test user to consume the code
        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, display_name, role)
            VALUES ('usr_test', 'testuser', 'test@test.com', 'Test User', 'user')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let request = CreateInviteCodeRequest {
            code: Some("CONSUME1".to_string()),
            max_uses: 2,
            expires_in_secs: None,
            note: None,
        };

        let code = repo.create(request, "usr_admin").await.unwrap();
        assert_eq!(code.uses_remaining, 2);

        // Consume once
        let consumed = repo.consume("CONSUME1", "usr_test").await.unwrap();
        assert_eq!(consumed.uses_remaining, 1);
        assert!(consumed.is_valid());

        // Consume again
        let consumed2 = repo.consume("CONSUME1", "usr_test").await.unwrap();
        assert_eq!(consumed2.uses_remaining, 0);
        assert!(!consumed2.is_valid());

        // Third attempt should fail
        let result = repo.consume("CONSUME1", "usr_test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_batch_create() {
        let pool = setup_test_db().await;
        let repo = InviteCodeRepository::new(pool);

        let codes = repo
            .create_batch(5, 1, None, Some("BATCH"), Some("Batch test"), "usr_admin")
            .await
            .unwrap();

        assert_eq!(codes.len(), 5);
        for code in &codes {
            assert!(code.code.starts_with("BATCH-"));
            assert_eq!(code.uses_remaining, 1);
        }
    }

    #[tokio::test]
    async fn test_revoke_invite_code() {
        let pool = setup_test_db().await;
        let repo = InviteCodeRepository::new(pool);

        let request = CreateInviteCodeRequest {
            code: Some("REVOKE1".to_string()),
            max_uses: 10,
            expires_in_secs: None,
            note: None,
        };

        let code = repo.create(request, "usr_admin").await.unwrap();
        assert!(code.is_valid());

        repo.revoke(&code.id).await.unwrap();

        let revoked = repo.get(&code.id).await.unwrap().unwrap();
        assert_eq!(revoked.uses_remaining, 0);
        assert!(!revoked.is_valid());
    }

    #[tokio::test]
    async fn test_validate_code() {
        let pool = setup_test_db().await;
        let repo = InviteCodeRepository::new(pool);

        let request = CreateInviteCodeRequest {
            code: Some("VALID1".to_string()),
            max_uses: 1,
            expires_in_secs: None,
            note: None,
        };

        repo.create(request, "usr_admin").await.unwrap();

        // Valid code
        assert!(repo.validate("VALID1").await.unwrap());

        // Invalid code
        assert!(!repo.validate("NONEXISTENT").await.unwrap());
    }
}

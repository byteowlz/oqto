use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};

use super::{ApiKeyAuthUser, ApiKeyListItem, parse_timestamp};

#[derive(Debug, Clone, FromRow)]
struct ApiKeyRow {
    id: String,
    user_id: String,
    name: String,
    key_prefix: String,
    key_hash: String,
    scopes: String,
    last_used_at: Option<String>,
    expires_at: Option<String>,
    created_at: String,
    revoked_at: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct ApiKeyAuthRow {
    id: String,
    user_id: String,
    name: String,
    key_prefix: String,
    key_hash: String,
    scopes: String,
    last_used_at: Option<String>,
    expires_at: Option<String>,
    created_at: String,
    revoked_at: Option<String>,
    email: String,
    display_name: String,
    role: String,
}

#[derive(Debug, Clone)]
pub struct ApiKeyRepository {
    pool: SqlitePool,
}

pub type ApiKeyStoreResult<T> = Result<T>;

#[derive(Debug)]
pub struct ApiKeyStoreError;

impl ApiKeyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_for_user(&self, user_id: &str) -> ApiKeyStoreResult<Vec<ApiKeyListItem>> {
        let rows = sqlx::query_as::<_, ApiKeyRow>(
            r#"SELECT id, user_id, name, key_prefix, key_hash, scopes, last_used_at, expires_at, created_at, revoked_at
               FROM api_keys WHERE user_id = ? ORDER BY created_at DESC"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("list api keys")?;

        Ok(rows.into_iter().map(map_row_to_list_item).collect())
    }

    pub async fn create_key(
        &self,
        user_id: &str,
        name: &str,
        key_prefix: &str,
        key_hash: &str,
        scopes: Vec<String>,
        expires_at: Option<String>,
    ) -> ApiKeyStoreResult<ApiKeyListItem> {
        let id = uuid::Uuid::new_v4().to_string();
        let scopes_json = serde_json::to_string(&scopes).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"INSERT INTO api_keys (id, user_id, name, key_prefix, key_hash, scopes, expires_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(user_id)
        .bind(name)
        .bind(key_prefix)
        .bind(key_hash)
        .bind(&scopes_json)
        .bind(&expires_at)
        .execute(&self.pool)
        .await
        .context("insert api key")?;

        let row_result = sqlx::query_as::<_, ApiKeyRow>(
            r#"SELECT id, user_id, name, key_prefix, key_hash, scopes, last_used_at, expires_at, created_at, revoked_at
               FROM api_keys WHERE id = ?"#,
        )
        .bind(&id)
        .fetch_optional(&self.pool)
        .await;

        match row_result {
            Ok(Some(row)) => Ok(map_row_to_list_item(row)),
            Ok(None) => {
                tracing::warn!("created api key row not found after insert");
                Ok(fallback_created_key(
                    id, name, key_prefix, scopes, expires_at,
                ))
            }
            Err(error) => {
                tracing::error!(?error, "failed to fetch created api key row");
                Ok(fallback_created_key(
                    id, name, key_prefix, scopes, expires_at,
                ))
            }
        }
    }

    pub async fn revoke_by_name(&self, user_id: &str, name: &str) -> ApiKeyStoreResult<u64> {
        let result = sqlx::query(
            r#"UPDATE api_keys SET revoked_at = datetime('now')
               WHERE user_id = ? AND name = ? AND revoked_at IS NULL"#,
        )
        .bind(user_id)
        .bind(name)
        .execute(&self.pool)
        .await
        .context("revoke api keys by name")?;

        Ok(result.rows_affected())
    }

    pub async fn revoke_key(&self, user_id: &str, key_id: &str) -> ApiKeyStoreResult<bool> {
        let result = sqlx::query(
            r#"UPDATE api_keys SET revoked_at = datetime('now')
               WHERE id = ? AND user_id = ? AND revoked_at IS NULL"#,
        )
        .bind(key_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .context("revoke api key")?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_key(&self, user_id: &str, key_id: &str) -> ApiKeyStoreResult<bool> {
        let result = sqlx::query("DELETE FROM api_keys WHERE id = ? AND user_id = ?")
            .bind(key_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .context("delete api key")?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn find_auth_user_by_hash(
        &self,
        key_hash: &str,
    ) -> ApiKeyStoreResult<Option<ApiKeyAuthUser>> {
        let row = sqlx::query_as::<_, ApiKeyAuthRow>(
            r#"SELECT k.id, k.user_id, k.name, k.key_prefix, k.key_hash, k.scopes, k.last_used_at,
                      k.expires_at, k.created_at, k.revoked_at, u.email, u.display_name, u.role
               FROM api_keys k
               JOIN users u ON u.id = k.user_id
               WHERE k.key_hash = ? AND k.revoked_at IS NULL
               LIMIT 1"#,
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await
        .context("get api key auth user")?;

        let Some(row) = row else {
            return Ok(None);
        };

        if let Some(ref expires_at) = row.expires_at {
            if let Some(expiry) = parse_timestamp(expires_at) {
                if expiry < chrono::Utc::now() {
                    return Ok(None);
                }
            }
        }

        Ok(Some(ApiKeyAuthUser {
            key_id: row.id,
            user_id: row.user_id,
            email: row.email,
            display_name: row.display_name,
            role: row.role,
            expires_at: row.expires_at,
        }))
    }

    pub async fn touch_last_used(&self, key_id: &str) -> ApiKeyStoreResult<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = datetime('now') WHERE id = ?")
            .bind(key_id)
            .execute(&self.pool)
            .await
            .context("touch api key last_used_at")?;
        Ok(())
    }
}

fn map_row_to_list_item(row: ApiKeyRow) -> ApiKeyListItem {
    ApiKeyListItem {
        id: row.id,
        name: row.name,
        key_prefix: row.key_prefix,
        scopes: parse_scopes(&row.scopes),
        last_used_at: row.last_used_at,
        expires_at: row.expires_at,
        created_at: row.created_at,
        revoked_at: row.revoked_at,
    }
}

fn parse_scopes(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .map(|values| {
            values
                .into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn fallback_created_key(
    id: String,
    name: &str,
    key_prefix: &str,
    scopes: Vec<String>,
    expires_at: Option<String>,
) -> ApiKeyListItem {
    ApiKeyListItem {
        id,
        name: name.to_string(),
        key_prefix: key_prefix.to_string(),
        scopes,
        last_used_at: None,
        expires_at,
        created_at: chrono::Utc::now().to_rfc3339(),
        revoked_at: None,
    }
}

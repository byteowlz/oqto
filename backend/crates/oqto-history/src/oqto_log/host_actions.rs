//! Canonical, metadata-only host action records. These are not chat Sessions,
//! Pi JSONL records or a second grant authority. Only trusted host composition
//! supplies the Account and workspace partition; paths do not authenticate.

use anyhow::{Context, Result, ensure};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostActionRecord {
    pub event_id: String,
    pub account_id: String,
    pub action: String,
    pub connection_id: String,
    pub connection_revision: Option<u32>,
    pub instance_id: Option<String>,
    pub slot: Option<String>,
    pub content_digest: Option<String>,
}

/// No arbitrary JSON, request paths/bodies, credential references or values.
/// Duplicate identities fail, rather than mutating an existing audit record.
pub async fn append(user_home: &Path, workspace: &str, record: &HostActionRecord) -> Result<()> {
    validate(record)?;
    let pool = super::store::open_workspace_pool(user_home, workspace).await?;
    insert(&pool, record).await
}

fn validate(record: &HostActionRecord) -> Result<()> {
    for value in [&record.event_id, &record.account_id, &record.connection_id] {
        ensure!(
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')),
            "invalid host action identity"
        );
    }
    ensure!(
        matches!(
            record.action.as_str(),
            "create" | "update" | "rotate" | "revoke" | "delete" | "bind" | "unbind" | "invoke"
        ),
        "unsupported host action"
    );
    for value in [&record.instance_id, &record.slot, &record.content_digest]
        .into_iter()
        .flatten()
    {
        ensure!(
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')),
            "invalid host action metadata"
        );
    }
    Ok(())
}

async fn insert(pool: &sqlx::SqlitePool, record: &HostActionRecord) -> Result<()> {
    sqlx::query("INSERT INTO host_action_audit(event_id,account_id,action,connection_id,connection_revision,instance_id,slot,content_digest,recorded_at_ms) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(&record.event_id).bind(&record.account_id).bind(&record.action)
        .bind(&record.connection_id).bind(record.connection_revision.map(i64::from))
        .bind(&record.instance_id).bind(&record.slot).bind(&record.content_digest)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(pool).await.context("append canonical host action audit")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn durable_identity_is_append_only_and_input_is_bounded() -> Result<()> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!(
            "../../migrations_oqto_log/20261003001_host_action_audit.sql"
        ))
        .execute(&pool)
        .await?;
        let mut record = HostActionRecord {
            event_id: "conn_evt_fixture".into(),
            account_id: "account_fixture".into(),
            action: "invoke".into(),
            connection_id: "conn_fixture".into(),
            connection_revision: Some(1),
            instance_id: Some("instance_fixture".into()),
            slot: Some("analytics".into()),
            content_digest: Some("fixture_digest".into()),
        };
        validate(&record)?;
        insert(&pool, &record).await?;
        assert!(insert(&pool, &record).await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM host_action_audit")
            .fetch_one(&pool)
            .await?;
        assert_eq!(count, 1);
        record.action = "arbitrary-secret-payload".into();
        assert!(validate(&record).is_err());
        record.action = "invoke".into();
        record.slot = Some("x".repeat(129));
        assert!(validate(&record).is_err());
        Ok(())
    }
}

//! Images only from the selected canonical conversation-message content.
//! The caller authenticates the principal; the Workspace ceiling is mandatory.
//! Raw extension/control envelopes are deliberately not a media source.
use anyhow::{Context, Result, ensure};
use futures::TryStreamExt;
use oqto_protocol::projection::MediaWorkspaceScope;
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::Path;
const MESSAGES: &str = "SELECT m.message_id AS row_id,m.json_payload AS payload_json FROM oqto_log_messages m JOIN oqto_log_turns t ON t.turn_id=m.turn_id WHERE t.session_id=? AND m.json_payload IS NOT NULL";
#[cfg(test)]
mod fts_tests;
#[cfg(test)]
mod tests;
type Binding = (String, String, Option<String>, Option<String>);
pub struct Snapshot {
    bytes: Vec<u8>,
    pool: sqlx::SqlitePool,
    binding: Binding,
    public: String,
    id: String,
    digest: String,
}
impl Snapshot {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn workspace(&self) -> Option<&str> {
        self.binding.2.as_deref()
    }
    pub async fn validate(&self) -> Result<()> {
        let fresh:Option<Binding>=sqlx::query_as("SELECT session_id,user_id,workspace_id,external_id FROM oqto_log_sessions WHERE platform_id=?").bind(&self.public).fetch_optional(&self.pool).await?;
        ensure!(
            fresh.as_ref() == Some(&self.binding),
            "canonical Session binding changed"
        );
        let payload: Option<String> = sqlx::query_scalar(&format!(
            "SELECT payload_json FROM ({MESSAGES}) WHERE row_id=?"
        ))
        .bind(&self.binding.0)
        .bind(&self.id)
        .fetch_optional(&self.pool)
        .await?;
        ensure!(
            payload.is_some_and(|p| hex::encode(Sha256::digest(p.as_bytes())) == self.digest),
            "canonical media record changed"
        );
        Ok(())
    }
}
pub async fn original(
    home: &Path,
    public: &str,
    hash: &str,
    scope: &MediaWorkspaceScope,
) -> Result<Snapshot> {
    ensure!(
        !public.is_empty()
            && public.len() <= 256
            && !public.starts_with("pending-")
            && !public.starts_with("tmp:"),
        "invalid public Session identity"
    );
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid media hash"
    );
    let mut selected = None;
    for (index, db) in super::list_db_paths(home).into_iter().enumerate() {
        ensure!(index < 1024, "media shard scan exceeds bound");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(db)
                    .read_only(true)
                    .create_if_missing(false),
            )
            .await?;
        let rows:Vec<Binding>=sqlx::query_as("SELECT session_id,user_id,workspace_id,external_id FROM oqto_log_sessions WHERE platform_id=?").bind(public).fetch_all(&pool).await?;
        ensure!(rows.len() <= 1, "ambiguous canonical Session identity");
        if let Some(binding) = rows.into_iter().next() {
            ensure!(selected.is_none(), "ambiguous public Session identity");
            selected = Some((pool, binding));
        } else {
            pool.close().await;
        }
    }
    let (pool, binding) = selected.context("canonical Session unavailable")?;
    ensure!(
        binding.3.as_deref() != Some(public),
        "native identity is not a public Session identity"
    );
    // Scope rejection precedes reading/parsing image records, including cache hits.
    ensure!(
        scope.permits(binding.2.as_deref()),
        "Session media outside Workspace authority"
    );
    let sizes:(Option<i64>,Option<i64>)=sqlx::query_as(&format!("SELECT SUM(length(CAST(payload_json AS BLOB))),MAX(length(CAST(payload_json AS BLOB))) FROM ({MESSAGES})")).bind(&binding.0).fetch_one(&pool).await?;
    ensure!(
        sizes.0.unwrap_or(0) <= 512 * 1024 * 1024 && sizes.1.unwrap_or(0) <= 16 * 1024 * 1024,
        "canonical media source exceeds bound"
    );
    let mut rows = sqlx::query_as::<_, (String, String)>(MESSAGES)
        .bind(&binding.0)
        .fetch(&pool);
    let mut found = None;
    while let Some((id, payload)) = rows.try_next().await? {
        let value: serde_json::Value = serde_json::from_str(&payload)?;
        // The history projector reads these message bodies, not private extras.
        let content = value
            .get("content")
            .or_else(|| value.get("parts"))
            .unwrap_or(&serde_json::Value::Null);
        if let Some(bytes) = oqto_media::inline::find_image(content, hash)? {
            found = Some((id, hex::encode(Sha256::digest(payload.as_bytes())), bytes));
            break;
        }
    }
    drop(rows);
    let (id, digest, bytes) =
        found.context("media not found in selected canonical Session content")?;
    oqto_media::inline::describe(&bytes)?;
    let snapshot = Snapshot {
        bytes,
        pool,
        binding,
        public: public.into(),
        id,
        digest,
    };
    snapshot.validate().await?;
    Ok(snapshot)
}

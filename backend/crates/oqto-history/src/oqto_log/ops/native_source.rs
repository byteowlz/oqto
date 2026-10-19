//! Strict, non-repairing identity bootstrap for the serialized native writer.
//! Source identity is durable and immutable. Reads never create/migrate stores.
use super::*;
use anyhow::ensure;
use sqlx::Row;

struct Identity {
    platform: String,
    external: Option<String>,
    workspace: String,
    source: Option<String>,
}

async fn identities(home: &Path, public: &str, external: &str) -> Result<Vec<Identity>> {
    let mut found = Vec::new();
    for db in list_db_paths(home) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db)
                    .read_only(true)
                    .create_if_missing(false),
            )
            .await
            .context("read native identity store")?;
        let rows = sqlx::query("SELECT platform_id, external_id, workspace_id, json_extract(extensions_json, '$.personal_native_source') AS source FROM oqto_log_sessions WHERE external_id=? OR platform_id=? OR session_id=?")
            .bind(external).bind(public).bind(public).fetch_all(&pool).await?;
        let mut connection = pool.acquire().await?;
        if let Some(binding) =
            crate::oqto_log::bindings::resolve_pi_session_identity(&mut connection, external)
                .await?
        {
            ensure!(
                rows.len() == 1
                    && rows[0].try_get::<String, _>("platform_id")? == binding.platform_id
                    && rows[0]
                        .try_get::<Option<String>, _>("external_id")?
                        .as_deref()
                        == Some(external),
                "Native binding facts conflict"
            );
        }
        drop(connection);
        for row in rows {
            found.push(Identity {
                platform: row.try_get("platform_id")?,
                external: row.try_get("external_id")?,
                workspace: row.try_get("workspace_id")?,
                source: row.try_get("source")?,
            });
        }
        pool.close().await;
    }
    Ok(found)
}

/// Only the existing serialized writer calls this. Existing identities/owners,
/// turns, branches and Fork provenance are never replaced or rebound.
/// Returns true when a new binding was committed (catalog invalidation needed).
pub async fn discover(
    home: &Path,
    user: &str,
    workspace: &str,
    external: &str,
    source: &str,
) -> Result<bool> {
    let public = crate::oqto_log::store::platform_id_for_external_id(external);
    let found = identities(home, &public, external).await?;
    ensure!(found.len() <= 1, "Ambiguous native canonical identity");
    if let Some(row) = found.first() {
        ensure!(
            row.external.as_deref() == Some(external)
                && row.platform.starts_with("oqto-")
                && row.platform != external
                && row.workspace == workspace
                && row.source.as_deref().is_none_or(|pin| pin == source),
            "Native source rebind denied"
        );
        if row.source.is_some() {
            return Ok(false);
        }
    }
    let db = crate::oqto_log::paths::resolve_user_home_workspace_db_path(home, workspace)?;
    let pool = open_migrated_pool(&db, "native source discovery").await?;
    let mut tx = pool.begin().await?;
    let resolved =
        crate::oqto_log::bindings::resolve_pi_session_identity(&mut tx, external).await?;
    let (session, public) = resolved
        .map(|row| (row.session_id, row.platform_id))
        .unwrap_or_else(|| (public.clone(), public));
    ensure!(
        public.starts_with("oqto-") && public != external,
        "Canonical public native identity required"
    );
    sqlx::query("INSERT OR IGNORE INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id) VALUES(?,?,?,?,?)")
        .bind(&session).bind(&public).bind(external).bind(user).bind(workspace).execute(&mut *tx).await?;
    // Recheck in the transaction, including a potential same-id competing writer.
    let row = sqlx::query("SELECT platform_id,external_id,workspace_id,json_extract(extensions_json,'$.personal_native_source') AS source FROM oqto_log_sessions WHERE session_id=?")
        .bind(&session).fetch_one(&mut *tx).await?;
    let pin: Option<String> = row.try_get("source")?;
    ensure!(
        row.try_get::<String, _>("platform_id")? == public
            && row.try_get::<String, _>("external_id")? == external
            && row.try_get::<String, _>("workspace_id")? == workspace
            && pin.as_deref().is_none_or(|pin| pin == source),
        "Native identity conflict"
    );
    sqlx::query("UPDATE oqto_log_sessions SET extensions_json=json_set(COALESCE(extensions_json,'{}'),'$.personal_native_source',?) WHERE session_id=?")
        .bind(source).bind(&session).execute(&mut *tx).await?;
    append_pi_session_binding(
        &mut tx,
        &public,
        Some(external),
        "personal-native-discovery",
    )
    .await?;
    sqlx::query("INSERT OR IGNORE INTO oqto_log_branches(branch_id,session_id) VALUES(?,?)")
        .bind(format!("branch:{session}:main"))
        .bind(&session)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    pool.close().await;
    crate::oqto_log::index::upsert_for_workspace_id(
        home,
        workspace,
        &session,
        Some(&public),
        Some(external),
    )
    .await?;
    Ok(true)
}

pub async fn is_bound(home: &Path, external: &str) -> Result<bool> {
    // This guard also runs before the retained legacy writer's migrations.
    // An old store without extensions cannot contain a new source pin; do not
    // let an unrelated pre-binding schema suppress default-source ingestion.
    for db in list_db_paths(home) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(db)
                    .read_only(true)
                    .create_if_missing(false),
            )
            .await?;
        let has_extensions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('oqto_log_sessions') WHERE name='extensions_json'")
            .fetch_one(&pool).await?;
        let bound = if has_extensions == 0 {
            false
        } else {
            sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM oqto_log_sessions WHERE (external_id=? OR platform_id=?) AND json_type(extensions_json,'$.personal_native_source')='text')")
                .bind(external).bind(crate::oqto_log::store::platform_id_for_external_id(external)).fetch_one(&pool).await?
        };
        pool.close().await;
        if bound {
            return Ok(true);
        }
    }
    Ok(false)
}

pub async fn validate(
    home: &Path,
    public: &str,
    external: &str,
    workspace: &str,
    source: &str,
) -> Result<()> {
    let found = identities(home, public, external).await?;
    ensure!(
        found.len() == 1,
        "Native canonical identity unavailable or ambiguous"
    );
    let row = &found[0];
    ensure!(
        row.platform == public
            && row.external.as_deref() == Some(external)
            && row.workspace == workspace
            && row.source.as_deref() == Some(source),
        "Native source binding mismatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn legacy_schema_without_source_pins_does_not_suppress_default_writer() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let db = crate::oqto_log::paths::resolve_user_home_workspace_db_path(
            temp.path(),
            "/synthetic/legacy",
        )?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db)
                    .create_if_missing(true),
            )
            .await?;
        sqlx::query(
            "CREATE TABLE oqto_log_sessions(session_id TEXT,platform_id TEXT,external_id TEXT)",
        )
        .execute(&pool)
        .await?;
        pool.close().await;
        let before = std::fs::read(&db)?;
        assert!(!is_bound(temp.path(), "native").await?);
        assert_eq!(std::fs::read(&db)?, before);
        Ok(())
    }
}

//! Strict, non-repairing identity bootstrap for the serialized native writer.
//! Reads never create/migrate stores or retain ownership answers.
use super::*;
use anyhow::ensure;
use sqlx::Row;
use std::time::Duration;
#[cfg(test)]
mod inventory_tests;
pub(super) mod reader;
#[cfg(test)]
mod resource_tests;

const SCAN_DEADLINE: Duration = Duration::from_secs(5);
// Bound native inventory's blocking metadata work across validators/homes too.
// The permit lives inside the blocking job, including if its caller cancels.
static INVENTORY_SCANNER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Debug, thiserror::Error)]
#[error("Native identity authority unavailable")]
pub struct AuthorityUnavailable(#[source] anyhow::Error);

pub fn authority_unavailable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<AuthorityUnavailable>().is_some()
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct IdentityConflict(&'static str);

struct Identity {
    platform: String,
    external: Option<String>,
    workspace: String,
    source: Option<String>,
}

// Unlike the general operational catalog iterator, this authority iterator must
// never flatten directory/stat errors into a silently incomplete inventory.
async fn identity_paths(home: &Path) -> Result<Vec<PathBuf>> {
    let root = home.join(".local/share/oqto/oqto-log");
    let permit = INVENTORY_SCANNER.acquire().await?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry?;
            // WAL/SHM accelerator files may retire after readdir. They are not
            // shard authority and must not be restatted as though they were.
            // Keep symlinks on the strict metadata path: a missing directory or
            // broken link must still refuse the complete inventory.
            let kind = entry.file_type()?;
            #[cfg(test)]
            inventory_tests::after_entry(&entry)?;
            if !kind.is_dir() && !kind.is_symlink() {
                continue;
            }
            if !std::fs::metadata(entry.path())?.is_dir() {
                continue;
            }
            let db = entry.path().join("oqto-log.sqlite");
            match std::fs::metadata(&db) {
                Ok(metadata) => {
                    ensure!(metadata.is_file(), "Invalid native identity shard");
                    paths.push(db);
                }
                // A canonical shard directory without a readable database is
                // incomplete authority, not an empty shard we may silently skip.
                Err(error) => return Err(error.into()),
            }
        }
        paths.sort();
        Ok(paths)
    })
    .await?
}

async fn probe(path: &Path) -> Result<()> {
    let mut shard = reader::ReadShard::open(path).await?;
    let columns =
        sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('oqto_log_sessions')")
            .fetch_all(shard.connection())
            .await?;
    ensure!(
        columns.iter().any(|column| column == "external_id"),
        "Missing native identity schema"
    );
    shard.finish().await
}

async fn shard_identities(path: &Path, public: &str, external: &str) -> Result<Vec<Identity>> {
    let mut shard = reader::ReadShard::open(path).await?;
    let connection = shard.connection();
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('oqto_log_sessions')")
            .fetch_all(&mut *connection)
            .await?;
    let source_expr = if columns.iter().any(|column| column == "extensions_json") {
        "json_extract(extensions_json, '$.personal_native_source')"
    } else {
        "NULL"
    };
    let query = format!(
        "SELECT platform_id, external_id, workspace_id, {source_expr} AS source FROM oqto_log_sessions WHERE external_id=? OR platform_id=? OR session_id=?"
    );
    let rows = sqlx::query(&query)
        .bind(external)
        .bind(public)
        .bind(public)
        .fetch_all(&mut *connection)
        .await?;
    let has_bindings: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM native_identity.sqlite_master WHERE type='table' AND name='oqto_log_session_bindings')")
        .fetch_one(&mut *connection).await?;
    if has_bindings {
        let binding = crate::oqto_log::bindings::resolve_pi_session_identity(connection, external)
            .await
            .map_err(|error| match error {
                SessionBindingError::Storage(_) => anyhow::Error::new(error),
                _ => anyhow::Error::new(IdentityConflict("Native binding facts conflict")),
            })?;
        if let Some(binding) = binding
            && (rows.len() != 1
                || rows[0].try_get::<String, _>("platform_id")? != binding.platform_id
                || rows[0]
                    .try_get::<Option<String>, _>("external_id")?
                    .as_deref()
                    != Some(external))
        {
            return Err(IdentityConflict("Native binding facts conflict").into());
        }
    }
    let found = rows
        .into_iter()
        .map(|row| {
            Ok(Identity {
                platform: row.try_get("platform_id")?,
                external: row.try_get("external_id")?,
                workspace: row.try_get("workspace_id")?,
                source: row.try_get("source")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    shard.finish().await?;
    Ok(found)
}

async fn identities(home: &Path, public: &str, external: &str) -> Result<Vec<Identity>> {
    let operation = async {
        use futures::{StreamExt, TryStreamExt, stream};
        // Use the existing TWO process-wide physical reader owners concurrently.
        // No identity/schema cache: every prepare/commit still enumerates and
        // reads every current shard, and any failure discards the whole result.
        // Cancellation safety remains owned by ReadShard's fixed worker slots.
        let shards: Vec<Vec<Identity>> = stream::iter(identity_paths(home).await?)
            .map(|path| async move { shard_identities(&path, public, external).await })
            .buffer_unordered(2)
            .try_collect()
            .await?;
        Ok::<_, anyhow::Error>(shards.into_iter().flatten().collect())
    };
    match tokio::time::timeout(SCAN_DEADLINE, operation).await {
        Ok(Ok(found)) => Ok(found),
        Ok(Err(error)) if error.downcast_ref::<IdentityConflict>().is_some() => Err(error),
        Ok(Err(error)) => Err(AuthorityUnavailable(error).into()),
        Err(error) => Err(AuthorityUnavailable(error.into()).into()),
    }
}

/// Completed read-only preflight request, not ownership evidence. No identity
/// answers survive preparation; commit always re-enumerates/reads current facts.
pub struct PreparedDiscovery {
    home: PathBuf,
    external: String,
}

/// No resident per-shard pools or schema/ownership cache. Every prepare enumerates
/// and queries ALL current shards via the process-wide bounded read workers.
/// An unavailable inventory/post-write verification poisons this whole batch.
pub struct DiscoveryBatch {
    home: PathBuf,
    poisoned: bool,
    #[cfg(test)]
    after_write: Option<Box<dyn FnOnce() + Send>>,
}
impl DiscoveryBatch {
    pub async fn open(home: &Path) -> Result<Self> {
        let operation = async {
            for path in identity_paths(home).await? {
                probe(&path).await?;
            }
            Ok::<_, anyhow::Error>(())
        };
        tokio::time::timeout(SCAN_DEADLINE, operation)
            .await
            .map_err(|error| AuthorityUnavailable(error.into()))?
            .map_err(AuthorityUnavailable)?;
        Ok(Self {
            home: home.to_owned(),
            poisoned: false,
            #[cfg(test)]
            after_write: None,
        })
    }
    async fn read_facts(&mut self, external: &str) -> Result<Vec<Identity>> {
        if self.poisoned {
            return Err(AuthorityUnavailable(anyhow::anyhow!("Batch poisoned")).into());
        }
        let public = crate::oqto_log::store::platform_id_for_external_id(external);
        let result = identities(&self.home, &public, external).await;
        if let Err(error) = &result {
            self.poisoned |= authority_unavailable(error);
        }
        result
    }
    pub async fn prepare(&mut self, external: &str) -> Result<PreparedDiscovery> {
        let found = self.read_facts(external).await?;
        ensure!(found.len() <= 1, "Ambiguous native canonical identity");
        Ok(PreparedDiscovery {
            home: self.home.clone(),
            external: external.to_owned(),
        })
    }
    pub async fn commit(
        &mut self,
        user: &str,
        workspace: &str,
        source: &str,
        prepared: PreparedDiscovery,
    ) -> Result<bool> {
        if self.poisoned {
            return Err(AuthorityUnavailable(anyhow::anyhow!("Batch poisoned")).into());
        }
        ensure!(
            prepared.home == self.home,
            "Wrong native inventory authority"
        );
        // Never adopt from a preflight snapshot, even if a caller retained its
        // request while a different writer or validator changed authority.
        let found = self.read_facts(&prepared.external).await?;
        let result = discover_identity(
            &self.home,
            user,
            workspace,
            &prepared.external,
            source,
            &found,
        )
        .await;
        if let Err(error) = &result {
            self.poisoned |= error
                .chain()
                .any(|cause| cause.is::<sqlx::Error>() || cause.is::<std::io::Error>());
        }
        if result.as_ref().is_ok_and(|changed| *changed) {
            #[cfg(test)]
            if let Some(fault) = self.after_write.take() {
                fault();
            }
            let path =
                crate::oqto_log::paths::existing_user_home_workspace_db_path(&self.home, workspace);
            if let Err(error) = probe(&path).await {
                self.poisoned = true;
                return Err(AuthorityUnavailable(error).into());
            }
        }
        result.map_err(|error| {
            if self.poisoned {
                AuthorityUnavailable(error).into()
            } else {
                error
            }
        })
    }
    pub async fn discover(
        &mut self,
        user: &str,
        workspace: &str,
        external: &str,
        source: &str,
    ) -> Result<bool> {
        let prepared = self.prepare(external).await?;
        self.commit(user, workspace, source, prepared).await
    }
}

/// Existing serialized writer only; never invoked by list/attach.
pub async fn discover(
    home: &Path,
    user: &str,
    workspace: &str,
    external: &str,
    source: &str,
) -> Result<bool> {
    DiscoveryBatch::open(home)
        .await?
        .discover(user, workspace, external, source)
        .await
}

async fn discover_identity(
    home: &Path,
    user: &str,
    workspace: &str,
    external: &str,
    source: &str,
    found: &[Identity],
) -> Result<bool> {
    let public = crate::oqto_log::store::platform_id_for_external_id(external);
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
    let operation = async {
        for path in identity_paths(home).await? {
            let mut shard = reader::ReadShard::open(&path).await?;
            let connection = shard.connection();
            let has_extensions: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pragma_table_info('oqto_log_sessions') WHERE name='extensions_json')")
                .fetch_one(&mut *connection).await?;
            let bound = if has_extensions {
                sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM oqto_log_sessions WHERE (external_id=? OR platform_id=?) AND json_type(extensions_json,'$.personal_native_source')='text')")
                    .bind(external).bind(crate::oqto_log::store::platform_id_for_external_id(external))
                    .fetch_one(&mut *connection).await?
            } else {
                false
            };
            shard.finish().await?;
            if bound {
                return Ok::<_, anyhow::Error>(true);
            }
        }
        Ok(false)
    };
    tokio::time::timeout(SCAN_DEADLINE, operation)
        .await
        .map_err(|error| AuthorityUnavailable(error.into()))?
        .map_err(|error| AuthorityUnavailable(error).into())
}

/// `sources`: every pin encoding that proves today's source (current and legacy).
pub async fn validate(
    home: &Path,
    public: &str,
    external: &str,
    workspace: &str,
    sources: &[&str],
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
            && row
                .source
                .as_deref()
                .is_some_and(|pin| sources.contains(&pin)),
        "Native source binding mismatch"
    );
    Ok(())
}

/// A pinned native Session in one Workspace shard.
#[derive(Debug, Clone)]
pub struct NativePin {
    pub public: String,
    pub external: String,
    pub pin: String,
}

/// Read-only: every pinned native Session in a Workspace's shard.
pub async fn native_pins(home: &Path, workspace: &str) -> Result<Vec<NativePin>> {
    let db = crate::oqto_log::paths::existing_user_home_workspace_db_path(home, workspace);
    if !db.exists() {
        return Ok(Vec::new());
    }
    let mut shard = reader::ReadShard::open(&db).await?;
    let rows = sqlx::query(
        "SELECT platform_id, external_id, json_extract(extensions_json,'$.personal_native_source') AS pin \
         FROM native_identity.oqto_log_sessions WHERE workspace_id=? AND external_id IS NOT NULL \
         AND json_extract(extensions_json,'$.personal_native_source') IS NOT NULL",
    )
    .bind(workspace)
    .fetch_all(shard.connection())
    .await?;
    shard.finish().await?;
    rows.into_iter()
        .map(|row| {
            Ok(NativePin {
                public: row.try_get("platform_id")?,
                external: row.try_get("external_id")?,
                pin: row.try_get("pin")?,
            })
        })
        .collect()
}

/// Compare-and-swap one pin: changes nothing unless the row still holds `old`.
pub async fn repin(
    home: &Path,
    workspace: &str,
    public: &str,
    external: &str,
    old: &str,
    new: &str,
) -> Result<bool> {
    let db = crate::oqto_log::paths::existing_user_home_workspace_db_path(home, workspace);
    ensure!(db.exists(), "Native shard unavailable");
    let pool = open_migrated_pool(&db, "native pin repair").await?;
    let changed = sqlx::query(
        "UPDATE oqto_log_sessions SET extensions_json=json_set(extensions_json,'$.personal_native_source',?) \
         WHERE platform_id=? AND external_id=? AND workspace_id=? \
         AND json_extract(extensions_json,'$.personal_native_source')=?",
    )
    .bind(new)
    .bind(public)
    .bind(external)
    .bind(workspace)
    .bind(old)
    .execute(&pool)
    .await?
    .rows_affected();
    pool.close().await;
    ensure!(changed <= 1, "Ambiguous native pin");
    Ok(changed == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn discovery_batch_refreshes_upgraded_and_new_shards_without_rebinding() -> Result<()> {
        for prefix in [1, 3] {
            let temp = tempfile::tempdir()?;
            let workspace = "/synthetic/retained";
            let db = crate::oqto_log::paths::resolve_user_home_workspace_db_path(
                temp.path(),
                workspace,
            )?;
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    SqliteConnectOptions::new()
                        .filename(&db)
                        .create_if_missing(true),
                )
                .await?;
            let mut migrator = sqlx::migrate!("./migrations_oqto_log");
            migrator.migrations =
                std::borrow::Cow::Owned(migrator.iter().take(prefix).cloned().collect());
            migrator.run(&pool).await?;
            sqlx::query("INSERT INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id) VALUES('legacy-row','oqto-retained-public','retained-native','original-owner',?)")
                .bind(workspace).execute(&pool).await?;
            pool.close().await;
            let mut batch = DiscoveryBatch::open(temp.path()).await?;
            assert!(
                batch
                    .discover("new-owner", workspace, "retained-native", "pin")
                    .await?
            );
            assert!(
                !batch
                    .discover("new-owner", workspace, "retained-native", "pin")
                    .await?
            );
            assert!(
                batch
                    .discover("new-owner", "/synthetic/fresh", "fresh-native", "fresh-pin")
                    .await?
            );
            assert!(
                batch
                    .discover("new-owner", "/synthetic/other", "fresh-native", "fresh-pin")
                    .await
                    .is_err()
            );
            validate(
                temp.path(),
                "oqto-retained-public",
                "retained-native",
                workspace,
                &["pin"],
            )
            .await?;
            let rows = list_sessions(temp.path(), None).await?;
            let row = rows
                .iter()
                .find(|row| row.external_id.as_deref() == Some("retained-native"))
                .context("retained row")?;
            assert_eq!(row.session_id, "legacy-row");
            assert_eq!(row.platform_id, "oqto-retained-public");
            assert_eq!(row.user_id, "original-owner");
        }
        Ok(())
    }

    #[tokio::test]
    async fn retained_migration_prefix_conflicts_are_denied_without_writes() -> Result<()> {
        for prefix in [1, 3] {
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
            let mut migrator = sqlx::migrate!("./migrations_oqto_log");
            migrator.migrations =
                std::borrow::Cow::Owned(migrator.iter().take(prefix).cloned().collect());
            migrator.run(&pool).await?;
            for id in ["oqto-old-owner-a", "oqto-old-owner-b"] {
                sqlx::query("INSERT INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id) VALUES(?,?,'conflict','retained-owner','/synthetic/legacy')")
                    .bind(id).bind(id).execute(&pool).await?;
            }
            pool.close().await;
            let before = std::fs::read(&db)?;
            assert!(
                discover(
                    temp.path(),
                    "new-owner",
                    "/synthetic/fresh",
                    "conflict",
                    "pin"
                )
                .await
                .is_err()
            );
            assert!(
                validate(
                    temp.path(),
                    "oqto-old-owner-a",
                    "conflict",
                    "/synthetic/legacy",
                    &["pin"]
                )
                .await
                .is_err()
            );
            assert_eq!(std::fs::read(&db)?, before);
            assert!(
                !crate::oqto_log::paths::existing_user_home_workspace_db_path(
                    temp.path(),
                    "/synthetic/fresh"
                )
                .exists()
            );
        }
        Ok(())
    }

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

use super::*;

pub(super) async fn prefix_shard(home: &Path, workspace: &str, prefix: usize) -> Result<PathBuf> {
    let db = crate::oqto_log::paths::resolve_user_home_workspace_db_path(home, workspace)?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db)
                .create_if_missing(true),
        )
        .await?;
    let mut migrator = sqlx::migrate!("./migrations_oqto_log");
    migrator.migrations = std::borrow::Cow::Owned(migrator.iter().take(prefix).cloned().collect());
    migrator.run(&pool).await?;
    sqlx::query("INSERT INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id) VALUES('legacy','oqto-retained','retained','old-owner',?)")
        .bind(workspace).execute(&pool).await?;
    pool.close().await;
    Ok(db)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn many_shards_and_overlapping_validators_share_two_sqlite_workers() -> Result<()> {
    const SHARDS: usize = 192;
    let temp = tempfile::tempdir()?;
    // Real migration-prefix shards outside the requested Session's Workspace.
    // They must all remain conflict inputs; none can be scope-skipped.
    for n in 0..SHARDS {
        prefix_shard(temp.path(), &format!("/synthetic/shard-{n}"), 1).await?;
    }
    discover(temp.path(), "owner", "/synthetic/active", "active", "pin").await?;
    let public = crate::oqto_log::store::platform_id_for_external_id("active");
    let started = std::time::Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let (home, public) = (temp.path().to_owned(), public.clone());
        tasks.spawn(async move {
            validate(&home, &public, "active", "/synthetic/active", &["pin"]).await
        });
    }
    while let Some(result) = tasks.join_next().await {
        result??;
    }
    assert!(reader::pool_size() <= reader::READ_CONNECTIONS);
    assert_eq!(reader::READ_CONNECTIONS, 2);
    // Every unrelated shard contains this other native fact. A fresh Session
    // must not win by considering only approved or recently opened shards.
    let mut batch = DiscoveryBatch::open(temp.path()).await?;
    assert!(
        batch
            .discover("new", "/synthetic/wrong", "retained", "pin")
            .await
            .is_err()
    );
    assert!(
        !crate::oqto_log::paths::existing_user_home_workspace_db_path(
            temp.path(),
            "/synthetic/wrong"
        )
        .exists()
    );
    eprintln!(
        "native identity resources: {SHARDS} retained shards, 12 overlapping validators, SQLite reader workers={}, elapsed={:?}",
        reader::pool_size(),
        started.elapsed()
    );
    Ok(())
}

#[tokio::test]
async fn commit_rereads_shards_added_after_preparation_instead_of_caching_ownership() -> Result<()>
{
    let temp = tempfile::tempdir()?;
    let mut batch = DiscoveryBatch::open(temp.path()).await?;
    let prepared = batch.prepare("retained").await?;
    let db = prefix_shard(temp.path(), "/synthetic/old-owner", 1).await?;
    let before = std::fs::read(&db)?;
    assert!(
        batch
            .commit("wrong-owner", "/synthetic/wrong", "pin", prepared)
            .await
            .is_err()
    );
    assert!(
        !crate::oqto_log::paths::existing_user_home_workspace_db_path(
            temp.path(),
            "/synthetic/wrong"
        )
        .exists()
    );
    assert_eq!(std::fs::read(db)?, before);
    assert!(
        batch
            .discover("owner", "/synthetic/fresh", "unrelated-fresh", "pin")
            .await?
    );
    Ok(())
}

#[tokio::test]
async fn initial_inventory_failure_refuses_all_adoption_and_new_batch_recovers() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let db = prefix_shard(temp.path(), "/synthetic/legacy", 1).await?;
    let before = std::fs::read(&db)?;
    std::fs::write(&db, b"injected unreadable SQLite shard")?;
    let error = match DiscoveryBatch::open(temp.path()).await {
        Ok(_) => anyhow::bail!("incomplete inventory must not become a ready batch"),
        Err(error) => error,
    };
    assert!(authority_unavailable(&error));
    assert!(
        !crate::oqto_log::paths::existing_user_home_workspace_db_path(
            temp.path(),
            "/synthetic/fresh"
        )
        .exists()
    );
    std::fs::write(&db, &before)?;
    let mut recovered = DiscoveryBatch::open(temp.path()).await?;
    assert!(
        recovered
            .discover("new", "/synthetic/wrong", "retained", "pin")
            .await
            .is_err()
    );
    assert!(
        recovered
            .discover("new", "/synthetic/fresh", "fresh", "pin")
            .await?
    );
    assert_eq!(std::fs::read(db)?, before);
    Ok(())
}

#[tokio::test]
async fn failed_post_upgrade_reopen_poisons_batch_without_omitting_conflict_facts() -> Result<()> {
    for prefix in [1, 3] {
        let temp = tempfile::tempdir()?;
        let workspace = "/synthetic/retained";
        let db = prefix_shard(temp.path(), workspace, prefix).await?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(&db))
            .await?;
        sqlx::query("INSERT INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id) VALUES('other-row','oqto-other-public','other-native','other-owner',?)")
            .bind(workspace).execute(&pool).await?;
        pool.close().await;
        let mut batch = DiscoveryBatch::open(temp.path()).await?;
        let saved = db.with_extension("held");
        let (fault_db, fault_saved) = (db.clone(), saved.clone());
        // Inject a real SQLite open failure exactly after the migration/pin
        // commit and before verification; no production flag or fallback path.
        batch.after_write = Some(Box::new(move || {
            std::fs::rename(fault_db, fault_saved).unwrap()
        }));
        let error = batch
            .discover("new-owner", workspace, "retained", "pin")
            .await
            .unwrap_err();
        assert!(authority_unavailable(&error));
        assert!(
            DiscoveryBatch::open(temp.path()).await.is_err(),
            "a missing database in an existing shard directory is not empty authority"
        );
        std::fs::rename(&saved, &db)?;
        let upgraded = std::fs::read(&db)?;
        let error = batch
            .discover("wrong-owner", "/synthetic/wrong", "other-native", "pin")
            .await
            .unwrap_err();
        assert!(
            authority_unavailable(&error),
            "a poisoned batch cannot silently resume, even after the file recovers"
        );
        assert!(
            !crate::oqto_log::paths::existing_user_home_workspace_db_path(
                temp.path(),
                "/synthetic/wrong"
            )
            .exists()
        );
        assert_eq!(std::fs::read(&db)?, upgraded);
        let mut recovered = DiscoveryBatch::open(temp.path()).await?;
        assert!(
            recovered
                .discover("wrong-owner", "/synthetic/wrong", "other-native", "pin")
                .await
                .is_err()
        );
        validate(
            temp.path(),
            "oqto-retained",
            "retained",
            workspace,
            &["pin"],
        )
        .await?;
        assert!(
            recovered
                .discover("new-owner", "/synthetic/fresh", "fresh", "fresh-pin")
                .await?
        );
        let rows = list_sessions(temp.path(), None).await?;
        assert_eq!(
            rows.iter()
                .find(|row| row.platform_id == "oqto-retained")
                .context("retained")?
                .user_id,
            "old-owner"
        );
    }
    Ok(())
}

#[tokio::test]
async fn bounded_readers_observe_fresh_wal_commits_not_cached_ownership() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let db = prefix_shard(temp.path(), "/synthetic/wal", 1).await?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal),
        )
        .await?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE oqto_log_sessions SET external_id='changed-native' WHERE session_id='legacy'",
    )
    .execute(&mut *tx)
    .await?;
    assert_eq!(
        shard_identities(&db, "oqto-retained", "retained").await?[0]
            .external
            .as_deref(),
        Some("retained")
    );
    tx.commit().await?;
    let current = shard_identities(&db, "oqto-retained", "changed-native").await?;
    assert_eq!(current[0].external.as_deref(), Some("changed-native"));
    assert!(
        shard_identities(&db, "unrelated-public", "retained")
            .await?
            .is_empty()
    );
    assert!(reader::pool_size() <= reader::READ_CONNECTIONS);
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn cancelled_attachment_cleans_up_before_next_read_and_never_writes_or_creates() -> Result<()>
{
    let temp = tempfile::tempdir()?;
    let home = temp.path().join("%23 #?");
    let db = prefix_shard(&home, "/synthetic/uri", 1).await?;
    let before = std::fs::read(&db)?;
    let mut shard = reader::ReadShard::open(&db).await?;
    assert!(
        sqlx::query("DELETE FROM oqto_log_sessions")
            .execute(shard.connection())
            .await
            .is_err()
    );
    // Contaminate BOTH slots, so a positive reuse cannot pass merely by taking
    // the other idle connection instead of exercising cancellation cleanup.
    let other = reader::ReadShard::open(&db).await?;
    drop(shard);
    drop(other);
    let mut first = reader::ReadShard::open(&db).await?;
    let mut second = reader::ReadShard::open(&db).await?;
    for shard in [&mut first, &mut second] {
        let public: String = sqlx::query_scalar("SELECT platform_id FROM oqto_log_sessions")
            .fetch_one(shard.connection())
            .await?;
        assert_eq!(public, "oqto-retained");
    }
    first.finish().await?;
    second.finish().await?;
    assert_eq!(std::fs::read(&db)?, before);
    let missing = temp.path().join("not-created.sqlite");
    assert!(reader::ReadShard::open(&missing).await.is_err());
    assert!(!missing.exists());
    Ok(())
}

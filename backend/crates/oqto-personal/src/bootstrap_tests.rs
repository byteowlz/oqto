use super::*;
use sqlx::Executor;

#[tokio::test]
async fn bootstrap_child() -> Result<()> {
    let Ok(path) = std::env::var("OQTO_PERSONAL_BOOTSTRAP_FIXTURE") else {
        return Ok(());
    };
    let (_root, database, _lock) = private::open_state(Path::new(&path))?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(database)
                .create_if_missing(false)
                .journal_mode(SqliteJournalMode::Wal),
        )
        .await?;
    if std::env::var("OQTO_PERSONAL_BOOTSTRAP_STAGE").as_deref() == Ok("migration") {
        let mut transaction = pool.begin().await?;
        sqlx::raw_sql(include_str!("../migrations/0001_devices.sql"))
            .execute(&mut *transaction)
            .await?;
        // Intentionally leave the real migration transaction uncommitted.
        std::process::exit(86);
    }
    std::process::exit(86);
}

#[tokio::test]
async fn process_death_before_or_during_migration_recovers_only_owned_inode() -> Result<()> {
    for stage in ["sqlite_open", "migration"] {
        let dir = tempfile::tempdir()?;
        let state = dir.path().join("state");
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "bootstrap_tests::bootstrap_child", "--nocapture"])
            .env("OQTO_PERSONAL_BOOTSTRAP_FIXTURE", &state)
            .env("OQTO_PERSONAL_BOOTSTRAP_STAGE", stage)
            .status()?;
        assert_eq!(status.code(), Some(86));
        let authority = PairingAuthority::open_private(&state).await?;
        let host = authority.host_id().to_owned();
        let identity = authority.transport_identity().await?;
        let leaf = identity.leaf_der().to_vec();
        drop(identity);
        drop(authority);
        let reopened = PairingAuthority::open_private(&state).await?;
        assert_eq!(reopened.host_id(), host);
        assert_eq!(reopened.transport_identity().await?.leaf_der(), leaf);
    }
    Ok(())
}

#[tokio::test]
async fn copied_marker_cannot_claim_foreign_database_and_corrupt_tls_is_not_rotated() -> Result<()>
{
    let (dir, authority) = tests::fixture().await?;
    let state = dir.path().join("authority");
    let leaf = authority.transport_identity().await?.leaf_der().to_vec();
    sqlx::query("UPDATE personal_transport_identity SET key_der=x'00'")
        .execute(&authority.pool)
        .await?;
    assert!(authority.transport_identity().await.is_err());
    let unchanged: Vec<u8> = sqlx::query_scalar("SELECT leaf_der FROM personal_transport_identity")
        .fetch_one(&authority.pool)
        .await?;
    assert_eq!(unchanged, leaf);
    drop(authority);
    // A new inode, even with an identical recognized SQLite header, is not
    // silently adopted under somebody else's interrupted-bootstrap marker.
    let database = state.join("personal.sqlite3");
    std::fs::rename(&database, state.join("saved.sqlite3"))?;
    std::fs::copy(state.join("saved.sqlite3"), &database)?;
    let before = std::fs::read(&database)?;
    assert!(PairingAuthority::open_private(&state).await.is_err());
    assert_eq!(std::fs::read(database)?, before);
    Ok(())
}

#[tokio::test]
async fn current_device_fence_closes_before_revoke_returns() -> Result<()> {
    let (_dir, authority) = tests::fixture().await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let id = authority.enroll(&tests::enrollment(&ticket, 1)).await?;
    let device = authority
        .authenticate(&tests::request(&id, 1, now()?))
        .await?;
    let admission = authority
        .require_current(&device, Access::Inventory)
        .await?;
    let fence = admission.fence();
    assert!(fence.is_current());
    drop(admission);
    assert!(authority.revoke(&id).await?);
    assert!(!fence.is_current());
    assert!(
        authority
            .require_current(&device, Access::Inventory)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn cancelled_durable_revoke_does_not_reopen_runtime_fence() -> Result<()> {
    let (_dir, authority) = tests::fixture().await?;
    let authority = Arc::new(authority);
    let id = authority
        .enroll(&tests::enrollment(
            &authority.issue_ticket(&Grants::default()).await?,
            1,
        ))
        .await?;
    let device = authority
        .authenticate(&tests::request(&id, 1, now()?))
        .await?;
    let admission = authority
        .require_current(&device, Access::Inventory)
        .await?;
    let fence = admission.fence();
    drop(admission);
    let mut blocker = authority.pool.acquire().await?;
    blocker.execute("BEGIN IMMEDIATE").await?;
    let revoker = Arc::clone(&authority);
    let revoking_id = id.clone();
    let task = tokio::spawn(async move { revoker.revoke(&revoking_id).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while fence.is_current() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    task.abort();
    let _ = task.await;
    blocker.execute("ROLLBACK").await?;
    drop(blocker);
    assert!(!fence.is_current());
    assert!(
        authority
            .require_current(&device, Access::Inventory)
            .await
            .is_err()
    );
    Ok(())
}

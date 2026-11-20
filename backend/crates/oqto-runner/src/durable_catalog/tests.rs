//! Real disposable SQLite, no runtime, owner files, credentials or model calls.
use super::*;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::path::Path;

fn home(workspace: &str, agent: &str) -> AgentHome {
    AgentHome {
        workspace_id: workspace.into(),
        agent_id: agent.into(),
    }
}
async fn open(path: &Path) -> Result<DurableCatalog> {
    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(std::time::Duration::from_secs(5)),
        )
        .await?;
    DurableCatalog::open(pool).await
}

#[tokio::test]
async fn reopen_retains_agent_store_and_exact_public_native_binding() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("catalog.sqlite");
    let scope = home("workspace-a", "agent-a");
    let first = open(&path).await?;
    let store = first.reserve_store(&scope).await?;
    let original = first
        .bind_session(&scope, "legacy-public-id", "native-conversation", "work-a")
        .await?;
    assert_eq!(original.store_id, store.store_id);
    first.close().await;
    let second = open(&path).await?;
    assert_eq!(second.reserve_store(&scope).await?, store);
    assert_eq!(
        second.resolve(&scope, "legacy-public-id").await?,
        Some(original)
    );
    assert_eq!(second.resolve(&scope, "0").await?, None);
    assert_eq!(second.resolve(&scope, "missing").await?, None);
    second.close().await;
    Ok(())
}

#[tokio::test]
async fn agent_and_workspace_scope_never_share_stores_or_disclose_foreign_sessions() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let catalog = open(&dir.path().join("catalog.sqlite")).await?;
    let a = home("workspace-a", "agent-a");
    let b = home("workspace-a", "agent-b");
    let c = home("workspace-b", "agent-a");
    let record = catalog
        .bind_session(&a, "private-session", "native", "work-a")
        .await?;
    let mut ids = std::collections::HashSet::from([record.store_id.clone()]);
    for scope in [&b, &c] {
        assert!(ids.insert(catalog.reserve_store(scope).await?.store_id));
        assert_eq!(catalog.resolve(scope, "private-session").await?, None);
        assert!(
            catalog
                .bind_session(scope, "private-session", "native", "work-a")
                .await
                .is_err()
        );
    }
    assert_eq!(catalog.resolve(&a, "private-session").await?, Some(record));
    catalog.close().await;
    Ok(())
}

#[tokio::test]
async fn immutable_binding_allows_exact_retry_but_never_retargets() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let catalog = open(&dir.path().join("catalog.sqlite")).await?;
    let scope = home("workspace", "agent");
    let binding = catalog
        .bind_session(&scope, "public", "native", "work-a")
        .await?;
    assert_eq!(
        catalog
            .bind_session(&scope, "public", "native", "work-a")
            .await?,
        binding
    );
    for (native, work) in [("other", "work-a"), ("native", "work-b")] {
        assert!(
            catalog
                .bind_session(&scope, "public", native, work)
                .await
                .is_err()
        );
    }
    assert_eq!(catalog.resolve(&scope, "public").await?, Some(binding));
    catalog.close().await;
    Ok(())
}

#[tokio::test]
async fn native_conversation_has_one_public_identity_per_store_not_globally() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let catalog = open(&dir.path().join("catalog.sqlite")).await?;
    let a = home("workspace", "agent-a");
    let b = home("workspace", "agent-b");
    let first = catalog
        .bind_session(&a, "public-a", "native", "work-a")
        .await?;
    assert!(
        catalog
            .bind_session(&a, "duplicate", "native", "work-a")
            .await
            .is_err()
    );
    assert_eq!(catalog.resolve(&a, "duplicate").await?, None);
    let second = catalog
        .bind_session(&b, "public-b", "native", "work-b")
        .await?;
    assert_ne!(first.store_id, second.store_id);
    catalog.close().await;
    Ok(())
}

#[tokio::test]
async fn concurrent_store_and_session_registration_share_exact_identity() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let catalog = open(&dir.path().join("catalog.sqlite")).await?;
    let scope = home("workspace", "agent");
    let mut calls = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let catalog = catalog.clone();
        let scope = scope.clone();
        calls.spawn(async move {
            let store = catalog.reserve_store(&scope).await?;
            let session = catalog
                .bind_session(&scope, "public", "native", "work")
                .await?;
            Ok::<_, anyhow::Error>((store, session))
        });
    }
    let mut result = None;
    while let Some(call) = calls.join_next().await {
        let value = call??;
        if let Some(first) = &result {
            assert_eq!(&value, first);
        }
        result = Some(value);
    }
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM durable_sessions")
        .fetch_one(&catalog.pool)
        .await?;
    assert_eq!(rows, 1);
    catalog.close().await;
    Ok(())
}

#[tokio::test]
async fn invalid_or_temporary_ids_never_materialize_authority() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let catalog = open(&dir.path().join("catalog.sqlite")).await?;
    let valid = home("workspace", "agent");
    for bad in [
        "",
        "pending-request",
        "tmp:1",
        "has\ncontrol",
        " leading",
        "trailing ",
        &"a".repeat(257),
    ] {
        assert!(
            catalog
                .reserve_store(&home("workspace", bad))
                .await
                .is_err()
        );
        assert!(catalog.reserve_store(&home(bad, "agent")).await.is_err());
        assert!(
            catalog
                .bind_session(&valid, bad, "native", "work")
                .await
                .is_err()
        );
        assert!(
            catalog
                .bind_session(&valid, "public", bad, "work")
                .await
                .is_err()
        );
        assert!(
            catalog
                .bind_session(&valid, "public", "native", bad)
                .await
                .is_err()
        );
        assert!(catalog.resolve(&valid, bad).await.is_err());
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM durable_stores")
        .fetch_one(&catalog.pool)
        .await?;
    assert_eq!(count, 0, "invalid input created a store before validation");
    catalog.close().await;
    Ok(())
}

#[tokio::test]
async fn one_agent_home_retains_a_store_across_work_directories() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let catalog = open(&dir.path().join("catalog.sqlite")).await?;
    let scope = home("workspace", "agent");
    let a = catalog
        .bind_session(&scope, "session-a", "native-a", "work-a")
        .await?;
    let b = catalog
        .bind_session(&scope, "session-b", "native-b", "work-b")
        .await?;
    assert_eq!(a.store_id, b.store_id);
    assert_ne!(a.origin_work_directory_id, b.origin_work_directory_id);
    catalog.close().await;
    Ok(())
}

#[tokio::test]
async fn conflicting_native_claims_have_one_winner_without_partial_binding() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let catalog = open(&dir.path().join("catalog.sqlite")).await?;
    let scope = home("workspace", "agent");
    let (a, b) = tokio::join!(
        catalog.bind_session(&scope, "session-a", "native", "work"),
        catalog.bind_session(&scope, "session-b", "native", "work")
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let a = catalog.resolve(&scope, "session-a").await?;
    let b = catalog.resolve(&scope, "session-b").await?;
    assert_ne!(a.is_some(), b.is_some());
    catalog.close().await;
    Ok(())
}

#[tokio::test]
async fn foreign_databases_are_refused_before_catalog_migrations() -> Result<()> {
    for table in [Some("unrelated"), Some("sqliteXforeign"), None] {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        if let Some(name) = table {
            // Names are fixed fixture inputs, including a SQL LIKE-underscore trap.
            sqlx::query(&format!("CREATE TABLE {name}(value TEXT)"))
                .execute(&pool)
                .await?;
            sqlx::query(&format!("INSERT INTO {name} VALUES('synthetic sentinel')"))
                .execute(&pool)
                .await?;
        } else {
            sqlx::query("PRAGMA application_id=12345")
                .execute(&pool)
                .await?;
        }
        assert!(DurableCatalog::open(pool.clone()).await.is_err());
        let marker: i64 = sqlx::query_scalar("PRAGMA application_id")
            .fetch_one(&pool)
            .await?;
        assert_eq!(marker, if table.is_some() { 0 } else { 12345 });
        let migrations: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name='_sqlx_migrations'")
                .fetch_one(&pool)
                .await?;
        assert_eq!(migrations, 0);
        if let Some(name) = table {
            let value: String = sqlx::query_scalar(&format!("SELECT value FROM {name}"))
                .fetch_one(&pool)
                .await?;
            assert_eq!(value, "synthetic sentinel");
        }
        pool.close().await;
    }
    Ok(())
}

#[tokio::test]
async fn future_migration_is_refused_without_remapping_existing_sessions() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("catalog.sqlite");
    let catalog = open(&path).await?;
    let scope = home("workspace", "agent");
    let record = catalog
        .bind_session(&scope, "public", "native", "work")
        .await?;
    sqlx::query("INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES(999999999999,'future',1,X'00',0)")
        .execute(&catalog.pool).await?;
    assert!(DurableCatalog::open(catalog.pool.clone()).await.is_err());
    assert_eq!(catalog.resolve(&scope, "public").await?, Some(record));
    catalog.close().await;
    Ok(())
}

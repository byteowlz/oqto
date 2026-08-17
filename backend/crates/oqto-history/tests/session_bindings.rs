use std::collections::HashMap;

use anyhow::Result;
use oqto_history::oqto_log::bindings::{
    NewSessionBinding, SessionBindingError, SessionBindingKey, append_session_binding,
    list_session_bindings, resolve_pi_session_identity, stable_binding_id,
};
use oqto_history::oqto_log::ops::{
    SessionIdentityInput, batch_upsert_session_identities, delete_session_in_workspace,
    upsert_session_identity,
};
use oqto_history::oqto_log::paths::resolve_user_home_workspace_db_path;
use oqto_history::oqto_log::store::{
    append_agent_end_snapshot, migrate_db_path, replace_session_with_snapshot,
};
use oqto_pi::AgentMessage;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

async fn open_pool(path: &std::path::Path) -> Result<sqlx::SqlitePool> {
    Ok(SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await?)
}

async fn insert_session(
    connection: &mut sqlx::SqliteConnection,
    session_id: &str,
    platform_id: &str,
    external_id: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO oqto_log_sessions (session_id, platform_id, external_id, user_id, workspace_id) VALUES (?, ?, ?, 'user-1', 'workspace-1')",
    )
    .bind(session_id)
    .bind(platform_id)
    .bind(external_id)
    .execute(connection)
    .await?;
    Ok(())
}

#[tokio::test]
async fn migration_backfills_legacy_binding_without_changing_timeline() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let db_path = temp.path().join("legacy.sqlite");
    let pool = open_pool(&db_path).await?;

    sqlx::raw_sql(include_str!(
        "../migrations_oqto_log/20260410001_oqto_log_schema_v1.sql"
    ))
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO oqto_log_sessions (session_id, platform_id, external_id, user_id, workspace_id, created_at) VALUES ('storage-1', 'oqto-public-1', 'pi-native-1', 'user-1', 'workspace-1', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO oqto_log_branches (branch_id, session_id) VALUES ('branch:storage-1:main', 'storage-1')")
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO oqto_log_turns (turn_id, session_id, branch_id, turn_version, role, status) VALUES ('turn-1', 'storage-1', 'branch:storage-1:main', 1, 'user', 'committed')",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO oqto_log_messages (message_id, turn_id, seq, kind, role, content) VALUES ('message-1', 'turn-1', 0, 'text', 'user', 'preserve me')",
    )
    .execute(&pool)
    .await?;

    let before: (i64, i64, String) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM oqto_log_turns), (SELECT COUNT(*) FROM oqto_log_messages), (SELECT content FROM oqto_log_messages WHERE message_id = 'message-1')",
    )
    .fetch_one(&pool)
    .await?;
    let before_search: Vec<(String, String)> = sqlx::query_as(
        "SELECT message_id, content FROM oqto_log_message_fts WHERE oqto_log_message_fts MATCH 'preserve' ORDER BY message_id",
    )
    .fetch_all(&pool)
    .await?;
    pool.close().await;

    migrate_db_path(&db_path).await?;
    // SQLx migrations are safe to run again after the first application.
    migrate_db_path(&db_path).await?;

    let pool = open_pool(&db_path).await?;
    let after: (i64, i64, String) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM oqto_log_turns), (SELECT COUNT(*) FROM oqto_log_messages), (SELECT content FROM oqto_log_messages WHERE message_id = 'message-1')",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(after, before);
    let after_search: Vec<(String, String)> = sqlx::query_as(
        "SELECT message_id, content FROM oqto_log_message_fts WHERE oqto_log_message_fts MATCH 'preserve' ORDER BY message_id",
    )
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        after_search, before_search,
        "search projection must not drift"
    );

    let row: (String, String, String, String) = sqlx::query_as(
        "SELECT oqto_session_id, harness, binding_kind, source FROM oqto_log_session_bindings WHERE external_id = 'pi-native-1'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        row,
        (
            "oqto-public-1".to_string(),
            "unknown".to_string(),
            "pi-compatible-session".to_string(),
            "legacy-external-id".to_string()
        )
    );
    Ok(())
}

#[tokio::test]
async fn binding_interface_is_idempotent_supports_many_and_fails_conflicts_closed() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let db_path = temp.path().join("bindings.sqlite");
    migrate_db_path(&db_path).await?;
    let pool = open_pool(&db_path).await?;
    let mut connection = pool.acquire().await?;
    insert_session(&mut connection, "storage-1", "oqto-public-1", None).await?;
    insert_session(&mut connection, "storage-2", "oqto-public-2", None).await?;

    let first_key = SessionBindingKey::pi_session("pi-native-1");
    let first_id = stable_binding_id(first_key, "oqto-public-1", None);
    let input = NewSessionBinding {
        binding_id: &first_id,
        oqto_session_id: "oqto-public-1",
        key: first_key,
        source: "test",
        first_seen_at: Some("2026-01-01T00:00:00Z"),
        last_seen_at: None,
        supersedes_binding_id: None,
        extensions_json: "{}",
    };
    let first = append_session_binding(&mut connection, input).await?;
    let retried = append_session_binding(&mut connection, input).await?;
    assert_eq!(first, retried, "identical delivery must be idempotent");

    let second_key = SessionBindingKey {
        harness: "claude-code",
        provider_instance: "local",
        binding_kind: "session",
        external_id: "claude-native-1",
    };
    let second_id = stable_binding_id(second_key, "oqto-public-1", None);
    append_session_binding(
        &mut connection,
        NewSessionBinding {
            binding_id: &second_id,
            oqto_session_id: "oqto-public-1",
            key: second_key,
            source: "test",
            first_seen_at: None,
            last_seen_at: None,
            supersedes_binding_id: None,
            extensions_json: "{}",
        },
    )
    .await?;
    assert_eq!(
        list_session_bindings(&mut connection, "oqto-public-1")
            .await?
            .len(),
        2,
        "one Oqto Session accepts multiple typed bindings"
    );

    let conflicting_id = stable_binding_id(first_key, "oqto-public-2", None);
    let error = append_session_binding(
        &mut connection,
        NewSessionBinding {
            binding_id: &conflicting_id,
            oqto_session_id: "oqto-public-2",
            key: first_key,
            source: "test-conflict",
            first_seen_at: None,
            last_seen_at: None,
            supersedes_binding_id: None,
            extensions_json: "{}",
        },
    )
    .await
    .expect_err("same native identity must not silently bind twice");
    assert!(matches!(error, SessionBindingError::Conflict(_)));
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM oqto_log_session_bindings WHERE external_id = 'pi-native-1'",
    )
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(count, 1, "conflict must not partially write");

    let successor_id = stable_binding_id(first_key, "oqto-public-2", Some(&first_id));
    append_session_binding(
        &mut connection,
        NewSessionBinding {
            binding_id: &successor_id,
            oqto_session_id: "oqto-public-2",
            key: first_key,
            source: "explicit-rebind",
            first_seen_at: None,
            last_seen_at: None,
            supersedes_binding_id: Some(&first_id),
            extensions_json: "{}",
        },
    )
    .await?;
    let rebound = resolve_pi_session_identity(&mut connection, "pi-native-1")
        .await?
        .expect("explicit successor must resolve");
    assert_eq!(rebound.platform_id, "oqto-public-2");

    // Binding facts survive Session retention/deletion. Resolution must report
    // the missing terminal owner, never fall back to the superseded ancestor.
    sqlx::query("DELETE FROM oqto_log_sessions WHERE session_id = 'storage-2'")
        .execute(&mut *connection)
        .await?;
    let error = resolve_pi_session_identity(&mut connection, "pi-native-1")
        .await
        .expect_err("missing terminal owner must not resurrect the old mapping");
    assert!(matches!(
        error,
        SessionBindingError::MissingBoundSession { .. }
    ));
    let facts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM oqto_log_session_bindings WHERE external_id = 'pi-native-1'",
    )
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(facts, 2, "Session deletion must preserve immutable facts");
    Ok(())
}

#[tokio::test]
async fn migration_preserves_legacy_conflicts_and_resolution_fails_closed() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let db_path = temp.path().join("legacy-conflict.sqlite");
    let pool = open_pool(&db_path).await?;
    sqlx::raw_sql(include_str!(
        "../migrations_oqto_log/20260410001_oqto_log_schema_v1.sql"
    ))
    .execute(&pool)
    .await?;
    let mut connection = pool.acquire().await?;
    insert_session(
        &mut connection,
        "oqto-empty",
        "oqto-empty",
        Some("pi-conflict"),
    )
    .await?;
    insert_session(
        &mut connection,
        "pi-conflict",
        "pi-conflict",
        Some("pi-conflict"),
    )
    .await?;
    drop(connection);
    pool.close().await;

    migrate_db_path(&db_path).await?;
    let pool = open_pool(&db_path).await?;
    let binding_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM oqto_log_session_bindings WHERE external_id = 'pi-conflict'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        binding_count, 0,
        "migration must not guess a winner for conflicting legacy rows"
    );
    let session_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM oqto_log_sessions WHERE external_id = 'pi-conflict'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        session_count, 2,
        "migration must not delete either history owner"
    );

    let mut connection = pool.acquire().await?;
    let error = resolve_pi_session_identity(&mut connection, "pi-conflict")
        .await
        .expect_err("ambiguous legacy identity must fail closed");
    assert!(matches!(error, SessionBindingError::LegacyConflict { .. }));
    Ok(())
}

#[tokio::test]
async fn deleted_session_reimport_recreates_the_bound_public_identity() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let workspace = "/tmp/session-binding-reimport";
    let external_id = "pi-reimport";
    let public_id = "oqto-reimport";
    let message = AgentMessage {
        role: "user".to_string(),
        content: Value::String("restore from authority".to_string()),
        timestamp: None,
        tool_call_id: None,
        tool_name: None,
        is_error: None,
        api: None,
        provider: None,
        model: None,
        usage: None,
        stop_reason: None,
        extra: HashMap::new(),
    };

    append_agent_end_snapshot(
        temp.path(),
        "user-1",
        workspace,
        public_id,
        public_id,
        Some(external_id),
        external_id,
        std::slice::from_ref(&message),
    )
    .await?;
    assert!(
        delete_session_in_workspace(temp.path(), workspace, public_id).await?,
        "seed Session must be deleted"
    );

    let stats = append_agent_end_snapshot(
        temp.path(),
        "user-1",
        workspace,
        external_id,
        external_id,
        Some(external_id),
        external_id,
        &[message],
    )
    .await?;
    assert_eq!(stats.session_id, public_id);

    let db_path = resolve_user_home_workspace_db_path(temp.path(), workspace)?;
    let pool = open_pool(&db_path).await?;
    let facts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM oqto_log_session_bindings WHERE external_id = ?")
            .bind(external_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(facts, 1, "reimport must reuse the immutable binding fact");
    let sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM oqto_log_sessions WHERE platform_id = ?")
            .bind(public_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(sessions, 1);
    Ok(())
}

#[tokio::test]
async fn old_external_only_writer_and_new_binding_writer_converge() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let workspace = "/tmp/session-binding-rolling-upgrade";
    let db_path = resolve_user_home_workspace_db_path(temp.path(), workspace)?;
    migrate_db_path(&db_path).await?;
    let pool = open_pool(&db_path).await?;
    let mut connection = pool.acquire().await?;
    insert_session(
        &mut connection,
        "oqto-old-writer",
        "oqto-old-writer",
        Some("pi-rolling-upgrade"),
    )
    .await?;
    drop(connection);
    pool.close().await;

    append_agent_end_snapshot(
        temp.path(),
        "user-1",
        workspace,
        "pi-rolling-upgrade",
        "pi-rolling-upgrade",
        Some("pi-rolling-upgrade"),
        "pi-rolling-upgrade",
        &[AgentMessage {
            role: "user".to_string(),
            content: Value::String("rolling upgrade".to_string()),
            timestamp: None,
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
            extra: HashMap::new(),
        }],
    )
    .await?;

    let pool = open_pool(&db_path).await?;
    let sessions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM oqto_log_sessions WHERE external_id = 'pi-rolling-upgrade'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(sessions, 1);
    let binding: String = sqlx::query_scalar(
        "SELECT oqto_session_id FROM oqto_log_session_bindings WHERE harness = 'pi' AND external_id = 'pi-rolling-upgrade'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(binding, "oqto-old-writer");
    Ok(())
}

#[tokio::test]
async fn all_current_pi_identity_writers_dual_write_bindings() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let workspace = "/tmp/session-binding-writers";

    upsert_session_identity(
        temp.path(),
        "user-1",
        workspace,
        "oqto-discovery",
        Some("oqto-discovery"),
        Some("pi-discovery"),
    )
    .await?;
    batch_upsert_session_identities(
        temp.path(),
        "user-1",
        workspace,
        &[SessionIdentityInput {
            external_id: "pi-import".to_string(),
            platform_id: "oqto-import".to_string(),
            title: None,
            readable_id: None,
            created_at: None,
            updated_at: None,
        }],
    )
    .await?;
    replace_session_with_snapshot(
        temp.path(),
        "user-1",
        workspace,
        "oqto-reconnect",
        "oqto-reconnect",
        Some("pi-reconnect"),
        "pi-reconnect",
        &[],
    )
    .await?;

    let db_path = resolve_user_home_workspace_db_path(temp.path(), workspace)?;
    let pool = open_pool(&db_path).await?;
    let bindings: Vec<(String, String)> = sqlx::query_as(
        "SELECT external_id, oqto_session_id FROM oqto_log_session_bindings ORDER BY external_id",
    )
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        bindings,
        vec![
            ("pi-discovery".to_string(), "oqto-discovery".to_string()),
            ("pi-import".to_string(), "oqto-import".to_string()),
            ("pi-reconnect".to_string(), "oqto-reconnect".to_string()),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn pi_snapshot_dual_writes_binding_and_rejects_provisional_identity() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let workspace = "/tmp/session-binding-dual-write";
    let external_id = "019f5f22-3777-78b7-ab0e-539863fb8232";
    let message = AgentMessage {
        role: "user".to_string(),
        content: Value::String("hello".to_string()),
        timestamp: Some(1_779_363_330_601),
        tool_call_id: None,
        tool_name: None,
        is_error: None,
        api: None,
        provider: None,
        model: None,
        usage: None,
        stop_reason: None,
        extra: HashMap::new(),
    };
    let stats = append_agent_end_snapshot(
        temp.path(),
        "user-1",
        workspace,
        external_id,
        external_id,
        Some(external_id),
        external_id,
        &[message],
    )
    .await?;

    let db_path = resolve_user_home_workspace_db_path(temp.path(), workspace)?;
    let pool = open_pool(&db_path).await?;
    let row: (String, String) = sqlx::query_as(
        "SELECT b.oqto_session_id, s.external_id FROM oqto_log_session_bindings b JOIN oqto_log_sessions s ON s.platform_id = b.oqto_session_id WHERE b.external_id = ?",
    )
    .bind(external_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.0, stats.session_id);
    assert_eq!(row.1, external_id);

    let mut connection = pool.acquire().await?;
    let key = SessionBindingKey::pi_session("pending-browser-id");
    let binding_id = stable_binding_id(key, "oqto-public-1", None);
    let error = append_session_binding(
        &mut connection,
        NewSessionBinding {
            binding_id: &binding_id,
            oqto_session_id: "oqto-public-1",
            key,
            source: "test",
            first_seen_at: None,
            last_seen_at: None,
            supersedes_binding_id: None,
            extensions_json: "{}",
        },
    )
    .await
    .expect_err("provisional identity must be rejected before storage");
    assert!(matches!(
        error,
        SessionBindingError::ProvisionalIdentity { .. }
    ));
    Ok(())
}

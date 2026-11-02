use super::*;
#[tokio::test]
async fn contentful_fts_tracks_update_null_restore_and_delete_without_payload_changes() -> Result<()>
{
    let home = tempfile::tempdir()?;
    let workspace = "/synthetic/fts";
    let message: oqto_pi::AgentMessage = serde_json::from_value(
        serde_json::json!({"role":"user","content":[{"type":"text","text":"alpha"}],"timestamp":1740000000000u64}),
    )?;
    let appended = crate::oqto_log::store::append_agent_end_snapshot(
        home.path(),
        "fixture",
        workspace,
        "public-fts",
        "public-fts",
        Some("native-fts"),
        "native-fts",
        &[message],
    )
    .await?;
    let pool = crate::oqto_log::store::open_workspace_pool(home.path(), workspace).await?;
    let (id, original): (String, String) =
        sqlx::query_as("SELECT message_id,json_payload FROM oqto_log_messages")
            .fetch_one(&pool)
            .await?;
    let count = |term: &str| {
        let pool = pool.clone();
        let term = term.to_owned();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM oqto_log_message_fts WHERE oqto_log_message_fts MATCH ?",
            )
            .bind(term)
            .fetch_one(&pool)
            .await
        }
    };
    assert_eq!(count("alpha").await?, 1);
    sqlx::query("UPDATE oqto_log_messages SET content='beta' WHERE message_id=?")
        .bind(&id)
        .execute(&pool)
        .await?;
    assert_eq!(count("alpha").await?, 0);
    assert_eq!(count("beta").await?, 1);
    sqlx::query("UPDATE oqto_log_messages SET content=NULL WHERE message_id=?")
        .bind(&id)
        .execute(&pool)
        .await?;
    assert_eq!(count("beta").await?, 0);
    sqlx::query("UPDATE oqto_log_messages SET content='gamma' WHERE message_id=?")
        .bind(&id)
        .execute(&pool)
        .await?;
    assert_eq!(count("gamma").await?, 1);
    let preserved: String =
        sqlx::query_scalar("SELECT json_payload FROM oqto_log_messages WHERE message_id=?")
            .bind(&id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(preserved, original);
    let deleted = sqlx::query("DELETE FROM oqto_log_sessions WHERE session_id=?")
        .bind(&appended.session_id)
        .execute(&pool)
        .await?;
    assert_eq!(
        deleted.rows_affected(),
        1,
        "delete must use the canonical identity returned by the writer"
    );
    assert_eq!(count("gamma").await?, 0);
    Ok(())
}

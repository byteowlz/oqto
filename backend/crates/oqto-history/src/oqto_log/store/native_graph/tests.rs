use super::*;

fn source(native: &str, workspace: &str) -> Vec<u8> {
    [
        serde_json::json!({"type":"session","id":native,"cwd":workspace}),
        serde_json::json!({"type":"message","id":"root","parentId":null,"message":{"role":"user","content":[{"type":"text","text":"same"}],"timestamp":1}}),
        serde_json::json!({"type":"model_change","id":"meta","parentId":"root","modelId":"synthetic"}),
        serde_json::json!({"type":"message","id":"a","parentId":"meta","message":{"role":"assistant","content":[{"type":"text","text":"same"}],"timestamp":2}}),
        serde_json::json!({"type":"message","id":"b","parentId":"meta","message":{"role":"assistant","content":[{"type":"text","text":"same"}],"timestamp":3}}),
    ].into_iter().map(|v| format!("{v}\n")).collect::<String>().into_bytes()
}
#[test]
fn source_identity_parents_and_actual_head_are_not_guessed() -> Result<()> {
    let bytes = source("native", "/synthetic");
    let a = NativeGraph::parse(&bytes, "native", "/synthetic", Some("a"))?;
    let b = NativeGraph::parse(&bytes, "native", "/synthetic", Some("b"))?;
    assert_eq!(a.entries.len(), b.entries.len());
    assert_eq!(a.head_message.as_deref(), Some("a"));
    assert_eq!(b.head_message.as_deref(), Some("b"));
    assert_eq!(a.entries[2].parent_message.as_deref(), Some("root"));
    assert!(NativeGraph::parse(&bytes, "wrong", "/synthetic", Some("a")).is_err());
    assert!(NativeGraph::parse(&bytes, "native", "/other", Some("a")).is_err());
    assert!(NativeGraph::parse(&bytes, "native", "/synthetic", Some("missing")).is_err());
    let mut partial = bytes.clone();
    partial.extend_from_slice(b"{\"type\":\"message\",\"id\":\"incomplete\"");
    assert!(NativeGraph::parse(&partial, "native", "/synthetic", Some("incomplete")).is_err());
    assert!(NativeGraph::parse(&partial, "native", "/synthetic", Some("a")).is_ok());
    assert!(
        NativeGraph::parse(&bytes, "native", "/synthetic", None)?
            .head_message
            .is_none()
    );
    let duplicated = String::from_utf8(bytes.clone())?.replace("\"id\":\"b\"", "\"id\":\"a\"");
    assert!(NativeGraph::parse(duplicated.as_bytes(), "native", "/synthetic", Some("a")).is_err());
    let cyclic = String::from_utf8(bytes)?.replace("\"parentId\":\"meta\"", "\"parentId\":\"b\"");
    assert!(NativeGraph::parse(cyclic.as_bytes(), "native", "/synthetic", Some("a")).is_err());
    Ok(())
}
#[tokio::test]
async fn equal_count_head_switch_preserves_siblings_ids_legacy_provenance_and_rejects_mutation()
-> Result<()> {
    let home = tempfile::tempdir()?;
    let native = "11111111-2222-3333-4444-555555555555";
    let public = platform_id_for_external_id(native);
    let workspace = "/synthetic";
    let bytes = source(native, workspace);
    let graph = NativeGraph::parse(&bytes, native, workspace, Some("a"))?;
    let legacy = graph.entries[0]
        .message
        .as_ref()
        .context("message")?
        .clone();
    replace_session_with_pi_jsonl_records(
        home.path(),
        "owner",
        workspace,
        &public,
        &public,
        Some(native),
        native,
        &[PiJsonlMessageRecord {
            source_entry_id: "root".into(),
            parent_source_entry_id: None,
            source_sequence: 1,
            message: legacy,
        }],
    )
    .await?;
    let pool = open_workspace_pool(home.path(), workspace).await?;
    let legacy_id: String =
        sqlx::query_scalar("SELECT turn_id FROM oqto_log_turns WHERE session_id=?")
            .bind(&public)
            .fetch_one(&pool)
            .await?;
    merge(home.path(), "owner", &public, &graph).await?;
    let projected =
        crate::oqto_log::projector::project_session_messages_auto(home.path(), &public, None)
            .await?
            .context("projection")?;
    assert_eq!(
        projected.len(),
        2,
        "Only the explicitly evidenced native ancestor path is chat"
    );
    let projected_ids: Vec<_> = projected.iter().map(|message| message.id.clone()).collect();
    let turns: Vec<String> = sqlx::query_scalar(
        "SELECT turn_id FROM oqto_log_turns WHERE session_id=? ORDER BY turn_version",
    )
    .bind(&public)
    .fetch_all(&pool)
    .await?;
    assert_eq!(turns.len(), 4);
    assert!(turns.contains(&legacy_id));
    let branch = format!("branch:{public}:{SOURCE}");
    let head_a: String =
        sqlx::query_scalar("SELECT head_turn_id FROM oqto_log_branches WHERE branch_id=?")
            .bind(&branch)
            .fetch_one(&pool)
            .await?;
    assert_eq!(head_a, stable_id("turn", &public, "a"));
    let changed =
        String::from_utf8(bytes.clone())?.replace("\"text\":\"same\"", "\"text\":\"mutation\"");
    let corrupt = NativeGraph::parse(changed.as_bytes(), native, workspace, Some("b"))?;
    assert!(
        merge(home.path(), "owner", &public, &corrupt)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT head_turn_id FROM oqto_log_branches WHERE branch_id=?"
        )
        .bind(&branch)
        .fetch_one(&pool)
        .await?,
        head_a
    );
    let next = NativeGraph::parse(&bytes, native, workspace, Some("b"))?;
    merge(home.path(), "owner", &public, &next).await?;
    merge(home.path(), "owner", &public, &next).await?;
    let after: Vec<String> = sqlx::query_scalar(
        "SELECT turn_id FROM oqto_log_turns WHERE session_id=? ORDER BY turn_version",
    )
    .bind(&public)
    .fetch_all(&pool)
    .await?;
    assert_eq!(turns, after);
    assert!(
        replace_session_with_pi_jsonl_records(
            home.path(),
            "owner",
            workspace,
            &public,
            &public,
            Some(native),
            native,
            &[]
        )
        .await
        .is_err(),
        "Legacy convergence may not destroy the graph"
    );
    let page = crate::oqto_log::projector::project_session_messages_page_auto(
        home.path(),
        &public,
        1,
        None,
    )
    .await?
    .context("page")?;
    assert_eq!(page.messages.len(), 1);
    assert!(page.has_more);
    let older = crate::oqto_log::projector::project_session_messages_page_auto(
        home.path(),
        &public,
        1,
        page.next_before.as_deref(),
    )
    .await?
    .context("older")?;
    assert_eq!(older.messages.len(), 1);
    assert!(!older.has_more);
    assert_eq!(older.messages[0].id, projected_ids[0]);
    let switched =
        crate::oqto_log::projector::project_session_messages_auto(home.path(), &public, None)
            .await?
            .context("projection")?;
    assert_eq!(switched.len(), 2);
    assert_eq!(switched[0].id, projected_ids[0]);
    assert_ne!(switched[1].id, projected_ids[1]);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT head_turn_id FROM oqto_log_branches WHERE branch_id=?"
        )
        .bind(&branch)
        .fetch_one(&pool)
        .await?,
        stable_id("turn", &public, "b")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM oqto_log_raw_envelopes WHERE session_id=? AND source=?"
        )
        .bind(&public)
        .bind(SOURCE)
        .fetch_one(&pool)
        .await?,
        5
    );
    Ok(())
}

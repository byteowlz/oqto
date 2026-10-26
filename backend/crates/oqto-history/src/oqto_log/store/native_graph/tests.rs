use super::*;

pub(super) fn source(native: &str, workspace: &str) -> Vec<u8> {
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
#[test]
fn an_unwritten_parent_is_imported_as_a_branch_root_like_pi() -> Result<()> {
    let mut bytes = source("native", "/synthetic");
    bytes.extend_from_slice(
        format!(
            "{}\n",
            serde_json::json!({"type":"message","id":"orphan","parentId":"never-written","message":{"role":"assistant","content":[{"type":"text","text":"after the gap"}],"timestamp":4}})
        )
        .as_bytes(),
    );
    let graph = NativeGraph::parse(&bytes, "native", "/synthetic", Some("orphan"))?;
    let branch = graph.branch_messages("public");
    assert_eq!(branch.len(), 1);
    assert_eq!(branch[0].parent_turn_id, None);
    assert_eq!(
        NativeGraph::parse(&bytes, "native", "/synthetic", Some("a"))?
            .branch_messages("public")
            .len(),
        2
    );
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
#[tokio::test]
async fn live_branch_pages_equal_the_imported_projection_and_follow_the_live_head() -> Result<()> {
    use crate::oqto_log::projector::{
        project_native_branch_page, project_session_messages_page_for_workspace,
    };
    let home = tempfile::tempdir()?;
    let native = "22222222-3333-4444-5555-666666666666";
    let public = platform_id_for_external_id(native);
    let workspace = "/synthetic";
    // A longer session: twelve turns after the fork point, plus an aborted reply
    // that the stored projection hides.
    let mut lines: Vec<serde_json::Value> =
        vec![serde_json::json!({"type":"session","id":native,"cwd":workspace})];
    let mut parent = serde_json::Value::Null;
    for n in 0..12 {
        let id = format!("m{n}");
        let role = if n % 2 == 0 { "user" } else { "assistant" };
        lines.push(serde_json::json!({"type":"message","id":id,"parentId":parent,"message":{"role":role,"content":[{"type":"text","text":format!("turn {n}")}],"timestamp":1_700_000_000_000_u64 + n}}));
        parent = serde_json::json!(id);
    }
    lines.push(serde_json::json!({"type":"message","id":"aborted","parentId":"m11","message":{"role":"assistant","content":[{"type":"text","text":"cut"}],"stopReason":"aborted","timestamp":1_700_000_000_100_u64}}));
    lines.push(serde_json::json!({"type":"message","id":"side","parentId":"m5","message":{"role":"user","content":[{"type":"text","text":"other branch"}],"timestamp":1_700_000_000_200_u64}}));
    let bytes = lines
        .iter()
        .map(|v| format!("{v}\n"))
        .collect::<String>()
        .into_bytes();
    // The live head is "aborted", not the last line ("side").
    let graph = NativeGraph::parse(&bytes, native, workspace, Some("aborted"))?;
    let branch = graph.branch_messages(&public);
    assert_eq!(branch.len(), 13);
    merge(home.path(), "owner", &public, &graph).await?;
    let stored =
        project_session_messages_page_for_workspace(home.path(), workspace, &public, 5, None)
            .await?
            .context("stored page")?;
    let live = project_native_branch_page(&public, &branch, 5, None)?;
    let shape = |page: &oqto_protocol::projection::ProjectedChatMessagePage| -> Vec<(String, String, Option<String>, Vec<String>)> {
        page.messages
            .iter()
            .map(|m| (m.id.clone(), m.role.clone(), m.parent_id.clone(), m.parts.iter().map(|p| format!("{}:{:?}", p.part_type, p.text)).collect()))
            .collect()
    };
    assert_eq!(
        shape(&live),
        shape(&stored),
        "live page must equal the imported projection"
    );
    assert!(
        live.messages
            .iter()
            .all(|m| !m.parts.iter().any(|p| p.text.as_deref() == Some("cut")))
    );
    assert!(live.messages.iter().all(|m| {
        !m.parts
            .iter()
            .any(|p| p.text.as_deref() == Some("other branch"))
    }));
    assert_eq!(
        live.messages.last().map(|m| m.created_at),
        Some(1_700_000_000_011)
    );
    // Older pages walk back to the start without overlap.
    let mut seen: Vec<String> = live.messages.iter().map(|m| m.id.clone()).collect();
    let mut cursor = live.next_before.clone();
    while let Some(before) = cursor {
        let older = project_native_branch_page(&public, &branch, 5, Some(&before))?;
        let mut ids: Vec<String> = older.messages.iter().map(|m| m.id.clone()).collect();
        ids.extend(seen);
        seen = ids;
        cursor = older.next_before;
    }
    assert_eq!(seen.len(), 12);
    assert_eq!(
        seen.iter().collect::<std::collections::HashSet<_>>().len(),
        12
    );
    assert!(project_native_branch_page(&public, &branch, 5, Some("native:99")).is_err());
    assert!(project_native_branch_page(&public, &branch, 5, Some("12:0")).is_err());
    // A known empty head has no messages.
    let empty = NativeGraph::parse(&bytes, native, workspace, None)?;
    assert!(
        project_native_branch_page(&public, &empty.branch_messages(&public), 5, None)?
            .messages
            .is_empty()
    );
    Ok(())
}

use super::*;
use std::io::Cursor;

mod benchmark;

#[test]
fn indexed_pages_match_native_graph_projection_for_both_branches_and_empty_head() -> Result<()> {
    let bytes = crate::oqto_log::store::native_graph::tests::source("native", "/synthetic");
    let mut file = Cursor::new(&bytes);
    let index = NativeFileIndex::read(&mut file, bytes.len() as u64, "native", "/synthetic")?;
    for leaf in [Some("a"), Some("b"), None] {
        let graph = NativeGraph::parse(&bytes, "native", "/synthetic", leaf)?;
        let branch = graph.branch_messages("public");
        let (mut old_cursor, mut new_cursor) = (None, None);
        loop {
            let expected = project_native_branch_page("public", &branch, 1, old_cursor.as_deref())?;
            let actual = index.page(&mut file, "public", leaf, 1, new_cursor.as_deref())?;
            assert_eq!(
                serde_json::to_value(&actual.messages)?,
                serde_json::to_value(&expected.messages)?
            );
            assert_eq!(actual.has_more, expected.has_more);
            if !expected.has_more {
                break;
            }
            old_cursor = expected.next_before;
            new_cursor = actual.next_before;
        }
    }
    Ok(())
}

#[test]
fn a_cursor_cannot_cross_a_selected_branch_switch_or_public_session() -> Result<()> {
    let bytes = crate::oqto_log::store::native_graph::tests::source("native", "/synthetic");
    let mut file = Cursor::new(&bytes);
    let index = NativeFileIndex::read(&mut file, bytes.len() as u64, "native", "/synthetic")?;
    let newest = index.page(&mut file, "public", Some("a"), 1, None)?;
    let cursor = newest.next_before.context("older page cursor")?;
    assert!(
        index
            .page(&mut file, "public", Some("b"), 1, Some(&cursor))
            .is_err()
    );
    assert!(
        index
            .page(&mut file, "another-public", Some("a"), 1, Some(&cursor))
            .is_err()
    );
    Ok(())
}

#[test]
fn visibility_structural_ancestors_authored_json_and_time_match_existing_projection() -> Result<()>
{
    let mut bytes = crate::oqto_log::store::native_graph::tests::source("native", "/synthetic");
    for value in [
        serde_json::json!({"type":"message","id":"aborted","parentId":"a","message":{"role":"assistant","stopReason":"aborted","content":"invisible"}}),
        serde_json::json!({"type":"message","id":"bash","parentId":"aborted","message":{"role":"bashExecution","content":"invisible"}}),
        serde_json::json!({"type":"custom","id":"structure","parentId":"bash","data":{"message":"not a chat"}}),
        serde_json::json!({"type":"message","id":"answer","parentId":"structure","timestamp":"2026-10-25T20:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":"[]"},{"type":"text","text":"{\"authored\":true}"}],"extra":"preserved"}}),
    ] {
        bytes.extend_from_slice(format!("{value}\n").as_bytes());
    }
    let mut file = Cursor::new(&bytes);
    let index = NativeFileIndex::read(&mut file, bytes.len() as u64, "native", "/synthetic")?;
    let expected = project_native_branch_page(
        "public",
        &NativeGraph::parse(&bytes, "native", "/synthetic", Some("answer"))?
            .branch_messages("public"),
        50,
        None,
    )?;
    let actual = index.page(&mut file, "public", Some("answer"), 50, None)?;
    assert_eq!(
        serde_json::to_value(&actual.messages)?,
        serde_json::to_value(&expected.messages)?
    );
    assert_eq!(actual.messages.len(), 3);
    assert_eq!(actual.messages[2].created_at, 1792958400000);
    Ok(())
}

#[test]
fn malformed_graphs_wrong_headers_partial_heads_and_stale_records_fail_closed() -> Result<()> {
    let original = crate::oqto_log::store::native_graph::tests::source("native", "/synthetic");
    let text = std::str::from_utf8(&original)?;
    for bad in [
        text.replace("\"id\":\"b\"", "\"id\":\"a\""),
        text.replace("\"parentId\":\"meta\"", "\"parentId\":\"b\""),
        text.replace("\"parentId\":\"meta\"", "\"parentId\":\"a\""),
        text.replace("\"parentId\":null,", ""),
        text.replace("\"parentId\":null", "\"parentId\":[null]"),
        text.replace("\"parentId\":null", "\"parentId\":true"),
        text.replace("\"timestamp\":1", "\"timestamp\":\"wrong\""),
        text.replace("\"cwd\":\"/synthetic\"", "\"cwd\":\"/elsewhere\""),
    ] {
        let mut file = Cursor::new(bad.as_bytes());
        assert!(
            NativeFileIndex::read(&mut file, bad.len() as u64, "native", "/synthetic").is_err(),
            "{bad}"
        );
    }
    let mut partial = original.clone();
    partial.extend_from_slice(b"{\"id\":\"not-durable\"");
    let mut file = Cursor::new(&partial);
    let index = NativeFileIndex::read(&mut file, partial.len() as u64, "native", "/synthetic")?;
    assert!(
        index
            .page(&mut file, "public", Some("not-durable"), 50, None)
            .is_err()
    );
    assert!(index.page(&mut file, "public", Some("a"), 50, None).is_ok());
    assert!(
        index
            .page(&mut file, "public", Some("a"), 50, Some("native:1"))
            .is_err()
    );
    let changed = text.replace("same", "evil").into_bytes();
    assert!(
        index
            .page(&mut Cursor::new(changed), "public", Some("a"), 50, None)
            .is_err()
    );
    Ok(())
}

/// Pi starts a branch at an entry whose parent was never written (its tree treats
/// orphans as roots and path building stops there). A parent written later, or the
/// entry itself, is a link Pi would follow, so it is refused rather than cut.
#[test]
fn an_unwritten_parent_starts_the_branch_like_pi_but_a_later_parent_is_refused() -> Result<()> {
    let mut bytes = crate::oqto_log::store::native_graph::tests::source("native", "/synthetic");
    for value in [
        serde_json::json!({"type":"message","id":"orphan","parentId":"never-written","message":{"role":"assistant","content":[{"type":"text","text":"after the gap"}],"timestamp":4}}),
        serde_json::json!({"type":"message","id":"next","parentId":"orphan","message":{"role":"user","content":[{"type":"text","text":"continue"}],"timestamp":5}}),
    ] {
        bytes.extend_from_slice(format!("{value}\n").as_bytes());
    }
    let mut file = Cursor::new(&bytes);
    let index = NativeFileIndex::read(&mut file, bytes.len() as u64, "native", "/synthetic")?;
    let expected = project_native_branch_page(
        "public",
        &NativeGraph::parse(&bytes, "native", "/synthetic", Some("next"))?
            .branch_messages("public"),
        50,
        None,
    )?;
    let actual = index.page(&mut file, "public", Some("next"), 50, None)?;
    assert_eq!(
        serde_json::to_value(&actual.messages)?,
        serde_json::to_value(&expected.messages)?
    );
    assert_eq!(actual.messages.len(), 2);
    assert!(!actual.has_more);
    // The earlier, connected branch is still readable at its own head.
    assert_eq!(
        index
            .page(&mut file, "public", Some("a"), 50, None)?
            .messages
            .len(),
        2
    );
    let text = std::str::from_utf8(&bytes)?;
    for refused in [
        text.replace("\"parentId\":\"never-written\"", "\"parentId\":\"next\""),
        text.replace("\"parentId\":\"never-written\"", "\"parentId\":\"orphan\""),
    ] {
        let mut file = Cursor::new(refused.as_bytes());
        assert!(
            NativeFileIndex::read(&mut file, refused.len() as u64, "native", "/synthetic").is_err(),
            "{refused}"
        );
        assert!(
            NativeGraph::parse(refused.as_bytes(), "native", "/synthetic", Some("next")).is_err()
        );
    }
    Ok(())
}

#[test]
fn cursor_survives_growth_only_on_its_original_ancestor_path() -> Result<()> {
    let mut bytes = crate::oqto_log::store::native_graph::tests::source("native", "/synthetic");
    let index = NativeFileIndex::read(
        &mut Cursor::new(&bytes),
        bytes.len() as u64,
        "native",
        "/synthetic",
    )?;
    let cursor = index
        .page(&mut Cursor::new(&bytes), "public", Some("a"), 1, None)?
        .next_before
        .context("cursor")?;
    let next = serde_json::json!({"type":"message","id":"growing","parentId":"a","message":{"role":"assistant","content":"new"}});
    bytes.extend_from_slice(format!("{next}\n").as_bytes());
    let index = NativeFileIndex::read(
        &mut Cursor::new(&bytes),
        bytes.len() as u64,
        "native",
        "/synthetic",
    )?;
    let older = index.page(
        &mut Cursor::new(&bytes),
        "public",
        Some("growing"),
        50,
        Some(&cursor),
    )?;
    assert_eq!(older.messages.len(), 1);
    assert!(!older.has_more);
    assert!(
        index
            .page(
                &mut Cursor::new(&bytes),
                "public",
                Some("b"),
                50,
                Some(&cursor)
            )
            .is_err()
    );
    Ok(())
}

struct Metered {
    file: Cursor<Vec<u8>>,
    bytes: usize,
    seeks: usize,
}
impl Read for Metered {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self.file.read(out)?;
        self.bytes += n;
        Ok(n)
    }
}
impl Seek for Metered {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        self.seeks += 1;
        self.file.seek(to)
    }
}

#[test]
fn indexed_page_reads_only_its_payload_not_off_branch_or_older_payloads() -> Result<()> {
    let mut bytes = crate::oqto_log::store::native_graph::tests::source("native", "/synthetic");
    let padding = "x".repeat(16 * 1024);
    for n in 0..200 {
        let value = serde_json::json!({"type":"custom","id":format!("off-{n}"),"parentId":null,"data":padding});
        bytes.extend_from_slice(format!("{value}\n").as_bytes());
    }
    let length = bytes.len() as u64;
    let mut file = Metered {
        file: Cursor::new(bytes),
        bytes: 0,
        seeks: 0,
    };
    let index = NativeFileIndex::read(&mut file, length, "native", "/synthetic")?;
    assert!(
        file.bytes > 3 * 1024 * 1024,
        "cold metadata pass was exercised"
    );
    file.bytes = 0;
    file.seeks = 0;
    let page = index.page(&mut file, "public", Some("a"), 1, None)?;
    assert_eq!(page.messages.len(), 1);
    assert!(page.has_more);
    assert!(
        file.bytes < 1024,
        "only the selected record payload is read: {}",
        file.bytes
    );
    assert_eq!(file.seeks, 1);
    Ok(())
}

#[test]
fn page_byte_bound_shortens_pages_without_losing_messages() -> Result<()> {
    let mut bytes = format!(
        "{}\n",
        serde_json::json!({"type":"session","id":"native","cwd":"/synthetic"})
    )
    .into_bytes();
    let text = "x".repeat(3 * 1024 * 1024);
    for (id, parent) in [("a", None), ("b", Some("a")), ("c", Some("b"))] {
        let value = serde_json::json!({"type":"message","id":id,"parentId":parent,"message":{"role":"assistant","content":text}});
        bytes.extend_from_slice(format!("{value}\n").as_bytes());
    }
    let mut file = Cursor::new(&bytes);
    let index = NativeFileIndex::read(&mut file, bytes.len() as u64, "native", "/synthetic")?;
    let newest = index.page(&mut file, "public", Some("c"), 50, None)?;
    assert_eq!(newest.messages.len(), 2);
    let cursor = newest.next_before.context("byte-bounded cursor")?;
    let older = index.page(&mut file, "public", Some("c"), 50, Some(&cursor))?;
    assert_eq!(older.messages.len(), 1);
    assert!(!older.has_more);
    let ids: std::collections::HashSet<_> = newest
        .messages
        .iter()
        .chain(older.messages.iter())
        .map(|m| &m.id)
        .collect();
    assert_eq!(ids.len(), 3);
    Ok(())
}

#[test]
fn file_record_entry_and_cursor_bounds_fail_explicitly() -> Result<()> {
    let original = crate::oqto_log::store::native_graph::tests::source("native", "/synthetic");
    assert!(
        NativeFileIndex::read(
            &mut Cursor::new(&original),
            MAX_FILE_BYTES + 1,
            "native",
            "/synthetic"
        )
        .is_err()
    );
    let mut long = original.clone();
    long.extend(std::iter::repeat_n(b'x', MAX_RECORD_BYTES + 1));
    assert!(
        NativeFileIndex::read(
            &mut Cursor::new(&long),
            long.len() as u64,
            "native",
            "/synthetic"
        )
        .is_err()
    );
    let index = NativeFileIndex::read(
        &mut Cursor::new(&original),
        original.len() as u64,
        "native",
        "/synthetic",
    )?;
    for cursor in [
        "native:v2:999:message:wrong",
        "native:v2:1:message:wrong",
        "native:v2:no:bad",
        "native:1",
        "stored:1",
    ] {
        assert!(
            index
                .page(
                    &mut Cursor::new(&original),
                    "public",
                    Some("a"),
                    1,
                    Some(cursor)
                )
                .is_err()
        );
    }
    let mut empty = format!(
        "{}\n",
        serde_json::json!({"type":"session","id":"native","cwd":"/synthetic"})
    )
    .into_bytes();
    for n in 0..=MAX_ENTRIES {
        empty.extend_from_slice(
            format!("{{\"type\":\"custom\",\"id\":\"e{n}\",\"parentId\":null}}\n").as_bytes(),
        );
    }
    assert!(
        NativeFileIndex::read(
            &mut Cursor::new(&empty),
            empty.len() as u64,
            "native",
            "/synthetic"
        )
        .is_err()
    );
    Ok(())
}

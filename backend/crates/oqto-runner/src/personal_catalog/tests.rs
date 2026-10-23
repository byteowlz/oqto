use super::*;
use oqto_history::oqto_log::ops::OqtoLogSessionRow;
use serde_json::json;

fn row(platform: &str, external: &str, workspace: &str, title: Option<&str>) -> OqtoLogSessionRow {
    OqtoLogSessionRow {
        session_id: platform.into(),
        platform_id: platform.into(),
        external_id: Some(external.into()),
        user_id: "owner".into(),
        workspace_id: Some(workspace.into()),
        parent_session_id: None,
        forked_from_entry_id: None,
        forked_at: None,
        created_at: String::new(),
        updated_at: String::new(),
        messages: 0,
        title: title.map(Into::into),
        readable_id: None,
    }
}

fn session_file(root: &Path, workspace: &str, external: &str, lines: &[Value]) -> PathBuf {
    let directory = root.join(oqto_pi::session_files::cwd_to_safe_dirname(Path::new(
        workspace,
    )));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("2026-10-23T00-00-00-000Z_{external}.jsonl"));
    let mut body = String::new();
    for line in lines {
        body.push_str(&line.to_string());
        body.push('\n');
    }
    std::fs::write(&path, body).unwrap();
    path
}

fn header(external: &str, workspace: &str) -> Value {
    json!({"type":"session","version":3,"id":external,"cwd":workspace})
}

fn prompt(text: Value) -> Value {
    json!({"type":"message","message":{"role":"user","content":text}})
}

#[test]
fn first_prompt_and_latest_name_from_complete_lines() {
    let mut bytes = Vec::new();
    for line in [
        header("a", "/w"),
        json!({"type":"message","message":{"role":"assistant","content":"user said hi"}}),
        prompt(json!([{"type":"text","text":"Fix the  login\nflow"}, {"type":"image"}])),
        prompt(json!("second prompt")),
        json!({"type":"session_info","name":"First name"}),
        json!({"type":"session_info","name":"Renamed [good-mole-corn]"}),
    ] {
        bytes.extend(line.to_string().bytes());
        bytes.push(b'\n');
    }
    let summary = summarize(&bytes, true);
    assert_eq!(summary.first_prompt.as_deref(), Some("Fix the login flow"));
    assert_eq!(summary.name.as_deref(), Some("Renamed [good-mole-corn]"));
    assert_eq!(display_name(summary.name.as_deref().unwrap()), "Renamed");
}

#[test]
fn cut_lines_are_never_parsed() {
    let complete = json!({"type":"session_info","name":"Kept"}).to_string();
    let cut = json!({"type":"session_info","name":"Cut"}).to_string();
    // Tail read: the first line is partial; the last is unterminated.
    let tail = format!("{}\n{complete}\n{}", &cut[5..], &cut[..cut.len() - 3]);
    assert_eq!(
        summarize(tail.as_bytes(), false).name.as_deref(),
        Some("Kept")
    );
    // A tail never supplies the first prompt.
    let prompts = format!("x\n{}\n", prompt(json!("late")));
    assert_eq!(summarize(prompts.as_bytes(), false).first_prompt, None);
}

#[test]
fn long_prompts_are_bounded_and_names_keep_non_suffix_brackets() {
    let long = "word ".repeat(100);
    let bytes = format!("{}\n", prompt(json!(long)));
    let title = summarize(bytes.as_bytes(), true).first_prompt.unwrap();
    assert_eq!(title.chars().count(), TITLE_CHARS + 1);
    assert!(title.ends_with('\u{2026}'));
    assert_eq!(display_name("Release [v2]"), "Release [v2]");
    assert_eq!(display_name("  Plain  "), "Plain");
}

#[tokio::test]
async fn enrich_titles_orders_by_activity_and_keeps_canonical_titles() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = dir.path().join("sessions");
    let sockets = dir.path().join("sockets");
    std::fs::create_dir_all(&sockets).unwrap();
    let old = session_file(
        &sessions,
        "/w/a",
        "old",
        &[header("old", "/w/a"), prompt(json!("Older work"))],
    );
    let new = session_file(
        &sessions,
        "/w/b",
        "new",
        &[
            header("new", "/w/b"),
            prompt(json!("Newer work")),
            json!({"type":"session_info","name":"Named [red-fox-den]"}),
        ],
    );
    session_file(
        &sessions,
        "/w/a",
        "kept",
        &[header("kept", "/w/a"), prompt(json!("Ignored"))],
    );
    let set = |path: &Path, secs: u64| {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    };
    set(&old, 1_000);
    set(&new, 2_000);
    let rows = vec![
        row("oqto-old", "old", "/w/a", None),
        row("oqto-new", "new", "/w/b", None),
        row("oqto-kept", "kept", "/w/a", Some("Canonical title")),
        row("oqto-gone", "gone", "/w/a", None),
    ];
    let mut data = json!({"sessions": [
        {"id":"oqto-old","title":null,"workspace":"/w/a"},
        {"id":"oqto-gone","title":null,"workspace":"/w/a"},
        {"id":"oqto-kept","title":"Canonical title","workspace":"/w/a"},
        {"id":"oqto-new","title":null,"workspace":"/w/b"},
    ], "read_only": true});
    let roots = NativeRoots {
        session_root: sessions,
        socket_root: sockets,
    };
    // Nested designated roots: the longest match wins.
    let workspace_roots = [PathBuf::from("/w"), PathBuf::from("/w/b")];
    enrich(&mut data, &rows, &roots, &workspace_roots).await;
    let listed = data["sessions"].as_array().unwrap();
    let ids: Vec<_> = listed.iter().map(|s| s["id"].as_str().unwrap()).collect();
    // "kept" has the newest mtime (just written); the missing file sorts last.
    assert_eq!(ids, ["oqto-kept", "oqto-new", "oqto-old", "oqto-gone"]);
    assert_eq!(listed[0]["display_title"], "Canonical title");
    assert_eq!(listed[0]["title"], "Canonical title");
    assert_eq!(listed[1]["display_title"], "Named");
    assert_eq!(listed[1]["title"], Value::Null);
    assert_eq!(listed[1]["activity_at_ms"], 2_000_000);
    assert_eq!(listed[2]["display_title"], "Older work");
    assert!(listed[3].get("display_title").is_none());
    assert!(
        listed
            .iter()
            .all(|s| s["live"] == false && s["streaming"] == false)
    );
    assert_eq!(listed[0]["workspace_root"], "/w");
    assert_eq!(listed[1]["workspace_root"], "/w/b");
    assert_eq!(data["read_only"], true);
}

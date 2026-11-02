use super::*;

/// Keep the source lifetime and its matching read identity together.
fn fixture() -> Result<(tempfile::TempDir, Read)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("native.jsonl");
    write(&path, "first")?;
    Ok((
        dir,
        Read {
            path,
            native: "native".into(),
            workspace: "/synthetic".into(),
            public: "public".into(),
            leaf: Some("a".into()),
            before: None,
        },
    ))
}
fn write(path: &std::path::Path, text: &str) -> Result<()> {
    let header = serde_json::json!({"type":"session","id":"native","cwd":"/synthetic"});
    let message = serde_json::json!({"type":"message","id":"a","parentId":null,"message":{"role":"user","content":text}});
    std::fs::write(path, format!("{header}\n{message}\n"))?;
    Ok(())
}

#[test]
fn cache_invalidates_on_same_length_rewrite_append_truncate_and_replacement() -> Result<()> {
    use std::io::Write;
    let (dir, request) = fixture()?;
    let path = &request.path;
    let length = std::fs::metadata(path)?.len();
    assert!(read(request.clone())?.to_string().contains("first"));
    assert!(read(request.clone())?.to_string().contains("first"));
    write(path, "other")?;
    assert_eq!(std::fs::metadata(path)?.len(), length);
    let changed = read(request.clone())?.to_string();
    assert!(changed.contains("other") && !changed.contains("first"));
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    writeln!(
        file,
        "{{\"type\":\"custom\",\"id\":\"a\",\"parentId\":null}}"
    )?;
    drop(file);
    assert!(
        read(request.clone()).is_err(),
        "duplicate appended IDs invalidate the cached index"
    );
    std::fs::write(
        path,
        "{\"type\":\"session\",\"id\":\"native\",\"cwd\":\"/synthetic\"}\n",
    )?;
    assert!(
        read(request.clone()).is_err(),
        "truncated source no longer holds the selected head"
    );
    let replacement = dir.path().join("replacement");
    write(&replacement, "replacement")?;
    std::fs::rename(replacement, path)?;
    assert!(read(request.clone())?.to_string().contains("replacement"));
    assert!(INDEXES.lock().map_err(|_| anyhow::anyhow!("cache"))?.len() <= INDEX_CACHE);
    Ok(())
}

#[test]
fn cache_never_crosses_native_workspace_identity_or_a_symlink_source() -> Result<()> {
    let (dir, request) = fixture()?;
    assert!(read(request.clone()).is_ok());
    let mut wrong = request.clone();
    wrong.native = "other-native".into();
    assert!(read(wrong).is_err());
    let mut wrong = request.clone();
    wrong.workspace = "/other-workspace".into();
    assert!(read(wrong).is_err());
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(&request.path, &alias)?;
    assert!(
        read(Read {
            path: alias,
            ..request
        })
        .is_err()
    );
    Ok(())
}

#[test]
fn actual_native_history_page_has_refs_and_a_screenshot_transfer_budget() -> Result<()> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let (_dir, mut request) = fixture()?;
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(640, 480).write_to(&mut png, image::ImageFormat::Png)?;
    let mut bytes = png.into_inner();
    bytes.resize(1024 * 1024, 0);
    let encoded = STANDARD.encode(&bytes);
    let mut source = format!(
        "{}\n",
        serde_json::json!({"type":"session","id":"native","cwd":"/synthetic"})
    );
    let mut parent: Option<&str> = None;
    for id in ["a", "b", "c"] {
        source.push_str(&format!("{}\n",serde_json::json!({"type":"message","id":id,"parentId":parent,
            "message":{"role":"toolResult","toolCallId":format!("call-{id}"),"toolName":"read","isError":false,
            "content":[{"type":"image","data":encoded,"mimeType":"image/png"}]}})));
        parent = Some(id);
    }
    std::fs::write(&request.path, &source)?;
    request.leaf = Some("c".into());
    let page = read(request.clone())?;
    let wire = serde_json::to_vec(&page)?;
    assert!(
        wire.len() < 16 * 1024,
        "actual projected screenshot page exceeded transfer budget: {}",
        wire.len()
    );
    assert_eq!(page["messages"].as_array().context("messages")?.len(), 3);
    assert!(String::from_utf8(wire)?.contains("media_ref"));
    assert_eq!(
        std::fs::read_to_string(&request.path)?,
        source,
        "projection must not rewrite Pi JSONL"
    );
    Ok(())
}

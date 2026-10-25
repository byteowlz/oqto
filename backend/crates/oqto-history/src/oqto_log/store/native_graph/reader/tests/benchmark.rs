use super::*;
use std::io::{BufWriter, Write};

#[test]
#[ignore = "Explicit release-only synthetic 327MB index/page timing probe"]
fn synthetic_large_index_and_cached_page_timings() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.jsonl");
    let mut writer = BufWriter::new(std::fs::File::create(&path)?);
    writeln!(
        writer,
        "{{\"type\":\"session\",\"id\":\"native\",\"cwd\":\"/synthetic\"}}"
    )?;
    for n in 0..300 {
        let parent = (n > 0).then(|| format!("m{}", n - 1));
        let value = serde_json::json!({"type":"message","id":format!("m{n}"),"parentId":parent,
            "message":{"role":"assistant","content":format!("message-{n}")}});
        writeln!(writer, "{value}")?;
    }
    let padding = "x".repeat(64 * 1024);
    for n in 0..5000 {
        writeln!(
            writer,
            "{}",
            serde_json::json!({"type":"message","id":format!("off-{n}"),"parentId":null,"message":{"role":"assistant","content":padding}})
        )?;
    }
    writer.flush()?;
    drop(writer);
    let mut file = std::fs::File::open(&path)?;
    let length = file.metadata()?.len();
    assert!(length > 320_000_000 && length < 350_000_000);
    let start = Instant::now();
    let index = NativeFileIndex::read(&mut file, length, "native", "/synthetic")?;
    let cold = start.elapsed();
    let mut cursor = None;
    let start = Instant::now();
    let mut messages = 0;
    for _ in 0..6 {
        let page = index.page(&mut file, "public", Some("m299"), 50, cursor.as_deref())?;
        assert_eq!(page.messages.len(), 50);
        messages += page.messages.len();
        cursor = page.next_before;
    }
    assert_eq!(messages, 300);
    assert!(cursor.is_none());
    eprintln!(
        "synthetic bytes={length}; index_entries={}; cold_index={cold:?}; six_indexed_pages={:?}",
        index.entries.len(),
        start.elapsed()
    );
    Ok(())
}

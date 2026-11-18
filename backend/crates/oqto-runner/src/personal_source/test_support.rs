//! Synthetic source files shared by source admission and discovery actor tests.
use super::*;

pub(crate) fn fixture() -> Result<(tempfile::TempDir, PersonalSource, PathBuf)> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().canonicalize()?;
    let home = root.join("service");
    let sessions = root.join("native/.pi/agent/sessions");
    let work = root.join("byteowlz/work");
    let sockets = root.join("terminal-tmp");
    for path in [&home, &sessions, &work, &sockets] {
        std::fs::create_dir_all(path)?;
    }
    let source = PersonalSource::new(home, sessions, sockets, vec![root.join("byteowlz")])?;
    Ok((dir, source, work))
}

pub(crate) fn write(source: &PersonalSource, work: &Path, native: &str) -> Result<PathBuf> {
    let dir = source
        .session_root()
        .join(oqto_pi::session_files::cwd_to_safe_dirname(work));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("2026-10-19_{native}.jsonl"));
    std::fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::json!({"type":"session","id":native,"cwd":work})
        ),
    )?;
    Ok(path)
}

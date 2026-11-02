//! Personal native media admission. Shared projection/encoding lives in oqto-media;
//! native Pi JSONL and canonical history are never rewritten.
use anyhow::{Context, Result, ensure};
pub(crate) use oqto_media::inline::project;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::{Arc, LazyLock},
};
const MAX_RECORD: usize = 16 * 1024 * 1024;
const MAX_FILE: u64 = 512 * 1024 * 1024;
static WORKERS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(2)));

/// Resolve ONLY in the already-admitted native file, never a global blob lookup.
fn resolve(path: &Path, hash: &str, thumbnail: bool) -> Result<Value> {
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid media hash"
    );
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file()
            && before.uid() == rustix::process::geteuid().as_raw()
            && before.len() <= MAX_FILE,
        "unsafe media source"
    );
    let signature = |m: &std::fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    let mut reader = BufReader::new(&mut file);
    let mut found = None;
    loop {
        let mut line = Vec::new();
        let n = reader
            .by_ref()
            .take(MAX_RECORD as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        ensure!(n <= MAX_RECORD, "native media record exceeds bound");
        let record: Value = serde_json::from_slice(&line)?;
        if record["type"] != "message" {
            continue;
        }
        // Conversation content only: never expose extension/control state by hash.
        let Some(content) = record
            .get("message")
            .and_then(|message| message.get("content"))
        else {
            continue;
        };
        if let Some(bytes) = oqto_media::inline::find_image(content, hash)? {
            found = Some(bytes);
            break;
        }
    }
    drop(reader);
    let bytes = found.context("media not found in admitted Session")?;
    // Validate even on a cache hit, independently of the derived cache.
    oqto_media::inline::describe(&bytes)?;
    let result = if thumbnail {
        let cache = if cfg!(test) {
            None
        } else {
            Some(oqto_media::cache::config()?)
        };
        oqto_media::thumbnail(&bytes, 768, cache)?
    } else {
        oqto_media::inline::full(&bytes)?
    };
    let published = std::fs::symlink_metadata(path)?;
    ensure!(
        published.is_file()
            && signature(&before) == signature(&file.metadata()?)
            && signature(&before) == signature(&published),
        "media source changed during read"
    );
    Ok(result)
}
pub(crate) async fn read(
    home: &Path,
    roots: &[std::path::PathBuf],
    source: &crate::personal_source::PersonalSource,
    session: &str,
    hash: String,
    thumbnail: bool,
) -> Result<Value> {
    ensure!(
        !session.is_empty() && session.len() <= 256,
        "invalid Session identity"
    );
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid media hash"
    );
    ensure!(
        source.home == home && source.workspace_roots() == roots,
        "native source authority mismatch"
    );
    source.validate()?;
    let row = crate::history_read::resolve_authorized_session(home, roots, session).await?;
    let external = row.external_id.context("native identity unavailable")?;
    let workspace = row.workspace_id.context("work directory unavailable")?;
    let file = oqto_pi::session_files::find_unique_session_file_in(
        source.session_root(),
        &external,
        Path::new(&workspace),
    )?
    .context("native media source unavailable")?;
    let validate = async || -> Result<()> {
        let header = source.header(&file)?;
        ensure!(
            header.external_id == external && header.workspace.to_str() == Some(workspace.as_str()),
            "native media identity mismatch"
        );
        oqto_history::oqto_log::ops::native_source::validate(
            home,
            session,
            &external,
            &workspace,
            &header.accepted_pins(),
        )
        .await
    };
    validate().await?;
    let permit = WORKERS.clone().acquire_owned().await?;
    let path = file.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        resolve(&path, &hash, thumbnail)
    })
    .await??;
    validate().await?;
    Ok(result)
}
#[cfg(test)]
mod tests;

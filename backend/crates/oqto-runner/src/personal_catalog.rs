//! Display metadata for the personal-host Session catalog.
//!
//! Header-only native discovery records identities without titles or activity,
//! so every row read "Untitled" in canonical order. This adds, per listed
//! Session, a readable `display_title` (live Pi name, else the latest
//! `session_info` name, else the first prompt), `activity_at_ms` (native file
//! modification time) and `live`/`streaming` (an attached pi-tui-rpc Pi has it
//! open), plus the designated `workspace_root` it belongs to, and orders live
//! and recent Sessions first. Reads are bounded and
//! read-only; canonical `title`, identity and history authority are unchanged.
use oqto_history::oqto_log::ops::OqtoLogSessionRow;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

/// Bytes read from the start (first prompt, early names) and end (latest name).
const HEAD_BYTES: u64 = 64 * 1024;
const TAIL_BYTES: u64 = 64 * 1024;
/// Only the most recently active files are read; older rows keep metadata only.
const SUMMARIZED: usize = 400;
const TITLE_CHARS: usize = 120;
const CACHE_ENTRIES: usize = 8192;
const DEADLINE: Duration = Duration::from_secs(4);
const LIVE_TTL: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeSummary {
    pub name: Option<String>,
    pub first_prompt: Option<String>,
}

type FileKey = (u64, u64, u64, i64, i64);
static SUMMARIES: LazyLock<Mutex<HashMap<PathBuf, (FileKey, NativeSummary)>>> =
    LazyLock::new(Default::default);
type LiveMap = HashMap<String, crate::tui_rpc::LiveSession>;
static LIVE: LazyLock<tokio::sync::Mutex<Option<(PathBuf, Instant, LiveMap)>>> =
    LazyLock::new(Default::default);

/// Native roots the catalog may read; both are designated by the host owner.
#[derive(Clone, Debug)]
pub(crate) struct NativeRoots {
    pub session_root: PathBuf,
    pub socket_root: PathBuf,
}

pub(crate) async fn enrich(
    data: &mut Value,
    rows: &[OqtoLogSessionRow],
    roots: &NativeRoots,
    workspace_roots: &[PathBuf],
) {
    let Some(sessions) = data.get_mut("sessions").and_then(Value::as_array_mut) else {
        return;
    };
    let natives: HashMap<&str, (&str, &str)> = rows
        .iter()
        .filter_map(|row| {
            Some((
                row.platform_id.as_str(),
                (row.external_id.as_deref()?, row.workspace_id.as_deref()?),
            ))
        })
        .collect();
    let wanted: Vec<(String, String)> = sessions
        .iter()
        .filter_map(|session| natives.get(session["id"].as_str()?))
        .map(|(external, workspace)| ((*external).to_owned(), (*workspace).to_owned()))
        .collect();
    let session_root = roots.session_root.clone();
    let files = tokio::time::timeout(
        DEADLINE,
        tokio::task::spawn_blocking(move || native_files(&session_root, &wanted)),
    );
    let live = live(roots);
    let (files, live) = tokio::join!(files, live);
    let files = match files {
        Ok(Ok(files)) => files,
        _ => HashMap::new(),
    };
    for session in sessions.iter_mut() {
        let Some(object) = session.as_object_mut() else {
            continue;
        };
        let Some((external, workspace)) = object
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| natives.get(id))
        else {
            continue;
        };
        let file = files.get(*external);
        let running = live.get(*external);
        let stored = object
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned);
        let title = stored
            .or_else(|| {
                running
                    .and_then(|live| live.name.as_deref())
                    .map(display_name)
            })
            .or_else(|| {
                file.and_then(|f| f.summary.name.as_deref())
                    .map(display_name)
            })
            .or_else(|| file.and_then(|f| f.summary.first_prompt.clone()));
        if let Some(title) = title {
            object.insert("display_title".into(), title.into());
        }
        if let Some(file) = file {
            object.insert("activity_at_ms".into(), file.modified_ms.into());
        }
        // Longest designated root wins when roots nest.
        if let Some(root) = workspace_roots
            .iter()
            .filter(|root| Path::new(workspace).starts_with(root))
            .max_by_key(|root| root.as_os_str().len())
            .and_then(|root| root.to_str())
        {
            object.insert("workspace_root".into(), root.into());
        }
        object.insert("live".into(), running.is_some().into());
        object.insert(
            "streaming".into(),
            running.is_some_and(|live| live.streaming).into(),
        );
    }
    // Live first, then most recent native activity; canonical order breaks ties.
    sessions.sort_by_key(|session| {
        (
            std::cmp::Reverse(session["live"] == true),
            std::cmp::Reverse(session["activity_at_ms"].as_i64().unwrap_or(i64::MIN)),
        )
    });
}

async fn live(roots: &NativeRoots) -> LiveMap {
    let mut cached = LIVE.lock().await;
    if let Some((root, at, live)) = cached.as_ref()
        && *root == roots.socket_root
        && at.elapsed() < LIVE_TTL
    {
        return live.clone();
    }
    let live = tokio::time::timeout(
        DEADLINE,
        crate::tui_rpc::live_sessions(&roots.socket_root, &roots.session_root),
    )
    .await
    .unwrap_or_default();
    *cached = Some((roots.socket_root.clone(), Instant::now(), live.clone()));
    live
}

struct NativeFile {
    modified_ms: i64,
    summary: NativeSummary,
}

/// One directory read per Workspace, a stat per Session, and bounded reads of
/// the most recently modified files only.
fn native_files(session_root: &Path, wanted: &[(String, String)]) -> HashMap<String, NativeFile> {
    use std::os::unix::fs::MetadataExt;
    let started = Instant::now();
    let mut by_workspace: HashMap<&str, Vec<&str>> = HashMap::new();
    for (external, workspace) in wanted {
        by_workspace.entry(workspace).or_default().push(external);
    }
    let mut found = Vec::new();
    for (workspace, externals) in by_workspace {
        let directory = session_root.join(oqto_pi::session_files::cwd_to_safe_dirname(Path::new(
            workspace,
        )));
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut names: HashMap<String, PathBuf> = HashMap::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".jsonl")
                && let Some((_, id)) = stem.split_once('_')
            {
                names.insert(id.to_owned(), entry.path());
            }
        }
        for external in externals {
            let Some(path) = names.remove(external) else {
                continue;
            };
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|age| age.as_millis() as i64)
                .unwrap_or(0);
            let key = (
                meta.dev(),
                meta.ino(),
                meta.len(),
                meta.mtime(),
                meta.mtime_nsec(),
            );
            found.push((external.to_owned(), path, key, modified_ms));
        }
    }
    found.sort_by_key(|(_, _, _, modified)| std::cmp::Reverse(*modified));
    let mut result = HashMap::new();
    for (index, (external, path, key, modified_ms)) in found.into_iter().enumerate() {
        let summary = if index < SUMMARIZED && started.elapsed() < DEADLINE {
            summary(&path, key)
        } else {
            NativeSummary::default()
        };
        result.insert(
            external,
            NativeFile {
                modified_ms,
                summary,
            },
        );
    }
    result
}

fn summary(path: &Path, key: FileKey) -> NativeSummary {
    if let Some((cached_key, summary)) = SUMMARIES
        .lock()
        .ok()
        .and_then(|cache| cache.get(path).cloned())
        && cached_key == key
    {
        return summary;
    }
    let summary = read_summary(path, key.2).unwrap_or_default();
    if let Ok(mut cache) = SUMMARIES.lock() {
        if cache.len() >= CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(path.to_owned(), (key, summary.clone()));
    }
    summary
}

fn read_summary(path: &Path, len: u64) -> std::io::Result<NativeSummary> {
    let mut file = std::fs::File::open(path)?;
    let mut head = Vec::new();
    (&mut file).take(HEAD_BYTES).read_to_end(&mut head)?;
    let mut summary = summarize(&head, true);
    if len > HEAD_BYTES {
        let start = len.saturating_sub(TAIL_BYTES).max(HEAD_BYTES);
        file.seek(SeekFrom::Start(start))?;
        let mut tail = Vec::new();
        file.take(TAIL_BYTES).read_to_end(&mut tail)?;
        // The tail's first line is usually partial; only complete lines parse.
        if let Some(name) = summarize(&tail, false).name {
            summary.name = Some(name);
        }
    }
    Ok(summary)
}

/// Complete JSONL lines only; the latest `session_info` name wins, and the
/// first user prompt is taken from the head.
pub(crate) fn summarize(bytes: &[u8], from_start: bool) -> NativeSummary {
    let mut summary = NativeSummary::default();
    let mut lines = bytes.split(|byte| *byte == b'\n').peekable();
    if !from_start {
        lines.next();
    }
    while let Some(line) = lines.next() {
        // An unterminated last line may be cut by the read bound.
        if lines.peek().is_none() && bytes.last() != Some(&b'\n') {
            break;
        }
        let is_name = contains(line, b"\"session_info\"");
        let is_prompt = from_start && summary.first_prompt.is_none() && contains(line, b"\"user\"");
        if !is_name && !is_prompt {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if value["type"] == "session_info" {
            if let Some(name) = value["name"]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty())
            {
                summary.name = Some(name.chars().take(240).collect());
            }
        } else if is_prompt
            && value["type"] == "message"
            && value["message"]["role"] == "user"
            && let Some(text) = prompt_text(&value["message"]["content"])
        {
            summary.first_prompt = Some(text);
        }
    }
    summary
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn prompt_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|part| part["type"] == "text")
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    let mut title: String = line.chars().take(TITLE_CHARS).collect();
    if line.chars().count() > TITLE_CHARS {
        title.push('\u{2026}');
    }
    Some(title)
}

/// Session names carry a trailing readable-id suffix (`Name [word-word-word]`);
/// the catalog shows the name itself.
pub(crate) fn display_name(name: &str) -> String {
    let trimmed = name.trim();
    if let Some(start) = trimmed.rfind(" [")
        && trimmed.ends_with(']')
    {
        let suffix = &trimmed[start + 2..trimmed.len() - 1];
        let words: Vec<_> = suffix.split('-').collect();
        if words.len() == 3
            && words
                .iter()
                .all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase()))
        {
            return trimmed[..start].trim_end().to_owned();
        }
    }
    trimmed.to_owned()
}

#[cfg(test)]
mod tests;

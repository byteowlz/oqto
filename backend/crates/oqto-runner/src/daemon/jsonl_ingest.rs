//! Serialized background ingestion of Pi JSONL session files into oqto-log.
//!
//! Pi owns the JSONL session files and can be driven outside Oqto (bare `pi`
//! continuing a session), so oqto-log needs a way to notice out-of-band
//! changes. That detection must never happen on the read path: chat reads are
//! read-only and serve oqto-log directly.
//!
//! This module is the single writer that reconciles JSONL into oqto-log:
//! - A per-file ingest cursor (size + mtime, stored next to the session
//!   index) turns "did anything change?" into one `stat()` call.
//! - A periodic sweep walks all session files as a safety net; chat opens
//!   nudge a specific session for prompt pickup.
//! - Files modified within the quiescence window are skipped: live sessions
//!   are fed to oqto-log by the event pipeline, and half-written files
//!   should not be parsed.
//! - All ingestion runs on one task, so concurrent repairs can no longer
//!   contend on workspace database locks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use log::{debug, warn};
use oqto_history::oqto_log::index::{self, IngestCursor};
use oqto_history::oqto_log::store::platform_id_for_external_id;
use oqto_history::oqto_log::{ops, store};
use tokio::sync::mpsc;

const SWEEP_INTERVAL: Duration = Duration::from_secs(300);
const QUIESCENCE: Duration = Duration::from_secs(30);
const NUDGE_DEDUPE_WINDOW: Duration = Duration::from_secs(10);

/// Handle for nudging the ingest task about a session that was just read.
#[derive(Clone)]
pub struct IngestHandle {
    tx: mpsc::Sender<Nudge>,
    pub(crate) source: Option<std::sync::Arc<crate::personal_source::PersonalSource>>,
}
#[derive(Clone)]
pub(crate) struct NativeObservation {
    pub binding: crate::tui_rpc::ObservationBinding,
    pub user_id: String,
    pub fence: oqto_personal::DeviceFence,
}
impl crate::tui_rpc::ObservationAuthority for NativeObservation {
    fn resolve(
        &self,
        authority: &str,
        public: &str,
    ) -> anyhow::Result<crate::tui_rpc::ObservationBinding> {
        anyhow::ensure!(
            authority == self.binding.authority_id && public == self.binding.public_session_id,
            "Native source identity denied"
        );
        self.revalidate(&self.binding)?;
        Ok(self.binding.clone())
    }
    fn revalidate(&self, binding: &crate::tui_rpc::ObservationBinding) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.fence.is_current()
                && binding.grant_generation == self.fence.generation()
                && binding.authority_id == self.binding.authority_id
                && binding.public_session_id == self.binding.public_session_id
                && binding.external_session_id == self.binding.external_session_id,
            "Native source grant ended"
        );
        Ok(())
    }
}
enum Nudge {
    Legacy(String),
    Native(Box<NativeObservation>),
}

impl IngestHandle {
    /// Ask the ingest task to check one session's JSONL file. Fire-and-forget.
    pub fn nudge(&self, session_or_external_id: &str) {
        if !session_or_external_id.is_empty() && session_or_external_id.len() <= 256 {
            let _ = self
                .tx
                .try_send(Nudge::Legacy(session_or_external_id.to_string()));
        }
    }
    /// Bounded, best-effort nudge from an already admitted existing-Pi snapshot.
    /// Not a synchronous repair or input replay; a dropped nudge may be repeated.
    pub(crate) fn nudge_native(&self, source: NativeObservation) -> bool {
        self.tx.try_send(Nudge::Native(Box::new(source))).is_ok()
    }
}

/// Spawn the background ingest task. Outside a tokio runtime (unit tests
/// constructing a `Runner` directly) the task is not spawned and nudges are
/// no-ops.
pub fn spawn() -> IngestHandle {
    spawn_with_source(None)
}

pub(crate) fn spawn_with_source(
    source: Option<std::sync::Arc<crate::personal_source::PersonalSource>>,
) -> IngestHandle {
    let (tx, rx) = mpsc::channel(128);
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(run(rx, source.clone()));
    }
    IngestHandle { tx, source }
}

#[cfg(test)]
pub(crate) fn spawn_for_test(home: PathBuf, user_id: String) -> IngestHandle {
    let (tx, rx) = mpsc::channel(128);
    tokio::spawn(run_at(rx, home, user_id, None));
    IngestHandle { tx, source: None }
}
async fn run(
    rx: mpsc::Receiver<Nudge>,
    source: Option<std::sync::Arc<crate::personal_source::PersonalSource>>,
) {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let user_id = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

    run_at(rx, home, user_id, source).await;
}
async fn run_at(
    mut rx: mpsc::Receiver<Nudge>,
    home: PathBuf,
    user_id: String,
    source: Option<std::sync::Arc<crate::personal_source::PersonalSource>>,
) {
    let mut tick = tokio::time::interval(SWEEP_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut recent: HashMap<String, Instant> = HashMap::new();
    let mut discovery: Option<Discovery> = None;
    let mut legacy = std::collections::VecDeque::new();
    let mut legacy_ids = std::collections::VecDeque::new();

    loop {
        tokio::select! {
            // A queued admitted head observation runs before another discovery
            // chunk. There is still exactly one writer, never a second task.
            biased;
            nudged = rx.recv() => {
                let Some(id) = nudged else {
                    return;
                };
                let mut pending = vec![id];
                while let Ok(more) = rx.try_recv() { pending.push(more); }
                let mut ids = Vec::new();
                let mut native = HashMap::new();
                for nudge in pending {
                    match nudge {
                        Nudge::Legacy(id) => ids.push(id),
                        Nudge::Native(source) => { native.insert(source.binding.public_session_id.clone(), source); }
                    }
                }
                for observation in native.into_values() {
                    if let Err(error) = ingest_native(&home, source.as_deref(), *observation).await { debug!("Native graph convergence deferred: {error:#}"); }
                }
                ids.sort();
                ids.dedup();
                let now = Instant::now();
                recent.retain(|_, seen| now.duration_since(*seen) < NUDGE_DEDUPE_WINDOW);
                let mut discovery_requested = false;
                for id in ids {
                    if recent.contains_key(&id) {
                        continue;
                    }
                    recent.insert(id.clone(), now);
                    discovery_requested = true;
                    if legacy_ids.len() < 128 { legacy_ids.push_back(id); }
                }
                if discovery_requested && discovery.is_none() && let Some(source) = &source {
                    discovery = Discovery::start(source).await.unwrap_or_else(|error| {
                        debug!("Native catalog nudge deferred: {error:#}"); None
                    });
                }
            }
            _ = tick.tick() => {
                if discovery.is_none() && let Some(source) = &source {
                    discovery = Discovery::start(source).await.unwrap_or_else(|error| {
                        debug!("Native catalog discovery deferred: {error:#}"); None
                    });
                }
                legacy = list_session_files(&home).into();
            }
            _ = std::future::ready(()), if discovery.is_some() || !legacy_ids.is_empty() || !legacy.is_empty() => {
                if let Some(pass) = &mut discovery {
                    if pass.step(&user_id).await { discovery = None; }
                } else if let Some(id) = legacy_ids.pop_front() {
                    handle_nudge(&home, &user_id, &id).await;
                } else if let Some(path) = legacy.pop_front()
                    && let Some(external) = super::server::parse_pi_session_id_from_path(&path) {
                    maybe_ingest(&home, &user_id, &external, &path).await;
                }
            }
        }
    }
}

/// Additional native discovery never reads message bodies or sets a stat cursor.
/// Only a fresh admitted socket observation can cause graph ingestion.
const DISCOVERY_CHUNK: usize = 16;
const DISCOVERY_SLICE: Duration = Duration::from_millis(25);

struct Discovery {
    home: PathBuf,
    headers: std::vec::IntoIter<crate::personal_source::NativeHeader>,
    inventory: ops::native_source::DiscoveryBatch,
}
impl Discovery {
    async fn start(
        source: &crate::personal_source::PersonalSource,
    ) -> anyhow::Result<Option<Self>> {
        let owned = source.clone();
        let (headers, defaults) = tokio::task::spawn_blocking(move || {
            Ok::<_, anyhow::Error>((owned.headers()?, list_session_files(&owned.home)))
        })
        .await??;
        let mut counts = HashMap::new();
        for header in &headers {
            *counts.entry(header.external_id.clone()).or_insert(0usize) += 1;
        }
        let headers: Vec<_> = headers
            .into_iter()
            .filter(|header| {
                let suffix = format!("_{}.jsonl", header.external_id);
                counts[&header.external_id] == 1
                    && !defaults.iter().any(|path| {
                        path != &header.path
                            && path
                                .file_name()
                                .is_some_and(|name| name.to_string_lossy().ends_with(&suffix))
                    })
            })
            .collect();
        if headers.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            home: source.home.clone(),
            headers: headers.into_iter(),
            inventory: ops::native_source::DiscoveryBatch::open(&source.home).await?,
        }))
    }
    // Soft time budget: finish the in-flight SQLite operation, then return to
    // the priority queue after at most 16 headers or 25ms, whichever comes first.
    async fn step(&mut self, user: &str) -> bool {
        let started = Instant::now();
        for _ in 0..DISCOVERY_CHUNK {
            let Some(header) = self.headers.next() else {
                return true;
            };
            if let Some(workspace) = header.workspace.to_str() {
                match self
                    .inventory
                    .discover(user, workspace, &header.external_id, &header.identity)
                    .await
                {
                    Ok(true) => crate::history_read::invalidate_catalog(&self.home).await,
                    Ok(false) => {}
                    Err(error) => debug!("Native identity discovery denied: {error:#}"),
                }
            }
            if started.elapsed() >= DISCOVERY_SLICE {
                break;
            }
        }
        self.headers.len() == 0
    }
}

#[cfg(test)]
async fn discover(
    source: &crate::personal_source::PersonalSource,
    user: &str,
) -> anyhow::Result<()> {
    if let Some(mut pass) = Discovery::start(source).await? {
        while !pass.step(user).await {}
    }
    Ok(())
}

async fn ingest_native(
    home: &Path,
    configured: Option<&crate::personal_source::PersonalSource>,
    source: NativeObservation,
) -> anyhow::Result<()> {
    use anyhow::ensure;
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    ensure!(source.fence.is_current(), "Native source authority ended");
    let expected_file = if let Some(configured) = configured {
        ensure!(home == configured.home, "Native canonical HOME mismatch");
        Some(configured.validate_binding(&source.binding).await?)
    } else {
        ensure!(
            source.binding.session_root == home.join(".pi/agent/sessions"),
            "Native source root denied"
        );
        None
    };
    // Read the native head HERE, on the serialized writer, rather than trusting
    // arrival order of observations from independent Device connections.
    let current = crate::tui_rpc::TuiObserver::attach_snapshots(
        std::sync::Arc::new(source.clone()),
        &source.binding.authority_id,
        &source.binding.public_session_id,
    )
    .await?;
    let leaf = current.initial_state().head.leaf_id()?.map(str::to_owned);
    drop(current);
    let path = source.binding.session_file.clone();
    let native_id = source.binding.external_session_id.clone();
    let workspace = source
        .binding
        .work_directory
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid native Workspace"))?
        .to_owned();
    let graph = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)?;
        let before = file.metadata()?;
        ensure!(
            before.is_file()
                && before.uid() == rustix::process::geteuid().as_raw()
                && before.len() <= 64 * 1024 * 1024,
            "Unsafe or oversized native source"
        );
        // Check the opened descriptor against the durable pin BEFORE reading
        // any body, not just the path after parsing (ancestor replacement race).
        if let Some(expected) = expected_file {
            ensure!(
                (before.dev(), before.ino()) == (expected.dev, expected.ino),
                "Native opened source identity changed"
            );
        }
        let mut bytes = Vec::new();
        (&file).take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        let published = std::fs::symlink_metadata(&path)?;
        ensure!(
            (
                before.dev(),
                before.ino(),
                before.len(),
                before.mtime(),
                before.mtime_nsec()
            ) == (
                after.dev(),
                after.ino(),
                after.len(),
                after.mtime(),
                after.mtime_nsec()
            ) && (after.dev(), after.ino()) == (published.dev(), published.ino()),
            "Native source changed during snapshot"
        );
        store::native_graph::NativeGraph::parse(&bytes, &native_id, &workspace, leaf.as_deref())
    })
    .await??;
    ensure!(source.fence.is_current(), "Native source authority ended");
    if let Some(configured) = configured {
        configured.validate_binding(&source.binding).await?;
    }
    store::native_graph::merge(
        home,
        &source.user_id,
        &source.binding.public_session_id,
        &graph,
    )
    .await
}

async fn handle_nudge(home: &Path, user_id: &str, id: &str) {
    let external_id = if id.starts_with("oqto-") {
        match ops::find_external_by_session(home, id).await {
            Some(external_id) => external_id,
            None => return,
        }
    } else {
        id.to_string()
    };

    // Resolve strictly under this writer's canonical default source, never
    // through the process-global Pi lookup or a first-of-many fallback.
    let suffix = format!("_{external_id}.jsonl");
    let mut matches = list_session_files(home).into_iter().filter(|path| {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(&suffix))
    });
    let Some(path) = matches.next() else {
        return;
    };
    if matches.next().is_some() {
        return;
    }
    maybe_ingest(home, user_id, &external_id, &path).await;
}

/// Read the workspace path from the JSONL session header line.
///
/// This is the only exact source: the safe dirname encoding is lossy (a '-'
/// in a real path segment is indistinguishable from a '/'), and decoding it
/// has historically mis-filed sessions into bogus workspaces.
pub(crate) fn read_session_cwd(path: &Path) -> Option<String> {
    use std::io::BufRead;

    #[derive(serde::Deserialize)]
    struct Header {
        #[serde(rename = "type")]
        entry_type: String,
        cwd: Option<String>,
    }

    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    for line in reader.lines().map_while(Result::ok).take(5) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(header) = serde_json::from_str::<Header>(trimmed)
            && header.entry_type == "session"
        {
            return header.cwd.filter(|cwd| !cwd.is_empty());
        }
    }
    None
}

#[cfg(test)]
async fn sweep(home: &Path, user_id: &str) {
    let started = Instant::now();
    let mut checked = 0usize;
    for path in list_session_files(home) {
        let Some(external_id) = super::server::parse_pi_session_id_from_path(&path) else {
            continue;
        };
        checked += 1;
        maybe_ingest(home, user_id, &external_id, &path).await;
    }
    debug!(
        "jsonl ingest sweep: checked {} session files in {:?}",
        checked,
        started.elapsed()
    );
}

fn list_session_files(home: &Path) -> Vec<PathBuf> {
    let base = home.join(".pi/agent/sessions");
    let Ok(workspaces) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for workspace in workspaces.flatten() {
        let dir = workspace.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                out.push(path);
            }
        }
    }
    out
}

async fn maybe_ingest(home: &Path, user_id: &str, external_id: &str, path: &Path) {
    // Source-bound native Sessions never enter the legacy count/cursor path.
    // Identity errors are fail-closed, not permission to repair another source.
    if ops::native_source::is_bound(home, external_id)
        .await
        .unwrap_or(true)
    {
        return;
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let Ok(modified) = meta.modified() else {
        return;
    };
    // Live sessions are fed by the event pipeline; a file this fresh may
    // still be mid-write. It will be picked up once quiescent.
    if modified.elapsed().unwrap_or(Duration::ZERO) < QUIESCENCE {
        return;
    }
    let mtime_ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let current = IngestCursor {
        file_size: meta.len() as i64,
        file_mtime_ms: mtime_ms,
    };
    if index::get_ingest_cursor(home, external_id).await == Some(current) {
        return;
    }
    ingest_file(home, user_id, external_id, path, current).await;
}

async fn ingest_file(
    home: &Path,
    user_id: &str,
    external_id: &str,
    path: &Path,
    cursor: IngestCursor,
) {
    let path_owned = path.to_path_buf();
    let records = match tokio::task::spawn_blocking(move || {
        super::server::read_jsonl_message_records(&path_owned)
    })
    .await
    {
        Ok(records) => records,
        Err(err) => {
            warn!("jsonl ingest: reading {} failed: {err}", path.display());
            return;
        }
    };

    if records.is_empty() {
        let _ = index::upsert_ingest_cursor(home, external_id, cursor).await;
        return;
    }

    let workspace_from_header = read_session_cwd(path);
    let (session_id, workspace_id) = match ops::find_session_by_external(home, external_id).await {
        Some((id, ws)) if !ws.is_empty() => (id, ws),
        Some((id, _)) => (
            id,
            workspace_from_header.unwrap_or_else(|| "global".to_string()),
        ),
        None => (
            platform_id_for_external_id(external_id),
            workspace_from_header.unwrap_or_else(|| "global".to_string()),
        ),
    };

    let oqto_log_count = store::read_session_stats(home, &workspace_id, &session_id)
        .await
        .map(|stats| stats.messages)
        .unwrap_or(0);

    if oqto_log_count < records.len() {
        // Converge deterministically on the JSONL contents: the replace is
        // keyed by source entry ids, so re-running it is idempotent.
        if let Err(err) = store::replace_session_with_pi_jsonl_records(
            home,
            user_id,
            &workspace_id,
            &session_id,
            &session_id,
            Some(external_id),
            external_id,
            &records,
        )
        .await
        {
            warn!(
                "jsonl ingest: replace failed session={session_id} external_id={external_id} oqto_log={oqto_log_count} jsonl={}: {err:#}",
                records.len()
            );
            return;
        }
        crate::history_read::invalidate_catalog(home).await;
        debug!(
            "jsonl ingest: replaced session={session_id} external_id={external_id} with {} messages (was {oqto_log_count})",
            records.len()
        );
    }

    if let Err(err) = index::upsert_ingest_cursor(home, external_id, cursor).await {
        debug!("jsonl ingest: cursor upsert failed for {external_id}: {err:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_session_file(home: &Path, external_id: &str, messages: &[&str]) -> PathBuf {
        let dir = home.join(".pi/agent/sessions/tmp-ingesttest");
        std::fs::create_dir_all(&dir).expect("create sessions dir");
        let path = dir.join(format!("2026-07-18T12-00-00-000Z_{external_id}.jsonl"));
        let mut lines = vec![format!(
            r#"{{"type":"session","id":"{external_id}","timestamp":"2026-07-18T12:00:00.000Z","cwd":"/tmp/ingest-test"}}"#
        )];
        for (idx, text) in messages.iter().enumerate() {
            let role = if idx % 2 == 0 { "user" } else { "assistant" };
            lines.push(format!(
                r#"{{"type":"message","id":"entry-{idx}","timestamp":"2026-07-18T12:00:01.000Z","message":{{"role":"{role}","content":[{{"type":"text","text":"{text}"}}],"timestamp":1768565213775}}}}"#
            ));
        }
        std::fs::write(&path, lines.join("\n")).expect("write session file");
        // Backdate mtime past the quiescence window.
        let status = std::process::Command::new("touch")
            .arg("-d")
            .arg("2026-07-01T12:00:02Z")
            .arg(&path)
            .status()
            .expect("touch");
        assert!(status.success());
        path
    }

    #[tokio::test]
    async fn split_source_discovery_ignores_legacy_cursors_and_nudges_never_use_ambient_home()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let (home, native, work, sockets) = (
            root.join("service"),
            root.join("native/.pi/agent/sessions"),
            root.join("work"),
            root.join("sockets"),
        );
        for path in [&home, &native, &work, &sockets] {
            std::fs::create_dir_all(path)?;
        }
        let source = crate::personal_source::PersonalSource::new(
            home.clone(),
            native.clone(),
            sockets,
            vec![work.clone()],
        )?;
        let directory = native.join(oqto_pi::session_files::cwd_to_safe_dirname(&work));
        std::fs::create_dir(&directory)?;
        let id = "fresh-external-source";
        let path = directory.join(format!("2026-10-19_{id}.jsonl"));
        let bytes = format!(
            "{}\nNOT JSON BODY: header discovery only\n",
            serde_json::json!({"type":"session","id":id,"cwd":work})
        );
        std::fs::write(&path, &bytes)?;
        let cursor = IngestCursor {
            file_size: bytes.len() as i64,
            file_mtime_ms: 42,
        };
        index::upsert_ingest_cursor(&home, id, cursor).await?;
        // Cache an empty catalog before the serialized writer discovers IDs.
        crate::history_read::read_authorized_source(
            &home,
            source.workspace_roots(),
            crate::history_read::HistoryReadOperation::List {},
        )
        .await?;
        discover(&source, "native-owner").await?;
        handle_nudge(&home, "native-owner", id).await;
        assert_eq!(index::get_ingest_cursor(&home, id).await, Some(cursor));
        let catalog = crate::history_read::read_authorized_source(
            &home,
            source.workspace_roots(),
            crate::history_read::HistoryReadOperation::List {},
        )
        .await?;
        assert_eq!(catalog.data["sessions"].as_array().map(Vec::len), Some(1));
        let legacy = "11111111-2222-3333-4444-555555555555";
        write_session_file(&home, legacy, &["retained default source"]);
        handle_nudge(&home, "service-owner", legacy).await;
        assert!(ops::find_session_by_external(&home, legacy).await.is_some());
        assert!(index::get_ingest_cursor(&home, legacy).await.is_some());
        assert_eq!(std::fs::read_to_string(&path)?, bytes);
        assert!(!root.join("native/.local").exists());
        Ok(())
    }

    #[tokio::test]
    async fn sweep_ingests_bare_pi_session_and_sets_cursor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path();
        let external_id = "11111111-2222-3333-4444-555555555555";
        let path = write_session_file(home, external_id, &["hello", "world"]);

        sweep(home, "testuser").await;

        let found = ops::find_session_by_external(home, external_id)
            .await
            .expect("session ingested into oqto-log");
        assert_eq!(found.1, "/tmp/ingest-test");
        assert_eq!(found.0, platform_id_for_external_id(external_id));
        let cursor = index::get_ingest_cursor(home, external_id)
            .await
            .expect("cursor recorded");
        assert_eq!(
            cursor.file_size,
            std::fs::metadata(&path).expect("meta").len() as i64
        );

        // Unchanged file: sweep is a no-op (cursor identical).
        sweep(home, "testuser").await;
        assert_eq!(
            index::get_ingest_cursor(home, external_id).await,
            Some(cursor)
        );

        // Out-of-band continuation: more messages appear in the JSONL.
        write_session_file(home, external_id, &["hello", "world", "continued", "again"]);
        sweep(home, "testuser").await;
        let new_cursor = index::get_ingest_cursor(home, external_id)
            .await
            .expect("cursor updated");
        assert!(new_cursor.file_size > cursor.file_size);

        let session_id = ops::find_session_by_external(home, external_id)
            .await
            .expect("session still present")
            .0;
        let messages = oqto_history::oqto_log::projector::project_session_messages_auto(
            home,
            &session_id,
            None,
        )
        .await
        .expect("project")
        .expect("messages present");
        assert_eq!(messages.len(), 4);
    }
}

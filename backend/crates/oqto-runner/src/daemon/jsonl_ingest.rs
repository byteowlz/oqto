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
use tokio::sync::{mpsc, watch};
type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiscoveryReadiness {
    Disabled,
    Checking,
    Discovering,
    Ready,
    Failed,
}

#[derive(Default)]
struct AuthorityHealth {
    faults: std::sync::atomic::AtomicU64,
    verified: std::sync::atomic::AtomicU64,
}
impl AuthorityHealth {
    fn generation(&self) -> u64 {
        self.faults.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn unavailable(&self) -> bool {
        self.generation() != self.verified.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn fail(&self) -> u64 {
        self.faults
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .wrapping_add(1)
    }
}

const SWEEP_INTERVAL: Duration = Duration::from_secs(300);
const QUIESCENCE: Duration = Duration::from_secs(30);
const NUDGE_DEDUPE_WINDOW: Duration = Duration::from_secs(10);
// A failed authority check retries the full header inventory and shard probe.
// Repeated failures back off so a persistently slow host cannot be pinned in a
// rescan loop (oqto-6qff); one success resets to the fast first retry.
const RETRY_MIN: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(60);
static HEADER_SCANNER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

/// Handle for nudging the ingest task about a session that was just read.
#[derive(Clone)]
pub struct IngestHandle {
    tx: mpsc::Sender<Nudge>,
    readiness: watch::Receiver<DiscoveryReadiness>,
    health: std::sync::Arc<AuthorityHealth>,
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
    AuthorityFailed(u64),
}

impl IngestHandle {
    pub(crate) fn readiness(&self) -> DiscoveryReadiness {
        if self.source.is_some()
            && (self.readiness.has_changed().is_err() || self.health.unavailable())
        {
            DiscoveryReadiness::Failed
        } else {
            *self.readiness.borrow()
        }
    }
    pub(crate) fn mark_authority_failed(&self) {
        if self.source.is_some() {
            let generation = self.health.fail();
            let _ = self.tx.try_send(Nudge::AuthorityFailed(generation));
        }
    }
    pub(crate) fn catalog_ready(&self) -> bool {
        !matches!(
            self.readiness(),
            DiscoveryReadiness::Checking | DiscoveryReadiness::Failed
        )
    }
    // An initial, complete per-attachment identity query can independently
    // admit an existing pin while initial discovery is still checking. A failed
    // authority remains unavailable throughout retry, until the full probe wins.
    pub(crate) fn native_available(&self) -> bool {
        self.readiness() != DiscoveryReadiness::Failed
    }
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
    let (status, readiness) = watch::channel(if source.is_some() {
        DiscoveryReadiness::Checking
    } else {
        DiscoveryReadiness::Disabled
    });
    let health = std::sync::Arc::new(AuthorityHealth::default());
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(run(rx, source.clone(), status, health.clone()));
    }
    IngestHandle {
        tx,
        source,
        readiness,
        health,
    }
}

#[cfg(test)]
pub(crate) fn spawn_for_test(home: PathBuf, user_id: String) -> IngestHandle {
    let (tx, rx) = mpsc::channel(128);
    let (status, readiness) = watch::channel(DiscoveryReadiness::Disabled);
    let health = std::sync::Arc::new(AuthorityHealth::default());
    tokio::spawn(run_at(
        rx,
        home,
        user_id,
        None,
        status,
        None,
        health.clone(),
    ));
    IngestHandle {
        tx,
        source: None,
        readiness,
        health,
    }
}
#[cfg(test)]
pub(crate) fn spawn_paused_inventory_for_test(
    home: PathBuf,
    user: String,
    source: std::sync::Arc<crate::personal_source::PersonalSource>,
) -> (IngestHandle, tokio::sync::oneshot::Sender<()>) {
    let (tx, rx) = mpsc::channel(128);
    let (status, readiness) = watch::channel(DiscoveryReadiness::Checking);
    let (release, paused) = tokio::sync::oneshot::channel();
    let health = std::sync::Arc::new(AuthorityHealth::default());
    let owned = source.clone();
    let initial = Box::pin(async move {
        let _ = paused.await;
        DiscoveryEvent::Started {
            generation: 0,
            result: Discovery::start(&owned).await,
        }
    });
    tokio::spawn(run_at(
        rx,
        home,
        user,
        Some(source.clone()),
        status,
        Some(initial),
        health.clone(),
    ));
    (
        IngestHandle {
            tx,
            source: Some(source),
            readiness,
            health,
        },
        release,
    )
}

async fn run(
    rx: mpsc::Receiver<Nudge>,
    source: Option<std::sync::Arc<crate::personal_source::PersonalSource>>,
    status: watch::Sender<DiscoveryReadiness>,
    health: std::sync::Arc<AuthorityHealth>,
) {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let user_id = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

    run_at(rx, home, user_id, source, status, None, health).await;
}
async fn run_at(
    mut rx: mpsc::Receiver<Nudge>,
    home: PathBuf,
    user_id: String,
    source: Option<std::sync::Arc<crate::personal_source::PersonalSource>>,
    status: watch::Sender<DiscoveryReadiness>,
    initial: Option<BoxFuture<'static, DiscoveryEvent>>,
    health: std::sync::Arc<AuthorityHealth>,
) {
    let mut tick = tokio::time::interval(SWEEP_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut retry_delay = RETRY_MIN;
    let retry = tokio::time::sleep(RETRY_MIN);
    tokio::pin!(retry);
    let mut recent: HashMap<String, Instant> = HashMap::new();
    let mut discovery = initial.map(DiscoveryReadJob::new);
    let mut epoch = 0u64;
    let mut legacy = std::collections::VecDeque::new();
    let mut legacy_ids = std::collections::VecDeque::new();
    loop {
        tokio::select! {
            biased;
            nudged = rx.recv() => {
                let Some(id) = nudged else { return; };
                let mut pending = vec![id];
                while let Ok(more) = rx.try_recv() { pending.push(more); }
                let mut ids = Vec::new();
                let mut native = HashMap::new();
                for nudge in pending {
                    match nudge {
                        Nudge::Legacy(id) => ids.push(id),
                        Nudge::Native(source) => { native.insert(source.binding.public_session_id.clone(), source); }
                        Nudge::AuthorityFailed(generation) => {
                            if generation > health.verified.load(std::sync::atomic::Ordering::SeqCst) {
                                discovery = None;
                                status.send_replace(DiscoveryReadiness::Failed);
                            }
                        }
                    }
                }
                for observation in native.into_values() {
                    // A prepared read that overlapped writer work is discarded,
                    // never committed as a cached ownership answer.
                    epoch = epoch.wrapping_add(1);
                    if let Err(error) = ingest_native(&home, source.as_deref(), *observation).await {
                        if ops::native_source::authority_unavailable(&error) {
                            discovery = None;
                            health.fail();
                            status.send_replace(DiscoveryReadiness::Failed);
                            warn!("Native identity authority unavailable; sessions refused until a complete retry");
                        }
                        debug!("Native graph convergence deferred: {error:#}");
                    }
                }
                ids.sort(); ids.dedup();
                let now = Instant::now();
                recent.retain(|_, seen| now.duration_since(*seen) < NUDGE_DEDUPE_WINDOW);
                let mut requested = false;
                for id in ids {
                    if recent.contains_key(&id) { continue; }
                    recent.insert(id.clone(), now);
                    requested = true;
                    if legacy_ids.len() < 128 { legacy_ids.push_back(id); }
                }
                if requested && discovery.is_none() && let Some(source) = &source {
                    discovery = Some(Discovery::start_job(source.clone(), health.generation()));
                }
            }
            _ = tick.tick() => {
                if discovery.is_none() && let Some(source) = &source {
                    discovery = Some(Discovery::start_job(source.clone(), health.generation()));
                }
                legacy = list_session_files(&home).into();
            }
            _ = &mut retry, if (*status.borrow() == DiscoveryReadiness::Failed || health.unavailable()) && discovery.is_none() => {
                if let Some(source) = &source { discovery = Some(Discovery::start_job(source.clone(), health.generation())); }
                retry.as_mut().reset(tokio::time::Instant::now() + retry_delay);
                retry_delay = (retry_delay * 2).min(RETRY_MAX);
            }
            event = async { match &mut discovery { Some(work) => work.wait().await, None => std::future::pending().await } } => {
                discovery = None;
                match event {
                    DiscoveryEvent::Started { generation, result: Ok(pass) } => {
                        if generation != health.generation() {
                            status.send_replace(DiscoveryReadiness::Failed);
                            continue;
                        }
                        health.verified.store(generation, std::sync::atomic::Ordering::SeqCst);
                        retry_delay = RETRY_MIN;
                        // Retry may follow a committed pin whose post-write
                        // verification failed. Never expose an old cached list.
                        crate::history_read::invalidate_catalog(&home).await;
                        status.send_replace(if pass.is_some() { DiscoveryReadiness::Discovering } else { DiscoveryReadiness::Ready });
                        discovery = pass.and_then(|pass| pass.next_job(epoch, None));
                    }
                    DiscoveryEvent::Scanned { pass, result } => {
                        if health.unavailable() {
                            status.send_replace(DiscoveryReadiness::Failed);
                            continue;
                        }
                        match result {
                            Ok(()) => {
                                debug!("Native header inventory progress: {} headers, scanning={}", pass.scan.as_ref().map_or(pass.headers.len(), |scan| scan.headers.len()), pass.scan.is_some());
                                discovery = pass.next_job(epoch, None);
                                if discovery.is_none() { status.send_replace(DiscoveryReadiness::Ready); }
                            }
                            Err(error) => {
                                status.send_replace(DiscoveryReadiness::Failed);
                                warn!("Native header inventory failed: {error:#}");
                            }
                        }
                    }
                    DiscoveryEvent::Started { result: Err(error), .. } => {
                        status.send_replace(DiscoveryReadiness::Failed);
                        warn!("Native discovery authority check failed; sessions unavailable: {error:#}");
                    }
                    DiscoveryEvent::Prepared { mut pass, header, read_epoch, result } => {
                        if health.unavailable() {
                            status.send_replace(DiscoveryReadiness::Failed);
                            continue;
                        }
                        if read_epoch != epoch && result.is_ok() {
                            discovery = pass.next_job(epoch, Some(header));
                            continue;
                        }
                        let result = match result {
                            Ok(prepared) => match header.workspace.to_str() {
                                Some(workspace) => pass.inventory.commit(&user_id, workspace, &header.identity, prepared).await,
                                None => Err(anyhow::anyhow!("Invalid native Workspace")),
                            },
                            Err(error) => Err(error),
                        };
                        match result {
                            Ok(true) => crate::history_read::invalidate_catalog(&home).await,
                            Ok(false) => {}
                            Err(error) if ops::native_source::authority_unavailable(&error) => {
                                status.send_replace(DiscoveryReadiness::Failed);
                                warn!("Native discovery authority lost; whole batch aborted: {error:#}");
                                continue;
                            }
                            Err(error) => debug!("Native identity discovery denied: {error:#}"),
                        }
                        discovery = pass.next_job(epoch, None);
                        if discovery.is_none() { status.send_replace(DiscoveryReadiness::Ready); }
                    }
                }
            }
            _ = std::future::ready(()), if discovery.is_none() && *status.borrow() != DiscoveryReadiness::Failed && !health.unavailable() && (!legacy_ids.is_empty() || !legacy.is_empty()) => {
                if let Some(id) = legacy_ids.pop_front() { handle_nudge(&home, &user_id, &id).await; }
                else if let Some(path) = legacy.pop_front()
                    && let Some(external) = super::server::parse_pi_session_id_from_path(&path) {
                    maybe_ingest(&home, &user_id, &external, &path).await;
                }
            }
        }
    }
}

/// A single owned READ-ONLY job, not another writer. It must progress while
/// the actor awaits native work: an inline suspended future may own both fixed
/// reader slots and deadlock the native operation borrowing the same readers.
/// Prepared results remain epoch-fenced; replacement/shutdown aborts the job.
struct DiscoveryReadJob {
    task: tokio::task::JoinHandle<DiscoveryEvent>,
}
impl DiscoveryReadJob {
    fn new(work: impl std::future::Future<Output = DiscoveryEvent> + Send + 'static) -> Self {
        Self {
            task: tokio::spawn(work),
        }
    }
    async fn wait(&mut self) -> DiscoveryEvent {
        match (&mut self.task).await {
            Ok(event) => event,
            Err(error) => DiscoveryEvent::Started {
                generation: 0,
                result: Err(anyhow::anyhow!(
                    "Native discovery reader task ended: {error}"
                )),
            },
        }
    }
}
impl Drop for DiscoveryReadJob {
    fn drop(&mut self) {
        self.task.abort();
    }
}

enum DiscoveryEvent {
    Started {
        generation: u64,
        result: anyhow::Result<Option<Discovery>>,
    },
    Scanned {
        pass: Discovery,
        result: anyhow::Result<()>,
    },
    Prepared {
        pass: Discovery,
        header: crate::personal_source::NativeHeader,
        read_epoch: u64,
        result: anyhow::Result<ops::native_source::PreparedDiscovery>,
    },
}
// Filename authority is metadata-only and must not silently flatten an I/O
// error into permission to rebind an ID already present in the default source.
fn discovery_defaults(home: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let directories = match std::fs::read_dir(home.join(".pi/agent/sessions")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut files = Vec::new();
    for directory in directories {
        let directory = directory?;
        if !std::fs::metadata(directory.path())?.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(directory.path())? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
                && std::fs::metadata(&path)?.is_file()
            {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn discovery_headers(
    source: &crate::personal_source::PersonalSource,
    headers: Vec<crate::personal_source::NativeHeader>,
) -> anyhow::Result<Vec<crate::personal_source::NativeHeader>> {
    let mut counts = HashMap::new();
    for header in &headers {
        *counts.entry(header.external_id.clone()).or_insert(0usize) += 1;
    }
    // Reverse sorted filenames turn exact suffix matching (including native IDs
    // containing underscores) into a prefix range, not headers x default files.
    let mut defaults: Vec<_> = discovery_defaults(&source.home)?
        .into_iter()
        .filter_map(|path| {
            let name: String = path.file_name()?.to_string_lossy().chars().rev().collect();
            Some((name, path))
        })
        .collect();
    defaults.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    Ok(headers
        .into_iter()
        .filter(|header| {
            if counts[&header.external_id] != 1 {
                return false;
            }
            let suffix: String = format!("_{}.jsonl", header.external_id)
                .chars()
                .rev()
                .collect();
            let start = defaults.partition_point(|(name, _)| name < &suffix);
            !defaults[start..]
                .iter()
                .take_while(|(name, _)| name.starts_with(&suffix))
                .any(|(_, path)| path != &header.path)
        })
        .collect())
}

struct Discovery {
    home: PathBuf,
    source: crate::personal_source::PersonalSource,
    scan: Option<crate::personal_source::HeaderScan>,
    headers: std::vec::IntoIter<crate::personal_source::NativeHeader>,
    inventory: ops::native_source::DiscoveryBatch,
}
impl Discovery {
    // Read-only initialization, including the initial complete shard probe, is
    // independently scheduled beside the nudge queue. No SQL writes occur in
    // these jobs; their resource owners must progress while native work awaits.
    fn start_job(
        source: std::sync::Arc<crate::personal_source::PersonalSource>,
        generation: u64,
    ) -> DiscoveryReadJob {
        DiscoveryReadJob::new(async move {
            DiscoveryEvent::Started {
                generation,
                result: Self::start(&source).await,
            }
        })
    }
    async fn start(
        source: &crate::personal_source::PersonalSource,
    ) -> anyhow::Result<Option<Self>> {
        // A complete shard probe still fences authority. Header discovery is
        // cooperative work, not authority failure just because the disk is cold.
        let inventory = ops::native_source::DiscoveryBatch::open(&source.home).await?;
        Ok(Some(Self {
            home: source.home.clone(),
            source: source.clone(),
            scan: Some(crate::personal_source::HeaderScan::default()),
            headers: Vec::new().into_iter(),
            inventory,
        }))
    }
    async fn scan_next(&mut self) -> anyhow::Result<()> {
        let Some(mut scan) = self.scan.take() else {
            return Ok(());
        };
        let source = self.source.clone();
        let permit = HEADER_SCANNER.acquire().await?;
        let (scan, headers) = tokio::task::spawn_blocking(move || {
            // Cancellation cannot release the scanner permit while filesystem
            // work still runs. A chunk yields its owned cursor, never restarts.
            let _permit = permit;
            #[cfg(not(test))]
            let budget = Duration::from_secs(5);
            #[cfg(test)]
            let budget = Duration::from_millis(
                source
                    .scan_budget_ms
                    .load(std::sync::atomic::Ordering::SeqCst),
            );
            if scan.advance(&source, 16, budget)? {
                // Ambiguity is global: do not publish fresh adoptions from a
                // partial scan before checking the full native/default tree.
                let headers = discovery_headers(&source, std::mem::take(&mut scan.headers))?;
                Ok::<_, anyhow::Error>((None, Some(headers)))
            } else {
                Ok((Some(scan), None))
            }
        })
        .await??;
        self.scan = scan;
        if let Some(headers) = headers {
            self.headers = headers.into_iter();
        }
        Ok(())
    }
    fn next_job(
        mut self,
        epoch: u64,
        retry: Option<crate::personal_source::NativeHeader>,
    ) -> Option<DiscoveryReadJob> {
        if self.scan.is_some() {
            return Some(DiscoveryReadJob::new(async move {
                let result = self.scan_next().await;
                tokio::task::yield_now().await;
                DiscoveryEvent::Scanned { pass: self, result }
            }));
        }
        let header = retry.or_else(|| self.headers.next())?;
        Some(DiscoveryReadJob::new(async move {
            let result = self.inventory.prepare(&header.external_id).await;
            DiscoveryEvent::Prepared {
                pass: self,
                header,
                read_epoch: epoch,
                result,
            }
        }))
    }
}

#[cfg(test)]
async fn discover(
    source: &crate::personal_source::PersonalSource,
    user: &str,
) -> anyhow::Result<()> {
    if let Some(mut pass) = Discovery::start(source).await? {
        while pass.scan.is_some() {
            pass.scan_next().await?;
        }
        for header in pass.headers {
            let prepared = pass.inventory.prepare(&header.external_id).await?;
            pass.inventory
                .commit(
                    user,
                    header
                        .workspace
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("Workspace"))?,
                    &header.identity,
                    prepared,
                )
                .await?;
            crate::history_read::invalidate_catalog(&pass.home).await;
        }
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
#[path = "jsonl_ingest/discovery_tests.rs"]
mod discovery_tests;

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn dropped_discovery_job_releases_its_owned_read_work() -> anyhow::Result<()> {
        let readers = std::sync::Arc::new(tokio::sync::Semaphore::new(2));
        let pool = readers.clone();
        let (held, acquired) = tokio::sync::oneshot::channel();
        let job = super::DiscoveryReadJob::new(async move {
            let _permits = pool.acquire_many_owned(2).await;
            let _ = held.send(());
            std::future::pending().await
        });
        acquired.await?;
        drop(job);
        let _returned = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            readers.acquire_many_owned(2),
        )
        .await??;
        Ok(())
    }

    #[tokio::test]
    async fn native_work_can_acquire_readers_while_discovery_event_is_not_polled()
    -> anyhow::Result<()> {
        // Match the actor's select -> await-native pattern: discovery owns both
        // reader permits when the actor stops polling its result to serve a nudge.
        let readers = std::sync::Arc::new(tokio::sync::Semaphore::new(2));
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        let (held, acquired) = tokio::sync::oneshot::channel();
        let pool = readers.clone();
        let finish = release.clone();
        let mut job = super::DiscoveryReadJob::new(async move {
            let permits = pool.acquire_many_owned(2).await;
            let _ = held.send(());
            finish.notified().await;
            drop(permits);
            super::DiscoveryEvent::Started {
                generation: 0,
                result: Ok(None),
            }
        });
        tokio::select! {
            result = acquired => { result?; }
            _ = job.wait() => anyhow::bail!("discovery completed before releasing readers"),
        }
        release.notify_one();
        let _native = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            readers.acquire_many_owned(2),
        )
        .await
        .map_err(|_| anyhow::anyhow!("unpolled discovery monopolized both native readers"))??;
        let _ = job.wait().await;
        Ok(())
    }
    use super::*;

    fn process_cpu() -> Duration {
        // SAFETY: getrusage only writes the provided, fully owned struct.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
        let time = |t: libc::timeval| {
            Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64)
        };
        time(usage.ru_utime) + time(usage.ru_stime)
    }

    /// oqto-6qff end to end: a personal host with 230 Workspace shards and
    /// many already-pinned native Sessions must finish its discovery pass and go
    /// idle quickly, with history answering well under two seconds. Before the
    /// fix every pinned Session re-attached all shards twice per sweep.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn pinned_discovery_over_230_shards_reaches_ready_and_goes_idle() -> anyhow::Result<()> {
        const WORKSPACES: usize = 230;
        const HEADERS: usize = 1200;
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
        let workspaces: Vec<PathBuf> = (0..WORKSPACES)
            .map(|n| {
                let path = work.join(format!("repo-{n}"));
                std::fs::create_dir_all(&path)?;
                std::fs::create_dir_all(
                    native.join(oqto_pi::session_files::cwd_to_safe_dirname(&path)),
                )?;
                Ok(path)
            })
            .collect::<anyhow::Result<_>>()?;
        for n in 0..HEADERS {
            let cwd = &workspaces[n % WORKSPACES];
            let id = format!("native-{n}");
            std::fs::write(
                native
                    .join(oqto_pi::session_files::cwd_to_safe_dirname(cwd))
                    .join(format!("2026-10-19_{id}.jsonl")),
                format!(
                    "{}\nNOT JSON BODY: header discovery only\n",
                    serde_json::json!({"type":"session","id":id,"cwd":cwd})
                ),
            )?;
        }
        let source = std::sync::Arc::new(crate::personal_source::PersonalSource::new(
            home.clone(),
            native,
            sockets,
            vec![work],
        )?);
        let headers = source.headers()?;
        assert_eq!(headers.len(), HEADERS);
        // Pin every header in its Workspace shard, as on a long-running host.
        let mut by_workspace: HashMap<String, Vec<&crate::personal_source::NativeHeader>> =
            HashMap::new();
        for header in &headers {
            by_workspace
                .entry(header.workspace.to_string_lossy().into_owned())
                .or_default()
                .push(header);
        }
        for (workspace, pinned) in by_workspace {
            let db = oqto_history::oqto_log::paths::resolve_user_home_workspace_db_path(
                &home, &workspace,
            )?;
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(&db)
                        .create_if_missing(true),
                )
                .await?;
            sqlx::migrate!("../oqto-history/migrations_oqto_log")
                .run(&pool)
                .await?;
            for header in pinned {
                let public = platform_id_for_external_id(&header.external_id);
                sqlx::query("INSERT INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id,extensions_json) VALUES(?,?,?,'owner',?,json_object('personal_native_source',?))")
                    .bind(&public).bind(&public).bind(&header.external_id).bind(&workspace).bind(&header.identity)
                    .execute(&pool).await?;
            }
            pool.close().await;
        }
        let (ingest, release) =
            spawn_paused_inventory_for_test(home.clone(), "owner".into(), source.clone());
        let started = Instant::now();
        release
            .send(())
            .map_err(|_| anyhow::anyhow!("inventory worker ended"))?;
        tokio::time::timeout(Duration::from_secs(30), async {
            while ingest.readiness() != DiscoveryReadiness::Ready {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("discovery pass did not finish within 30 s"))?;
        let ready = started.elapsed();
        let cpu = process_cpu();
        tokio::time::sleep(Duration::from_secs(2)).await;
        let idle_cpu = process_cpu() - cpu;
        let listed = Instant::now();
        crate::history_read::read_authorized_source(
            &home,
            source.workspace_roots(),
            crate::history_read::HistoryReadOperation::List {},
        )
        .await?;
        let list = listed.elapsed();
        eprintln!(
            "oqto-6qff e2e: {WORKSPACES} shards, {HEADERS} pinned headers: Ready after {ready:?}, process CPU over 2 s idle {idle_cpu:?}, history list {list:?}"
        );
        assert_eq!(ingest.readiness(), DiscoveryReadiness::Ready);
        assert!(ready < Duration::from_secs(15), "pass took {ready:?}");
        assert!(list < Duration::from_secs(2), "history list took {list:?}");
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cold_slow_headers_progress_across_budgets_and_reach_ready() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let (home, native, work, sockets) = (
            root.join("home"),
            root.join("native"),
            root.join("work"),
            root.join("sockets"),
        );
        for path in [&home, &native, &work, &sockets] {
            std::fs::create_dir_all(path)?;
        }
        let directory = native.join(oqto_pi::session_files::cwd_to_safe_dirname(&work));
        std::fs::create_dir(&directory)?;
        for n in 0..6 {
            std::fs::write(
                directory.join(format!("date_slow-{n}.jsonl")),
                format!(
                    "{}\n",
                    serde_json::json!({"type":"session","id":format!("slow-{n}"),"cwd":work})
                ),
            )?;
        }
        let source = std::sync::Arc::new(crate::personal_source::PersonalSource::new(
            home.clone(),
            native,
            sockets,
            vec![work],
        )?);
        source
            .header_latency_ms
            .store(1000, std::sync::atomic::Ordering::SeqCst);
        source
            .scan_budget_ms
            .store(10, std::sync::atomic::Ordering::SeqCst);
        let (ingest, release) =
            spawn_paused_inventory_for_test(home.clone(), "owner".into(), source);
        release
            .send(())
            .map_err(|_| anyhow::anyhow!("inventory ended"))?;
        let started = Instant::now();
        let mut saw_discovering = false;
        tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                let state = ingest.readiness();
                anyhow::ensure!(
                    state != DiscoveryReadiness::Failed,
                    "slow disk discarded progress after {:?}",
                    started.elapsed()
                );
                saw_discovering |= state == DiscoveryReadiness::Discovering;
                if state == DiscoveryReadiness::Ready {
                    break anyhow::Ok(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??;
        assert!(saw_discovering);
        let rows = ops::list_sessions(&home, None).await?;
        assert_eq!(rows.len(), 6);
        eprintln!(
            "oqto-9e90 slow cold scan: 6 headers x 1 s latency, 10 ms chunk budget, Ready {:?}",
            started.elapsed()
        );
        Ok(())
    }

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
        std::fs::File::open(&path)
            .expect("session file")
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(120))
            .expect("backdate session file");
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
        // Suffix authority must retain IDs containing underscores; a parser
        // that only takes the final underscore segment would miss this fact.
        let collision = "native_with_underscore";
        std::fs::write(
            directory.join(format!("2026-10-19_{collision}.jsonl")),
            format!(
                "{}\n",
                serde_json::json!({"type":"session","id":collision,"cwd":work})
            ),
        )?;
        write_session_file(&home, collision, &["default source identity fact"]);
        discover(&source, "native-owner").await?;
        assert!(
            ops::find_session_by_external(&home, collision)
                .await
                .is_none()
        );
        // Metadata failure cannot look like an empty retained default source.
        let broken = home.join(".pi/agent/sessions/broken-workspace");
        std::os::unix::fs::symlink(home.join("missing-workspace"), &broken)?;
        assert!(discover(&source, "native-owner").await.is_err());
        std::fs::remove_file(broken)?;
        assert!(discover(&source, "native-owner").await.is_ok());
        Ok(())
    }

    /// A subagent whose working folder was deleted afterwards (a removed
    /// checkout) is still discovered and indexed, also on a cached rescan.
    #[tokio::test]
    async fn discovery_indexes_sessions_whose_folder_was_deleted() -> anyhow::Result<()> {
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
        let gone = work.join("elox/extracted/elox-vendor");
        std::fs::create_dir_all(&gone)?;
        let directory = native.join(oqto_pi::session_files::cwd_to_safe_dirname(&gone));
        std::fs::create_dir(&directory)?;
        let id = "subagent-in-deleted-folder";
        std::fs::write(
            directory.join(format!("2026-10-19_{id}.jsonl")),
            format!(
                "{}\n",
                serde_json::json!({"type":"session","id":id,"cwd":gone})
            ),
        )?;
        // Warm the header cache while the folder exists, then delete it: the
        // cached-header path must not reject what a fresh read accepts.
        let _ = source.headers()?;
        std::fs::remove_dir_all(work.join("elox"))?;
        discover(&source, "native-owner").await?;
        assert!(
            ops::find_session_by_external(&home, id).await.is_some(),
            "the transcript of a deleted folder stays indexed history"
        );
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

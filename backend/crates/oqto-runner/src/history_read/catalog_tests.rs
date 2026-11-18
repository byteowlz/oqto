//! Synthetic work/cancellation probes; never read real homes or native Sessions.
use super::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Probe {
    scans: AtomicUsize,
    loads: AtomicUsize,
    workspaces: AtomicUsize,
    entered: tokio::sync::Notify,
    gate: tokio::sync::Semaphore,
    deadline_ms: AtomicUsize,
    row_limit: AtomicUsize,
    entry_limit: AtomicUsize,
    fs_pause: AtomicBool,
    fs_entered: tokio::sync::Notify,
    fs_release: (Mutex<bool>, std::sync::Condvar),
    delivery_pause: AtomicBool,
    delivery_entered: tokio::sync::Notify,
    delivery_gate: tokio::sync::Semaphore,
    operations: Mutex<Vec<(PathBuf, &'static str)>>,
}
impl Default for Probe {
    fn default() -> Self {
        Self {
            scans: AtomicUsize::new(0),
            loads: AtomicUsize::new(0),
            workspaces: AtomicUsize::new(0),
            entered: tokio::sync::Notify::new(),
            gate: tokio::sync::Semaphore::new(0),
            deadline_ms: AtomicUsize::new(0),
            row_limit: AtomicUsize::new(0),
            entry_limit: AtomicUsize::new(0),
            fs_pause: AtomicBool::new(false),
            fs_entered: tokio::sync::Notify::new(),
            fs_release: (Mutex::new(false), std::sync::Condvar::new()),
            delivery_pause: AtomicBool::new(false),
            delivery_entered: tokio::sync::Notify::new(),
            delivery_gate: tokio::sync::Semaphore::new(0),
            operations: Mutex::new(Vec::new()),
        }
    }
}
static PROBES: std::sync::LazyLock<Mutex<std::collections::HashMap<PathBuf, Arc<Probe>>>> =
    std::sync::LazyLock::new(Default::default);

pub(super) fn scanned(roots: &[PathBuf]) {
    let probe = roots
        .first()
        .and_then(|root| PROBES.lock().unwrap().get(root).cloned());
    if let Some(probe) = probe {
        probe.scans.fetch_add(1, Ordering::SeqCst);
        if probe.fs_pause.load(Ordering::SeqCst) {
            probe.fs_entered.notify_one();
            let (lock, changed) = &probe.fs_release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = changed.wait(released).unwrap();
            }
        }
    }
}
pub(super) fn entry_type(path: &std::path::Path) {
    let probe = PROBES
        .lock()
        .unwrap()
        .iter()
        .find(|(home, _)| path.starts_with(home))
        .map(|(_, probe)| probe.clone());
    if let Some(probe) = probe {
        probe
            .operations
            .lock()
            .unwrap()
            .push((path.into(), "file_type_no_follow"));
    }
}
pub(super) fn operation(roots: &[PathBuf], path: &std::path::Path, kind: &'static str) {
    if let Some(probe) = roots
        .first()
        .and_then(|root| PROBES.lock().unwrap().get(root).cloned())
    {
        probe.operations.lock().unwrap().push((path.into(), kind));
    }
}
fn limit(home: &std::path::Path, pick: impl FnOnce(&Probe) -> &AtomicUsize) -> Option<usize> {
    let probe = PROBES.lock().unwrap().get(home).cloned()?;
    let limit = pick(&probe).load(Ordering::SeqCst);
    (limit > 0).then_some(limit)
}
pub(super) fn deadline(home: &std::path::Path) -> Option<std::time::Duration> {
    limit(home, |probe| &probe.deadline_ms).map(|ms| std::time::Duration::from_millis(ms as u64))
}
pub(super) fn row_limit(home: &std::path::Path) -> Option<usize> {
    limit(home, |probe| &probe.row_limit)
}
pub(super) fn entry_limit(home: &std::path::Path) -> Option<usize> {
    limit(home, |probe| &probe.entry_limit)
}
pub(super) async fn before_delivery(home: &std::path::Path) {
    let probe = PROBES.lock().unwrap().get(home).cloned();
    if let Some(probe) = probe
        && probe.delivery_pause.load(Ordering::SeqCst)
    {
        probe.delivery_entered.notify_one();
        probe.delivery_gate.acquire().await.unwrap().forget();
    }
}
pub(super) async fn before_load(home: &std::path::Path, workspaces: &[String]) -> Result<()> {
    let probe = PROBES.lock().unwrap().get(home).cloned();
    if let Some(probe) = probe {
        probe.loads.fetch_add(1, Ordering::SeqCst);
        probe.workspaces.store(workspaces.len(), Ordering::SeqCst);
        probe.entered.notify_one();
        probe.gate.acquire().await?.forget();
    }
    Ok(())
}
struct Fixture {
    home: tempfile::TempDir,
    probe: Arc<Probe>,
    id: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        *self.probe.fs_release.0.lock().unwrap() = true;
        self.probe.fs_release.1.notify_all();
        self.probe.gate.add_permits(16);
        self.probe.delivery_gate.add_permits(16);
        PROBES.lock().unwrap().remove(self.home.path());
    }
}
#[path = "catalog_tests/lifecycle.rs"]
mod lifecycle;
#[path = "catalog_tests/negative.rs"]
mod negative;

impl Fixture {
    fn request(
        &self,
    ) -> tokio::task::JoinHandle<Result<oqto_history::oqto_log::ops::OqtoLogSessionRow>> {
        let home = self.home.path().to_owned();
        let id = self.id.clone();
        tokio::spawn(async move {
            resolve_authorized_session(&home, std::slice::from_ref(&home), &id).await
        })
    }
    async fn lookup(&self) -> Result<oqto_history::oqto_log::ops::OqtoLogSessionRow> {
        resolve_authorized_session(self.home.path(), &[self.home.path().into()], &self.id).await
    }
    async fn new() -> Result<Self> {
        let home = tempfile::tempdir()?;
        let probe = Arc::new(Probe::default());
        let mut id = String::new();
        for number in 0..4 {
            let workspace = home.path().join(format!("work-{number}"));
            std::fs::create_dir(&workspace)?;
            let native = format!("synthetic-{number}");
            let public = oqto_history::oqto_log::store::platform_id_for_external_id(&native);
            let message: oqto_pi::AgentMessage = serde_json::from_value(
                serde_json::json!({"role":"user","content":"synthetic","timestamp":1700000000000_i64}),
            )?;
            oqto_history::oqto_log::store::replace_session_with_snapshot(
                home.path(),
                "fixture",
                workspace.to_str().context("Workspace")?,
                &public,
                &public,
                Some(&native),
                &native,
                &[message],
            )
            .await?;
            if number == 0 {
                id = public;
            }
        }
        // Unrelated whole-root work must not multiply with concurrent lookups.
        for number in 0..32 {
            std::fs::create_dir_all(home.path().join(format!("unrelated-{number}/nested")))?;
        }
        PROBES
            .lock()
            .unwrap()
            .insert(home.path().to_owned(), probe.clone());
        Ok(Self { home, probe, id })
    }
}

#[tokio::test]
async fn parallel_single_session_misses_share_one_complete_catalog_build() -> Result<()> {
    let fixture = Fixture::new().await?;
    let mut requests = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let home = fixture.home.path().to_owned();
        let id = fixture.id.clone();
        requests.spawn(async move {
            resolve_authorized_session(&home, std::slice::from_ref(&home), &id).await
        });
    }
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        fixture.probe.entered.notified(),
    )
    .await?;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let scans = fixture.probe.scans.load(Ordering::SeqCst);
    let loads = fixture.probe.loads.load(Ordering::SeqCst);
    eprintln!(
        "synthetic cold lookup: 8 callers, 4 shards, {} candidate workspaces, scans={scans}, loads={loads}",
        fixture.probe.workspaces.load(Ordering::SeqCst)
    );
    assert_eq!(scans, 1, "cold lookups repeated the whole root traversal");
    assert_eq!(loads, 1, "cold lookups repeated the complete shard load");
    fixture.probe.gate.add_permits(8);
    while let Some(reply) = requests.join_next().await {
        assert_eq!(reply??.platform_id, fixture.id);
    }
    Ok(())
}

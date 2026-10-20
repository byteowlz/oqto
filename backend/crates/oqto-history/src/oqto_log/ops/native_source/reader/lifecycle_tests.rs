//! Run in fresh test processes so the process-wide readers start cold. The
//! progress-handler capture belongs to SQLite's ConnectionState (not an acquire
//! future/permit). Its Drop observes physical connection destruction; blocking
//! the callback parks the real SQLx worker while cancelled callers are replaced.
use super::*;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

static PROBE: OnceLock<Arc<Probe>> = OnceLock::new();

#[derive(Default)]
struct Counts {
    live: usize,
    peak: usize,
    created: usize,
    entered: usize,
    released: bool,
    armed: bool,
    workers: std::collections::HashSet<std::thread::ThreadId>,
}

#[derive(Default)]
struct Probe {
    counts: Mutex<Counts>,
    gate: Condvar,
    initial: bool,
}

struct PhysicalOwner(Arc<Probe>);
impl Drop for PhysicalOwner {
    fn drop(&mut self) {
        self.0.counts.lock().unwrap().live -= 1;
    }
}

// Release blocked physical workers even when an assertion fails.
struct Release(Arc<Probe>);
impl Drop for Release {
    fn drop(&mut self) {
        self.0.counts.lock().unwrap().released = true;
        self.0.gate.notify_all();
    }
}

pub(super) async fn established(
    connection: &mut sqlx::SqliteConnection,
) -> std::result::Result<(), sqlx::Error> {
    let Some(probe) = PROBE.get() else {
        return Ok(());
    };
    {
        let mut counts = probe.counts.lock().unwrap();
        counts.created += 1;
        counts.live += 1;
        counts.peak = counts.peak.max(counts.live);
    }
    let owner = PhysicalOwner(probe.clone());
    connection
        .lock_handle()
        .await?
        .set_progress_handler(1, move || {
            let mut counts = owner.0.counts.lock().unwrap();
            counts.workers.insert(std::thread::current().id());
            if counts.armed && !counts.released {
                counts.entered += 1;
                owner.0.gate.notify_all();
                while !counts.released {
                    counts = owner.0.gate.wait(counts).unwrap();
                }
            }
            true
        });
    if probe.initial {
        sqlx::query("SELECT 1").execute(connection).await?;
    }
    Ok(())
}

pub(super) async fn idle_acquire(
    connection: &mut sqlx::SqliteConnection,
) -> std::result::Result<(), sqlx::Error> {
    if PROBE.get().is_some_and(|probe| !probe.initial) {
        // The same idle-acquire await boundary as SQLx's ping, but executing
        // actual SQLite lets the progress callback deterministically stall it.
        sqlx::query("SELECT 1").execute(connection).await?;
    }
    Ok(())
}

async fn wait_for_workers(probe: &Probe) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if probe.counts.lock().unwrap().entered >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?;
    Ok(())
}

async fn fixture_homes() -> Result<Vec<tempfile::TempDir>> {
    let mut homes = Vec::new();
    for _ in 0..4 {
        let home = tempfile::tempdir()?;
        let db = super::super::resource_tests::prefix_shard(
            home.path(),
            "/synthetic/retained",
            usize::MAX,
        )
        .await?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(db))
            .await?;
        sqlx::query(
            "UPDATE oqto_log_sessions SET extensions_json='{\"personal_native_source\":\"pin\"}'",
        )
        .execute(&pool)
        .await?;
        pool.close().await;
        homes.push(home);
    }
    Ok(homes)
}

fn operation(home: PathBuf, catalog: bool) -> tokio::task::JoinHandle<Result<()>> {
    tokio::spawn(async move {
        if catalog {
            let rows = list_sessions(&home, None).await?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].platform_id, "oqto-retained");
        } else {
            validate(
                &home,
                "oqto-retained",
                "retained",
                "/synthetic/retained",
                "pin",
            )
            .await?;
        }
        Ok(())
    })
}

struct Recovery {
    homes: Vec<tempfile::TempDir>,
    before: Vec<(PathBuf, Vec<u8>)>,
    probe: Arc<Probe>,
    release: Release,
}

async fn cancel_callers(initial: bool) -> Result<Recovery> {
    let homes = fixture_homes().await?;
    let before: Vec<_> = homes
        .iter()
        .map(|home| {
            let path = crate::oqto_log::paths::existing_user_home_workspace_db_path(
                home.path(),
                "/synthetic/retained",
            );
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    let probe = Arc::new(Probe {
        initial,
        ..Probe::default()
    });
    assert!(PROBE.set(probe.clone()).is_ok());
    let release = Release(probe.clone());
    if !initial {
        let first = ReadShard::open(&before[0].0).await?;
        let second = ReadShard::open(&before[1].0).await?;
        first.finish().await?;
        second.finish().await?;
    }
    probe.counts.lock().unwrap().armed = true;
    let first = operation(homes[0].path().to_owned(), false);
    let second = operation(homes[1].path().to_owned(), true);
    wait_for_workers(&probe).await?;
    first.abort();
    second.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert!(second.await.unwrap_err().is_cancelled());
    // Replacement pressure comes from both production read entry points and
    // different canonical homes, not a stand-in pool-size assertion.
    for n in 0..12 {
        let task = operation(homes[n % homes.len()].path().to_owned(), n % 2 == 0);
        tokio::time::sleep(Duration::from_millis(20)).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
    let deadline_calls = [
        operation(homes[2].path().to_owned(), false),
        operation(homes[3].path().to_owned(), true),
    ];
    for task in deadline_calls {
        let result = tokio::time::timeout(Duration::from_secs(3), task).await??;
        assert!(
            result.is_err(),
            "blocked acquisition must refuse on its own deadline"
        );
    }
    eprintln!("R4a acquisition_deadline_refusals=2 while_workers_still_blocked=true");
    let (peak, created, live) = {
        let counts = probe.counts.lock().unwrap();
        (counts.peak, counts.created, counts.live)
    };
    eprintln!(
        "R4a phase={} cancelled=14 synthetic_homes=4 physical_owner_peak={peak} physical_owners_created={created} still_live_before_release={live}",
        if initial {
            "establishment"
        } else {
            "idle-acquire"
        }
    );
    assert!(
        peak <= 2,
        "cancelled acquisition exceeded physical owner bound: {peak}"
    );
    assert_eq!(
        created, 2,
        "cancellation must reuse, not replace, fixed workers"
    );
    Ok(Recovery {
        homes,
        before,
        probe,
        release,
    })
}

async fn recover(recovery: Recovery) -> Result<()> {
    let Recovery {
        homes,
        before,
        probe,
        release,
    } = recovery;
    drop(release);
    let mut recovered = Vec::new();
    for n in 0..16 {
        recovered.push(operation(
            homes[n % homes.len()].path().to_owned(),
            n % 2 == 0,
        ));
    }
    for task in recovered {
        tokio::time::timeout(Duration::from_secs(5), task).await???;
    }
    for (path, bytes) in before {
        assert_eq!(std::fs::read(path)?, bytes);
    }
    let counts = probe.counts.lock().unwrap();
    assert_eq!(counts.created, 2);
    assert_eq!(
        counts.workers.len(),
        2,
        "queries must execute on the same two physical threads"
    );
    eprintln!(
        "R4a recovery=16/16 owners_created={} live={} distinct_SQLite_threads={} no_replacement=true",
        counts.created,
        counts.live,
        counts.workers.len()
    );
    Ok(())
}

#[test]
fn cancelled_establishment_and_idle_acquire_bound_physical_owners() -> Result<()> {
    const ENV: &str = "OQTO_R4A_READER_PHASE";
    if let Ok(phase) = std::env::var(ENV) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let recovery = runtime.block_on(cancel_callers(phase == "initial"))?;
        // No caller runtime survives. Establishment and connection ownership
        // must outlive its shutdown too, not only an individual task's abort.
        drop(runtime);
        eprintln!("R4a caller_runtime_dropped=true while_workers_blocked=true");
        return tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?
            .block_on(recover(recovery));
    }
    let test = "oqto_log::ops::native_source::reader::lifecycle_tests::cancelled_establishment_and_idle_acquire_bound_physical_owners";
    for phase in ["initial", "idle"] {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", test, "--nocapture"])
            .env(ENV, phase)
            .status()?;
        assert!(
            status.success(),
            "{phase} cancellation child failed: {status}"
        );
    }
    Ok(())
}

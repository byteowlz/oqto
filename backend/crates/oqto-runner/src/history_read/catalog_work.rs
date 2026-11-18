//! Bounded fills of the existing complete, scoped canonical catalog, not a new index.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::watch;

type Rows = Arc<Vec<oqto_history::oqto_log::ops::OqtoLogSessionRow>>;
type Outcome = std::result::Result<Rows, Arc<anyhow::Error>>;
const DEADLINE: Duration = Duration::from_secs(5);
const FAILURE_TTL: Duration = Duration::from_secs(2);
const MAX_CACHED_SCOPES: usize = 64;
const MAX_FLIGHTS: usize = 16;
// A cancelled caller cannot release this permit while a filesystem call still runs.
pub(super) static SCANNER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Debug)]
pub(crate) struct Unavailable {
    pub(super) source_fault: bool,
    invalidated: bool,
}
impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Canonical catalog is not ready")
    }
}
impl std::error::Error for Unavailable {}

pub(super) struct Flight {
    epoch: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    reply: watch::Sender<Option<Outcome>>,
}
impl Flight {
    pub(super) fn invalidate(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.reply.send_replace(Some(Err(Arc::new(invalidated()))));
    }
}
struct Stop(Arc<AtomicBool>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
fn unavailable(message: impl Into<String>, source_fault: bool) -> anyhow::Error {
    anyhow::Error::new(Unavailable {
        source_fault,
        invalidated: false,
    })
    .context(message.into())
}
fn invalidated() -> anyhow::Error {
    anyhow::Error::new(Unavailable {
        source_fault: false,
        invalidated: true,
    })
    .context("Catalog source invalidated")
}
fn response_error(error: &anyhow::Error) -> anyhow::Error {
    let source_fault = error
        .downcast_ref::<Unavailable>()
        .is_none_or(|error| error.source_fault)
        && !error.is::<oqto_history::oqto_log::ops::CatalogRowBudget>();
    let invalidated = error
        .downcast_ref::<Unavailable>()
        .is_some_and(|error| error.invalidated);
    anyhow::Error::new(Unavailable {
        source_fault,
        invalidated,
    })
    .context(error.to_string())
}
pub(super) fn budget(available: bool, message: &'static str) -> Result<()> {
    if !available {
        return Err(unavailable(message, false));
    }
    Ok(())
}

pub(super) async fn read(home: &std::path::Path, roots: &[PathBuf]) -> Result<Rows> {
    tokio::time::timeout(DEADLINE, async {
        let mut retries = 0;
        loop {
            let result = read_once(home, roots).await;
            if retries < 3
                && result
                    .as_ref()
                    .err()
                    .and_then(|error| error.downcast_ref::<Unavailable>())
                    .is_some_and(|error| error.invalidated)
            {
                retries += 1;
                tokio::task::yield_now().await;
                continue;
            }
            return result;
        }
    })
    .await
    .map_err(|_| unavailable("Catalog lookup response deadline", false))?
}

pub(super) async fn read_once(home: &std::path::Path, roots: &[PathBuf]) -> Result<Rows> {
    let mut roots = roots.to_vec();
    roots.sort();
    roots.dedup();
    budget(roots.len() <= 8192, "Catalog root budget exceeded")?;
    let key = (home.to_owned(), roots);
    let (mut receiver, epoch) = {
        let mut cache = CATALOG_CACHE.lock().await;
        if let Some((at, rows)) = cache.rows.get(&key)
            && at.elapsed() < CATALOG_TTL
        {
            return Ok(rows.clone());
        }
        cache
            .failures
            .retain(|_, (at, _)| at.elapsed() < FAILURE_TTL);
        if let Some((_, error)) = cache.failures.get(&key) {
            return Err(response_error(error));
        }
        if let Some(flight) = cache.flights.get(&key) {
            (flight.reply.subscribe(), flight.epoch.clone())
        } else {
            budget(
                cache.flights.len() < MAX_FLIGHTS,
                "Catalog fill admission budget exceeded",
            )?;
            let (reply, receiver) = watch::channel(None);
            let flight = Arc::new(Flight {
                epoch: cache.begin(home),
                stopped: Arc::new(AtomicBool::new(false)),
                reply,
            });
            cache.flights.insert(key.clone(), flight.clone());
            let epoch = flight.epoch.clone();
            tokio::spawn(run(key, flight));
            (receiver, epoch)
        }
    };
    loop {
        let value = receiver.borrow_and_update().clone();
        if let Some(value) = value {
            #[cfg(test)]
            catalog_tests::before_delivery(home).await;
            if !epoch.load(Ordering::SeqCst) {
                return Err(invalidated());
            }
            return value.map_err(|error| response_error(&error));
        }
        receiver
            .changed()
            .await
            .map_err(|_| unavailable("Catalog fill ended", false))?;
    }
}

async fn run(key: CatalogKey, flight: Arc<Flight>) {
    let _stop = Stop(flight.stopped.clone());
    let deadline = DEADLINE;
    #[cfg(test)]
    let deadline = catalog_tests::deadline(&key.0).unwrap_or(deadline);
    let result = tokio::select! {
        biased;
        _ = flight.reply.closed() => Err(unavailable("Catalog lookup abandoned", false)),
        result = tokio::time::timeout(deadline, load(&key, &flight)) =>
            result.map_err(|_| unavailable("Catalog response deadline", false)).and_then(|result| result),
    };
    let outcome = result.map_err(Arc::new);
    let mut cache = CATALOG_CACHE.lock().await;
    let current = cache
        .flights
        .get(&key)
        .is_some_and(|active| Arc::ptr_eq(active, &flight));
    if current {
        cache.flights.remove(&key);
    }
    // Source invalidation and delivery/cache publication serialize on this lock.
    if !current || !flight.epoch.load(Ordering::SeqCst) || flight.stopped.load(Ordering::SeqCst) {
        flight.invalidate();
        return;
    }
    if flight.reply.receiver_count() == 0 {
        return;
    }
    match &outcome {
        Ok(rows) => cache.finish(key, &flight.epoch, rows.clone()),
        Err(error) => {
            if cache.failures.len() >= MAX_CACHED_SCOPES
                && let Some(oldest) = cache
                    .failures
                    .iter()
                    .min_by_key(|(_, (at, _))| *at)
                    .map(|(key, _)| key.clone())
            {
                cache.failures.remove(&oldest);
            }
            cache.failures.insert(key, (Instant::now(), error.clone()));
        }
    }
    flight.reply.send_replace(Some(outcome));
}

async fn load(key: &CatalogKey, flight: &Flight) -> Result<Rows> {
    let roots = key.1.clone();
    let home = key.0.clone();
    let epoch = flight.epoch.clone();
    let stopped = flight.stopped.clone();
    let permit = SCANNER.acquire().await?;
    let (workspaces, shards) = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let workspaces = workspaces_under(&roots, &epoch, &stopped)?;
        let mut shards = Vec::new();
        for workspace in &workspaces {
            check_scan(&epoch, &stopped)?;
            let path = oqto_history::oqto_log::paths::existing_user_home_workspace_db_path(
                &home, workspace,
            );
            match std::fs::metadata(&path) {
                Ok(metadata) => {
                    ensure!(metadata.is_file(), "Invalid catalog shard");
                    shards.push(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    check_scan(&epoch, &stopped)?;
                    // No store is normal; an existing canonical shard directory
                    // without its database is incomplete authority, not empty.
                    match std::fs::symlink_metadata(path.parent().context("Catalog shard parent")?)
                    {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Ok(_) => anyhow::bail!("Canonical catalog shard database missing"),
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok::<_, anyhow::Error>((workspaces, shards))
    })
    .await??;
    check_scan(&flight.epoch, &flight.stopped)?;
    #[cfg(test)]
    catalog_tests::before_load(&key.0, &workspaces).await?;
    #[cfg(not(test))]
    let _ = workspaces;
    let max_rows = 50_000;
    #[cfg(test)]
    let max_rows = catalog_tests::row_limit(&key.0).unwrap_or(max_rows);
    let rows =
        oqto_history::oqto_log::ops::list_sessions_in_shards_bounded(shards, max_rows).await?;
    check_scan(&flight.epoch, &flight.stopped)?;
    Ok(Arc::new(
        rows.into_iter()
            .filter(|row| {
                row.workspace_id
                    .as_deref()
                    .is_some_and(|workspace| is_under_roots(workspace, &key.1))
            })
            .collect(),
    ))
}

pub(super) fn check_scan(epoch: &AtomicBool, stopped: &AtomicBool) -> Result<()> {
    if !epoch.load(Ordering::SeqCst) {
        return Err(invalidated());
    }
    budget(!stopped.load(Ordering::SeqCst), "Catalog scan cancelled")?;
    Ok(())
}

//! Bounded native index/page work. Cache only exact unchanged file snapshots;
//! append, truncate, rewrite, replacement and head switches cannot reuse stale
//! branch payloads. The caller revalidates source authority before and after.
use anyhow::{Context, Result, ensure};
use oqto_history::oqto_log::store::native_graph::reader::NativeFileIndex;
use serde_json::Value;
use std::fs::Metadata;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

const INDEX_CACHE: usize = 4;
static WORKERS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(2)));
type Signature = (u64, u64, u64, i64, i64, i64, i64);
#[derive(Clone, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    native: String,
    workspace: String,
    signature: Signature,
}
type Index = Arc<NativeFileIndex>;
static INDEXES: LazyLock<Mutex<Vec<(Key, Index)>>> = LazyLock::new(Default::default);

#[derive(Clone)]
pub(super) struct Read {
    pub path: PathBuf,
    pub native: String,
    pub workspace: String,
    pub public: String,
    pub leaf: Option<String>,
    pub before: Option<String>,
}

pub(super) async fn page(request: Read) -> Result<Value> {
    // The blocking worker owns the permit even if its HTTP caller is cancelled.
    // Metadata scans have their own deadline; cancellation cannot accumulate
    // unbounded detached filesystem workers or page allocations.
    let permit = WORKERS.clone().acquire_owned().await?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        read(request)
    })
    .await?
}

fn signature(meta: &Metadata) -> Signature {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    )
}

fn read(request: Read) -> Result<Value> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(&request.path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.uid() == rustix::process::geteuid().as_raw(),
        "Unsafe live source"
    );
    let key = Key {
        path: request.path.clone(),
        native: request.native,
        workspace: request.workspace,
        signature: signature(&meta),
    };
    let cached = INDEXES
        .lock()
        .map_err(|_| anyhow::anyhow!("Native index cache unavailable"))?
        .iter()
        .find(|(k, _)| k == &key)
        .map(|(_, index)| index.clone());
    let index = match cached {
        Some(index) => index,
        None => Arc::new(NativeFileIndex::read(
            &mut file,
            meta.len(),
            &key.native,
            &key.workspace,
        )?),
    };
    let page = index.page(
        &mut file,
        &request.public,
        request.leaf.as_deref(),
        super::LIVE_PAGE,
        request.before.as_deref(),
    )?;
    let published = std::fs::symlink_metadata(&request.path)?;
    ensure!(
        signature(&file.metadata()?) == key.signature
            && signature(&published) == key.signature
            && published.is_file(),
        "Native source changed during history read"
    );
    let mut cache = INDEXES
        .lock()
        .map_err(|_| anyhow::anyhow!("Native index cache unavailable"))?;
    cache.retain(|(k, _)| k.path != key.path);
    if cache.len() >= INDEX_CACHE {
        cache.remove(0);
    }
    cache.push((key, index));
    drop(cache);
    // The native payload/page bound is not permission to emit an oversized wire reply.
    ensure!(
        serde_json::to_vec(&page)?.len() <= 16 * 1024 * 1024,
        "Native projected page exceeds response bound"
    );
    serde_json::to_value(page).context("Invalid native page")
}

#[cfg(test)]
mod tests;

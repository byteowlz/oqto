//! Optional hints, never admission authority. No context is constructed by the
//! production catalog caller. Every Some context still proves current headers
//! and persisted source pins with PersonalSource::validate_binding.
use super::ValidatedRelation;
use crate::{personal_source::PersonalSource, tui_rpc::ObservationBinding};
use anyhow::{Context as _, Result, ensure};
use oqto_session_relations::{MAX_CATALOG_ROWS, Relation, RelationKind};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

mod catalog;
mod io;
pub(in crate::personal_catalog) use catalog::{
    Context as CatalogContext, ReadStatus, read as read_catalog,
};
#[cfg(test)]
mod tests;

/// Owner-supplied, not deserializable and not constructible from a peer sourceId.
/// Bindings must completely cover the same fresh already-authorized outgoing
/// selection. Missing/unpinned/unsupported rows keep that selection flat.
pub(crate) struct Context<'a> {
    pub source: &'a PersonalSource,
    pub bindings: &'a [ObservationBinding],
}

#[derive(Clone, Copy)]
struct Limits {
    candidates: usize,
    files: usize,
    bytes: usize,
    edges: usize,
    descriptors: usize,
    present_files: usize,
    deadline: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            candidates: 64,
            files: 64,
            bytes: 512 * 1024,
            edges: 1024,
            descriptors: 512,
            present_files: 64,
            deadline: Duration::from_secs(4),
        }
    }
}

// Resource accounting ONLY, never a source/HOME/authority registry. No queue:
// optional hints disappear when busy. Permit is owned by the blocking job even
// after cancellation/timeout, so abandoned reads cannot multiply workers.
static WORKER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
struct Work {
    limits: Limits,
    started: Instant,
    cancelled: Arc<AtomicBool>,
    files: usize,
    bytes: usize,
    edges: usize,
    descriptors: usize,
    present_files: usize,
}
impl Work {
    fn descriptor(&mut self) -> Result<()> {
        // Reserve32 slots for bounded transient opens/validator reader owners.
        ensure!(
            self.descriptors < self.limits.descriptors.saturating_sub(32),
            "Relation descriptor budget exceeded"
        );
        self.descriptors += 1;
        Ok(())
    }
    fn present(&mut self) -> Result<()> {
        ensure!(
            self.present_files < self.limits.present_files,
            "Relation present-file budget exceeded"
        );
        self.present_files += 1;
        Ok(())
    }
    fn check(&self) -> Result<()> {
        ensure!(
            !self.cancelled.load(Ordering::Acquire)
                && self.started.elapsed() < self.limits.deadline,
            "Relation hint work unavailable"
        );
        Ok(())
    }
    fn file(&mut self) -> Result<()> {
        self.check()?;
        ensure!(
            self.files < self.limits.files,
            "Relation file work exhausted"
        );
        self.files += 1;
        Ok(())
    }
    fn bytes(&mut self, bytes: usize) -> Result<()> {
        ensure!(
            bytes <= self.limits.bytes.saturating_sub(self.bytes),
            "Relation byte work exhausted"
        );
        self.bytes += bytes;
        Ok(())
    }
    fn edges(&mut self, edges: usize) -> Result<()> {
        ensure!(
            edges <= self.limits.edges.saturating_sub(self.edges),
            "Relation edge work exhausted"
        );
        self.edges += edges;
        Ok(())
    }
}

pub(super) async fn read(data: &Value, context: Context<'_>) -> Vec<ValidatedRelation> {
    read_with_limits(data, context, Limits::default()).await
}
async fn read_with_limits(
    data: &Value,
    context: Context<'_>,
    limits: Limits,
) -> Vec<ValidatedRelation> {
    let Some(ids) = admitted_ids(data) else {
        return Vec::new();
    };
    // Complete one-source selection only: a caller-supplied prefix could omit
    // another admitted author whose sidecar conflicts with an early fact.
    if context.bindings.len() != ids.len()
        || context.bindings.len() > limits.candidates
        || limits.deadline.is_zero()
    {
        return Vec::new();
    }
    let Ok(permit) = WORKER.try_acquire() else {
        return Vec::new();
    };
    let source = context.source.clone();
    let bindings = context.bindings.to_vec();
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel = CancelOnDrop(cancelled.clone());
    let runtime = tokio::runtime::Handle::current();
    let worker = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let work = Work {
            limits,
            started: Instant::now(),
            cancelled,
            files: 0,
            bytes: 0,
            edges: 0,
            descriptors: 0,
            present_files: 0,
        };
        runtime.block_on(collect(&source, &bindings, &ids, work))
    });
    match tokio::time::timeout(limits.deadline, worker).await {
        Ok(Ok(Ok(facts))) => facts,
        _ => Vec::new(),
    }
}

fn admitted_ids(data: &Value) -> Option<HashSet<String>> {
    let rows = data.get("sessions")?.as_array()?;
    if rows.len() > MAX_CATALOG_ROWS {
        return None;
    }
    let mut ids = HashSet::new();
    for row in rows {
        let id = row.get("id")?.as_str()?;
        if id.is_empty() || id.len() > 256 || !ids.insert(id.to_owned()) {
            return None;
        }
    }
    Some(ids)
}
fn native_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

async fn collect(
    source: &PersonalSource,
    bindings: &[ObservationBinding],
    ids: &HashSet<String>,
    mut work: Work,
) -> Result<Vec<ValidatedRelation>> {
    let mut by_native = HashMap::new();
    let mut public = HashSet::new();
    // Validate the ENTIRE supplied set before reading any hints. No prefix
    // selection, duplicate overwrite, filtered-parent lookup or cached joining.
    let mut headers = Vec::new();
    for binding in bindings {
        work.check()?;
        ensure!(
            ids.contains(&binding.public_session_id)
                && public.insert(&binding.public_session_id)
                && native_id(&binding.external_session_id)
                && by_native
                    .insert(
                        binding.external_session_id.as_str(),
                        binding.public_session_id.as_str()
                    )
                    .is_none(),
            "Relation endpoints unavailable or ambiguous"
        );
        headers.push(source.validate_binding(binding).await?);
    }
    let mut snapshots = Vec::new();
    let mut directories = io::Directories::default();
    let mut facts = Vec::new();
    for (binding, header) in bindings.iter().zip(&headers) {
        work.file()?;
        let snapshot = io::Snapshot::read(
            source,
            &binding.session_file,
            header,
            &mut work,
            &mut directories,
        )?;
        if let Some(bytes) = &snapshot.bytes {
            facts.extend(parse(
                bytes,
                &binding.public_session_id,
                &binding.external_session_id,
                &by_native,
                &mut work,
            )?);
        }
        snapshots.push(snapshot);
    }
    #[cfg(test)]
    tests::after_read()?;
    // All current source pins and publication snapshots are rechecked. If any
    // optional file became unavailable, discard the whole batch, not a prefix
    // which could hide a second admitted parent/conflicting late record.
    for (binding, before) in bindings.iter().zip(&headers) {
        work.check()?;
        ensure!(
            source.validate_binding(binding).await? == *before,
            "Native source changed during relation read"
        );
    }
    for snapshot in snapshots {
        work.check()?;
        snapshot.revalidate(source)?;
    }
    work.check()?;
    Ok(facts)
}

fn parse(
    bytes: &[u8],
    author_public: &str,
    author_native: &str,
    by_native: &HashMap<&str, &str>,
    work: &mut Work,
) -> Result<Vec<ValidatedRelation>> {
    let envelope: Value = serde_json::from_slice(bytes)?;
    ensure!(
        envelope["v"].as_u64() == Some(1)
            && envelope["kind"] == "pi_session_relations"
            && envelope["complete"] == false
            && envelope["parent_session_id"] == author_native
            && envelope["updated_at"]
                .as_str()
                .is_some_and(|value| value.len() <= 64
                    && chrono::DateTime::parse_from_rfc3339(value).is_ok()),
        "Relation envelope unavailable"
    );
    let records = envelope["relations"]
        .as_array()
        .context("Relation records unavailable")?;
    ensure!(records.len() <= 200, "Relation file edge bound exceeded");
    work.edges(records.len())?;
    let mut facts = Vec::new();
    for record in records {
        work.check()?;
        if record["kind"] != "delegation"
            || record["evidence"] != "launch"
            || record["parent_session_id"] != author_native
            || record["confirmed_at_ms"]
                .as_u64()
                .is_none_or(|value| value == 0)
        {
            continue;
        }
        let Some(child) = record["child_session_id"]
            .as_str()
            .filter(|id| native_id(id))
        else {
            continue;
        };
        if child == author_native {
            continue;
        }
        if let Some(child_public) = by_native.get(child) {
            facts.push(ValidatedRelation {
                child_public: (*child_public).to_owned(),
                relation: Relation {
                    kind: RelationKind::Delegation,
                    parent_session_id: author_public.to_owned(),
                },
            });
        }
    }
    Ok(facts)
}

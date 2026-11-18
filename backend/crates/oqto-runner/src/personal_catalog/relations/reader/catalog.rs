//! Actual catalog adapter. Eligibility comes ONLY from complete canonical
//! provenance snapshots; the snapshots are NOT substitutes for real validate.
use super::*;
use oqto_history::oqto_log::ops::{OqtoLogSessionRow, native_source};
use std::path::{Path, PathBuf};

pub(crate) struct Context<'a> {
    pub source: &'a PersonalSource,
    pub home: &'a Path,
    pub roots: &'a [PathBuf],
    pub rows: &'a [OqtoLogSessionRow],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::personal_catalog) enum ReadStatus {
    Read,
    NoSource,
    CatalogMismatch,
    SourceUnavailable,
    Busy,
    Budget,
    ResponseBudget,
}
#[derive(Default, Clone, Copy, Debug)]
pub(in crate::personal_catalog) struct Counts {
    pub native: usize,
    pub validations: usize,
    pub pin_inventories: usize,
    pub identity_rows: usize,
    pub files: usize,
    pub bytes: usize,
    pub records: usize,
    pub retained_fds: usize,
    pub skipped: usize,
    pub elapsed_ms: u128,
}
/// Sidecar-publishing Sessions read per catalog. Beyond it the optional
/// relations fail closed as a whole; the base catalog is never trimmed.
pub(in crate::personal_catalog) const CATALOG_AUTHORS: usize = 128;

struct Identity {
    public: String,
    external: Option<String>,
    workspace: Option<String>,
}
struct Endpoint {
    public: String,
    native: String,
    workspace: PathBuf,
    file: PathBuf,
}

fn identities(data: &Value, rows: &[OqtoLogSessionRow]) -> Option<Vec<Identity>> {
    let ids = admitted_ids(data)?;
    if rows.len() != ids.len() || rows.len() > MAX_CATALOG_ROWS {
        return None;
    }
    let mut seen = HashSet::new();
    let mut selected = Vec::with_capacity(rows.len());
    for row in rows {
        if !ids.contains(&row.platform_id)
            || !seen.insert(&row.platform_id)
            || row
                .external_id
                .as_ref()
                .is_some_and(|value| value.len() > 256)
            || row
                .workspace_id
                .as_ref()
                .is_some_and(|value| value.len() > 4096)
        {
            return None;
        }
        // Do not clone titles/readable IDs/other unbounded presentation fields.
        selected.push(Identity {
            public: row.platform_id.clone(),
            external: row.external_id.clone(),
            workspace: row.workspace_id.clone(),
        });
    }
    Some(selected)
}

pub(in crate::personal_catalog) async fn read(
    data: &Value,
    context: Context<'_>,
) -> (Vec<ValidatedRelation>, ReadStatus, Counts) {
    let Some(rows) = identities(data, context.rows) else {
        return (Vec::new(), ReadStatus::CatalogMismatch, Counts::default());
    };
    if context.source.home != context.home || context.source.workspace_roots() != context.roots {
        return (Vec::new(), ReadStatus::SourceUnavailable, Counts::default());
    }
    let Ok(permit) = WORKER.try_acquire() else {
        return (Vec::new(), ReadStatus::Busy, Counts::default());
    };
    let source = context.source.clone();
    let runtime = tokio::runtime::Handle::current();
    // Authors (sidecar publishers) hold a native file, a sidecar and at most one
    // directory each: 128 * 3 + root stays inside the 480 usable descriptors.
    let limits = Limits {
        candidates: MAX_CATALOG_ROWS,
        files: MAX_CATALOG_ROWS,
        edges: MAX_CATALOG_ROWS * 2,
        present_files: CATALOG_AUTHORS,
        ..Limits::default()
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel = CancelOnDrop(cancelled.clone());
    let worker = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut work = Work {
            limits,
            started: Instant::now(),
            cancelled,
            files: 0,
            bytes: 0,
            edges: 0,
            descriptors: 0,
            present_files: 0,
        };
        let mut counts = Counts::default();
        let result = runtime.block_on(collect_catalog(&source, &rows, &mut work, &mut counts));
        counts.files = work.files;
        counts.bytes = work.bytes;
        counts.records = work.edges;
        counts.retained_fds = work.descriptors;
        counts.elapsed_ms = work.started.elapsed().as_millis();
        match result {
            Ok(facts) => (facts, ReadStatus::Read, counts),
            Err(_) => (Vec::new(), ReadStatus::SourceUnavailable, counts),
        }
    });
    match tokio::time::timeout(limits.deadline, worker).await {
        Ok(Ok(result)) => result,
        _ => (Vec::new(), ReadStatus::Budget, Counts::default()),
    }
}

/// One endpoint against the inventory already read this pass: same facts as
/// `native_source::validate`, without re-enumerating every shard per author.
/// Uniqueness, the pinned workspace and the accepted source pins are all still
/// required, and the whole inventory is re-read and compared afterwards.
fn validate_against(
    pins: &oqto_history::oqto_log::ops::native_source::CatalogPins,
    endpoint: &Endpoint,
    accepted: &[&str],
) -> Result<()> {
    let mut matching = pins.pins.iter().filter(|pin| pin.public == endpoint.public);
    let Some(pin) = matching.next() else {
        return Err(anyhow::anyhow!(
            "Native canonical identity unavailable or ambiguous"
        ));
    };
    ensure!(
        matching.next().is_none(),
        "Native canonical identity unavailable or ambiguous"
    );
    ensure!(
        pin.external == endpoint.native
            && pin.workspace == *endpoint.workspace.to_string_lossy()
            && accepted.contains(&pin.pin.as_str()),
        "Native source binding mismatch"
    );
    Ok(())
}

async fn collect_catalog(
    source: &PersonalSource,
    rows: &[Identity],
    work: &mut Work,
    counts: &mut Counts,
) -> Result<Vec<ValidatedRelation>> {
    work.check()?;
    let before = native_source::catalog_pins(&source.home).await?;
    counts.pin_inventories += 1;
    counts.identity_rows = before.identity_rows;
    let selected: HashMap<_, _> = rows.iter().map(|row| (row.public.as_str(), row)).collect();
    let mut endpoints = Vec::new();
    let mut publics = HashSet::new();
    let mut natives = HashSet::new();
    for pin in &before.pins {
        work.check()?;
        let Some(row) = selected.get(pin.public.as_str()) else {
            continue;
        };
        // A pinned Session whose external id is an older Pi name (for example
        // `pi-1770403246506-31fd802b06a26`) is outside the native Session domain:
        // it can never contribute an edge, and treating it as a fault failed this
        // batch as a whole for the entire catalog (owner report 2026-11-17), so no
        // Session nested anywhere. Only this class is skipped.
        if !native_id(&pin.external) {
            counts.skipped += 1;
            continue;
        }
        // Everything below is a real inconsistency between the catalog and the
        // durable pin, which stays a whole-batch refusal: never a guessed edge.
        ensure!(
            row.external.as_deref() == Some(pin.external.as_str())
                && row.workspace.as_deref() == Some(pin.workspace.as_str())
                && publics.insert(pin.public.as_str())
                && natives.insert(pin.external.as_str()),
            "Ambiguous native catalog provenance"
        );
        endpoints.push(Endpoint {
            public: pin.public.clone(),
            native: pin.external.clone(),
            workspace: PathBuf::from(&pin.workspace),
            file: PathBuf::new(),
        });
    }
    counts.native = endpoints.len();
    // Children are matched through the pinned identities of the whole catalog;
    // only authors (Sessions that publish a sidecar) are opened, validated and
    // read. A catalog of thousands of Sessions opened one descriptor each and
    // failed closed, so no relation ever reached the phone (owner 2026-11-16).
    let by_native: HashMap<String, String> = endpoints
        .iter()
        .map(|endpoint| (endpoint.native.clone(), endpoint.public.clone()))
        .collect();
    let authors = sidecar_authors(source.session_root(), &by_native, work)?;
    endpoints.retain(|endpoint| authors.contains(&endpoint.native));
    // One native descriptor per author is retained. Do not start a prefix
    // when the minimum required descriptors cannot fit the whole author set.
    ensure!(
        endpoints.len() + 1 < work.limits.descriptors.saturating_sub(32),
        "Native catalog descriptor budget exceeded"
    );
    let mut headers = Vec::with_capacity(endpoints.len());
    let mut validated: Vec<Endpoint> = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints.drain(..) {
        work.check()?;
        let mut endpoint = endpoint;
        // Per-author work is one file lookup and one header read. Identity is
        // proven against the inventory taken above (and re-proven unchanged at the
        // end), never by re-reading every shard per author: that was O(authors x
        // shards) and could not fit the deadline on a real 264-shard home.
        // An author that cannot be proven today keeps its own Session flat and is
        // counted, instead of blinding the whole catalog. Measured 2026-11-17 on a
        // real home: 44 authors held pins that no longer matched (rewritten files,
        // and pin formats `repair-native-pins` cannot classify), and a whole-batch
        // refusal turned that into "no Session nests anywhere", silently. Nothing
        // here is authority: a skipped author contributes no edge, and every edge
        // that IS emitted is still proven below and re-proven after the read.
        let Ok(Some(file)) = oqto_pi::session_files::find_unique_session_file_in(
            source.session_root(),
            &endpoint.native,
            &endpoint.workspace,
        ) else {
            counts.skipped += 1;
            continue;
        };
        endpoint.file = file;
        let Ok(header) = source.header(&endpoint.file) else {
            counts.skipped += 1;
            continue;
        };
        if validate_against(&before, &endpoint, &header.accepted_pins()).is_err() {
            counts.skipped += 1;
            continue;
        }
        headers.push(header);
        validated.push(endpoint);
        counts.validations += 1;
    }
    let endpoints = validated;
    let by_native: HashMap<_, _> = by_native
        .iter()
        .map(|(native, public)| (native.as_str(), public.as_str()))
        .collect();
    let mut directories = io::Directories::default();
    let mut snapshots = Vec::with_capacity(endpoints.len());
    let mut facts = Vec::new();
    for (endpoint, header) in endpoints.iter().zip(&headers) {
        work.file()?;
        let snapshot = io::Snapshot::read(source, &endpoint.file, header, work, &mut directories)?;
        if let Some(bytes) = &snapshot.bytes {
            facts.extend(parse(
                bytes,
                &endpoint.public,
                &endpoint.native,
                &by_native,
                work,
            )?);
        }
        snapshots.push(snapshot);
    }
    #[cfg(test)]
    super::tests::after_read()?;
    // A file that changed underneath this read is a race, not a stale pin: refuse
    // the batch so the next read retries, and never emit a half-read edge set.
    for (endpoint, before_header) in endpoints.iter().zip(&headers) {
        work.check()?;
        ensure!(
            source.header(&endpoint.file).ok().as_ref() == Some(before_header),
            "Native catalog source changed"
        );
        counts.validations += 1;
    }
    for snapshot in snapshots {
        work.check()?;
        snapshot.revalidate(source)?;
    }
    work.check()?;
    let after = native_source::catalog_pins(&source.home).await?;
    counts.pin_inventories += 1;
    ensure!(before == after, "Native catalog eligibility changed");
    work.check()?;
    Ok(facts)
}

/// Which pinned Sessions publish `<file>.relations.json`. A directory listing is
/// only a hint: every author found here is still located, validated and read
/// through the held-descriptor path above, and nothing absent is inferred.
fn sidecar_authors(
    root: &Path,
    pinned: &HashMap<String, String>,
    work: &Work,
) -> Result<HashSet<String>> {
    const SUFFIX: &str = ".jsonl.relations.json";
    const MAX_ENTRIES: usize = 200_000;
    let mut seen = 0usize;
    let mut authors = HashSet::new();
    for directory in std::fs::read_dir(root)? {
        let directory = directory?;
        if !directory.file_type()?.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(directory.path())? {
            seen += 1;
            ensure!(seen <= MAX_ENTRIES, "Native session listing bound exceeded");
            if seen.is_multiple_of(1024) {
                work.check()?;
            }
            let entry = entry?;
            let name = entry.file_name();
            let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(SUFFIX)) else {
                continue;
            };
            let Some((_, native)) = stem.rsplit_once('_') else {
                continue;
            };
            if native_id(native) && pinned.contains_key(native) && entry.file_type()?.is_file() {
                authors.insert(native.to_owned());
            }
        }
    }
    work.check()?;
    Ok(authors)
}

#[cfg(test)]
mod tests;

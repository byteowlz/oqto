//! Account-bound, read-only presentation of an explicitly selected oqto-log home.
//! No Pi process is created, no credentials are loaded, and Pi JSONL is never written.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryReadConfig {
    pub account_id: String,
    pub home: PathBuf,
    /// Absolute directories the operator designates as workspaces.
    ///
    /// Pi writes session history wherever it happens to be run, so a machine
    /// accumulates history for directories that were never meant to be shared.
    /// Only history under these roots is exposed; an empty list exposes none.
    #[serde(default)]
    pub workspace_roots: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoryReadOperation {
    List {},
    Messages {
        session_id: String,
        before: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryReadRequest {
    pub account_id: String,
    pub operation: HistoryReadOperation,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryReadResponse {
    pub data: serde_json::Value,
}
impl std::fmt::Debug for HistoryReadResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HistoryReadResponse(<private history>)")
    }
}

/// Bounded search for workspaces beneath a designated root.
///
/// Deep trees (dependencies, build output, checkouts of checkouts) would make
/// discovery cost more than the scan it replaces, so descent is limited and
/// obviously non-workspace directories are skipped.
const MAX_ROOT_DEPTH: usize = 4;
#[cfg(test)]
#[path = "history_read/catalog_tests.rs"]
mod catalog_tests;
#[path = "history_read/catalog_work.rs"]
mod catalog_work;

pub(crate) fn catalog_unavailable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<catalog_work::Unavailable>().is_some()
}
pub(crate) fn catalog_source_fault(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<catalog_work::Unavailable>()
        .is_none_or(|error| error.source_fault)
}

fn workspaces_under(
    roots: &[PathBuf],
    epoch: &std::sync::atomic::AtomicBool,
    stopped: &std::sync::atomic::AtomicBool,
) -> Result<Vec<String>> {
    #[cfg(test)]
    catalog_tests::scanned(roots);
    let started = std::time::Instant::now();
    let checkpoint = || {
        catalog_work::check_scan(epoch, stopped)?;
        catalog_work::budget(
            started.elapsed() < std::time::Duration::from_secs(5),
            "Catalog traversal budget exceeded",
        )?;
        Ok::<_, anyhow::Error>(())
    };
    let mut entries_seen = 0_usize;
    let max_entries = 65_536;
    #[cfg(test)]
    let max_entries = roots
        .first()
        .and_then(|root| catalog_tests::entry_limit(root))
        .unwrap_or(max_entries);
    let mut out = Vec::new();
    let mut pending: Vec<_> = roots
        .iter()
        .map(|root| (root.clone(), MAX_ROOT_DEPTH))
        .collect();
    while let Some((dir, depth)) = pending.pop() {
        checkpoint()?;
        catalog_work::budget(out.len() < 8192, "Catalog workspace budget exceeded")?;
        ensure!(dir.is_absolute(), "Catalog root must be absolute");
        let workspace = dir.to_str().context("Invalid workspace encoding")?;
        out.push(workspace.trim_end_matches('/').to_owned());
        if depth == 0 {
            continue;
        }
        #[cfg(test)]
        catalog_tests::operation(roots, &dir, "read_dir");
        let entries = std::fs::read_dir(&dir)?;
        for entry in entries {
            checkpoint()?;
            entries_seen += 1;
            catalog_work::budget(entries_seen <= max_entries, "Catalog entry budget exceeded")?;
            if let Some(child) = workspace_directory(entry?)? {
                catalog_work::budget(
                    pending.len().saturating_add(out.len()) < 8192,
                    "Catalog workspace budget exceeded",
                )?;
                pending.push((child, depth - 1));
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Exclude by name before any metadata; DirEntry's type never follows a target.
fn workspace_directory(entry: std::fs::DirEntry) -> Result<Option<PathBuf>> {
    let name = entry.file_name();
    if name.to_str().is_none_or(|name| {
        name.starts_with('.') || matches!(name, "node_modules" | "target" | "vendor")
    }) {
        return Ok(None);
    }
    #[cfg(test)]
    catalog_tests::entry_type(&entry.path());
    Ok(entry.file_type()?.is_dir().then(|| entry.path()))
}

fn is_under_roots(workspace: &str, roots: &[PathBuf]) -> bool {
    let workspace = std::path::Path::new(workspace.trim_end_matches('/'));
    roots.iter().any(|root| workspace.starts_with(root))
}

/// Building the catalog reads one canonical shard per workspace, so
/// expanding a machine in the UI must not pay for it on every expand. A recent
/// catalog is reused briefly, in memory only, keyed by the source and the roots
/// it was built from so a config change cannot serve the previous scope.
const CATALOG_TTL: std::time::Duration = std::time::Duration::from_secs(30);

type CatalogKey = (PathBuf, Vec<PathBuf>);
type CatalogEntry = (
    std::time::Instant,
    std::sync::Arc<Vec<oqto_history::oqto_log::ops::OqtoLogSessionRow>>,
);
#[derive(Default)]
struct CatalogCache {
    rows: std::collections::HashMap<CatalogKey, CatalogEntry>,
    flights: std::collections::HashMap<CatalogKey, std::sync::Arc<catalog_work::Flight>>,
    failures:
        std::collections::HashMap<CatalogKey, (std::time::Instant, std::sync::Arc<anyhow::Error>)>,
    // Weak references retain no idle homes. Concurrent reads of the same home
    // share a fence; invalidation removes it and cancels only those builds.
    builds: std::collections::HashMap<PathBuf, std::sync::Weak<std::sync::atomic::AtomicBool>>,
}
impl CatalogCache {
    fn begin(&mut self, home: &std::path::Path) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.builds.retain(|_, fence| fence.strong_count() > 0);
        if let Some(fence) = self.builds.get(home).and_then(std::sync::Weak::upgrade) {
            return fence;
        }
        let fence = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        self.builds
            .insert(home.to_owned(), std::sync::Arc::downgrade(&fence));
        fence
    }
    fn invalidate(&mut self, home: &std::path::Path) {
        self.flights.retain(|(source, _), flight| {
            if source == home {
                flight.invalidate();
                false
            } else {
                true
            }
        });
        self.failures.retain(|(source, _), _| source != home);
        if let Some(fence) = self.builds.remove(home).and_then(|fence| fence.upgrade()) {
            fence.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        self.rows.retain(|(cached_home, _), _| cached_home != home);
    }
    fn finish(
        &mut self,
        key: CatalogKey,
        fence: &std::sync::atomic::AtomicBool,
        rows: std::sync::Arc<Vec<oqto_history::oqto_log::ops::OqtoLogSessionRow>>,
    ) {
        self.rows.retain(|_, (at, _)| at.elapsed() < CATALOG_TTL);
        if fence.load(std::sync::atomic::Ordering::SeqCst) {
            if self.rows.len() >= 64
                && let Some(oldest) = self
                    .rows
                    .iter()
                    .min_by_key(|(_, (at, _))| *at)
                    .map(|(key, _)| key.clone())
            {
                self.rows.remove(&oldest);
            }
            self.rows.insert(key, (std::time::Instant::now(), rows));
        }
    }
}
static CATALOG_CACHE: once_cell::sync::Lazy<tokio::sync::Mutex<CatalogCache>> =
    once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(CatalogCache::default()));

pub(crate) async fn invalidate_catalog(home: &std::path::Path) {
    CATALOG_CACHE.lock().await.invalidate(home);
}

/// The same cached, root-filtered rows the catalog lists from.
pub(crate) async fn authorized_rows(
    home: &std::path::Path,
    workspace_roots: &[PathBuf],
) -> Result<std::sync::Arc<Vec<oqto_history::oqto_log::ops::OqtoLogSessionRow>>> {
    catalog_source(home, workspace_roots).await
}

async fn catalog_source(
    home: &std::path::Path,
    workspace_roots: &[PathBuf],
) -> Result<std::sync::Arc<Vec<oqto_history::oqto_log::ops::OqtoLogSessionRow>>> {
    catalog_work::read(home, workspace_roots).await
}

/// One exact public/native binding in the already-authorized local source.
/// Never pick the first index hit across Workspace owners or invent a mapping.
pub(crate) async fn resolve_authorized_session(
    home: &std::path::Path,
    roots: &[PathBuf],
    public_id: &str,
) -> Result<oqto_history::oqto_log::ops::OqtoLogSessionRow> {
    let rows = catalog_source(home, roots).await?;
    let mut found = rows.iter().filter(|row| row.platform_id == public_id);
    let row = found.next().context("Canonical Session unavailable")?;
    ensure!(found.next().is_none(), "Ambiguous public Session identity");
    ensure!(
        !public_id.starts_with("pending-")
            && !public_id.starts_with("tmp:")
            && row.external_id.as_ref().is_some_and(|id| id != public_id),
        "Canonical native binding required"
    );
    Ok(row.clone())
}

/// Personal native chat is unavailable until the background writer has an
/// evidenced head. Never fall back to an unselected legacy/linear transcript.
pub(crate) async fn read_native_authorized_history(
    home: &std::path::Path,
    roots: &[PathBuf],
    session_id: String,
    before: Option<String>,
) -> Result<HistoryReadResponse> {
    ensure!(
        session_id.len() <= 256 && before.as_ref().is_none_or(|cursor| cursor.len() <= 2048),
        "Invalid native history request"
    );
    let row = resolve_authorized_session(home, roots, &session_id).await?;
    let workspace = row.workspace_id.context("Workspace unavailable")?;
    let head =
        oqto_history::oqto_log::projector::native_head_for_workspace(home, &workspace, &session_id)
            .await?
            .context("Native history synchronization pending")?;
    let page = oqto_history::oqto_log::projector::project_session_messages_page_for_workspace(
        home,
        &workspace,
        &session_id,
        50,
        before.as_deref(),
    )
    .await?
    .context("Native history unavailable")?;
    let after =
        oqto_history::oqto_log::projector::native_head_for_workspace(home, &workspace, &session_id)
            .await?;
    ensure!(
        after.as_ref() == Some(&head),
        "Native head changed during history read; discard page and resnapshot"
    );
    let mut data = serde_json::to_value(page)?;
    data.as_object_mut()
        .context("Invalid history page")?
        .insert("native_graph".into(), head);
    Ok(HistoryReadResponse { data })
}

pub async fn read(
    config: Option<&HistoryReadConfig>,
    dedicated: bool,
    request: HistoryReadRequest,
) -> Result<HistoryReadResponse> {
    let config = config.context("History access disabled")?;
    ensure!(
        dedicated && !config.account_id.is_empty() && config.account_id == request.account_id,
        "History access denied"
    );
    read_authorized_source(&config.home, &config.workspace_roots, request.operation).await
}

/// Runner-only source adapter AFTER local paired-device capability admission.
/// No synthetic Account/MFA is created; durable IDs/projections remain oqto-log's.
pub(crate) async fn read_authorized_source(
    home: &std::path::Path,
    workspace_roots: &[PathBuf],
    operation: HistoryReadOperation,
) -> Result<HistoryReadResponse> {
    ensure!(
        home.is_absolute() && home.is_dir(),
        "History home unavailable"
    );
    let rows = catalog_source(home, workspace_roots).await?;
    let mut identities = std::collections::HashMap::new();
    for row in rows.iter() {
        *identities.entry(row.platform_id.clone()).or_insert(0_usize) += 1;
    }
    let data = match operation {
        HistoryReadOperation::List {} => {
            let sessions: Vec<_> = rows
                .iter()
                .filter(|row| {
                    !row.platform_id.is_empty() && identities.get(&row.platform_id) == Some(&1)
                })
                .take(5000)
                .map(|row| {
                    serde_json::json!({
                        "id": row.platform_id, "title": row.title, "workspace": row.workspace_id,
                        "updated_at": row.updated_at,
                    })
                })
                .collect();
            serde_json::json!({"sessions": sessions, "read_only": true})
        }
        HistoryReadOperation::Messages { session_id, before } => {
            ensure!(
                !session_id.is_empty()
                    && session_id.len() <= 256
                    && before.as_ref().is_none_or(|s| s.len() <= 2048),
                "Invalid history request"
            );
            ensure!(
                identities.get(&session_id) == Some(&1),
                "Unambiguous public Session ID required"
            );
            let page = oqto_history::oqto_log::projector::project_session_messages_page_auto(
                home,
                &session_id,
                50,
                before.as_deref(),
            )
            .await?
            .context("History unavailable")?;
            serde_json::to_value(page)?
        }
    };
    Ok(HistoryReadResponse { data })
}

#[cfg(test)]
async fn catalog(
    config: &HistoryReadConfig,
) -> Result<std::sync::Arc<Vec<oqto_history::oqto_log::ops::OqtoLogSessionRow>>> {
    catalog_source(&config.home, &config.workspace_roots).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn authorization_precedes_source_access_and_rejects_shared_mode() {
        let config = HistoryReadConfig {
            account_id: "alice".into(),
            home: "/not/a/history/home".into(),
            workspace_roots: Vec::new(),
        };
        for (account, dedicated) in [("bob", true), ("alice", false)] {
            let result = read(
                Some(&config),
                dedicated,
                HistoryReadRequest {
                    account_id: account.into(),
                    operation: HistoryReadOperation::List {},
                },
            )
            .await;
            assert_eq!(result.unwrap_err().to_string(), "History access denied");
        }
    }
    fn scoped(home: &std::path::Path, roots: Vec<PathBuf>) -> HistoryReadConfig {
        HistoryReadConfig {
            account_id: "alice".into(),
            home: home.into(),
            workspace_roots: roots,
        }
    }

    #[test]
    fn catalog_build_fences_are_home_scoped_and_reject_every_stale_writer() {
        use std::sync::Arc;
        let mut cache = CatalogCache::default();
        let a = PathBuf::from("/synthetic/a");
        let b = PathBuf::from("/synthetic/b");
        let key_a = (a.clone(), vec![a.clone()]);
        let key_b = (b.clone(), vec![b.clone()]);
        let first = cache.begin(&a);
        let overlapping = cache.begin(&a);
        assert!(Arc::ptr_eq(&first, &overlapping));
        let unrelated = cache.begin(&b);
        cache.invalidate(&b);
        let rows = Arc::new(Vec::new());
        cache.finish(key_a.clone(), &first, rows.clone());
        cache.finish(key_b.clone(), &unrelated, rows.clone());
        assert!(Arc::ptr_eq(&cache.rows[&key_a].1, &rows));
        assert!(!cache.rows.contains_key(&key_b));
        cache.invalidate(&a);
        cache.finish(key_a.clone(), &first, rows.clone());
        cache.finish(key_a.clone(), &overlapping, rows.clone());
        assert!(
            !cache.rows.contains_key(&key_a),
            "neither stale writer may repopulate"
        );
        let fresh = cache.begin(&a);
        let new_scope = (a.clone(), vec![a.clone(), b.clone()]);
        cache.finish(key_a.clone(), &fresh, rows.clone());
        cache.finish(new_scope.clone(), &fresh, rows.clone());
        cache.finish(key_a.clone(), &first, Arc::new(Vec::new()));
        assert!(
            Arc::ptr_eq(&cache.rows[&key_a].1, &rows),
            "late old result cannot replace the new snapshot"
        );
        cache.invalidate(&a);
        assert!(!cache.rows.contains_key(&key_a) && !cache.rows.contains_key(&new_scope));
        drop((first, overlapping, fresh, unrelated));
        let _fresh = cache.begin(&b);
        assert_eq!(
            cache.builds.len(),
            1,
            "finished homes must not accumulate fences"
        );
    }

    #[tokio::test]
    async fn a_cached_catalog_is_reused_briefly_and_then_expires() {
        let home = tempfile::tempdir().unwrap();
        let config = scoped(home.path(), vec![home.path().into()]);
        let rows = catalog(&config).await.unwrap();
        assert!(
            std::sync::Arc::ptr_eq(&rows, &catalog(&config).await.unwrap()),
            "a second read within the TTL must reuse the catalog"
        );

        // Age the entry past its TTL: the next read must rebuild rather than
        // serve history that no longer reflects the machine.
        {
            let mut cache = CATALOG_CACHE.lock().await;
            let key = (config.home.clone(), config.workspace_roots.clone());
            let entry = cache.rows.get_mut(&key).expect("cached");
            entry.0 = std::time::Instant::now() - CATALOG_TTL - std::time::Duration::from_secs(1);
        }
        assert!(
            !std::sync::Arc::ptr_eq(&rows, &catalog(&config).await.unwrap()),
            "stale catalog served"
        );

        // A different scope must never be served from another scope's entry.
        let other = tempfile::tempdir().unwrap();
        let widened = scoped(home.path(), vec![home.path().into(), other.path().into()]);
        assert!(!std::sync::Arc::ptr_eq(
            &catalog(&config).await.unwrap(),
            &catalog(&widened).await.unwrap()
        ));
    }

    #[test]
    fn only_designated_workspaces_are_in_scope() {
        let home = tempfile::tempdir().unwrap();
        let designated = home.path().join("byteowlz");
        std::fs::create_dir_all(designated.join("oqto/backend")).unwrap();
        std::fs::create_dir_all(designated.join(".git/objects")).unwrap();
        std::fs::create_dir_all(designated.join("oqto/node_modules/pkg")).unwrap();
        std::fs::create_dir_all(home.path().join("private-notes")).unwrap();

        let roots = vec![designated.clone()];
        let found = workspaces_under(
            &roots,
            &std::sync::atomic::AtomicBool::new(true),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        let has = |p: &std::path::Path| found.iter().any(|w| std::path::Path::new(w) == p);

        assert!(has(&designated), "the root itself is a workspace");
        assert!(has(&designated.join("oqto/backend")), "nested workspace");
        assert!(
            !has(&designated.join(".git")),
            "hidden dirs are not workspaces"
        );
        assert!(
            !has(&designated.join("oqto/node_modules/pkg")),
            "dependency trees are not workspaces"
        );
        assert!(
            !found
                .iter()
                .any(|w| w.contains("private-notes") || w.contains("Pi ran here")),
            "history outside the designated roots must never be listed"
        );

        // Rows are filtered on the workspace they record, not only on which
        // store happened to be opened.
        assert!(is_under_roots(
            designated.join("oqto").to_str().unwrap(),
            &roots
        ));
        assert!(!is_under_roots("/tmp/somewhere-else", &roots));
        assert!(!is_under_roots(
            home.path().join("private-notes").to_str().unwrap(),
            &roots
        ));
    }

    #[tokio::test]
    async fn no_designated_roots_exposes_no_history() {
        let home = tempfile::tempdir().unwrap();
        let data = read(
            Some(&scoped(home.path(), Vec::new())),
            true,
            HistoryReadRequest {
                account_id: "alice".into(),
                operation: HistoryReadOperation::List {},
            },
        )
        .await
        .unwrap()
        .data;
        assert_eq!(data["sessions"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn reads_canonical_history_without_touching_pi_or_accepting_harness_ids() {
        let home = tempfile::tempdir().unwrap();
        let pi = home.path().join(".pi/agent/sessions");
        std::fs::create_dir_all(&pi).unwrap();
        let sentinel = pi.join("fixture.jsonl");
        std::fs::write(&sentinel, "Pi-owned sentinel\n").unwrap();
        let designated = home.path().join("byteowlz");
        let workspace = designated.join("project");
        std::fs::create_dir_all(&workspace).unwrap();
        let workspace = workspace.to_str().unwrap().to_owned();
        let public_id =
            oqto_history::oqto_log::store::platform_id_for_external_id("native-fixture");
        let message: oqto_pi::AgentMessage = serde_json::from_value(serde_json::json!({"role":"user","content":"History fixture","timestamp":1700000000000_i64})).unwrap();
        oqto_history::oqto_log::store::replace_session_with_snapshot(
            home.path(),
            "native-user",
            &workspace,
            &public_id,
            &public_id,
            Some("native-fixture"),
            "native-fixture",
            &[message],
        )
        .await
        .unwrap();
        let config = HistoryReadConfig {
            account_id: "alice".into(),
            home: home.path().to_owned(),
            workspace_roots: vec![designated],
        };
        let request = |operation| HistoryReadRequest {
            account_id: "alice".into(),
            operation,
        };
        let catalog = read(Some(&config), true, request(HistoryReadOperation::List {}))
            .await
            .unwrap();
        assert_eq!(catalog.data["sessions"][0]["id"], public_id);
        let page = read(
            Some(&config),
            true,
            request(HistoryReadOperation::Messages {
                session_id: public_id,
                before: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(page.data["messages"].as_array().unwrap().len(), 1);
        assert!(
            read(
                Some(&config),
                true,
                request(HistoryReadOperation::Messages {
                    session_id: "native-fixture".into(),
                    before: None
                })
            )
            .await
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(sentinel).unwrap(),
            "Pi-owned sentinel\n"
        );
    }
    #[test]
    fn wire_is_read_only_and_does_not_accept_source_paths() {
        assert!(
            serde_json::from_value::<HistoryReadOperation>(
                serde_json::json!({"command":"list","home":"/other"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<HistoryReadOperation>(
                serde_json::json!({"command":"prompt","message":"no"})
            )
            .is_err()
        );
        let response = crate::protocol::RunnerResponse::HistoryRead(HistoryReadResponse {
            data: serde_json::json!({"sessions":[]}),
        });
        let encoded = serde_json::to_value(&response).unwrap();
        assert!(serde_json::from_value::<crate::protocol::RunnerResponse>(encoded).is_ok());
        assert!(
            !format!(
                "{:?}",
                HistoryReadResponse {
                    data: serde_json::json!("private-message")
                }
            )
            .contains("private-message")
        );
    }
}

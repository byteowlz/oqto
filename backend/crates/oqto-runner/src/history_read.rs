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

fn workspaces_under(roots: &[PathBuf]) -> Vec<String> {
    fn walk(dir: &std::path::Path, depth: usize, out: &mut Vec<String>) {
        if let Some(path) = dir.to_str() {
            out.push(path.trim_end_matches('/').to_string());
        }
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let child = entry.path();
            if !child.is_dir() || child.is_symlink() {
                continue;
            }
            let skip = child
                .file_name()
                .and_then(|n| n.to_str())
                .is_none_or(|name| {
                    name.starts_with('.') || matches!(name, "node_modules" | "target" | "vendor")
                });
            if !skip {
                walk(&child, depth - 1, out);
            }
        }
    }

    let mut out = Vec::new();
    for root in roots {
        if root.is_absolute() && root.is_dir() {
            walk(root, MAX_ROOT_DEPTH, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn is_under_roots(workspace: &str, roots: &[PathBuf]) -> bool {
    let workspace = std::path::Path::new(workspace.trim_end_matches('/'));
    roots.iter().any(|root| workspace.starts_with(root))
}

/// Building the catalog opens and migrates one store per workspace, so
/// expanding a machine in the UI must not pay for it on every expand. A recent
/// catalog is reused briefly, in memory only, keyed by the source and the roots
/// it was built from so a config change cannot serve the previous scope.
const CATALOG_TTL: std::time::Duration = std::time::Duration::from_secs(30);

type CatalogKey = (PathBuf, Vec<PathBuf>);
type CatalogEntry = (
    std::time::Instant,
    std::sync::Arc<Vec<oqto_history::oqto_log::ops::OqtoLogSessionRow>>,
);
static CATALOG_CACHE: once_cell::sync::Lazy<
    tokio::sync::Mutex<std::collections::HashMap<CatalogKey, CatalogEntry>>,
> = once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(std::collections::HashMap::new()));

async fn catalog(
    config: &HistoryReadConfig,
) -> Result<std::sync::Arc<Vec<oqto_history::oqto_log::ops::OqtoLogSessionRow>>> {
    let key: CatalogKey = (config.home.clone(), config.workspace_roots.clone());
    {
        let cache = CATALOG_CACHE.lock().await;
        if let Some((at, rows)) = cache.get(&key)
            && at.elapsed() < CATALOG_TTL
        {
            return Ok(rows.clone());
        }
    }

    let workspaces = workspaces_under(&config.workspace_roots);
    let rows = oqto_history::oqto_log::ops::list_sessions_for_workspaces(&config.home, &workspaces)
        .await?
        .into_iter()
        .filter(|row| {
            row.workspace_id
                .as_deref()
                .is_some_and(|workspace| is_under_roots(workspace, &config.workspace_roots))
        })
        .collect::<Vec<_>>();

    let rows = std::sync::Arc::new(rows);
    let mut cache = CATALOG_CACHE.lock().await;
    cache.retain(|_, (at, _)| at.elapsed() < CATALOG_TTL);
    cache.insert(key, (std::time::Instant::now(), rows.clone()));
    Ok(rows)
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
    ensure!(
        config.home.is_absolute() && config.home.is_dir(),
        "History home unavailable"
    );
    let rows = catalog(config).await?;
    let mut identities = std::collections::HashMap::new();
    for row in rows.iter() {
        *identities.entry(row.platform_id.clone()).or_insert(0_usize) += 1;
    }
    let data = match request.operation {
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
                &config.home,
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
            let entry = cache.get_mut(&key).expect("cached");
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
        let found = workspaces_under(&roots);
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

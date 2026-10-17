//! Read-only current activity for authorized managed Pi sessions. No execution routing/recovery.
use std::path::{Component, Path};
use std::time::Duration;

use axum::{
    Json,
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use oqto_runner::runtime_activity::{ACTIVITY_VERSION, ActivitySnapshot, SessionActivity};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiResult, AppState};
use crate::auth::CurrentUser;
use crate::runner::router::{ExecutionTarget, resolve_existing_runner_for_observation};
use crate::session_target::{SessionTargetRecord, SessionTargetScope};

#[derive(Default, Deserialize)]
pub struct ActivityQuery {
    pub shared_workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActivityFeed {
    pub version: u32,
    /// Scope is explicit: arbitrary external/herdr terminals are not silently counted.
    pub source: &'static str,
    pub coverage: &'static str,
    pub observed_at_ms: i64,
    pub sessions: Vec<SessionActivity>,
}

fn within_root(path: &Path, root: &Path) -> bool {
    path.is_absolute()
        && root.is_absolute()
        && path.starts_with(root)
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
}

fn personal_record_matches(record: &SessionTargetRecord, owner: &str, path: &Path) -> bool {
    record.scope == SessionTargetScope::Personal
        && record.workspace_id.is_none()
        && record.owner_user_id.as_deref() == Some(owner)
        && record.workspace_path.as_deref() == path.to_str()
}

fn feed(coverage: &'static str, sessions: Vec<SessionActivity>) -> ActivityFeed {
    ActivityFeed {
        version: ACTIVITY_VERSION,
        source: "oqto_managed_pi",
        coverage,
        observed_at_ms: chrono::Utc::now().timestamp_millis(),
        sessions,
    }
}
fn response(feed: ActivityFeed) -> Response {
    ([(header::CACHE_CONTROL, "no-store")], Json(feed)).into_response()
}

pub async fn read(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<ActivityQuery>,
) -> ApiResult<Response> {
    let target = query
        .shared_workspace_id
        .as_ref()
        .map_or(ExecutionTarget::Personal, |id| {
            ExecutionTarget::SharedWorkspace {
                workspace_id: id.clone(),
            }
        });
    // Authorize shared membership before endpoint lookup/dial, even when unavailable.
    if let Some(id) = query.shared_workspace_id.as_deref() {
        let service = state
            .shared_workspaces
            .as_ref()
            .ok_or_else(|| ApiError::forbidden("activity access denied"))?;
        service
            .get(id, user.id())
            .await
            .map_err(|_| ApiError::forbidden("activity access denied"))?
            .ok_or_else(|| ApiError::forbidden("activity access denied"))?;
    }
    let client = match resolve_existing_runner_for_observation(&state, user.id(), &target).await {
        Ok(client) => client,
        Err(_) => return Ok(response(feed("unavailable", Vec::new()))),
    };
    let snapshot =
        match tokio::time::timeout(Duration::from_secs(2), client.pi_activity_snapshot()).await {
            Ok(Ok(snapshot))
                if snapshot.version == ACTIVITY_VERSION
                    && snapshot.sessions.len()
                        <= oqto_runner::runtime_activity::MAX_ACTIVITY_SESSIONS =>
            {
                snapshot
            }
            _ => return Ok(response(feed("unavailable", Vec::new()))),
        };
    let ActivitySnapshot {
        mut complete,
        sessions,
        ..
    } = snapshot;
    let allowed_root = state.sessions.for_user(user.id()).workspace_root();
    let mut visible = Vec::new();
    for item in sessions {
        if item.session_id.is_empty()
            || item.session_id.len() > 256
            || item.execution_id.len() > 64
            || item.attention_count > 128
            || item.workspace_path.as_os_str().len() > 4096
            || !item.workspace_path.is_absolute()
            || item
                .workspace_path
                .components()
                .any(|p| matches!(p, Component::ParentDir))
        {
            complete = false;
            continue;
        }
        if let Some(id) = query.shared_workspace_id.as_deref() {
            let service = state
                .shared_workspaces
                .as_ref()
                .ok_or_else(|| ApiError::forbidden("activity access denied"))?;
            let access = service
                .check_access_for_path(&item.workspace_path.to_string_lossy(), user.id())
                .await
                .map_err(|_| ApiError::forbidden("activity access denied"))?;
            if !access.is_some_and(|(workspace, _)| workspace.id == id) {
                continue;
            }
        } else {
            if !within_root(&item.workspace_path, &allowed_root) {
                continue;
            }
            match state.session_targets.get(&item.session_id).await {
                Ok(Some(record))
                    if personal_record_matches(&record, user.id(), &item.workspace_path) => {}
                Ok(Some(record))
                    if record.scope == SessionTargetScope::Personal
                        && record.owner_user_id.as_deref() == Some(user.id()) =>
                {
                    complete = false;
                    continue; // Owned but missing/moved binding is a coverage gap, not idle.
                }
                Ok(Some(_)) => continue,
                // Legacy development-only rows still require the authorized personal root.
                Ok(None) if !state.user_isolation_enabled() => {}
                _ => {
                    complete = false;
                    continue;
                }
            }
        }
        visible.push(item);
    }
    // Recheck membership after the bounded remote read: revocation is not an idle snapshot.
    if let Some(id) = query.shared_workspace_id.as_deref() {
        let service = state
            .shared_workspaces
            .as_ref()
            .ok_or_else(|| ApiError::forbidden("activity access denied"))?;
        service
            .get(id, user.id())
            .await
            .map_err(|_| ApiError::forbidden("activity access denied"))?
            .ok_or_else(|| ApiError::forbidden("activity access denied"))?;
    }
    Ok(response(feed(
        if complete { "complete" } else { "partial" },
        visible,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn personal_activity_requires_exact_owner_scope_and_work_directory() {
        let mut record = SessionTargetRecord {
            session_id: "synthetic".into(),
            owner_user_id: Some("owner".into()),
            scope: SessionTargetScope::Personal,
            workspace_id: None,
            workspace_path: Some("/home/demo/projects/p".into()),
        };
        let path = Path::new("/home/demo/projects/p");
        assert!(personal_record_matches(&record, "owner", path));
        assert!(!personal_record_matches(&record, "other", path));
        assert!(!personal_record_matches(
            &record,
            "owner",
            Path::new("/home/demo/projects/p-other")
        ));
        record.scope = SessionTargetScope::SharedWorkspace;
        assert!(!personal_record_matches(&record, "owner", path));
        record.scope = SessionTargetScope::RemoteMachine;
        assert!(!personal_record_matches(&record, "owner", path));
        record.scope = SessionTargetScope::Personal;
        record.owner_user_id = None;
        assert!(!personal_record_matches(&record, "owner", path));
    }
    #[test]
    fn activity_roots_are_exact_and_do_not_allow_sibling_or_parent_escape() {
        let root = Path::new("/home/demo/projects");
        assert!(within_root(Path::new("/home/demo/projects/tool"), root));
        assert!(!within_root(
            Path::new("/home/demo/projects-other/tool"),
            root
        ));
        assert!(!within_root(
            Path::new("/home/demo/projects/../private"),
            root
        ));
        assert!(!within_root(Path::new("relative"), root));
    }
    #[test]
    fn missing_coverage_is_explicit_and_never_a_fake_idle_count() {
        let value = serde_json::to_value(feed("unavailable", Vec::new())).unwrap();
        assert_eq!(value["coverage"], "unavailable");
        assert_eq!(value["source"], "oqto_managed_pi");
        assert!(value.get("running").is_none());
    }
}

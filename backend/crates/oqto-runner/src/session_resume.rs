//! Resolve the Pi session file to continue, on the machine that owns it.
//!
//! Only this host can see its own Pi session files, so resolution cannot happen
//! on a control plane acting for a remote machine. A public Oqto session id is
//! also not a Pi filename: imported sessions carry a minted id bound to Pi's
//! native id, and the binding is the only bridge back to the file.

use oqto_history::oqto_log::{bindings, paths};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::{Path, PathBuf};

/// Where a machine's Pi session files may live.
#[derive(Debug, Clone)]
pub struct SessionFileSources {
    /// Sessions this runner creates.
    pub pi_sessions_dir: PathBuf,
    /// Home whose native Pi profile and canonical stores were imported, if any.
    pub history_home: Option<PathBuf>,
}

impl SessionFileSources {
    fn sessions_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.pi_sessions_dir.clone()];
        if let Some(home) = &self.history_home {
            let native = home.join(".pi").join("agent").join("sessions");
            if !dirs.contains(&native) {
                dirs.push(native);
            }
        }
        dirs
    }
}

/// Native Pi session ids bound to a public Oqto session id.
async fn bound_native_ids(home: &Path, workspace: &Path, session_id: &str) -> Vec<String> {
    let db = paths::existing_user_home_workspace_db_path(home, &workspace.to_string_lossy());
    if !db.exists() {
        return Vec::new();
    }
    let Ok(pool) = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&db).read_only(true))
        .await
    else {
        return Vec::new();
    };
    let Ok(mut conn) = pool.acquire().await else {
        return Vec::new();
    };
    bindings::list_session_bindings(&mut conn, session_id)
        .await
        .map(|found| {
            found
                .into_iter()
                .filter(|binding| binding.harness == "pi")
                .map(|binding| binding.external_id)
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve the session file to continue for `session_id` in `workspace`.
///
/// Returns `None` when nothing is known, which starts a fresh session rather
/// than silently continuing an unrelated conversation.
pub async fn resolve_continue_session(
    sources: &SessionFileSources,
    session_id: &str,
    workspace: &Path,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    // Sessions Oqto started are named by their own id.
    candidates.push(session_id.to_string());
    if session_id.starts_with("oqto-")
        && let Some(home) = &sources.history_home
    {
        candidates.extend(bound_native_ids(home, workspace, session_id).await);
    }

    for dir in sources.sessions_dirs() {
        for candidate in &candidates {
            if let Some(found) =
                oqto_pi::session_files::find_session_file_in(&dir, candidate, Some(workspace))
            {
                return Some(found);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_session(dir: &Path, workspace: &str, native_id: &str) -> PathBuf {
        let safe = format!(
            "--{}--",
            workspace.trim_start_matches('/').replace('/', "-")
        );
        let workspace_dir = dir.join(safe);
        std::fs::create_dir_all(&workspace_dir).unwrap();
        let file = workspace_dir.join(format!("2026-09-09T10-00-00-000Z_{native_id}.jsonl"));
        std::fs::write(&file, "{}\n").unwrap();
        file
    }

    #[tokio::test]
    async fn an_oqto_started_session_resolves_by_its_own_id() {
        let home = tempfile::tempdir().unwrap();
        let sessions = home.path().join("sessions");
        let workspace = home.path().join("work");
        std::fs::create_dir_all(&workspace).unwrap();
        let expected = write_session(&sessions, &workspace.to_string_lossy(), "sess-1");

        let sources = SessionFileSources {
            pi_sessions_dir: sessions,
            history_home: None,
        };
        assert_eq!(
            resolve_continue_session(&sources, "sess-1", &workspace).await,
            Some(expected)
        );
    }

    #[tokio::test]
    async fn an_imported_session_resolves_through_its_native_binding() {
        let home = tempfile::tempdir().unwrap();
        let workspace = home.path().join("byteowlz/project");
        std::fs::create_dir_all(&workspace).unwrap();
        let workspace_str = workspace.to_string_lossy().to_string();

        let native_id = "9f1c2d3e-4a5b-6c7d-8e9f-0a1b2c3d4e5f";
        let public_id = oqto_history::oqto_log::store::platform_id_for_external_id(native_id);
        let message: oqto_pi::AgentMessage = serde_json::from_value(serde_json::json!({
            "role": "user", "content": "imported", "timestamp": 1700000000000_i64
        }))
        .unwrap();
        oqto_history::oqto_log::store::replace_session_with_snapshot(
            home.path(),
            "user",
            &workspace_str,
            &public_id,
            &public_id,
            Some(native_id),
            native_id,
            &[message],
        )
        .await
        .unwrap();

        // The file lives in the imported native profile, not the runner's own.
        let native_sessions = home.path().join(".pi/agent/sessions");
        let expected = write_session(&native_sessions, &workspace_str, native_id);

        let sources = SessionFileSources {
            pi_sessions_dir: home.path().join("managed/sessions"),
            history_home: Some(home.path().to_path_buf()),
        };

        assert_eq!(
            resolve_continue_session(&sources, &public_id, &workspace).await,
            Some(expected),
            "a public id must resolve to Pi's file through the binding"
        );

        // An unknown public id must not continue somebody else's conversation.
        assert_eq!(
            resolve_continue_session(
                &sources,
                "oqto-00000000-0000-5000-8000-000000000000",
                &workspace
            )
            .await,
            None
        );
    }
}

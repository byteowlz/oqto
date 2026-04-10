use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

fn decode_workspace_path_from_safe_dirname(dirname: &str) -> Option<String> {
    let trimmed = dirname.trim();
    let core = trimmed
        .strip_prefix("--")
        .and_then(|v| v.strip_suffix("--"))
        .unwrap_or(trimmed);
    if core.is_empty() {
        return None;
    }
    Some(format!("/{}", core.replace('-', "/")))
}

fn parse_pi_session_id_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    let (_, session_id) = stem.rsplit_once('_')?;
    if session_id.is_empty() {
        None
    } else {
        Some(session_id.to_string())
    }
}

#[derive(Debug, serde::Deserialize)]
struct JsonlMessageEntry {
    #[serde(rename = "type")]
    entry_type: String,
    message: Option<crate::pi::AgentMessage>,
}

fn count_jsonl_message_entries(path: &Path) -> usize {
    use std::io::BufRead;

    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return 0,
    };

    let reader = std::io::BufReader::new(file);
    let mut count = 0usize;

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(entry) = serde_json::from_str::<JsonlMessageEntry>(trimmed) else {
            continue;
        };

        // Must match importer semantics exactly: only parseable message entries
        // with a concrete AgentMessage payload are considered importable.
        if entry.entry_type == "message" && entry.message.is_some() {
            count += 1;
        }
    }

    count
}

#[derive(Debug, Default, Clone)]
pub struct ValidationReport {
    pub sessions_checked: usize,
    pub sessions_ok: usize,
    pub sessions_mismatch: usize,
    pub jsonl_messages_total: usize,
    pub oqto_log_messages_total: usize,
    pub mismatches: Vec<String>,
}

async fn count_oqto_log_session_messages(
    user_home: &Path,
    workspace_id: &str,
    session_id: &str,
) -> usize {
    let db_path =
        crate::oqto_log::paths::resolve_user_home_workspace_db_path(user_home, workspace_id)
            .unwrap_or_else(|_| {
                user_home.join(".local/share/oqto/oqto-log/invalid/oqto-log.sqlite")
            });
    if !db_path.exists() {
        return 0;
    }

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .read_only(true);
    let Ok(pool) = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
    else {
        return 0;
    };

    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM oqto_log_messages m
        JOIN oqto_log_turns t ON t.turn_id = m.turn_id
        WHERE t.session_id = ?
        "#,
    )
    .bind(session_id)
    .fetch_one(&pool)
    .await
    .ok()
    .unwrap_or(0)
    .max(0) as usize
}

pub async fn validate_bootstrap_import(user_home: &Path) -> Result<ValidationReport> {
    let base = user_home.join(".pi").join("agent").join("sessions");
    let mut report = ValidationReport::default();

    let Ok(workspaces) = std::fs::read_dir(base) else {
        return Ok(report);
    };

    let mut session_files: HashMap<(String, String), PathBuf> = HashMap::new();
    for workspace in workspaces.flatten() {
        let workspace_dir_path = workspace.path();
        if !workspace_dir_path.is_dir() {
            continue;
        }

        let workspace_id = workspace_dir_path
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(decode_workspace_path_from_safe_dirname)
            .unwrap_or_else(|| "global".to_string());

        let Ok(entries) = std::fs::read_dir(&workspace_dir_path) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|v| v.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(session_id) = parse_pi_session_id_from_path(&path) else {
                continue;
            };
            session_files.insert((workspace_id.clone(), session_id), path);
        }
    }

    for ((workspace_id, session_id), path) in session_files {
        let jsonl_count = count_jsonl_message_entries(&path);
        if jsonl_count == 0 {
            continue;
        }

        let oqto_count =
            count_oqto_log_session_messages(user_home, &workspace_id, &session_id).await;
        report.sessions_checked += 1;
        report.jsonl_messages_total += jsonl_count;
        report.oqto_log_messages_total += oqto_count;

        if oqto_count >= jsonl_count {
            report.sessions_ok += 1;
        } else {
            report.sessions_mismatch += 1;
            report.mismatches.push(format!(
                "workspace={} session={} jsonl_messages={} oqto_log_messages={}",
                workspace_id, session_id, jsonl_count, oqto_count
            ));
        }
    }

    Ok(report)
}

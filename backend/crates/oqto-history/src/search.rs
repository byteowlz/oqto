use std::{env, path::PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// JSON envelope returned by the hstry CLI.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HstryJsonResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub error: Option<String>,
}

/// A single search hit returned by hstry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HstrySearchHit {
    pub message_id: String,
    pub conversation_id: String,
    pub message_idx: i32,
    pub role: String,
    pub content: String,
    pub snippet: String,
    pub created_at: Option<DateTime<Utc>>,
    pub conv_created_at: DateTime<Utc>,
    pub conv_updated_at: Option<DateTime<Utc>>,
    pub score: f32,
    pub source_id: String,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub source_adapter: String,
    pub source_path: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
}

fn resolve_hstry_path() -> String {
    if let Ok(path) = env::var("HSTRY_PATH") {
        return path;
    }
    if let Ok(home) = env::var("HOME") {
        let local_bin = PathBuf::from(&home).join(".local/bin/hstry");
        if local_bin.exists() {
            return local_bin.to_string_lossy().to_string();
        }
    }
    "hstry".to_string()
}

/// Run a fast hstry search via the CLI.
pub async fn search_hstry(query: &str, limit: usize) -> Result<Vec<HstrySearchHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let limit = limit.max(1);
    let hstry_path = resolve_hstry_path();
    let output = tokio::process::Command::new(&hstry_path)
        .arg("search")
        .arg(query)
        .arg("--limit")
        .arg(limit.to_string())
        .arg("--scope")
        .arg("local")
        .arg("--json")
        .env("HOME", env::var("HOME").unwrap_or_default())
        .output()
        .await
        .with_context(|| format!("Failed to execute hstry at '{hstry_path}'"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.trim().is_empty() {
            "hstry search failed".to_string()
        } else {
            stderr.trim().to_string()
        };
        anyhow::bail!(message);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }

    let response: HstryJsonResponse<Vec<HstrySearchHit>> =
        serde_json::from_str(&stdout).context("Failed to parse hstry search output")?;
    if !response.ok {
        let error = response
            .error
            .unwrap_or_else(|| "hstry search failed".to_string());
        anyhow::bail!(error);
    }

    Ok(response.result.unwrap_or_default())
}

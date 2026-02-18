//! Audit logging for user-facing backend events.

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

#[derive(Debug, Serialize)]
pub struct AuditEvent {
    pub timestamp: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
}

#[derive(Clone)]
pub struct AuditLogger {
    file: Arc<Mutex<File>>,
    path: PathBuf,
}

impl AuditLogger {
    pub async fn new(path: PathBuf) -> Result<Self> {
        ensure_parent_dir(&path)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .with_context(|| format!("opening audit log file {}", path.display()))?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn log_http(
        &self,
        user_id: &str,
        method: &str,
        path: &str,
        status: u16,
        duration_ms: u128,
    ) {
        let event = AuditEvent {
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            event: "http_request".to_string(),
            user_id: Some(user_id.to_string()),
            method: Some(method.to_string()),
            path: Some(path.to_string()),
            status: Some(status),
            duration_ms: Some(duration_ms),
            ws_command: None,
            session_id: None,
            workspace_path: None,
        };
        self.write_event(&event).await;
    }

    pub async fn log_ws_command(
        &self,
        user_id: &str,
        command: &str,
        session_id: Option<&str>,
        workspace_path: Option<&str>,
    ) {
        let event = AuditEvent {
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            event: "ws_command".to_string(),
            user_id: Some(user_id.to_string()),
            method: None,
            path: None,
            status: None,
            duration_ms: None,
            ws_command: Some(command.to_string()),
            session_id: session_id.map(|s| s.to_string()),
            workspace_path: workspace_path.map(|s| s.to_string()),
        };
        self.write_event(&event).await;
    }

    async fn write_event(&self, event: &AuditEvent) {
        if let Ok(line) = serde_json::to_string(event) {
            let mut file = self.file.lock().await;
            if file.write_all(line.as_bytes()).await.is_ok() {
                let _ = file.write_all(b"\n").await;
            }
        }
    }
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating audit log directory {}", parent.display()))?;
    }
    Ok(())
}

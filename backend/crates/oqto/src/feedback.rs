use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::time;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedbackConfig {
    /// Public dropbox path (readable/writable by all).
    pub public_dropbox: PathBuf,
    /// Private archive path (owner-only).
    pub private_archive: PathBuf,
    /// Keep public copies after syncing.
    pub keep_public: bool,
    /// Sync interval in seconds.
    pub sync_interval_seconds: u64,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            public_dropbox: PathBuf::from("/usr/local/share/oqto/issues"),
            private_archive: PathBuf::from("/var/lib/oqto/issue-archive"),
            keep_public: true,
            sync_interval_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackEntry {
    pub id: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub user_id: String,
    pub user_name: Option<String>,
    pub workspace_path: Option<String>,
    pub tags: Vec<String>,
}

pub fn ensure_feedback_dirs(config: &FeedbackConfig) -> Result<()> {
    std::fs::create_dir_all(&config.public_dropbox)
        .with_context(|| format!("creating {}", config.public_dropbox.display()))?;
    std::fs::create_dir_all(&config.private_archive)
        .with_context(|| format!("creating {}", config.private_archive.display()))?;
    Ok(())
}

pub async fn write_feedback_entry(
    config: &FeedbackConfig,
    entry: &FeedbackEntry,
) -> Result<PathBuf> {
    fs::create_dir_all(&config.public_dropbox)
        .await
        .with_context(|| format!("creating {}", config.public_dropbox.display()))?;

    let filename = format!("{}_{}.json", entry.created_at.replace(':', "-"), entry.id);
    let path = config.public_dropbox.join(filename);
    let payload = serde_json::to_vec_pretty(entry).context("serializing feedback entry")?;
    fs::write(&path, payload)
        .await
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

pub fn new_feedback_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let random: u32 = rand::random();
    format!("{:x}{:08x}", now, random)
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub async fn sync_feedback_loop(config: FeedbackConfig) {
    let mut interval = time::interval(Duration::from_secs(config.sync_interval_seconds));
    loop {
        interval.tick().await;
        if let Err(err) = sync_feedback_once(&config).await {
            tracing::warn!("feedback sync failed: {}", err);
        }
    }
}

async fn sync_feedback_once(config: &FeedbackConfig) -> Result<()> {
    fs::create_dir_all(&config.private_archive)
        .await
        .with_context(|| format!("creating {}", config.private_archive.display()))?;

    let mut entries = fs::read_dir(&config.public_dropbox)
        .await
        .with_context(|| format!("reading {}", config.public_dropbox.display()))?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let file_name = match path.file_name() {
                Some(name) => name.to_owned(),
                None => continue,
            };
            let target = config.private_archive.join(file_name);
            if target.exists() {
                continue;
            }
            fs::copy(&path, &target)
                .await
                .with_context(|| format!("copying {} -> {}", path.display(), target.display()))?;
            if !config.keep_public {
                let _ = fs::remove_file(&path).await;
            }
        }
    }

    Ok(())
}

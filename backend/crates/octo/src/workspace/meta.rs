use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    pub display_name: Option<String>,
    pub language: Option<String>,
    pub pinned: Option<bool>,
    pub bootstrap_pending: Option<bool>,
}

pub fn workspace_meta_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".octo").join("workspace.toml")
}

pub fn load_workspace_meta(workspace_root: &Path) -> Option<WorkspaceMeta> {
    let path = workspace_meta_path(workspace_root);
    let contents = std::fs::read_to_string(&path).ok()?;
    parse_workspace_meta(&contents)
}

pub fn parse_workspace_meta(contents: &str) -> Option<WorkspaceMeta> {
    toml::from_str(contents).ok()
}

pub fn workspace_display_name(workspace_root: &Path) -> Option<String> {
    let meta = load_workspace_meta(workspace_root)?;
    meta.display_name.and_then(|name| {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

pub fn write_workspace_meta(workspace_root: &Path, meta: &WorkspaceMeta) -> Result<PathBuf> {
    let path = workspace_meta_path(workspace_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating workspace metadata dir: {}", parent.display()))?;
    }
    let body = toml::to_string_pretty(meta).context("serializing workspace metadata")?;
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

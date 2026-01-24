use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub project_id: String,
    pub shared: bool,
    #[serde(default)]
    pub template_path: Option<String>,
}

pub fn metadata_path(workspace_path: &Path) -> PathBuf {
    workspace_path.join(".octo").join("project.json")
}

#[allow(dead_code)]
pub fn read_metadata(workspace_path: &Path) -> Result<Option<ProjectMetadata>> {
    let path = metadata_path(workspace_path);
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("reading project metadata {:?}", path))?;
    let metadata = serde_json::from_str(&contents).context("parsing project metadata")?;
    Ok(Some(metadata))
}

pub fn write_metadata(workspace_path: &Path, metadata: &ProjectMetadata) -> Result<()> {
    let path = metadata_path(workspace_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating metadata directory {:?}", parent))?;
    }
    let contents =
        serde_json::to_string_pretty(metadata).context("serializing project metadata")?;
    std::fs::write(&path, contents)
        .with_context(|| format!("writing project metadata {:?}", path))?;
    Ok(())
}

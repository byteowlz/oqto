use anyhow::Result;
use log::{debug, warn};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelMode {
    #[default]
    Global,
    Merge,
    Restrict,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModelsConfig {
    #[serde(default)]
    pub mode: ModelMode,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub models: ModelsConfig,
}

impl WorkspaceConfig {
    pub fn config_path(workspace: &Path) -> PathBuf {
        workspace.join(".oqto").join("config.toml")
    }

    pub fn models_json_path(workspace: &Path) -> PathBuf {
        workspace.join(".oqto").join("models.json")
    }

    pub fn load(workspace: &Path) -> Self {
        let config_path = Self::config_path(workspace);
        if !config_path.exists() {
            debug!(
                "No workspace config at {}, using defaults",
                config_path.display()
            );
            return Self::default();
        }

        match std::fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str::<WorkspaceConfig>(&content) {
                Ok(config) => config,
                Err(err) => {
                    warn!(
                        "Failed to parse {}: {}. Using defaults.",
                        config_path.display(),
                        err
                    );
                    Self::default()
                }
            },
            Err(err) => {
                warn!(
                    "Failed to read {}: {}. Using defaults.",
                    config_path.display(),
                    err
                );
                Self::default()
            }
        }
    }

    pub fn has_workspace_models(workspace: &Path) -> bool {
        Self::models_json_path(workspace).exists()
    }

    pub fn effective_model_mode(&self, workspace: &Path) -> ModelMode {
        match self.models.mode {
            ModelMode::Global => ModelMode::Global,
            mode @ (ModelMode::Merge | ModelMode::Restrict) => {
                if Self::has_workspace_models(workspace) {
                    mode
                } else {
                    warn!(
                        "Workspace config requests models.mode={:?} but .oqto/models.json not found in {}. Falling back to global.",
                        mode,
                        workspace.display()
                    );
                    ModelMode::Global
                }
            }
        }
    }

    pub fn merge_models_json(global_path: &Path, workspace_path: &Path) -> Result<String> {
        let global: serde_json::Value = if global_path.exists() {
            let content = std::fs::read_to_string(global_path)?;
            serde_json::from_str(&content)?
        } else {
            serde_json::json!({ "providers": {} })
        };

        let workspace: serde_json::Value = {
            let content = std::fs::read_to_string(workspace_path)?;
            serde_json::from_str(&content)?
        };

        let mut merged = global.clone();
        if let (Some(merged_providers), Some(ws_providers)) = (
            merged
                .as_object_mut()
                .and_then(|o| o.get_mut("providers"))
                .and_then(|p| p.as_object_mut()),
            workspace
                .as_object()
                .and_then(|o| o.get("providers"))
                .and_then(|p| p.as_object()),
        ) {
            for (key, value) in ws_providers {
                merged_providers.insert(key.clone(), value.clone());
            }
        }

        Ok(serde_json::to_string_pretty(&merged)?)
    }
}

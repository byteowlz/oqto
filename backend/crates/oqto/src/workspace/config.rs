//! Workspace configuration from `.oqto/config.toml`.
//!
//! This file is checked into version control and defines workspace-level
//! preferences (model catalog mode, harness settings, etc.).

use anyhow::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Model catalog resolution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelMode {
    /// Use global ~/.pi/agent/models.json as-is (default, no-op).
    #[default]
    Global,
    /// Merge global + workspace .oqto/models.json (workspace adds/overrides).
    Merge,
    /// Only workspace .oqto/models.json, global hidden via bwrap bind-mount.
    Restrict,
}

/// `[models]` section of `.oqto/config.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModelsConfig {
    /// How to resolve the model catalog for this workspace.
    #[serde(default)]
    pub mode: ModelMode,
}

/// Top-level `.oqto/config.toml` structure.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WorkspaceConfig {
    /// Model catalog configuration.
    #[serde(default)]
    pub models: ModelsConfig,
}

impl WorkspaceConfig {
    /// Path to `.oqto/config.toml` within a workspace.
    pub fn config_path(workspace: &Path) -> PathBuf {
        workspace.join(".oqto").join("config.toml")
    }

    /// Path to `.oqto/models.json` within a workspace.
    pub fn models_json_path(workspace: &Path) -> PathBuf {
        workspace.join(".oqto").join("models.json")
    }

    /// Load workspace config from `.oqto/config.toml`.
    /// Returns default config if file doesn't exist or can't be parsed.
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
                Ok(config) => {
                    debug!(
                        "Loaded workspace config from {}: models.mode={:?}",
                        config_path.display(),
                        config.models.mode
                    );
                    config
                }
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

    /// Check if workspace has a models.json file.
    pub fn has_workspace_models(workspace: &Path) -> bool {
        Self::models_json_path(workspace).exists()
    }

    /// Resolve the effective model catalog mode.
    ///
    /// If mode is `merge` or `restrict` but no `.oqto/models.json` exists,
    /// falls back to `global` with a warning.
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

    /// Merge global and workspace models.json files.
    ///
    /// Workspace providers are upserted into the global catalog.
    /// Returns the merged JSON string.
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

        // Merge: workspace providers override/add to global
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = WorkspaceConfig::default();
        assert_eq!(config.models.mode, ModelMode::Global);
    }

    #[test]
    fn test_load_missing_file() {
        let tmp = TempDir::new().unwrap();
        let config = WorkspaceConfig::load(tmp.path());
        assert_eq!(config.models.mode, ModelMode::Global);
    }

    #[test]
    fn test_load_restrict_mode() {
        let tmp = TempDir::new().unwrap();
        let oqto_dir = tmp.path().join(".oqto");
        std::fs::create_dir_all(&oqto_dir).unwrap();
        std::fs::write(
            oqto_dir.join("config.toml"),
            r#"
[models]
mode = "restrict"
"#,
        )
        .unwrap();
        let config = WorkspaceConfig::load(tmp.path());
        assert_eq!(config.models.mode, ModelMode::Restrict);
    }

    #[test]
    fn test_effective_mode_fallback() {
        let tmp = TempDir::new().unwrap();
        let config = WorkspaceConfig {
            models: ModelsConfig {
                mode: ModelMode::Restrict,
            },
        };
        // No .oqto/models.json → falls back to global
        assert_eq!(config.effective_model_mode(tmp.path()), ModelMode::Global);

        // Create .oqto/models.json → restrict works
        let oqto_dir = tmp.path().join(".oqto");
        std::fs::create_dir_all(&oqto_dir).unwrap();
        std::fs::write(oqto_dir.join("models.json"), r#"{"providers":{}}"#).unwrap();
        assert_eq!(config.effective_model_mode(tmp.path()), ModelMode::Restrict);
    }

    #[test]
    fn test_merge_models() {
        let tmp = TempDir::new().unwrap();
        let global_path = tmp.path().join("global.json");
        let ws_path = tmp.path().join("workspace.json");

        std::fs::write(
            &global_path,
            r#"{"providers":{"eavs-anthropic":{"baseUrl":"http://eavs/anthropic/v1","models":[{"id":"claude-sonnet-4"}]}}}"#,
        )
        .unwrap();
        std::fs::write(
            &ws_path,
            r#"{"providers":{"ollama":{"baseUrl":"http://localhost:11434/v1","apiKey":"EAVS_API_KEY","models":[{"id":"qwen3:32b"}]}}}"#,
        )
        .unwrap();

        let merged = WorkspaceConfig::merge_models_json(&global_path, &ws_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&merged).unwrap();
        let providers = parsed["providers"].as_object().unwrap();
        assert!(providers.contains_key("eavs-anthropic"));
        assert!(providers.contains_key("ollama"));
    }
}

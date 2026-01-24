//! Configuration for onboarding templates.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Default templates repository URL.
pub const DEFAULT_TEMPLATES_REPO: &str = "https://github.com/byteowlz/octo-templates";

/// Configuration for onboarding templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OnboardingTemplatesConfig {
    /// Git repository URL for templates.
    /// Default: https://github.com/byteowlz/octo-templates
    pub repo_url: String,

    /// Local path to templates directory.
    /// If set, overrides repo_url (no git sync).
    pub local_path: Option<PathBuf>,

    /// Path where remote repo is cloned.
    /// Default: ~/.local/share/octo/onboarding-templates
    pub cache_path: Option<PathBuf>,

    /// Whether to sync from remote before using templates.
    pub sync_enabled: bool,

    /// Minimum seconds between sync attempts.
    pub sync_interval_seconds: u64,

    /// Use embedded templates as fallback if repo unavailable.
    pub use_embedded_fallback: bool,

    /// Branch to use from remote repo.
    pub branch: String,

    /// Subdirectory within repo containing onboarding templates.
    /// Default: "onboarding"
    pub subdirectory: String,

    /// Default template files for new users.
    #[serde(default)]
    pub defaults: TemplateDefaults,

    /// Named presets for different user types.
    #[serde(default)]
    pub presets: HashMap<String, TemplatePreset>,
}

impl Default for OnboardingTemplatesConfig {
    fn default() -> Self {
        Self {
            repo_url: DEFAULT_TEMPLATES_REPO.to_string(),
            local_path: None,
            cache_path: None,
            sync_enabled: true,
            sync_interval_seconds: 300, // 5 minutes
            use_embedded_fallback: true,
            branch: "main".to_string(),
            subdirectory: "onboarding".to_string(),
            defaults: TemplateDefaults::default(),
            presets: default_presets(),
        }
    }
}

/// Default template file names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateDefaults {
    pub onboard: String,
    pub personality: String,
    pub user: String,
    pub agents: String,
}

impl Default for TemplateDefaults {
    fn default() -> Self {
        Self {
            onboard: "ONBOARD.md".to_string(),
            personality: "PERSONALITY.md".to_string(),
            user: "USER.md".to_string(),
            agents: "AGENTS.md".to_string(),
        }
    }
}

/// A preset configuration for a specific user type.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TemplatePreset {
    /// Description of this preset.
    #[serde(default)]
    pub description: String,

    /// Override for ONBOARD.md template.
    pub onboard: Option<String>,

    /// Override for PERSONALITY.md template.
    pub personality: Option<String>,

    /// Override for USER.md template.
    pub user: Option<String>,

    /// Override for AGENTS.md template.
    pub agents: Option<String>,

    /// Stages to skip during onboarding.
    #[serde(default)]
    pub skip_stages: Vec<String>,

    /// Components to unlock immediately.
    #[serde(default)]
    pub unlock_components: Vec<String>,

    /// User level to set (beginner, intermediate, technical).
    pub user_level: Option<String>,
}

/// Create default presets.
fn default_presets() -> HashMap<String, TemplatePreset> {
    let mut presets = HashMap::new();

    presets.insert(
        "developer".to_string(),
        TemplatePreset {
            description: "Technical users familiar with AI coding assistants".to_string(),
            personality: Some("PERSONALITY_TECHNICAL.md".to_string()),
            user_level: Some("technical".to_string()),
            unlock_components: vec!["terminal".to_string(), "file_tree".to_string()],
            ..Default::default()
        },
    );

    presets.insert(
        "beginner".to_string(),
        TemplatePreset {
            description: "New users who need more guidance".to_string(),
            onboard: Some("ONBOARD_BEGINNER.md".to_string()),
            personality: Some("PERSONALITY_FRIENDLY.md".to_string()),
            user_level: Some("beginner".to_string()),
            ..Default::default()
        },
    );

    presets.insert(
        "enterprise".to_string(),
        TemplatePreset {
            description: "Work-focused setup, skip personal customization".to_string(),
            onboard: Some("ONBOARD_ENTERPRISE.md".to_string()),
            skip_stages: vec!["personality".to_string()],
            user_level: Some("intermediate".to_string()),
            unlock_components: vec![
                "sidebar".to_string(),
                "session_list".to_string(),
                "file_tree".to_string(),
                "settings".to_string(),
            ],
            ..Default::default()
        },
    );

    presets
}

/// Per-user template overrides (used when creating a user).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserTemplateOverrides {
    /// Preset name to use (e.g., "developer", "beginner").
    pub preset: Option<String>,

    /// Language code for i18n templates (e.g., "de", "fr").
    pub language: Option<String>,

    /// Direct template file overrides.
    #[serde(default)]
    pub templates: HashMap<String, String>,

    /// Additional stages to skip.
    #[serde(default)]
    pub skip_stages: Vec<String>,

    /// Additional components to unlock.
    #[serde(default)]
    pub unlock_components: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OnboardingTemplatesConfig::default();
        assert_eq!(config.repo_url, DEFAULT_TEMPLATES_REPO);
        assert!(config.sync_enabled);
        assert!(config.use_embedded_fallback);
        assert_eq!(config.defaults.onboard, "ONBOARD.md");
    }

    #[test]
    fn test_default_presets() {
        let config = OnboardingTemplatesConfig::default();
        assert!(config.presets.contains_key("developer"));
        assert!(config.presets.contains_key("beginner"));
        assert!(config.presets.contains_key("enterprise"));
    }

    #[test]
    fn test_serialization() {
        let config = OnboardingTemplatesConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: OnboardingTemplatesConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.repo_url, config.repo_url);
    }
}

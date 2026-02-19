//! Service for managing onboarding templates.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use super::config::{OnboardingTemplatesConfig, TemplatePreset, UserTemplateOverrides};

/// Embedded fallback templates (compiled into binary).
mod embedded {
    pub const ONBOARD: &str = include_str!("embedded/BOOTSTRAP.md");
    pub const PERSONALITY: &str = include_str!("embedded/PERSONALITY.md");
    pub const USER: &str = include_str!("embedded/USER.md");
    pub const AGENTS: &str = include_str!("embedded/AGENTS.md");
}

/// Template types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateType {
    Onboard,
    Personality,
    User,
    Agents,
}

impl TemplateType {
    pub fn filename(&self) -> &'static str {
        match self {
            TemplateType::Onboard => "BOOTSTRAP.md",
            TemplateType::Personality => "PERSONALITY.md",
            TemplateType::User => "USER.md",
            TemplateType::Agents => "AGENTS.md",
        }
    }

    pub fn embedded(&self) -> &'static str {
        match self {
            TemplateType::Onboard => embedded::ONBOARD,
            TemplateType::Personality => embedded::PERSONALITY,
            TemplateType::User => embedded::USER,
            TemplateType::Agents => embedded::AGENTS,
        }
    }
}

/// Resolved templates for a user.
#[derive(Debug, Clone)]
pub struct ResolvedTemplates {
    pub onboard: String,
    pub personality: String,
    pub user: String,
    pub agents: String,
    #[allow(dead_code)]
    pub skip_stages: Vec<String>,
    pub unlock_components: Vec<String>,
    pub user_level: Option<String>,
}

/// Service for managing onboarding templates.
#[derive(Debug)]
pub struct OnboardingTemplatesService {
    config: OnboardingTemplatesConfig,
    templates_dir: PathBuf,
    last_sync: Arc<Mutex<Option<Instant>>>,
}

impl OnboardingTemplatesService {
    /// Create a new service from config.
    pub fn new(config: OnboardingTemplatesConfig, data_dir: &Path) -> Self {
        // Determine templates directory
        let templates_dir = config
            .local_path
            .clone()
            .or_else(|| config.cache_path.clone())
            .unwrap_or_else(|| data_dir.join("onboarding-templates"));

        Self {
            config,
            templates_dir,
            last_sync: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the templates directory path.
    pub fn templates_dir(&self) -> &Path {
        &self.templates_dir
    }

    /// Get the subdirectory within the templates dir.
    pub fn subdirectory(&self) -> &str {
        &self.config.subdirectory
    }

    /// Check if using local path (no git sync).
    pub fn is_local(&self) -> bool {
        self.config.local_path.is_some()
    }

    /// Sync templates from remote repository if needed.
    pub async fn sync(&self) -> Result<()> {
        // Skip if using local path
        if self.is_local() {
            debug!("Using local templates path, skipping sync");
            return Ok(());
        }

        // Skip if sync disabled
        if !self.config.sync_enabled {
            debug!("Template sync disabled");
            return Ok(());
        }

        // Check if we synced recently
        let should_sync = {
            let last_sync = self.last_sync.lock().await;
            match *last_sync {
                Some(instant) => {
                    instant.elapsed() > Duration::from_secs(self.config.sync_interval_seconds)
                }
                None => true,
            }
        };

        if !should_sync {
            debug!("Templates synced recently, skipping");
            return Ok(());
        }

        // Clone or pull
        if !self.templates_dir.exists() {
            self.clone_repo().await?;
        } else if self.templates_dir.join(".git").exists() {
            self.pull_repo().await?;
        } else {
            warn!(
                "Templates directory exists but is not a git repo: {}",
                self.templates_dir.display()
            );
        }

        // Update last sync time
        *self.last_sync.lock().await = Some(Instant::now());
        Ok(())
    }

    /// Clone the templates repository.
    async fn clone_repo(&self) -> Result<()> {
        info!(
            "Cloning templates repo {} to {}",
            self.config.repo_url,
            self.templates_dir.display()
        );

        // Create parent directory
        if let Some(parent) = self.templates_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg("--branch")
            .arg(&self.config.branch)
            .arg(&self.config.repo_url)
            .arg(&self.templates_dir)
            .output()
            .await
            .context("failed to run git clone")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git clone failed: {}", stderr);
        }

        info!("Templates repo cloned successfully");
        Ok(())
    }

    /// Pull latest changes from remote.
    async fn pull_repo(&self) -> Result<()> {
        debug!("Pulling templates repo updates");

        let output = Command::new("git")
            .arg("-C")
            .arg(&self.templates_dir)
            .arg("pull")
            .arg("--ff-only")
            .output()
            .await
            .context("failed to run git pull")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("git pull failed (will use cached): {}", stderr);
        } else {
            debug!("Templates repo updated");
        }

        Ok(())
    }

    /// Resolve templates for a user with optional overrides.
    pub async fn resolve(
        &self,
        overrides: Option<&UserTemplateOverrides>,
    ) -> Result<ResolvedTemplates> {
        // Try to sync first (non-blocking if fails)
        if let Err(e) = self.sync().await {
            warn!("Failed to sync templates: {}", e);
        }

        // Get preset if specified
        let preset = overrides
            .and_then(|o| o.preset.as_ref())
            .and_then(|name| self.config.presets.get(name));

        // Resolve each template
        let onboard = self
            .resolve_template(TemplateType::Onboard, overrides, preset)
            .await?;
        let personality = self
            .resolve_template(TemplateType::Personality, overrides, preset)
            .await?;
        let user = self
            .resolve_template(TemplateType::User, overrides, preset)
            .await?;
        let agents = self
            .resolve_template(TemplateType::Agents, overrides, preset)
            .await?;

        // Merge skip stages and unlock components
        let mut skip_stages = Vec::new();
        let mut unlock_components = Vec::new();
        let mut user_level = None;

        if let Some(preset) = preset {
            skip_stages.extend(preset.skip_stages.clone());
            unlock_components.extend(preset.unlock_components.clone());
            user_level = preset.user_level.clone();
        }

        if let Some(o) = overrides {
            skip_stages.extend(o.skip_stages.clone());
            unlock_components.extend(o.unlock_components.clone());
        }

        Ok(ResolvedTemplates {
            onboard,
            personality,
            user,
            agents,
            skip_stages,
            unlock_components,
            user_level,
        })
    }

    /// Resolve a single template.
    async fn resolve_template(
        &self,
        template_type: TemplateType,
        overrides: Option<&UserTemplateOverrides>,
        preset: Option<&TemplatePreset>,
    ) -> Result<String> {
        // Check for direct override
        let override_file = overrides.and_then(|o| {
            o.templates
                .get(template_type.filename().trim_end_matches(".md"))
        });

        // Check for preset override
        let preset_file = preset.and_then(|p| match template_type {
            TemplateType::Onboard => p.onboard.as_ref(),
            TemplateType::Personality => p.personality.as_ref(),
            TemplateType::User => p.user.as_ref(),
            TemplateType::Agents => p.agents.as_ref(),
        });

        // Check for language-specific template
        let language = overrides.and_then(|o| o.language.as_ref());

        // Determine filename to look for
        let filename = override_file
            .or(preset_file)
            .map(|s| s.as_str())
            .unwrap_or(template_type.filename());

        // Build paths to try
        let mut paths_to_try = Vec::new();

        let base_dir = self.templates_dir.join(&self.config.subdirectory);

        // Language-specific path
        if let Some(lang) = language {
            paths_to_try.push(base_dir.join("i18n").join(lang).join(filename));
        }

        // Standard path
        paths_to_try.push(base_dir.join(filename));

        // Try each path
        for path in &paths_to_try {
            if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        debug!("Loaded template from {}", path.display());
                        return Ok(content);
                    }
                    Err(e) => {
                        warn!("Failed to read template {}: {}", path.display(), e);
                    }
                }
            }
        }

        // Fall back to embedded
        if self.config.use_embedded_fallback {
            debug!("Using embedded fallback for {}", template_type.filename());
            return Ok(template_type.embedded().to_string());
        }

        anyhow::bail!(
            "Template not found: {} (tried: {:?})",
            filename,
            paths_to_try
        );
    }

    /// List available presets.
    pub fn list_presets(&self) -> Vec<(String, String)> {
        self.config
            .presets
            .iter()
            .map(|(name, preset)| (name.clone(), preset.description.clone()))
            .collect()
    }

    /// List available templates in the repo.
    pub async fn list_templates(&self) -> Result<Vec<String>> {
        // Sync first
        if let Err(e) = self.sync().await {
            warn!("Failed to sync templates: {}", e);
        }

        let base_dir = self.templates_dir.join(&self.config.subdirectory);
        if !base_dir.exists() {
            return Ok(Vec::new());
        }

        let mut templates = Vec::new();
        for entry in std::fs::read_dir(&base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && path.extension().is_some_and(|e| e == "md")
                && let Some(name) = path.file_name()
            {
                templates.push(name.to_string_lossy().to_string());
            }
        }

        // Also check i18n subdirectories
        let i18n_dir = base_dir.join("i18n");
        if i18n_dir.exists() {
            for lang_entry in std::fs::read_dir(&i18n_dir)? {
                let lang_entry = lang_entry?;
                if lang_entry.path().is_dir() {
                    let lang = lang_entry.file_name().to_string_lossy().to_string();
                    for entry in std::fs::read_dir(lang_entry.path())? {
                        let entry = entry?;
                        let path = entry.path();
                        if path.is_file()
                            && path.extension().is_some_and(|e| e == "md")
                            && let Some(name) = path.file_name()
                        {
                            templates.push(format!("i18n/{}/{}", lang, name.to_string_lossy()));
                        }
                    }
                }
            }
        }

        templates.sort();
        templates.dedup();
        Ok(templates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_embedded_fallback() {
        let config = OnboardingTemplatesConfig {
            use_embedded_fallback: true,
            sync_enabled: false,
            ..Default::default()
        };

        let temp_dir = tempdir().unwrap();
        let service = OnboardingTemplatesService::new(config, temp_dir.path());

        let templates = service.resolve(None).await.unwrap();
        assert!(templates.onboard.contains("BOOTSTRAP"));
        assert!(templates.personality.contains("Identity"));
        assert!(templates.user.contains("About the User"));
        assert!(templates.agents.contains("Main Chat Assistant"));
    }

    #[tokio::test]
    async fn test_preset_application() {
        let config = OnboardingTemplatesConfig {
            use_embedded_fallback: true,
            sync_enabled: false,
            ..Default::default()
        };

        let temp_dir = tempdir().unwrap();
        let service = OnboardingTemplatesService::new(config, temp_dir.path());

        let overrides = UserTemplateOverrides {
            preset: Some("developer".to_string()),
            ..Default::default()
        };

        let templates = service.resolve(Some(&overrides)).await.unwrap();
        assert!(
            templates
                .unlock_components
                .contains(&"terminal".to_string())
        );
        assert_eq!(templates.user_level, Some("technical".to_string()));
    }

    #[test]
    fn test_list_presets() {
        let config = OnboardingTemplatesConfig::default();
        let temp_dir = tempdir().unwrap();
        let service = OnboardingTemplatesService::new(config, temp_dir.path());

        let presets = service.list_presets();
        assert!(presets.iter().any(|(name, _)| name == "developer"));
        assert!(presets.iter().any(|(name, _)| name == "beginner"));
        assert!(presets.iter().any(|(name, _)| name == "enterprise"));
    }
}

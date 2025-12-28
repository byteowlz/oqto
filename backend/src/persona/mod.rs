//! Persona management module.
//!
//! Personas combine UI metadata with opencode agents. Each persona directory has:
//! - `persona.toml` - UI metadata (name, description, color, avatar, workspace preference)
//! - `.opencode/agent/<name>.md` - opencode agent config (prompt, model, tools, permissions)
//! - `AGENTS.md` - Optional working directory instructions
//!
//! Directory structure:
//! ```text
//! ~/octo/
//! +-- workspace/           # General workspace for non-project chats
//! +-- projects/            # Coding projects
//! |   +-- my-app/
//! +-- personas/            # Persona configurations
//!     +-- developer/
//!     |   +-- persona.toml
//!     |   +-- avatar.png
//!     |   +-- .opencode/agent/developer.md
//!     +-- researcher/
//!         +-- persona.toml
//!         +-- avatar.png
//!         +-- .opencode/agent/researcher.md
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Workspace preference for a persona.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspacePreference {
    /// Use general workspace (~/octo/workspace/)
    #[default]
    General,
    /// Requires a project directory
    Project,
    /// Ask user to choose
    Ask,
}

/// Persona metadata from persona.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Persona {
    /// Unique identifier (directory name).
    #[serde(skip)]
    pub id: String,
    /// Display name of the persona.
    #[serde(default)]
    pub name: String,
    /// Short description of what this persona does.
    #[serde(default)]
    pub description: String,
    /// Accent color for UI (hex color, e.g., "#6366f1").
    #[serde(default)]
    pub color: Option<String>,
    /// Path to avatar image (relative to persona directory).
    #[serde(default)]
    pub avatar: Option<String>,
    /// Whether this is the default persona.
    #[serde(default)]
    pub is_default: bool,
    /// opencode agent ID to use (defaults to persona id).
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Workspace preference (general, project, or ask).
    #[serde(default)]
    pub workspace: WorkspacePreference,
}

impl Persona {
    /// Load persona from a directory containing persona.toml.
    ///
    /// If persona.toml doesn't exist, returns a default persona with
    /// the directory name as the persona name.
    pub fn load(persona_dir: &Path) -> Result<Self> {
        let id = persona_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let toml_path = persona_dir.join("persona.toml");

        let mut persona = if toml_path.exists() {
            let content = std::fs::read_to_string(&toml_path)
                .with_context(|| format!("reading persona.toml from {:?}", toml_path))?;
            toml::from_str(&content)
                .with_context(|| format!("parsing persona.toml from {:?}", toml_path))?
        } else {
            // No persona.toml - create a default persona from directory name
            Persona {
                name: id.clone(),
                description: String::new(),
                ..Default::default()
            }
        };

        // Set the ID from directory name
        persona.id = id;

        // If name is empty, use ID
        if persona.name.is_empty() {
            persona.name = persona.id.clone();
        }

        Ok(persona)
    }

    /// Check if a directory is a persona (has AGENTS.md or persona.toml).
    pub fn is_persona_dir(path: &Path) -> bool {
        path.join("persona.toml").exists() || path.join("AGENTS.md").exists()
    }

    /// Get the effective agent ID (agent_id field or persona id).
    pub fn effective_agent_id(&self) -> &str {
        self.agent_id.as_deref().unwrap_or(&self.id)
    }

    /// List all personas in a directory.
    pub fn list(personas_dir: &Path) -> Result<Vec<Self>> {
        let mut personas = Vec::new();

        if !personas_dir.exists() {
            return Ok(personas);
        }

        let entries = std::fs::read_dir(personas_dir)
            .with_context(|| format!("reading personas directory {:?}", personas_dir))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() && Self::is_persona_dir(&path) {
                match Self::load(&path) {
                    Ok(persona) => personas.push(persona),
                    Err(e) => {
                        tracing::warn!("Failed to load persona from {:?}: {}", path, e);
                    }
                }
            }
        }

        // Sort by name
        personas.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(personas)
    }

    /// Get the persona directory path given the octo home and persona id.
    pub fn path(octo_home: &Path, persona_id: &str) -> PathBuf {
        octo_home.join("personas").join(persona_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_persona_with_toml() {
        let dir = TempDir::new().unwrap();
        let persona_dir = dir.path().join("test-persona");
        std::fs::create_dir(&persona_dir).unwrap();

        let toml_content = r##"
            name = "Test Persona"
            description = "A test persona"
            color = "#ff0000"
            avatar = "avatar.png"
            is_default = true
            agent_id = "custom-agent"
            workspace = "project"
        "##;
        std::fs::write(persona_dir.join("persona.toml"), toml_content).unwrap();

        let persona = Persona::load(&persona_dir).unwrap();
        assert_eq!(persona.id, "test-persona");
        assert_eq!(persona.name, "Test Persona");
        assert_eq!(persona.description, "A test persona");
        assert_eq!(persona.color, Some("#ff0000".to_string()));
        assert_eq!(persona.avatar, Some("avatar.png".to_string()));
        assert!(persona.is_default);
        assert_eq!(persona.agent_id, Some("custom-agent".to_string()));
        assert_eq!(persona.workspace, WorkspacePreference::Project);
        assert_eq!(persona.effective_agent_id(), "custom-agent");
    }

    #[test]
    fn test_load_persona_without_toml() {
        let dir = TempDir::new().unwrap();
        let persona_dir = dir.path().join("my-persona");
        std::fs::create_dir(&persona_dir).unwrap();

        // Create AGENTS.md to make it a persona dir
        std::fs::write(persona_dir.join("AGENTS.md"), "# Instructions").unwrap();

        let persona = Persona::load(&persona_dir).unwrap();
        assert_eq!(persona.id, "my-persona");
        assert_eq!(persona.name, "my-persona");
        assert_eq!(persona.description, "");
        assert_eq!(persona.color, None);
        assert_eq!(persona.workspace, WorkspacePreference::General);
        assert_eq!(persona.effective_agent_id(), "my-persona");
    }

    #[test]
    fn test_is_persona_dir() {
        let dir = TempDir::new().unwrap();

        // Empty dir is not a persona
        assert!(!Persona::is_persona_dir(dir.path()));

        // With AGENTS.md it is
        std::fs::write(dir.path().join("AGENTS.md"), "# Test").unwrap();
        assert!(Persona::is_persona_dir(dir.path()));

        // With persona.toml it is
        let dir2 = TempDir::new().unwrap();
        std::fs::write(dir2.path().join("persona.toml"), "name = \"Test\"").unwrap();
        assert!(Persona::is_persona_dir(dir2.path()));
    }

    #[test]
    fn test_list_personas() {
        let dir = TempDir::new().unwrap();
        let personas_dir = dir.path().join("personas");
        std::fs::create_dir(&personas_dir).unwrap();

        // Create two personas
        let dev_dir = personas_dir.join("developer");
        std::fs::create_dir(&dev_dir).unwrap();
        std::fs::write(
            dev_dir.join("persona.toml"),
            r##"name = "Developer"
description = "Coding assistant"
color = "#3b82f6"
workspace = "project""##,
        )
        .unwrap();

        let researcher_dir = personas_dir.join("researcher");
        std::fs::create_dir(&researcher_dir).unwrap();
        std::fs::write(
            researcher_dir.join("persona.toml"),
            r##"name = "Researcher"
description = "Research assistant"
color = "#8b5cf6"
workspace = "general""##,
        )
        .unwrap();

        // Create a non-persona directory (should be ignored)
        let other_dir = personas_dir.join("not-a-persona");
        std::fs::create_dir(&other_dir).unwrap();

        let personas = Persona::list(&personas_dir).unwrap();
        assert_eq!(personas.len(), 2);
        assert_eq!(personas[0].name, "Developer");
        assert_eq!(personas[1].name, "Researcher");
    }
}

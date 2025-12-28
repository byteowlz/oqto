//! Persona management module.
//!
//! Personas define AI agent behavior and appearance. Each persona has:
//! - A persona.toml file with metadata (name, description, color, avatar)
//! - An AGENTS.md file with instructions for the AI
//! - Optionally, an opencode.json for opencode configuration

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Persona metadata from persona.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Persona {
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
}

impl Persona {
    /// Load persona from a directory containing persona.toml.
    ///
    /// If persona.toml doesn't exist, returns a default persona with
    /// the directory name as the persona name.
    pub fn load(persona_dir: &Path) -> Result<Self> {
        let toml_path = persona_dir.join("persona.toml");

        if toml_path.exists() {
            let content = std::fs::read_to_string(&toml_path)
                .with_context(|| format!("reading persona.toml from {:?}", toml_path))?;
            let persona: Persona = toml::from_str(&content)
                .with_context(|| format!("parsing persona.toml from {:?}", toml_path))?;
            Ok(persona)
        } else {
            // No persona.toml - create a default persona from directory name
            let name = persona_dir
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            Ok(Persona {
                name,
                description: String::new(),
                color: None,
                avatar: None,
                is_default: false,
            })
        }
    }

    /// Check if a directory is a persona (has AGENTS.md or persona.toml).
    pub fn is_persona_dir(path: &Path) -> bool {
        path.join("persona.toml").exists() || path.join("AGENTS.md").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_persona_with_toml() {
        let dir = TempDir::new().unwrap();
        let toml_content = r##"
            name = "Test Persona"
            description = "A test persona"
            color = "#ff0000"
            avatar = "avatar.png"
            is_default = true
        "##;
        std::fs::write(dir.path().join("persona.toml"), toml_content).unwrap();

        let persona = Persona::load(dir.path()).unwrap();
        assert_eq!(persona.name, "Test Persona");
        assert_eq!(persona.description, "A test persona");
        assert_eq!(persona.color, Some("#ff0000".to_string()));
        assert_eq!(persona.avatar, Some("avatar.png".to_string()));
        assert!(persona.is_default);
    }

    #[test]
    fn test_load_persona_without_toml() {
        let dir = TempDir::new().unwrap();
        // Create AGENTS.md to make it a persona dir
        std::fs::write(dir.path().join("AGENTS.md"), "# Instructions").unwrap();

        let persona = Persona::load(dir.path()).unwrap();
        // Name should be derived from directory name
        assert!(!persona.name.is_empty());
        assert_eq!(persona.description, "");
        assert_eq!(persona.color, None);
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
}

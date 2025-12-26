//! Persona management module.
//!
//! Personas combine UI metadata with opencode agents. Each persona directory has:
//! - `persona.toml` - UI metadata + agent/workspace settings
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

/// Workspace mode for a persona.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    /// Use the persona's default_workdir if set, otherwise use the workspace root.
    #[default]
    DefaultOnly,
    /// Ask the user to choose a directory.
    Ask,
    /// Allow any directory selection (default or user choice).
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersonaToml {
    #[serde(default)]
    metadata: PersonaMetadataToml,
    #[serde(default)]
    persona: PersonaSettingsToml,
    #[serde(default)]
    agent: PersonaAgentToml,
    #[serde(default)]
    workspace: PersonaWorkspaceToml,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersonaSettingsToml {
    /// If true, this persona has its own directory with opencode.json.
    /// If false, it's a wrapper around an existing opencode agent.
    #[serde(default = "default_true")]
    standalone: bool,
    /// If true, this persona can work on external projects.
    /// If false, it only works within its own persona directory.
    #[serde(default = "default_true")]
    project_access: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersonaMetadataToml {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    avatar: Option<String>,
    #[serde(default)]
    is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersonaAgentToml {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersonaWorkspaceToml {
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    mode: WorkspaceMode,
}

impl PersonaAgentToml {
    fn has_content(&self) -> bool {
        self.id.as_ref().is_some_and(|id| !id.trim().is_empty())
            || self.mode.as_ref().is_some_and(|mode| !mode.trim().is_empty())
            || self.model.as_ref().is_some_and(|model| !model.trim().is_empty())
            || self
                .prompt
                .as_ref()
                .is_some_and(|prompt| !prompt.trim().is_empty())
            || !self.tools.is_empty()
            || !self.permissions.is_empty()
    }
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
    pub agent_id: String,
    /// Default working directory (optional, relative or absolute).
    #[serde(default)]
    pub default_workdir: Option<String>,
    /// Workspace mode (default_only, ask, or any).
    #[serde(default)]
    pub workspace_mode: WorkspaceMode,
    /// If true, this persona has its own directory with opencode.json.
    /// If false, it's a wrapper around an existing opencode agent.
    #[serde(default = "default_true")]
    pub standalone: bool,
    /// If true, this persona can work on external projects.
    /// If false, it only works within its own persona directory.
    #[serde(default = "default_true")]
    pub project_access: bool,
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
            let parsed: PersonaToml = toml::from_str(&content)
                .with_context(|| format!("parsing persona.toml from {:?}", toml_path))?;

            let agent_id = parsed
                .agent
                .id
                .clone()
                .unwrap_or_else(|| id.clone());

            if parsed.agent.has_content() {
                if let Err(err) =
                    write_agent_file(persona_dir, &agent_id, &parsed.metadata, &parsed.agent)
                {
                    tracing::warn!(
                        "Failed to generate opencode agent file for {:?}: {}",
                        persona_dir,
                        err
                    );
                }
            }

            Persona {
                id: id.clone(),
                name: parsed.metadata.name,
                description: parsed.metadata.description,
                color: parsed.metadata.color,
                avatar: parsed.metadata.avatar,
                is_default: parsed.metadata.is_default,
                agent_id,
                default_workdir: parsed.workspace.default,
                workspace_mode: parsed.workspace.mode,
                standalone: parsed.persona.standalone,
                project_access: parsed.persona.project_access,
            }
        } else {
            // No persona.toml - create a default persona from directory name
            Persona {
                id: id.clone(),
                name: id.clone(),
                description: String::new(),
                standalone: true,
                project_access: true,
                ..Default::default()
            }
        };

        // If name is empty, use ID
        if persona.name.is_empty() {
            persona.name = persona.id.clone();
        }

        // If agent_id is empty, use ID
        if persona.agent_id.is_empty() {
            persona.agent_id = persona.id.clone();
        }

        Ok(persona)
    }

    /// Check if a directory is a persona (has AGENTS.md or persona.toml).
    pub fn is_persona_dir(path: &Path) -> bool {
        path.join("persona.toml").exists() || path.join("AGENTS.md").exists()
    }

    /// Get the effective agent ID (agent_id field or persona id).
    pub fn effective_agent_id(&self) -> &str {
        if self.agent_id.is_empty() {
            &self.id
        } else {
            &self.agent_id
        }
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

fn write_agent_file(
    persona_dir: &Path,
    agent_id: &str,
    metadata: &PersonaMetadataToml,
    agent: &PersonaAgentToml,
) -> Result<()> {
    let agent_dir = persona_dir.join(".opencode").join("agent");
    std::fs::create_dir_all(&agent_dir)
        .with_context(|| format!("creating agent directory {:?}", agent_dir))?;

    // Build description from persona name and description
    let description = if metadata.description.is_empty() {
        metadata.name.clone()
    } else {
        format!("{} - {}", metadata.name, metadata.description)
    };
    let description = description.replace('"', "\\\"");
    
    let mode = agent.mode.as_deref().unwrap_or("primary");
    let model = agent.model.as_deref().unwrap_or("");
    
    // Tools format: object with tool_name: true/false
    // Input format: ["bash", "write", "edit"] -> tools:\n  bash: true\n  write: true\n  edit: true
    let tools = if agent.tools.is_empty() {
        String::new()
    } else {
        format!(
            "tools:\n{}",
            agent
                .tools
                .iter()
                .map(|tool| format!("  {}: true", tool))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    
    // Permissions format: object with tool_name: allow/ask/deny
    // Input format: ["edit: allow", "bash: ask"] -> permission:\n  edit: allow\n  bash: ask
    let permissions = if agent.permissions.is_empty() {
        String::new()
    } else {
        format!(
            "permission:\n{}",
            agent
                .permissions
                .iter()
                .map(|permission| format!("  {}", permission))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    
    let prompt = agent.prompt.as_deref().unwrap_or("").trim();

    // Build YAML frontmatter
    let mut frontmatter = format!("description: \"{}\"\nmode: {}", description, mode);
    if !model.is_empty() {
        frontmatter.push_str(&format!("\nmodel: {}", model));
    }
    if !tools.is_empty() {
        frontmatter.push_str(&format!("\n{}", tools));
    }
    if !permissions.is_empty() {
        frontmatter.push_str(&format!("\n{}", permissions));
    }

    let content = format!("---\n{}\n---\n\n{}\n", frontmatter, prompt);

    let agent_path = agent_dir.join(format!("{}.md", agent_id));
    std::fs::write(&agent_path, content)
        .with_context(|| format!("writing opencode agent file {:?}", agent_path))?;
    Ok(())
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
            [metadata]
            name = "Test Persona"
            description = "A test persona"
            color = "#ff0000"
            avatar = "avatar.png"
            is_default = true

            [agent]
            id = "custom-agent"
            mode = "primary"
            model = "openai/gpt-4o"
            prompt = "Be helpful."
            tools = ["bash", "browser"]
            permissions = ["filesystem"]

            [workspace]
            default = "projects/acme"
            mode = "any"
        "##;
        std::fs::write(persona_dir.join("persona.toml"), toml_content).unwrap();

        let persona = Persona::load(&persona_dir).unwrap();
        assert_eq!(persona.id, "test-persona");
        assert_eq!(persona.name, "Test Persona");
        assert_eq!(persona.description, "A test persona");
        assert_eq!(persona.color, Some("#ff0000".to_string()));
        assert_eq!(persona.avatar, Some("avatar.png".to_string()));
        assert!(persona.is_default);
        assert_eq!(persona.agent_id, "custom-agent");
        assert_eq!(persona.default_workdir, Some("projects/acme".to_string()));
        assert_eq!(persona.workspace_mode, WorkspaceMode::Any);
        assert_eq!(persona.effective_agent_id(), "custom-agent");

        let agent_path = persona_dir.join(".opencode/agent/custom-agent.md");
        let agent_contents = std::fs::read_to_string(agent_path).unwrap();
        assert!(agent_contents.contains("description: \"Test Persona - A test persona\""));
        assert!(agent_contents.contains("bash: true"));
        assert!(agent_contents.contains("Be helpful."));
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
        assert_eq!(persona.workspace_mode, WorkspaceMode::DefaultOnly);
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
            r##"
[metadata]
name = "Developer"
description = "Coding assistant"
color = "#3b82f6"

[workspace]
mode = "ask"
"##,
        )
        .unwrap();

        let researcher_dir = personas_dir.join("researcher");
        std::fs::create_dir(&researcher_dir).unwrap();
        std::fs::write(
            researcher_dir.join("persona.toml"),
            r##"
[metadata]
name = "Researcher"
description = "Research assistant"
color = "#8b5cf6"

[workspace]
mode = "default_only"
"##,
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

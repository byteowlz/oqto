//! Template discovery and management.

use std::path::Path;

use anyhow::{Context, Result};
use walkdir::WalkDir;

/// Discover templates from a directory.
pub fn discover_templates(path: &Path) -> Result<Vec<TemplateInfo>> {
    let mut templates = Vec::new();

    if !path.exists() {
        return Ok(templates);
    }

    for entry in WalkDir::new(path).min_depth(1).max_depth(1) {
        let entry = entry.context("reading template directory")?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Skip hidden directories
        if name.starts_with('.') {
            continue;
        }

        // Try to read description from template.json or README
        let description = read_template_description(path);

        templates.push(TemplateInfo {
            name,
            path: path.to_path_buf(),
            description,
        });
    }

    templates.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(templates)
}

/// Read template description from template.json or README.md
fn read_template_description(path: &Path) -> Option<String> {
    // Try template.json first
    let template_json = path.join("template.json");
    if template_json.exists()
        && let Ok(content) = std::fs::read_to_string(&template_json)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(desc) = json.get("description").and_then(|v| v.as_str())
    {
        return Some(desc.to_string());
    }

    // Try README.md - take first non-empty, non-heading line
    let readme = path.join("README.md");
    if readme.exists()
        && let Ok(content) = std::fs::read_to_string(&readme)
    {
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

/// Copy template to destination, skipping .git directory.
pub fn copy_template(src: &Path, dest: &Path) -> Result<()> {
    if !src.exists() {
        anyhow::bail!("Template not found: {}", src.display());
    }

    std::fs::create_dir_all(dest).context("creating destination directory")?;

    for entry in WalkDir::new(src) {
        let entry = entry.context("reading template file")?;
        let src_path = entry.path();

        // Get relative path
        let rel_path = src_path
            .strip_prefix(src)
            .context("getting relative path")?;

        // Skip .git directory
        if rel_path.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }

        // Skip template.json (it's metadata, not project content)
        if rel_path.to_string_lossy() == "template.json" {
            continue;
        }

        let dest_path = dest.join(rel_path);

        if src_path.is_dir() {
            std::fs::create_dir_all(&dest_path)
                .with_context(|| format!("creating directory: {}", dest_path.display()))?;
        } else {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(src_path, &dest_path)
                .with_context(|| format!("copying file: {}", dest_path.display()))?;
        }
    }

    Ok(())
}

/// Template information.
#[derive(Debug, Clone)]
pub struct TemplateInfo {
    pub name: String,
    #[allow(dead_code)]
    pub path: std::path::PathBuf,
    pub description: Option<String>,
}

/// List available templates.
pub fn list_templates(
    path: Option<&Path>,
    scaffold_config: &crate::tiers::ScaffoldConfig,
) -> Result<()> {
    // CLI path takes precedence, then config
    let templates_path = path
        .map(|p| p.to_path_buf())
        .or_else(|| scaffold_config.templates_path());

    match templates_path {
        Some(path) => {
            let templates = discover_templates(&path)?;

            if templates.is_empty() {
                println!("No templates found in: {}", path.display());
                println!(
                    "\nTo add templates, create directories in: {}",
                    path.display()
                );
            } else {
                println!("Available templates in {}:\n", path.display());
                for template in templates {
                    print!("  {}", template.name);
                    if let Some(desc) = &template.description {
                        print!(" - {}", desc);
                    }
                    println!();
                }
            }
        }
        None => {
            println!("No templates directory configured.");
            println!("\nSet templates path via:");
            println!("  --path <directory>");
            println!("  OCTO_TEMPLATES_PATH environment variable");
            println!("  templates_path in ~/.config/octo/scaffold.toml");
        }
    }

    Ok(())
}

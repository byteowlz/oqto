//! Project/workspace handlers.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Context;
use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::instrument;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::projects::{self, ProjectMetadata};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::{AppState, TemplatesRepoType};

/// Query for listing workspace directories.
#[derive(Debug, Deserialize)]
pub struct WorkspaceDirQuery {
    pub path: Option<String>,
}

/// Workspace directory entry.
#[derive(Debug, Serialize)]
pub struct WorkspaceDirEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    /// Relative path to project logo (if found in logo/ directory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<ProjectLogo>,
}

/// Project logo information.
#[derive(Debug, Serialize)]
pub struct ProjectLogo {
    /// Path relative to project root (e.g., "logo/project_logo_white.svg")
    pub path: String,
    /// Logo variant (e.g., "white", "black", "white_on_black")
    pub variant: String,
}

/// Project template entry.
#[derive(Debug, Serialize)]
pub struct ProjectTemplateEntry {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Response for listing project templates.
#[derive(Debug, Serialize)]
pub struct ListProjectTemplatesResponse {
    /// Whether templates are configured (repo_path is set).
    pub configured: bool,
    /// List of available templates.
    pub templates: Vec<ProjectTemplateEntry>,
}

/// Request to create a project from a template.
#[derive(Debug, Deserialize)]
pub struct CreateProjectFromTemplateRequest {
    pub template_path: String,
    pub project_path: String,
    #[serde(default)]
    pub shared: bool,
}

/// Find the best logo file for a project directory.
/// Prefers SVG over PNG, and "white" variants for dark UI.
fn find_project_logo(project_path: &std::path::Path, project_name: &str) -> Option<ProjectLogo> {
    let logo_dir = project_path.join("logo");
    if !logo_dir.is_dir() {
        return None;
    }

    let entries = std::fs::read_dir(&logo_dir).ok()?;

    // Collect all logo files
    let mut logos: Vec<(String, String, bool)> = Vec::new(); // (filename, variant, is_svg)

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "svg" && ext != "png" {
            continue;
        }

        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_svg = ext == "svg";

        // Extract variant from filename pattern: {project}_logo_{variant}.{ext}
        // or just {variant}.{ext} for simpler naming
        let variant = if let Some(rest) = filename.strip_prefix(&format!("{}_logo_", project_name))
        {
            rest.strip_suffix(&format!(".{}", ext))
                .unwrap_or(rest)
                .to_string()
        } else if let Some(rest) = filename.strip_prefix("logo_") {
            rest.strip_suffix(&format!(".{}", ext))
                .unwrap_or(rest)
                .to_string()
        } else {
            // Fallback: use filename without extension as variant
            filename
                .strip_suffix(&format!(".{}", ext))
                .unwrap_or(filename)
                .to_string()
        };

        logos.push((filename.to_string(), variant, is_svg));
    }

    if logos.is_empty() {
        return None;
    }

    // Priority order for dark UI: white variants first, then SVG over PNG
    let variant_priority = |variant: &str| -> i32 {
        match variant {
            "white" => 0,
            "white_on_black" => 1,
            v if v.contains("white") && !v.contains("black_on_white") => 2,
            "black_on_white" => 3,
            "black" => 4,
            _ => 5,
        }
    };

    logos.sort_by(|a, b| {
        let prio_a = variant_priority(&a.1);
        let prio_b = variant_priority(&b.1);
        if prio_a != prio_b {
            return prio_a.cmp(&prio_b);
        }
        // Prefer SVG over PNG
        b.2.cmp(&a.2)
    });

    let (filename, variant, _) = &logos[0];
    Some(ProjectLogo {
        path: format!("logo/{}", filename),
        variant: variant.clone(),
    })
}

fn sanitize_relative_path(raw: &str) -> Result<PathBuf, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("path is required"));
    }
    let normalized = trimmed.replace('\\', "/");
    if std::path::Path::new(&normalized).is_absolute() {
        return Err(ApiError::bad_request("invalid path"));
    }
    let normalized = normalized.trim_matches('/');
    let rel_path = PathBuf::from(normalized);
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ApiError::bad_request("invalid path"));
    }
    Ok(rel_path)
}

fn read_template_description(template_dir: &std::path::Path) -> Option<String> {
    let metadata_path = template_dir.join("template.json");
    let contents = fs::read_to_string(metadata_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    value
        .get("description")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

fn copy_template_dir(src: &std::path::Path, dest: &std::path::Path) -> Result<(), ApiError> {
    fs::create_dir_all(dest)
        .map_err(|e| ApiError::internal(format!("Failed to create project dir: {}", e)))?;
    for entry in fs::read_dir(src)
        .map_err(|e| ApiError::internal(format!("Failed to read template dir: {}", e)))?
    {
        let entry = entry
            .map_err(|e| ApiError::internal(format!("Failed to read template entry: {}", e)))?;
        let file_type = entry.file_type().map_err(|e| {
            ApiError::internal(format!("Failed to read template entry type: {}", e))
        })?;
        let file_name = entry.file_name();
        if file_name.to_string_lossy() == ".git" {
            continue;
        }
        let src_path = entry.path();
        let dest_path = dest.join(&file_name);
        if file_type.is_dir() {
            copy_template_dir(&src_path, &dest_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dest_path)
                .map_err(|e| ApiError::internal(format!("Failed to copy template file: {}", e)))?;
        }
    }
    Ok(())
}

async fn maybe_sync_templates_repo(state: &AppState) -> Result<(), ApiError> {
    let repo_path = match state.templates.repo_path.as_ref() {
        Some(path) => path.clone(),
        None => return Ok(()),
    };
    if state.templates.repo_type == TemplatesRepoType::Local {
        return Ok(());
    }
    if !state.templates.sync_on_list {
        return Ok(());
    }
    let should_sync = {
        let last_sync = state.templates.last_sync.lock().await;
        match *last_sync {
            Some(instant) if instant.elapsed() < state.templates.sync_interval => false,
            _ => true,
        }
    };
    if !should_sync {
        return Ok(());
    }
    if !repo_path.join(".git").exists() {
        return Err(ApiError::internal("templates repo is not a git repository"));
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .arg("pull")
        .arg("--ff-only")
        .output()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to run git pull: {}", e)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::internal(format!(
            "Failed to sync templates repo: {}",
            stderr.trim()
        )));
    }
    let mut last_sync = state.templates.last_sync.lock().await;
    *last_sync = Some(Instant::now());
    Ok(())
}

/// List directories under the workspace root (projects view).
#[instrument(skip(state))]
pub async fn list_workspace_dirs(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceDirQuery>,
) -> ApiResult<Json<Vec<WorkspaceDirEntry>>> {
    let root = state.sessions.for_user(user.id()).workspace_root();
    let relative = query.path.unwrap_or_else(|| ".".to_string());
    let rel_path = std::path::PathBuf::from(&relative);

    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ApiError::bad_request("invalid path"));
    }

    let target = root.join(&rel_path);
    let entries = std::fs::read_dir(&target)
        .with_context(|| format!("reading workspace directory {:?}", target))
        .map_err(|e| ApiError::internal(format!("Failed to list workspace directories: {}", e)))?;

    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|e| ApiError::internal(format!("Failed to read directory entry: {}", e)))?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_str().unwrap_or_default().to_string();
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let logo = find_project_logo(&path, &name);
            dirs.push(WorkspaceDirEntry {
                name,
                path: if rel.is_empty() { ".".to_string() } else { rel },
                entry_type: "directory".to_string(),
                logo,
            });
        }
    }

    dirs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(dirs))
}

/// List available project templates from the templates repository.
#[instrument(skip(state))]
pub async fn list_project_templates(
    State(state): State<AppState>,
) -> ApiResult<Json<ListProjectTemplatesResponse>> {
    let repo_path = match state.templates.repo_path.as_ref() {
        Some(path) => path.clone(),
        None => {
            return Ok(Json(ListProjectTemplatesResponse {
                configured: false,
                templates: Vec::new(),
            }));
        }
    };

    maybe_sync_templates_repo(&state).await?;

    let entries = fs::read_dir(&repo_path)
        .with_context(|| format!("reading templates directory {:?}", repo_path))
        .map_err(|e| ApiError::internal(format!("Failed to list templates: {}", e)))?;

    let mut templates = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|e| ApiError::internal(format!("Failed to read template: {}", e)))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let rel = path
            .strip_prefix(&repo_path)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let description = read_template_description(&path);
        templates.push(ProjectTemplateEntry {
            name,
            path: rel,
            description,
        });
    }
    templates.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(ListProjectTemplatesResponse {
        configured: true,
        templates,
    }))
}

/// Create a new project from a template.
#[instrument(skip(state, request))]
pub async fn create_project_from_template(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<CreateProjectFromTemplateRequest>,
) -> ApiResult<Json<WorkspaceDirEntry>> {
    let repo_path = state
        .templates
        .repo_path
        .clone()
        .ok_or_else(|| ApiError::bad_request("templates repo not configured"))?;

    maybe_sync_templates_repo(&state).await?;

    let template_rel = sanitize_relative_path(&request.template_path)?;
    let template_dir = repo_path.join(&template_rel);
    if !template_dir.is_dir() {
        return Err(ApiError::bad_request("template not found"));
    }

    let project_rel = sanitize_relative_path(&request.project_path)?;
    let is_current_dir = project_rel
        .components()
        .all(|c| matches!(c, std::path::Component::CurDir));
    if is_current_dir {
        return Err(ApiError::bad_request("project path is required"));
    }

    let workspace_root = state.sessions.for_user(user.id()).workspace_root();
    let target_dir = workspace_root.join(&project_rel);
    if target_dir.exists() {
        return Err(ApiError::bad_request("project path already exists"));
    }

    copy_template_dir(&template_dir, &target_dir)?;

    let status = Command::new("git")
        .arg("init")
        .arg("--branch")
        .arg("main")
        .current_dir(&target_dir)
        .status()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to init git repo: {}", e)))?;
    if !status.success() {
        return Err(ApiError::internal("git init failed"));
    }

    if request.shared {
        let metadata = ProjectMetadata {
            project_id: format!("proj_{}", Uuid::new_v4().simple()),
            shared: true,
            template_path: Some(template_rel.to_string_lossy().to_string()),
        };
        projects::write_metadata(&target_dir, &metadata)
            .context("writing project metadata")
            .map_err(|e| ApiError::internal(format!("Failed to write project metadata: {}", e)))?;
    }

    let name = project_rel
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();
    let rel_path = project_rel.to_string_lossy().to_string();
    let logo = find_project_logo(&target_dir, &name);
    Ok(Json(WorkspaceDirEntry {
        name,
        path: if rel_path.is_empty() {
            ".".to_string()
        } else {
            rel_path
        },
        entry_type: "directory".to_string(),
        logo,
    }))
}

/// Serve a project logo file.
/// Path format: {project_path}/logo/{filename}
#[instrument(skip(state))]
pub async fn get_project_logo(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    use axum::http::header;

    let root = state.sessions.for_user(user.id()).workspace_root();
    let file_path = std::path::PathBuf::from(&path);

    // Security: prevent path traversal
    if file_path.is_absolute()
        || file_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ApiError::bad_request("invalid path"));
    }

    // Must be in a logo/ subdirectory
    let components: Vec<_> = file_path.components().collect();
    if components.len() < 3 {
        return Err(ApiError::bad_request("invalid logo path"));
    }

    // Check that the path contains "logo" as a directory component
    let has_logo_dir = components
        .iter()
        .any(|c| matches!(c, std::path::Component::Normal(s) if s.to_str() == Some("logo")));
    if !has_logo_dir {
        return Err(ApiError::bad_request("path must be in logo/ directory"));
    }

    let full_path = root.join(&file_path);

    // Check file exists and is a file
    if !full_path.is_file() {
        return Err(ApiError::not_found("logo not found"));
    }

    // Determine content type from extension
    let content_type = match full_path.extension().and_then(|e| e.to_str()) {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };

    // Read file contents
    let contents = tokio::fs::read(&full_path)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read logo file: {}", e)))?;

    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=86400"), // Cache for 1 day
        ],
        contents,
    ))
}

#[cfg(test)]
pub mod tests {
    use super::{copy_template_dir, sanitize_relative_path};
    use std::fs;

    #[test]
    fn sanitize_relative_path_rejects_invalid() {
        assert!(sanitize_relative_path("../foo").is_err());
        assert!(sanitize_relative_path("/absolute").is_err());
    }

    #[test]
    fn sanitize_relative_path_accepts_nested() {
        let path = sanitize_relative_path("projects/demo").unwrap();
        assert_eq!(path.to_string_lossy(), "projects/demo");
    }

    #[test]
    fn copy_template_dir_skips_git_dir() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("template");
        let dest = temp.path().join("project");
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::write(src.join("README.md"), "hello").unwrap();
        fs::write(src.join(".git").join("HEAD"), "ref").unwrap();

        copy_template_dir(&src, &dest).unwrap();

        assert!(dest.join("README.md").exists());
        assert!(!dest.join(".git").exists());
    }
}

//! Project/workspace handlers.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::Command;
use tracing::instrument;
use uuid::Uuid;

use crate::api::handlers::trx::validate_workspace_path;
use crate::auth::CurrentUser;
use crate::local::{SandboxConfigFile, SandboxProfile};
use crate::projects::{self, ProjectMetadata};
use crate::session::WorkspaceLocationInput;
use crate::settings::{ConfigUpdate, SettingsScope};
use crate::workspace::meta::{WorkspaceMeta, load_workspace_meta, write_workspace_meta};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<ProjectTemplateDefaults>,
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

/// Query for listing workspace locations.
#[derive(Debug, Deserialize)]
pub struct WorkspaceLocationQuery {
    pub workspace_id: String,
}

/// Request to upsert a workspace location.
#[derive(Debug, Deserialize)]
pub struct UpsertWorkspaceLocationRequest {
    pub workspace_id: String,
    pub location_id: String,
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub runner_id: Option<String>,
    #[serde(default)]
    pub repo_fingerprint: Option<String>,
    #[serde(default)]
    pub set_active: Option<bool>,
}

/// Request to set active workspace location.
#[derive(Debug, Deserialize)]
pub struct SetActiveWorkspaceLocationRequest {
    pub workspace_id: String,
    pub location_id: String,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceLocationSummary {
    pub id: String,
    pub workspace_id: String,
    pub location_id: String,
    pub kind: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_fingerprint: Option<String>,
    pub is_active: bool,
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

fn read_template_defaults(template_dir: &std::path::Path) -> Option<ProjectTemplateDefaults> {
    let mut defaults = ProjectTemplateDefaults {
        display_name: None,
        sandbox_profile: None,
        default_provider: None,
        default_model: None,
        skills_mode: None,
        extensions_mode: None,
        skills: Vec::new(),
        extensions: Vec::new(),
    };
    let mut has_any = false;

    let workspace_meta_path = template_dir.join(".octo").join("workspace.toml");
    if let Ok(contents) = fs::read_to_string(workspace_meta_path) {
        if let Ok(meta) = toml::from_str::<WorkspaceMeta>(&contents) {
            if let Some(name) = meta.display_name {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() {
                    defaults.display_name = Some(trimmed);
                    has_any = true;
                }
            }
        }
    }

    let sandbox_path = template_dir.join(".octo").join("sandbox.toml");
    if let Ok(contents) = fs::read_to_string(sandbox_path) {
        if let Ok(file) = toml::from_str::<SandboxConfigFile>(&contents) {
            if !file.profile.trim().is_empty() {
                defaults.sandbox_profile = Some(file.profile.trim().to_string());
                has_any = true;
            }
        }
    }

    let pi_settings_path = template_dir.join(".pi").join("settings.json");
    if let Ok(contents) = fs::read_to_string(pi_settings_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&contents) {
            defaults.default_provider = value
                .get("defaultProvider")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            defaults.default_model = value
                .get("defaultModel")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());

            if defaults.default_provider.is_some() || defaults.default_model.is_some() {
                has_any = true;
            }

            let skills_paths = read_settings_paths(&value, "skills");
            let extensions_paths = read_settings_paths(&value, "extensions");
            let global_skills_dir = expand_path(GLOBAL_PI_SKILLS_DIR).ok();
            let global_extensions_dir = expand_path(GLOBAL_PI_EXTENSIONS_DIR).ok();

            if global_skills_dir
                .as_ref()
                .map(|global| paths_contain(&skills_paths, global))
                .unwrap_or(false)
            {
                defaults.skills_mode = Some("all".to_string());
                has_any = true;
            } else {
                let template_skills_dir = template_dir.join(".pi").join("skills");
                if template_skills_dir.exists()
                    || skills_paths
                        .iter()
                        .any(|path| path.to_string_lossy().ends_with(".pi/skills"))
                {
                    defaults.skills_mode = Some("custom".to_string());
                    defaults.skills =
                        list_dir_entries(&template_skills_dir, true).unwrap_or_default();
                    has_any = true;
                }
            }

            if global_extensions_dir
                .as_ref()
                .map(|global| paths_contain(&extensions_paths, global))
                .unwrap_or(false)
            {
                defaults.extensions_mode = Some("all".to_string());
                has_any = true;
            } else {
                let template_extensions_dir = template_dir.join(".pi").join("extensions");
                if template_extensions_dir.exists()
                    || extensions_paths
                        .iter()
                        .any(|path| path.to_string_lossy().ends_with(".pi/extensions"))
                {
                    defaults.extensions_mode = Some("custom".to_string());
                    defaults.extensions =
                        list_dir_entries(&template_extensions_dir, false).unwrap_or_default();
                    has_any = true;
                }
            }
        }
    }

    if has_any {
        Some(defaults)
    } else {
        None
    }
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

    // Auto-create workspace root for the user if it doesn't exist yet.
    if !target.exists() {
        if let Err(e) = std::fs::create_dir_all(&target) {
            tracing::warn!("Could not create workspace directory {:?}: {}", target, e);
            // Return empty list instead of 500 when the directory can't be created.
            return Ok(Json(Vec::new()));
        }
    }

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

    // Template sync is handled by a background task; no blocking sync here.

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
        let defaults = read_template_defaults(&path);
        templates.push(ProjectTemplateEntry {
            name,
            path: rel,
            description,
            defaults,
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

    // Template sync is handled by a background task; no blocking sync here.

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

    // Auto-create workspace root for the user if it doesn't exist yet.
    if !workspace_root.exists() {
        fs::create_dir_all(&workspace_root).map_err(|e| {
            ApiError::internal(format!(
                "Failed to create workspace directory {:?}: {}",
                workspace_root, e
            ))
        })?;
    }

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

// Workspace overview + Pi resource management.

const GLOBAL_PI_SKILLS_DIR: &str = "~/.pi/agent/skills";
const GLOBAL_PI_EXTENSIONS_DIR: &str = "~/.pi/agent/extensions";

#[derive(Debug, Deserialize)]
pub struct WorkspaceMetaQuery {
    pub workspace_path: String,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceMetaResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceMetaUpdateRequest {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceSandboxResponse {
    pub enabled: bool,
    pub profile: String,
    pub profiles: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceSandboxUpdateRequest {
    pub profile: String,
}

#[derive(Debug, Serialize)]
pub struct PiResourceEntry {
    pub name: String,
    pub selected: bool,
}

#[derive(Debug, Serialize)]
pub struct WorkspacePiResourcesResponse {
    pub skills_mode: String,
    pub extensions_mode: String,
    pub skills: Vec<PiResourceEntry>,
    pub extensions: Vec<PiResourceEntry>,
    pub global_skills_dir: String,
    pub global_extensions_dir: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspacePiResourcesUpdateRequest {
    pub workspace_path: String,
    pub skills_mode: String,
    pub extensions_mode: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectTemplateDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

#[instrument(skip(state, user, query))]
pub async fn get_workspace_meta(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceMetaQuery>,
) -> ApiResult<Json<WorkspaceMetaResponse>> {
    let workspace_root = validate_workspace_path(&state, user.id(), &query.workspace_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?;

    let display_name = load_workspace_meta(&workspace_root)
        .and_then(|meta| meta.display_name)
        .and_then(|name| {
            let trimmed = name.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

    Ok(Json(WorkspaceMetaResponse { display_name }))
}

#[instrument(skip(state, user, query, request))]
pub async fn update_workspace_meta(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceMetaQuery>,
    Json(request): Json<WorkspaceMetaUpdateRequest>,
) -> ApiResult<Json<WorkspaceMetaResponse>> {
    let workspace_root = validate_workspace_path(&state, user.id(), &query.workspace_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?;

    let mut meta = load_workspace_meta(&workspace_root).unwrap_or_default();
    meta.display_name = request
        .display_name
        .and_then(|name| {
            let trimmed = name.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

    write_workspace_meta(&workspace_root, &meta)
        .map_err(|e| ApiError::internal(format!("Failed to write workspace metadata: {}", e)))?;

    Ok(Json(WorkspaceMetaResponse {
        display_name: meta.display_name,
    }))
}

#[instrument(skip(state, user, query))]
pub async fn get_workspace_sandbox(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceMetaQuery>,
) -> ApiResult<Json<WorkspaceSandboxResponse>> {
    let workspace_root = validate_workspace_path(&state, user.id(), &query.workspace_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?;

    let sandbox_path = workspace_root.join(".octo").join("sandbox.toml");
    let file = if sandbox_path.exists() {
        let contents = std::fs::read_to_string(&sandbox_path).map_err(|e| {
            ApiError::internal(format!("Failed to read sandbox config: {}", e))
        })?;
        toml::from_str::<SandboxConfigFile>(&contents).map_err(|e| {
            ApiError::internal(format!("Failed to parse sandbox config: {}", e))
        })?
    } else {
        SandboxConfigFile::default()
    };

    let profile_name = if file.profile.is_empty() {
        "development".to_string()
    } else {
        file.profile.clone()
    };

    let mut profiles = HashSet::new();
    profiles.extend(["minimal", "development", "strict"].map(String::from));
    profiles.extend(file.profiles.keys().cloned());

    Ok(Json(WorkspaceSandboxResponse {
        enabled: file.enabled,
        profile: profile_name,
        profiles: profiles.into_iter().collect(),
    }))
}

#[instrument(skip(state, user, query, request))]
pub async fn update_workspace_sandbox(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceMetaQuery>,
    Json(request): Json<WorkspaceSandboxUpdateRequest>,
) -> ApiResult<Json<WorkspaceSandboxResponse>> {
    let workspace_root = validate_workspace_path(&state, user.id(), &query.workspace_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?;

    let sandbox_path = workspace_root.join(".octo").join("sandbox.toml");
    let mut file = if sandbox_path.exists() {
        let contents = std::fs::read_to_string(&sandbox_path).map_err(|e| {
            ApiError::internal(format!("Failed to read sandbox config: {}", e))
        })?;
        toml::from_str::<SandboxConfigFile>(&contents).map_err(|e| {
            ApiError::internal(format!("Failed to parse sandbox config: {}", e))
        })?
    } else {
        SandboxConfigFile::default()
    };

    let profile = request.profile.trim().to_string();
    if profile.is_empty() {
        return Err(ApiError::bad_request("Profile cannot be empty"));
    }

    file.profile = profile.clone();
    if let Some(parent) = sandbox_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ApiError::internal(format!("Failed to create sandbox config dir: {}", e))
        })?;
    }

    let body = toml::to_string_pretty(&file)
        .map_err(|e| ApiError::internal(format!("Failed to serialize sandbox config: {}", e)))?;
    std::fs::write(&sandbox_path, body)
        .map_err(|e| ApiError::internal(format!("Failed to write sandbox config: {}", e)))?;

    let mut profiles = HashSet::new();
    profiles.extend(["minimal", "development", "strict"].map(String::from));
    profiles.extend(file.profiles.keys().cloned());

    Ok(Json(WorkspaceSandboxResponse {
        enabled: file.enabled,
        profile,
        profiles: profiles.into_iter().collect(),
    }))
}

#[instrument(skip(state, user, query))]
pub async fn get_workspace_pi_resources(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceMetaQuery>,
) -> ApiResult<Json<WorkspacePiResourcesResponse>> {
    let workspace_root = validate_workspace_path(&state, user.id(), &query.workspace_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?;

    let global_skills_dir = expand_path(GLOBAL_PI_SKILLS_DIR)?;
    let global_extensions_dir = expand_path(GLOBAL_PI_EXTENSIONS_DIR)?;

    let settings_value = read_pi_settings_value(&workspace_root.join(".pi"));
    let skills_paths = settings_value
        .as_ref()
        .map(|value| read_settings_paths(value, "skills"))
        .unwrap_or_default();
    let extensions_paths = settings_value
        .as_ref()
        .map(|value| read_settings_paths(value, "extensions"))
        .unwrap_or_default();

    let workspace_skills_dir = workspace_root.join(".pi").join("skills");
    let workspace_extensions_dir = workspace_root.join(".pi").join("extensions");

    let skills_mode = if paths_contain(&skills_paths, &global_skills_dir) {
        "all"
    } else if paths_contain(&skills_paths, &workspace_skills_dir)
        || paths_match_suffix(&skills_paths, ".pi/skills")
    {
        "custom"
    } else {
        "all"
    };

    let extensions_mode = if paths_contain(&extensions_paths, &global_extensions_dir) {
        "all"
    } else if paths_contain(&extensions_paths, &workspace_extensions_dir)
        || paths_match_suffix(&extensions_paths, ".pi/extensions")
    {
        "custom"
    } else {
        "all"
    };

    let global_skills = list_dir_entries(&global_skills_dir, true)?;
    let global_extensions = list_dir_entries(&global_extensions_dir, false)?;

    let selected_skills = list_dir_entries(&workspace_skills_dir, true).unwrap_or_default();
    let selected_extensions = list_dir_entries(&workspace_extensions_dir, false)
        .unwrap_or_default();

    let skills = global_skills
        .iter()
        .map(|name| PiResourceEntry {
            name: name.clone(),
            selected: if skills_mode == "all" {
                true
            } else {
                selected_skills.contains(name)
            },
        })
        .collect();

    let extensions = global_extensions
        .iter()
        .map(|name| PiResourceEntry {
            name: name.clone(),
            selected: if extensions_mode == "all" {
                true
            } else {
                selected_extensions.contains(name)
            },
        })
        .collect();

    Ok(Json(WorkspacePiResourcesResponse {
        skills_mode: skills_mode.to_string(),
        extensions_mode: extensions_mode.to_string(),
        skills,
        extensions,
        global_skills_dir: global_skills_dir.to_string_lossy().to_string(),
        global_extensions_dir: global_extensions_dir.to_string_lossy().to_string(),
    }))
}

#[instrument(skip(state, user, request))]
pub async fn apply_workspace_pi_resources(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<WorkspacePiResourcesUpdateRequest>,
) -> ApiResult<Json<WorkspacePiResourcesResponse>> {
    let workspace_root = validate_workspace_path(&state, user.id(), &request.workspace_path)
        .await
        .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?;

    let global_skills_dir = expand_path(GLOBAL_PI_SKILLS_DIR)?;
    let global_extensions_dir = expand_path(GLOBAL_PI_EXTENSIONS_DIR)?;

    let skills_mode = normalize_mode(&request.skills_mode)?;
    let extensions_mode = normalize_mode(&request.extensions_mode)?;

    let global_skills = list_dir_entries(&global_skills_dir, true)?;
    let global_extensions = list_dir_entries(&global_extensions_dir, false)?;

    let workspace_pi_dir = workspace_root.join(".pi");
    let workspace_skills_dir = workspace_pi_dir.join("skills");
    let workspace_extensions_dir = workspace_pi_dir.join("extensions");

    if skills_mode == "custom" {
        replace_dir_contents(
            &workspace_skills_dir,
            &global_skills_dir,
            &request.skills,
            &global_skills,
        )?;
    }
    if extensions_mode == "custom" {
        replace_dir_contents(
            &workspace_extensions_dir,
            &global_extensions_dir,
            &request.extensions,
            &global_extensions,
        )?;
    }

    let settings_service = state
        .settings_pi_agent
        .as_ref()
        .ok_or_else(|| ApiError::internal("Pi agent settings not configured"))?
        .with_config_dir(workspace_pi_dir)
        .map_err(|e| ApiError::internal(format!("Failed to create settings service: {}", e)))?;

    let mut updates = HashMap::new();
    let skills_path = if skills_mode == "all" {
        global_skills_dir.to_string_lossy().to_string()
    } else {
        workspace_skills_dir.to_string_lossy().to_string()
    };
    let extensions_path = if extensions_mode == "all" {
        global_extensions_dir.to_string_lossy().to_string()
    } else {
        workspace_extensions_dir.to_string_lossy().to_string()
    };

    updates.insert("skills".to_string(), json!([skills_path]));
    updates.insert("extensions".to_string(), json!([extensions_path]));

    settings_service
        .update_values(ConfigUpdate { values: updates }, SettingsScope::User)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update Pi settings: {}", e)))?;

    update_workspace_sandbox_pi_access(
        &workspace_root,
        &global_skills_dir,
        &global_extensions_dir,
        skills_mode == "custom",
        extensions_mode == "custom",
    )?;

    // Respond with refreshed view.
    get_workspace_pi_resources(
        State(state),
        user,
        Query(WorkspaceMetaQuery {
            workspace_path: request.workspace_path,
        }),
    )
    .await
}

fn expand_path(path: &str) -> Result<PathBuf, ApiError> {
    let expanded = shellexpand::full(path)
        .map_err(|e| ApiError::internal(format!("Failed to expand path {}: {}", path, e)))?;
    Ok(PathBuf::from(expanded.as_ref()))
}

fn read_pi_settings_value(config_dir: &Path) -> Option<Value> {
    let path = config_dir.join("settings.json");
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn read_settings_paths(settings: &Value, key: &str) -> Vec<PathBuf> {
    settings
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .filter_map(|path| shellexpand::full(path).ok())
                .map(|expanded| PathBuf::from(expanded.as_ref()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn paths_contain(paths: &[PathBuf], candidate: &Path) -> bool {
    paths.iter().any(|path| path == candidate)
}

fn paths_match_suffix(paths: &[PathBuf], suffix: &str) -> bool {
    paths
        .iter()
        .any(|path| path.to_string_lossy().ends_with(suffix))
}

fn normalize_mode(mode: &str) -> Result<&str, ApiError> {
    match mode {
        "all" => Ok("all"),
        "custom" => Ok("custom"),
        _ => Err(ApiError::bad_request("Invalid mode")),
    }
}

fn list_dir_entries(path: &Path, directories_only: bool) -> Result<Vec<String>, ApiError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(path)
        .with_context(|| format!("reading directory {}", path.display()))
        .map_err(|e| ApiError::internal(format!("Failed to list directory: {}", e)))?;

    let mut names = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|e| ApiError::internal(format!("Failed to read entry: {}", e)))?;
        let file_type = entry
            .file_type()
            .map_err(|e| ApiError::internal(format!("Failed to read entry type: {}", e)))?;
        if directories_only && !file_type.is_dir() {
            continue;
        }
        if !directories_only && !(file_type.is_dir() || file_type.is_file()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        names.push(name);
    }

    names.sort();
    Ok(names)
}

fn replace_dir_contents(
    target_dir: &Path,
    source_dir: &Path,
    requested: &[String],
    available: &[String],
) -> Result<(), ApiError> {
    let requested_set: HashSet<String> = requested.iter().cloned().collect();
    for name in requested {
        if !available.contains(name) {
            return Err(ApiError::bad_request(format!(
                "Requested item not found: {}",
                name
            )));
        }
    }

    if target_dir.exists() {
        std::fs::remove_dir_all(target_dir).map_err(|e| {
            ApiError::internal(format!("Failed to clear directory {}: {}", target_dir.display(), e))
        })?;
    }
    std::fs::create_dir_all(target_dir).map_err(|e| {
        ApiError::internal(format!("Failed to create directory {}: {}", target_dir.display(), e))
    })?;

    for name in requested_set {
        let src = source_dir.join(&name);
        let dest = target_dir.join(&name);
        copy_entry_recursive(&src, &dest)?;
    }

    Ok(())
}

fn copy_entry_recursive(src: &Path, dest: &Path) -> Result<(), ApiError> {
    let metadata = std::fs::metadata(src)
        .map_err(|e| ApiError::internal(format!("Failed to read source {}: {}", src.display(), e)))?;
    if metadata.is_dir() {
        std::fs::create_dir_all(dest).map_err(|e| {
            ApiError::internal(format!("Failed to create dir {}: {}", dest.display(), e))
        })?;
        for entry in std::fs::read_dir(src)
            .map_err(|e| ApiError::internal(format!("Failed to read dir {}: {}", src.display(), e)))?
        {
            let entry = entry
                .map_err(|e| ApiError::internal(format!("Failed to read entry: {}", e)))?;
            let file_name = entry.file_name();
            let src_path = entry.path();
            let dest_path = dest.join(file_name);
            copy_entry_recursive(&src_path, &dest_path)?;
        }
    } else if metadata.is_file() {
        std::fs::copy(src, dest).map_err(|e| {
            ApiError::internal(format!("Failed to copy file {}: {}", src.display(), e))
        })?;
    }
    Ok(())
}

fn update_workspace_sandbox_pi_access(
    workspace_root: &Path,
    global_skills_dir: &Path,
    global_extensions_dir: &Path,
    deny_skills: bool,
    deny_extensions: bool,
) -> Result<(), ApiError> {
    let sandbox_path = workspace_root.join(".octo").join("sandbox.toml");
    let mut file = if sandbox_path.exists() {
        let contents = std::fs::read_to_string(&sandbox_path)
            .map_err(|e| ApiError::internal(format!("Failed to read sandbox config: {}", e)))?;
        toml::from_str::<SandboxConfigFile>(&contents)
            .map_err(|e| ApiError::internal(format!("Failed to parse sandbox config: {}", e)))?
    } else {
        SandboxConfigFile::default()
    };

    let profile_name = if file.profile.is_empty() {
        "development".to_string()
    } else {
        file.profile.clone()
    };

    let base_profile = file
        .profiles
        .get(&profile_name)
        .cloned()
        .or_else(|| SandboxProfile::builtin(&profile_name))
        .unwrap_or_else(SandboxProfile::development);

    let mut profile = base_profile;

    let skills_path = global_skills_dir.to_string_lossy().to_string();
    let extensions_path = global_extensions_dir.to_string_lossy().to_string();

    profile
        .deny_read
        .retain(|entry| entry != &skills_path && entry != &extensions_path);

    if deny_skills {
        profile.deny_read.push(skills_path);
    }
    if deny_extensions {
        profile.deny_read.push(extensions_path);
    }

    profile.deny_read.sort();
    profile.deny_read.dedup();

    file.profile = profile_name.clone();
    file.profiles.insert(profile_name, profile);

    if let Some(parent) = sandbox_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ApiError::internal(format!("Failed to create sandbox config dir: {}", e))
        })?;
    }

    let body = toml::to_string_pretty(&file)
        .map_err(|e| ApiError::internal(format!("Failed to serialize sandbox config: {}", e)))?;
    std::fs::write(&sandbox_path, body)
        .map_err(|e| ApiError::internal(format!("Failed to write sandbox config: {}", e)))?;

    Ok(())
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

#[instrument(skip(state, user, query))]
pub async fn list_workspace_locations(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WorkspaceLocationQuery>,
) -> ApiResult<Json<Vec<WorkspaceLocationSummary>>> {
    let locations = state
        .sessions
        .workspace_locations()
        .list_locations(user.id(), &query.workspace_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list workspace locations: {}", e)))?;

    let summaries = locations
        .into_iter()
        .map(|location| WorkspaceLocationSummary {
            id: location.id,
            workspace_id: location.workspace_id,
            location_id: location.location_id,
            kind: location.kind,
            path: location.path,
            runner_id: location.runner_id,
            repo_fingerprint: location.repo_fingerprint,
            is_active: location.is_active == 1,
        })
        .collect();

    Ok(Json(summaries))
}

#[instrument(skip(state, user, request))]
pub async fn upsert_workspace_location(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<UpsertWorkspaceLocationRequest>,
) -> ApiResult<Json<WorkspaceLocationSummary>> {
    let existing = state
        .sessions
        .workspace_locations()
        .get_location(user.id(), &request.workspace_id, &request.location_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch workspace location: {}", e)))?;

    let id = existing
        .as_ref()
        .map(|location| location.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let is_active = request.set_active.unwrap_or(false);

    let input = WorkspaceLocationInput {
        id: id.clone(),
        user_id: user.id().to_string(),
        workspace_id: request.workspace_id.clone(),
        location_id: request.location_id.clone(),
        kind: request.kind.clone(),
        path: request.path.clone(),
        runner_id: request.runner_id.clone(),
        repo_fingerprint: request.repo_fingerprint.clone(),
        is_active: if is_active { 1 } else { 0 },
    };

    state
        .sessions
        .workspace_locations()
        .upsert_location(&input, is_active)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to upsert workspace location: {}", e)))?;

    let summary = WorkspaceLocationSummary {
        id,
        workspace_id: request.workspace_id,
        location_id: request.location_id,
        kind: request.kind,
        path: request.path,
        runner_id: request.runner_id,
        repo_fingerprint: request.repo_fingerprint,
        is_active,
    };

    Ok(Json(summary))
}

#[instrument(skip(state, user, request))]
pub async fn set_active_workspace_location(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<SetActiveWorkspaceLocationRequest>,
) -> ApiResult<Json<WorkspaceLocationSummary>> {
    state
        .sessions
        .workspace_locations()
        .set_active_location(user.id(), &request.workspace_id, &request.location_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set active location: {}", e)))?;

    let location = state
        .sessions
        .workspace_locations()
        .get_location(user.id(), &request.workspace_id, &request.location_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch workspace location: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Workspace location not found".to_string()))?;

    Ok(Json(WorkspaceLocationSummary {
        id: location.id,
        workspace_id: location.workspace_id,
        location_id: location.location_id,
        kind: location.kind,
        path: location.path,
        runner_id: location.runner_id,
        repo_fingerprint: location.repo_fingerprint,
        is_active: location.is_active == 1,
    }))
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

//! API request handlers.

use std::convert::Infallible;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header::SET_COOKIE},
    response::sse::{Event, KeepAlive, Sse},
    response::{AppendHeaders, IntoResponse},
};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_stream::{StreamExt, wrappers::IntervalStream};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::auth::{AuthError, CurrentUser, RequireAdmin};
use crate::observability::{CpuTimes, HostMetrics, read_host_metrics};
use crate::projects::{self, ProjectMetadata};
use crate::session::{CreateSessionRequest, Session, SessionContainerStats};
use crate::session_ui::SessionAutoAttachMode;
use crate::user::{
    CreateUserRequest, UpdateUserRequest, UserInfo as DbUserInfo, UserListQuery, UserStats,
};

use super::error::{ApiError, ApiResult};
use super::state::AppState;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
struct AdminMetricsSnapshot {
    pub timestamp: String,
    pub host: Option<HostMetrics>,
    pub containers: Vec<SessionContainerStats>,
    pub error: Option<String>,
}

/// Health check endpoint.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Admin stats response for status bar.
#[derive(Debug, Serialize)]
pub struct AdminStatsResponse {
    pub total_users: i64,
    pub active_users: i64,
    pub total_sessions: i64,
    pub running_sessions: i64,
}

/// Get admin stats for the status bar (admin only).
#[instrument(skip(state, _user))]
pub async fn get_admin_stats(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<AdminStatsResponse>> {
    // Get user stats
    let user_stats = state.users.get_stats().await?;

    // Get session counts
    let sessions = state.sessions.list_sessions().await?;
    let total_sessions = sessions.len() as i64;
    let running_sessions = sessions
        .iter()
        .filter(|s| s.status == crate::session::SessionStatus::Running)
        .count() as i64;

    // Count active users (users with running sessions)
    let active_user_ids: std::collections::HashSet<_> = sessions
        .iter()
        .filter(|s| s.status == crate::session::SessionStatus::Running)
        .map(|s| s.user_id.as_str())
        .collect();
    let active_users = active_user_ids.len() as i64;

    Ok(Json(AdminStatsResponse {
        total_users: user_stats.total,
        active_users,
        total_sessions,
        running_sessions,
    }))
}

/// WebSocket debug info.
#[derive(Debug, Serialize)]
pub struct WsDebugResponse {
    pub connected_users: usize,
}

/// Get WebSocket debug info (public, harmless).
pub async fn ws_debug(State(state): State<AppState>) -> Json<WsDebugResponse> {
    Json(WsDebugResponse {
        connected_users: state.ws_hub.connected_user_count(),
    })
}

/// Feature flags exposed to the frontend.
#[derive(Debug, Serialize)]
pub struct FeaturesResponse {
    /// Whether mmry (memories) integration is enabled.
    pub mmry_enabled: bool,
    /// Auto-attach mode when opening chat history.
    pub session_auto_attach: SessionAutoAttachMode,
    /// Whether to scan running sessions for matching chat session IDs.
    pub session_auto_attach_scan: bool,
    /// Voice mode configuration (null if disabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceConfig>,
    /// Whether WebSocket events are enabled (vs SSE).
    pub websocket_events: bool,
}

/// Voice configuration exposed to frontend.
#[derive(Debug, Serialize)]
pub struct VoiceConfig {
    /// WebSocket URL for the eaRS STT service.
    pub stt_url: String,
    /// WebSocket URL for the kokorox TTS service.
    pub tts_url: String,
    /// VAD timeout in milliseconds.
    pub vad_timeout_ms: u32,
    /// Default kokorox voice ID.
    pub default_voice: String,
    /// Default TTS speed (0.1 - 3.0).
    pub default_speed: f32,
    /// Enable auto language detection.
    pub auto_language_detect: bool,
    /// Whether TTS is muted by default.
    pub tts_muted: bool,
    /// Continuous conversation mode.
    pub continuous_mode: bool,
    /// Default visualizer style ("orb" or "kitt").
    pub default_visualizer: String,
    /// Minimum words to interrupt TTS (0 = disabled).
    pub interrupt_word_count: u32,
    /// Reset word count after this silence in ms (0 = disabled).
    pub interrupt_backoff_ms: u32,
    /// Per-visualizer voice/speed settings.
    pub visualizer_voices: std::collections::HashMap<String, VisualizerVoice>,
}

/// Per-visualizer voice settings.
#[derive(Debug, Serialize)]
pub struct VisualizerVoice {
    pub voice: String,
    pub speed: f32,
}

/// Get enabled features/capabilities.
pub async fn features(State(state): State<AppState>) -> Json<FeaturesResponse> {
    let voice = if state.voice.enabled {
        Some(VoiceConfig {
            stt_url: "/api/voice/stt".to_string(),
            tts_url: "/api/voice/tts".to_string(),
            vad_timeout_ms: state.voice.vad_timeout_ms,
            default_voice: state.voice.default_voice.clone(),
            default_speed: state.voice.default_speed,
            auto_language_detect: state.voice.auto_language_detect,
            tts_muted: state.voice.tts_muted,
            continuous_mode: state.voice.continuous_mode,
            default_visualizer: state.voice.default_visualizer.clone(),
            interrupt_word_count: state.voice.interrupt_word_count,
            interrupt_backoff_ms: state.voice.interrupt_backoff_ms,
            visualizer_voices: state
                .voice
                .visualizer_voices
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        VisualizerVoice {
                            voice: v.voice.clone(),
                            speed: v.speed,
                        },
                    )
                })
                .collect(),
        })
    } else {
        None
    };

    Json(FeaturesResponse {
        mmry_enabled: state.mmry.enabled,
        session_auto_attach: state.session_ui.auto_attach,
        session_auto_attach_scan: state.session_ui.auto_attach_scan,
        voice,
        // WebSocket events are always enabled when the ws module is compiled in
        websocket_events: true,
    })
}

/// Session response with URLs.
#[derive(Debug, Serialize)]
pub struct SessionWithUrls {
    #[serde(flatten)]
    pub session: Session,
    pub urls: SessionUrls,
}

/// URLs for accessing session services.
#[derive(Debug, Serialize)]
pub struct SessionUrls {
    pub opencode: String,
    pub fileserver: String,
    pub terminal: String,
}

impl SessionWithUrls {
    pub fn from_session(session: Session, host: &str) -> Self {
        let _ = host;
        let urls = SessionUrls {
            opencode: format!("/session/{}/code", session.id),
            fileserver: format!("/session/{}/files", session.id),
            terminal: format!("/session/{}/term", session.id),
        };
        Self { session, urls }
    }
}

/// List all sessions.
#[instrument(skip(state))]
pub async fn list_sessions(State(state): State<AppState>) -> ApiResult<Json<Vec<Session>>> {
    let sessions = state.sessions.list_sessions().await?;
    info!(count = sessions.len(), "Listed sessions");
    Ok(Json(sessions))
}

/// Get a specific session by ID or readable alias.
///
/// The session_id parameter can be either:
/// - A full session UUID (e.g., "6a03da55-2757-4d71-b421-af929bc4aef5")
/// - A readable alias (e.g., "foxy-geek")
#[instrument(skip(state))]
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<Session>> {
    if let Some(session) = state.sessions.get_session(&session_id).await? {
        return Ok(Json(session));
    }

    Err(ApiError::not_found(format!(
        "Session {} not found",
        session_id
    )))
}

/// Create a new session.
#[instrument(skip(state, request), fields(workspace_path = ?request.workspace_path))]
pub async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> ApiResult<(StatusCode, Json<SessionWithUrls>)> {
    let session = state.sessions.create_session(request).await?;
    info!(session_id = %session.id, "Created new session");

    // TODO: Get actual host from request headers
    let response = SessionWithUrls::from_session(session, "localhost");
    Ok((StatusCode::CREATED, Json(response)))
}

/// Stop a session.
#[instrument(skip(state))]
pub async fn stop_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    // First check if session exists
    let session = state.sessions.get_session(&session_id).await?;

    if session.is_none() {
        return Err(ApiError::not_found(format!(
            "Session {} not found",
            session_id
        )));
    }

    state.sessions.stop_session(&session_id).await?;
    info!(session_id = %session_id, "Stopped session");

    Ok(StatusCode::NO_CONTENT)
}

/// Delete a session.
#[instrument(skip(state))]
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    // Uses centralized From<anyhow::Error> conversion
    state.sessions.delete_session(&session_id).await?;

    info!(session_id = %session_id, "Deleted session");
    Ok(StatusCode::NO_CONTENT)
}

/// Resume a stopped session.
///
/// This restarts a stopped container, which is faster than creating a new session
/// because the container already exists with all its state preserved.
#[instrument(skip(state))]
pub async fn resume_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state.sessions.resume_session(&session_id).await?;
    info!(session_id = %session_id, "Resumed session");

    let response = SessionWithUrls::from_session(session, "localhost");
    Ok(Json(response))
}

/// Get or create a session.
///
/// This is the preferred way to get a session. It will:
/// 1. Resume an existing stopped session if one exists (fast)
/// 2. Create a new session if no resumable session exists
#[instrument(skip(state, request), fields(workspace_path = ?request.workspace_path))]
pub async fn get_or_create_session(
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state.sessions.get_or_create_session(request).await?;
    info!(session_id = %session.id, status = ?session.status, "Got or created session");

    let response = SessionWithUrls::from_session(session, "localhost");
    Ok(Json(response))
}

/// Request for getting or creating a session for a specific workspace.
#[derive(Debug, Deserialize)]
pub struct GetOrCreateForWorkspaceRequest {
    /// Path to the workspace directory.
    pub workspace_path: String,
}

/// Get or create a session for a specific workspace path.
///
/// This is the preferred way to resume a session from chat history.
/// It will:
/// 1. Find an existing running session for the workspace (if any)
/// 2. Enforce LRU cap by stopping oldest idle session if needed
/// 3. Create a new session for that workspace (if none running)
#[instrument(skip(state, request), fields(workspace_path = %request.workspace_path))]
pub async fn get_or_create_session_for_workspace(
    State(state): State<AppState>,
    Json(request): Json<GetOrCreateForWorkspaceRequest>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state
        .sessions
        .get_or_create_session_for_workspace(&request.workspace_path)
        .await?;
    info!(
        session_id = %session.id,
        workspace_path = %request.workspace_path,
        status = ?session.status,
        "Got or created session for workspace"
    );

    let response = SessionWithUrls::from_session(session, "localhost");
    Ok(Json(response))
}

/// Touch session activity (update last_activity_at).
///
/// This should be called when the user interacts with the session
/// (e.g., sends a message, runs a command).
#[instrument(skip(state))]
pub async fn touch_session_activity(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    state.sessions.touch_session_activity(&session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Check if a session has an available image update.
#[instrument(skip(state))]
pub async fn check_session_update(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionUpdateStatus>> {
    let update_available = state.sessions.check_for_image_update(&session_id).await?;

    Ok(Json(SessionUpdateStatus {
        session_id,
        update_available: update_available.is_some(),
        new_digest: update_available,
    }))
}

/// Upgrade a session to the latest image version.
#[instrument(skip(state))]
pub async fn upgrade_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionWithUrls>> {
    let session = state.sessions.upgrade_session(&session_id).await?;
    info!(session_id = %session_id, "Upgraded session");

    let response = SessionWithUrls::from_session(session, "localhost");
    Ok(Json(response))
}

/// Response for session update check.
#[derive(Debug, Serialize)]
pub struct SessionUpdateStatus {
    pub session_id: String,
    pub update_available: bool,
    pub new_digest: Option<String>,
}

/// Check all sessions for available updates.
#[instrument(skip(state))]
pub async fn check_all_updates(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<SessionUpdateStatus>>> {
    let updates = state.sessions.check_all_for_updates().await?;

    let statuses: Vec<SessionUpdateStatus> = updates
        .into_iter()
        .map(|(session_id, new_digest)| SessionUpdateStatus {
            session_id,
            update_available: true,
            new_digest: Some(new_digest),
        })
        .collect();

    info!(count = statuses.len(), "Checked all sessions for updates");
    Ok(Json(statuses))
}

// ============================================================================
// Project/Workspace Handlers
// ============================================================================

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
    Query(query): Query<WorkspaceDirQuery>,
) -> ApiResult<Json<Vec<WorkspaceDirEntry>>> {
    let root = state.sessions.workspace_root();
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

    let workspace_root = state.sessions.workspace_root();
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

#[cfg(test)]
mod tests {
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

/// Serve a project logo file.
/// Path format: {project_path}/logo/{filename}
#[instrument(skip(state))]
pub async fn get_project_logo(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    use axum::http::header;

    let root = state.sessions.workspace_root();
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

// ============================================================================
// Authentication Handlers
// ============================================================================

/// Login request for dev mode.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

/// User info in login response.
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub email: String,
    pub role: String,
}

/// Dev mode login endpoint.
pub async fn dev_login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, AuthError> {
    // Only works in dev mode
    if !state.auth.is_dev_mode() {
        return Err(AuthError::InvalidCredentials);
    }

    // Validate credentials
    let user = state
        .auth
        .validate_dev_credentials(&request.username, &request.password)
        .ok_or(AuthError::InvalidCredentials)?;

    // Generate token
    let token = state.auth.generate_dev_token(user)?;

    // Build cookie with security flags
    // In dev mode, omit Secure flag to allow http://localhost
    // In production, always include Secure flag
    let secure_flag = if state.auth.is_dev_mode() {
        ""
    } else {
        " Secure;"
    };
    let cookie = format!(
        "auth_token={}; Path=/; HttpOnly; SameSite=Lax;{} Max-Age={}",
        token,
        secure_flag,
        60 * 60 * 24
    );

    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(LoginResponse {
            token,
            user: UserInfo {
                id: user.id.clone(),
                name: user.name.clone(),
                email: user.email.clone(),
                role: user.role.to_string(),
            },
        }),
    ))
}

/// Get current user info.
#[allow(dead_code)]
pub async fn get_current_user(user: CurrentUser) -> Json<UserInfo> {
    Json(UserInfo {
        id: user.id().to_string(),
        name: user.display_name().to_string(),
        email: user.claims.email.clone().unwrap_or_default(),
        role: user.role().to_string(),
    })
}

/// Registration request.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub invite_code: String,
    pub display_name: Option<String>,
}

/// Registration response.
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub token: String,
    pub user: UserInfo,
}

/// Register a new user with invite code.
///
/// This operation is designed to be safe against race conditions:
/// 1. Atomically consume the invite code (prevents double-use)
/// 2. Create the user
/// 3. If user creation fails, restore the invite code use
#[instrument(skip(state, request), fields(username = %request.username))]
pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> ApiResult<impl IntoResponse> {
    // Atomically consume the invite code first.
    // This prevents TOCTOU race conditions where two requests could both
    // validate and then both try to use the same single-use code.
    let _invite_code_id = state
        .invites
        .try_consume_atomic(&request.invite_code, "pending") // Use "pending" as placeholder
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    // Create the user
    let user = match state
        .users
        .create_user(CreateUserRequest {
            username: request.username.clone(),
            email: request.email.clone(),
            password: Some(request.password),
            display_name: request.display_name,
            role: None, // Default to user role
            external_id: None,
        })
        .await
    {
        Ok(user) => user,
        Err(e) => {
            // User creation failed - restore the invite code use
            // This is best-effort; if it fails, we log but don't change the error
            if let Err(restore_err) = state.invites.restore_use(&request.invite_code).await {
                warn!(
                    "Failed to restore invite code use after user creation failure: {:?}",
                    restore_err
                );
            }
            return Err(e.into());
        }
    };

    // Update the invite code to record the actual user ID
    // This is informational and not critical for correctness
    if let Err(e) = sqlx::query("UPDATE invite_codes SET used_by = ? WHERE code = ?")
        .bind(&user.id)
        .bind(&request.invite_code)
        .execute(state.invites.pool())
        .await
    {
        warn!("Failed to update invite code used_by: {:?}", e);
    }

    // Generate JWT token for the new user
    let token = state.auth.generate_token(
        &user.id,
        &user.email,
        &user.display_name,
        &user.role.to_string(),
    )?;

    // Build cookie
    let secure_flag = if state.auth.is_dev_mode() {
        ""
    } else {
        " Secure;"
    };
    let cookie = format!(
        "auth_token={}; Path=/; HttpOnly; SameSite=Lax;{} Max-Age={}",
        token,
        secure_flag,
        60 * 60 * 24 // 24 hours
    );

    info!(user_id = %user.id, username = %user.username, "User registered successfully");

    Ok((
        StatusCode::CREATED,
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(RegisterResponse {
            token,
            user: UserInfo {
                id: user.id,
                name: user.display_name,
                email: user.email,
                role: user.role.to_string(),
            },
        }),
    ))
}

/// Login endpoint (works with database users).
#[instrument(skip(state, request), fields(username = %request.username))]
pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    // Try to verify against database users first
    let user = state
        .users
        .verify_credentials(&request.username, &request.password)
        .await?;

    let (token, user_info) = match user {
        Some(db_user) => {
            // Database user found and verified
            let token = state.auth.generate_token(
                &db_user.id,
                &db_user.email,
                &db_user.display_name,
                &db_user.role.to_string(),
            )?;
            let user_info = UserInfo {
                id: db_user.id,
                name: db_user.display_name,
                email: db_user.email,
                role: db_user.role.to_string(),
            };
            (token, user_info)
        }
        None => {
            // Fall back to dev mode credentials if enabled
            if state.auth.is_dev_mode() {
                let dev_user = state
                    .auth
                    .validate_dev_credentials(&request.username, &request.password)
                    .ok_or_else(|| ApiError::unauthorized("Invalid username or password"))?;

                let token = state.auth.generate_dev_token(dev_user)?;
                let user_info = UserInfo {
                    id: dev_user.id.clone(),
                    name: dev_user.name.clone(),
                    email: dev_user.email.clone(),
                    role: dev_user.role.to_string(),
                };
                (token, user_info)
            } else {
                return Err(ApiError::unauthorized("Invalid username or password"));
            }
        }
    };

    // Build cookie
    let secure_flag = if state.auth.is_dev_mode() {
        ""
    } else {
        " Secure;"
    };
    let cookie = format!(
        "auth_token={}; Path=/; HttpOnly; SameSite=Lax;{} Max-Age={}",
        token,
        secure_flag,
        60 * 60 * 24 // 24 hours
    );

    info!(user_id = %user_info.id, "User logged in successfully");

    Ok((
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(LoginResponse {
            token,
            user: user_info,
        }),
    ))
}

/// Logout endpoint (clears auth cookie).
pub async fn logout() -> impl IntoResponse {
    // Clear the auth cookie by setting it to empty with immediate expiry
    let cookie = "auth_token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";

    (
        AppendHeaders([(SET_COOKIE, cookie.to_string())]),
        StatusCode::NO_CONTENT,
    )
}

// ============================================================================
// Admin Handlers
// ============================================================================

/// List all sessions (admin only).
#[instrument(skip(state, _user))]
pub async fn admin_list_sessions(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<Vec<Session>>> {
    let sessions = state.sessions.list_sessions().await?;
    info!(count = sessions.len(), "Admin listed all sessions");
    Ok(Json(sessions))
}

/// Force stop a session (admin only).
#[instrument(skip(state, _user))]
pub async fn admin_force_stop_session(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    // Uses centralized From<anyhow::Error> conversion
    state.sessions.stop_session(&session_id).await?;

    info!(session_id = %session_id, "Admin force stopped session");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub struct LocalCleanupResponse {
    cleared: usize,
}

/// Clean up orphan local session processes (admin only).
#[instrument(skip(state, _user))]
pub async fn admin_cleanup_local_sessions(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<LocalCleanupResponse>> {
    let cleared = state.sessions.cleanup_local_orphans().await?;
    info!(cleared, "Admin cleaned up local sessions");
    Ok(Json(LocalCleanupResponse { cleared }))
}

/// SSE metrics stream (admin only).
#[instrument(skip(state, _user))]
pub async fn admin_metrics_stream(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>> {
    let state = state.clone();
    let cpu_state: Arc<Mutex<Option<CpuTimes>>> = Arc::new(Mutex::new(None));
    let interval = tokio::time::interval(Duration::from_secs(2));

    let stream = IntervalStream::new(interval).then(move |_| {
        let state = state.clone();
        let cpu_state = cpu_state.clone();
        async move {
            let mut guard = cpu_state.lock().await;
            let snapshot = build_admin_metrics_snapshot(&state, &mut guard).await;
            let data = match serde_json::to_string(&snapshot) {
                Ok(data) => data,
                Err(err) => {
                    warn!("Failed to serialize metrics snapshot: {:?}", err);
                    "{\"error\":\"metrics_serialization_failed\"}".to_string()
                }
            };
            Ok(Event::default().data(data))
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

async fn build_admin_metrics_snapshot(
    state: &AppState,
    prev_cpu: &mut Option<CpuTimes>,
) -> AdminMetricsSnapshot {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut errors = Vec::new();

    let previous_cpu = prev_cpu.clone();
    let host = match read_host_metrics(previous_cpu.clone()).await {
        Ok((metrics, cpu)) => {
            *prev_cpu = Some(cpu);
            Some(metrics)
        }
        Err(err) => {
            *prev_cpu = previous_cpu;
            errors.push(format!("host_metrics: {}", err));
            None
        }
    };

    let containers = match state.sessions.collect_container_stats().await {
        Ok(report) => {
            if !report.errors.is_empty() {
                errors.extend(report.errors);
            }
            report.stats
        }
        Err(err) => {
            errors.push(format!("container_stats: {}", err));
            Vec::new()
        }
    };

    let error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    AdminMetricsSnapshot {
        timestamp,
        host,
        containers,
        error,
    }
}

// ============================================================================
// User Management Handlers
// ============================================================================

/// List all users (admin only).
#[instrument(skip(state, _user))]
pub async fn list_users(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Query(query): Query<UserListQuery>,
) -> ApiResult<Json<Vec<DbUserInfo>>> {
    // Uses centralized From<anyhow::Error> conversion
    let users = state.users.list_users(query).await?;

    let user_infos: Vec<DbUserInfo> = users.into_iter().map(|u| u.into()).collect();
    info!(count = user_infos.len(), "Listed users");
    Ok(Json(user_infos))
}

/// Get a specific user (admin only).
#[instrument(skip(state, _user))]
pub async fn get_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
) -> ApiResult<Json<DbUserInfo>> {
    // Uses centralized From<anyhow::Error> conversion
    state
        .users
        .get_user(&user_id)
        .await?
        .map(|u| Json(u.into()))
        .ok_or_else(|| ApiError::not_found(format!("User {} not found", user_id)))
}

/// Create a new user (admin only).
#[instrument(skip(state, _user, request), fields(username = ?request.username))]
pub async fn create_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Json(request): Json<CreateUserRequest>,
) -> ApiResult<(StatusCode, Json<DbUserInfo>)> {
    // Uses centralized From<anyhow::Error> conversion
    let user = state.users.create_user(request).await?;

    info!(user_id = %user.id, "Created new user");
    Ok((StatusCode::CREATED, Json(user.into())))
}

/// Update a user (admin only).
#[instrument(skip(state, _user, request))]
pub async fn update_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> ApiResult<Json<DbUserInfo>> {
    // Uses centralized From<anyhow::Error> conversion
    let user = state.users.update_user(&user_id, request).await?;

    info!(user_id = %user.id, "Updated user");
    Ok(Json(user.into()))
}

/// Delete a user (admin only).
#[instrument(skip(state, _user))]
pub async fn delete_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
) -> ApiResult<StatusCode> {
    // Uses centralized From<anyhow::Error> conversion
    state.users.delete_user(&user_id).await?;

    info!(user_id = %user_id, "Deleted user");
    Ok(StatusCode::NO_CONTENT)
}

/// Deactivate a user (admin only).
#[instrument(skip(state, _user))]
pub async fn deactivate_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
) -> ApiResult<Json<DbUserInfo>> {
    // Uses centralized From<anyhow::Error> conversion
    let user = state.users.deactivate_user(&user_id).await?;

    info!(user_id = %user.id, "Deactivated user");
    Ok(Json(user.into()))
}

/// Activate a user (admin only).
#[instrument(skip(state, _user))]
pub async fn activate_user(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
    Path(user_id): Path<String>,
) -> ApiResult<Json<DbUserInfo>> {
    // Uses centralized From<anyhow::Error> conversion
    let user = state.users.activate_user(&user_id).await?;

    info!(user_id = %user.id, "Activated user");
    Ok(Json(user.into()))
}

/// Get user statistics (admin only).
#[instrument(skip(state, _user))]
pub async fn get_user_stats(
    State(state): State<AppState>,
    RequireAdmin(_user): RequireAdmin,
) -> ApiResult<Json<UserStats>> {
    // Uses centralized From<anyhow::Error> conversion
    let stats = state.users.get_stats().await?;

    Ok(Json(stats))
}

/// Get current user profile.
#[instrument(skip(state, user))]
pub async fn get_me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<DbUserInfo>> {
    // Try to get user from database (uses centralized From<anyhow::Error> conversion)
    if let Some(db_user) = state.users.get_user(user.id()).await? {
        return Ok(Json(db_user.into()));
    }

    // Fallback to creating UserInfo from JWT claims
    Ok(Json(DbUserInfo {
        id: user.id().to_string(),
        username: user
            .claims
            .preferred_username
            .clone()
            .unwrap_or_else(|| user.id().to_string()),
        email: user.claims.email.clone().unwrap_or_default(),
        display_name: user.display_name().to_string(),
        avatar_url: None,
        role: user.role().into(),
        is_active: true,
        created_at: String::new(),
        last_login_at: None,
    }))
}

/// Update current user profile.
#[instrument(skip(state, user, request))]
pub async fn update_me(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<UpdateMeRequest>,
) -> ApiResult<Json<DbUserInfo>> {
    // Only allow updating display_name, avatar_url, and settings
    let update = UpdateUserRequest {
        display_name: request.display_name,
        avatar_url: request.avatar_url,
        settings: request.settings,
        ..Default::default()
    };

    // Uses centralized From<anyhow::Error> conversion
    let updated = state.users.update_user(user.id(), update).await?;

    Ok(Json(updated.into()))
}

/// Request body for updating own profile.
#[derive(Debug, Deserialize)]
pub struct UpdateMeRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub settings: Option<String>,
}

// Helper to convert auth Role to user Role
impl From<crate::auth::Role> for crate::user::UserRole {
    fn from(role: crate::auth::Role) -> Self {
        match role {
            crate::auth::Role::Admin => crate::user::UserRole::Admin,
            crate::auth::Role::User => crate::user::UserRole::User,
        }
    }
}

// ============================================================================
// Invite Code Management Handlers (Admin)
// ============================================================================

use crate::invite::{
    BatchCreateInviteCodesRequest, CreateInviteCodeRequest, InviteCodeListQuery, InviteCodeSummary,
};

/// List all invite codes (admin only).
#[instrument(skip(state, user))]
pub async fn list_invite_codes(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Query(query): Query<InviteCodeListQuery>,
) -> ApiResult<Json<Vec<InviteCodeSummary>>> {
    let _ = user;
    let codes = state.invites.list(query).await?;
    let summaries: Vec<InviteCodeSummary> = codes.into_iter().map(|c| c.into()).collect();
    info!(count = summaries.len(), "Listed invite codes");
    Ok(Json(summaries))
}

/// Create a single invite code (admin only).
#[instrument(skip(state, user, request))]
pub async fn create_invite_code(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Json(request): Json<CreateInviteCodeRequest>,
) -> ApiResult<(StatusCode, Json<InviteCodeSummary>)> {
    let code = state.invites.create(request, user.id()).await?;
    info!(code_id = %code.id, "Created invite code");
    Ok((StatusCode::CREATED, Json(code.into())))
}

/// Create multiple invite codes at once (admin only).
#[instrument(skip(state, user, request))]
pub async fn create_invite_codes_batch(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Json(request): Json<BatchCreateInviteCodesRequest>,
) -> ApiResult<(StatusCode, Json<Vec<InviteCodeSummary>>)> {
    let codes = state
        .invites
        .create_batch(
            request.count,
            request.uses_per_code,
            request.expires_in_secs,
            request.prefix.as_deref(),
            request.note.as_deref(),
            user.id(),
        )
        .await?;

    let summaries: Vec<InviteCodeSummary> = codes.into_iter().map(|c| c.into()).collect();
    info!(count = summaries.len(), "Created batch of invite codes");
    Ok((StatusCode::CREATED, Json(summaries)))
}

/// Get a specific invite code (admin only).
#[instrument(skip(state, user))]
pub async fn get_invite_code(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Path(code_id): Path<String>,
) -> ApiResult<Json<InviteCodeSummary>> {
    let _ = user;
    state
        .invites
        .get(&code_id)
        .await?
        .map(|c| Json(c.into()))
        .ok_or_else(|| ApiError::not_found(format!("Invite code {} not found", code_id)))
}

/// Revoke an invite code (admin only).
#[instrument(skip(state, user))]
pub async fn revoke_invite_code(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Path(code_id): Path<String>,
) -> ApiResult<StatusCode> {
    let _ = user;
    state.invites.revoke(&code_id).await?;
    info!(code_id = %code_id, "Revoked invite code");
    Ok(StatusCode::NO_CONTENT)
}

/// Delete an invite code (admin only).
#[instrument(skip(state, user))]
pub async fn delete_invite_code(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
    Path(code_id): Path<String>,
) -> ApiResult<StatusCode> {
    let _ = user;
    state.invites.delete(&code_id).await?;
    info!(code_id = %code_id, "Deleted invite code");
    Ok(StatusCode::NO_CONTENT)
}

/// Get invite code statistics (admin only).
#[derive(Debug, Serialize)]
pub struct InviteCodeStats {
    pub total: i64,
    pub valid: i64,
}

#[instrument(skip(state, user))]
pub async fn get_invite_code_stats(
    State(state): State<AppState>,
    RequireAdmin(user): RequireAdmin,
) -> ApiResult<Json<InviteCodeStats>> {
    let _ = user;
    let total = state.invites.count().await?;
    let valid = state.invites.count_valid().await?;
    Ok(Json(InviteCodeStats { total, valid }))
}

// ============================================================================
// Agent Management Handlers
// ============================================================================

use super::super::agent::{
    AgentExecRequest, AgentExecResponse, AgentInfo, CreateAgentRequest, CreateAgentResponse,
    StartAgentRequest, StartAgentResponse, StopAgentResponse,
};

#[derive(Debug, Deserialize)]
pub struct AgentListQuery {
    #[serde(default)]
    pub include_context: bool,
}

/// List all agents for a session (running + available directories).
#[instrument(skip(state))]
pub async fn list_agents(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<AgentListQuery>,
) -> ApiResult<Json<Vec<AgentInfo>>> {
    let opencode_session = state.sessions.get_or_create_opencode_session().await?;
    let agents = state
        .agents
        .list_agents(&opencode_session.id, query.include_context)
        .await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        count = agents.len(),
        "Listed agents"
    );
    Ok(Json(agents))
}

/// Get a specific agent.
#[instrument(skip(state))]
pub async fn get_agent(
    State(state): State<AppState>,
    Path((session_id, agent_id)): Path<(String, String)>,
    Query(query): Query<AgentListQuery>,
) -> ApiResult<Json<AgentInfo>> {
    let opencode_session = state.sessions.get_or_create_opencode_session().await?;
    state
        .agents
        .get_agent(&opencode_session.id, &agent_id, query.include_context)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("Agent {} not found", agent_id)))
}

/// Start an agent in a subdirectory.
#[instrument(skip(state, request), fields(directory = ?request.directory))]
pub async fn start_agent(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<StartAgentRequest>,
) -> ApiResult<(StatusCode, Json<StartAgentResponse>)> {
    let opencode_session = state.sessions.get_or_create_opencode_session().await?;
    let response = state
        .agents
        .start_agent(&opencode_session.id, &request.directory)
        .await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        agent_id = %response.id,
        port = response.port,
        "Started agent"
    );
    Ok((StatusCode::CREATED, Json(response)))
}

/// Stop an agent.
#[instrument(skip(state))]
pub async fn stop_agent(
    State(state): State<AppState>,
    Path((session_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<StopAgentResponse>> {
    let opencode_session = state.sessions.get_or_create_opencode_session().await?;
    let response = state
        .agents
        .stop_agent(&opencode_session.id, &agent_id)
        .await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        agent_id = %agent_id,
        stopped = response.stopped,
        "Stopped agent"
    );
    Ok(Json(response))
}

/// Rediscover agents after control plane restart.
#[instrument(skip(state))]
pub async fn rediscover_agents(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    let opencode_session = state.sessions.get_or_create_opencode_session().await?;
    state.agents.rediscover_agents(&opencode_session.id).await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        "Rediscovered agents"
    );
    Ok(StatusCode::NO_CONTENT)
}

/// Create a new agent directory with AGENTS.md file.
#[instrument(skip(state, request), fields(name = ?request.name))]
pub async fn create_agent(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<CreateAgentRequest>,
) -> ApiResult<(StatusCode, Json<CreateAgentResponse>)> {
    let opencode_session = state.sessions.get_or_create_opencode_session().await?;
    let response = state
        .agents
        .create_agent(
            &opencode_session.id,
            &request.name,
            &request.description,
            request.scaffold.as_ref(),
        )
        .await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        agent_id = %response.id,
        directory = %response.directory,
        "Created agent"
    );
    Ok((StatusCode::CREATED, Json(response)))
}

/// Execute a command in a session workspace.
#[instrument(skip(state, request), fields(command = %request.command))]
pub async fn exec_agent_command(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<AgentExecRequest>,
) -> ApiResult<Json<AgentExecResponse>> {
    let opencode_session = state.sessions.get_or_create_opencode_session().await?;
    let response = state
        .agents
        .exec_command(&opencode_session.id, request)
        .await?;
    Ok(Json(response))
}

// ============================================================================
// Chat History Handlers
// ============================================================================

use crate::history::ChatSession;

/// Query parameters for listing chat history.
#[derive(Debug, Deserialize)]
pub struct ChatHistoryQuery {
    /// Filter by workspace path.
    pub workspace: Option<String>,
    /// Include child sessions (default: false).
    #[serde(default)]
    pub include_children: bool,
    /// Maximum number of sessions to return.
    pub limit: Option<usize>,
}

/// List all chat sessions from OpenCode history.
///
/// This reads sessions directly from disk without requiring a running OpenCode instance.
#[instrument(skip(_state))]
pub async fn list_chat_history(
    State(_state): State<AppState>,
    Query(query): Query<ChatHistoryQuery>,
) -> ApiResult<Json<Vec<ChatSession>>> {
    let sessions = crate::history::list_sessions()
        .map_err(|e| ApiError::internal(format!("Failed to list chat history: {}", e)))?;

    let mut filtered: Vec<ChatSession> = sessions
        .into_iter()
        .filter(|s| {
            // Filter by workspace if specified
            if let Some(ref ws) = query.workspace {
                if s.workspace_path != *ws {
                    return false;
                }
            }
            // Filter out child sessions unless explicitly included
            if !query.include_children && s.is_child {
                return false;
            }
            true
        })
        .collect();

    // Apply limit if specified
    if let Some(limit) = query.limit {
        filtered.truncate(limit);
    }

    info!(count = filtered.len(), "Listed chat history");
    Ok(Json(filtered))
}

/// Get a specific chat session by ID.
#[instrument]
pub async fn get_chat_session(Path(session_id): Path<String>) -> ApiResult<Json<ChatSession>> {
    crate::history::get_session(&session_id)
        .map_err(|e| ApiError::internal(format!("Failed to get chat session: {}", e)))?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("Chat session {} not found", session_id)))
}

/// Request to update a chat session.
#[derive(Debug, Deserialize)]
pub struct UpdateChatSessionRequest {
    /// New title for the session
    pub title: Option<String>,
}

/// Update a chat session (e.g., rename).
#[instrument]
pub async fn update_chat_session(
    Path(session_id): Path<String>,
    Json(request): Json<UpdateChatSessionRequest>,
) -> ApiResult<Json<ChatSession>> {
    // Currently only title updates are supported
    if let Some(title) = request.title {
        let session = crate::history::update_session_title(&session_id, &title).map_err(|e| {
            if e.to_string().contains("not found") {
                ApiError::not_found(format!("Chat session {} not found", session_id))
            } else {
                ApiError::internal(format!("Failed to update chat session: {}", e))
            }
        })?;

        info!(session_id = %session_id, title = %title, "Updated chat session title");
        Ok(Json(session))
    } else {
        // No updates requested - just return the current session
        crate::history::get_session(&session_id)
            .map_err(|e| ApiError::internal(format!("Failed to get chat session: {}", e)))?
            .map(Json)
            .ok_or_else(|| ApiError::not_found(format!("Chat session {} not found", session_id)))
    }
}

/// Response for grouped chat history.
#[derive(Debug, Serialize)]
pub struct GroupedChatHistory {
    pub workspace_path: String,
    pub project_name: String,
    pub sessions: Vec<ChatSession>,
}

/// List chat sessions grouped by workspace/project.
#[instrument(skip(_state))]
pub async fn list_chat_history_grouped(
    State(_state): State<AppState>,
    Query(query): Query<ChatHistoryQuery>,
) -> ApiResult<Json<Vec<GroupedChatHistory>>> {
    let grouped = crate::history::list_sessions_grouped()
        .map_err(|e| ApiError::internal(format!("Failed to list chat history: {}", e)))?;

    let mut result: Vec<GroupedChatHistory> = grouped
        .into_iter()
        .map(|(workspace_path, mut sessions)| {
            // Filter out child sessions unless explicitly included
            if !query.include_children {
                sessions.retain(|s| !s.is_child);
            }

            // Apply limit per workspace
            if let Some(limit) = query.limit {
                sessions.truncate(limit);
            }

            let project_name = sessions
                .first()
                .map(|s| s.project_name.clone())
                .unwrap_or_else(|| crate::history::project_name_from_path(&workspace_path));

            GroupedChatHistory {
                workspace_path,
                project_name,
                sessions,
            }
        })
        .filter(|g| !g.sessions.is_empty())
        .collect();

    // Sort by most recently updated session in each group
    result.sort_by(|a, b| {
        let a_updated = a.sessions.first().map(|s| s.updated_at).unwrap_or(0);
        let b_updated = b.sessions.first().map(|s| s.updated_at).unwrap_or(0);
        b_updated.cmp(&a_updated)
    });

    info!(count = result.len(), "Listed grouped chat history");
    Ok(Json(result))
}

use crate::history::ChatMessage;

/// Query parameters for chat messages endpoint.
#[derive(Debug, Deserialize)]
pub struct ChatMessagesQuery {
    /// If true, include pre-rendered HTML for text parts (slower but saves client CPU)
    #[serde(default)]
    pub render: bool,
}

/// Get all messages for a chat session.
///
/// This reads messages and their parts directly from OpenCode's storage on disk.
/// Uses async I/O with caching for better performance on large sessions.
///
/// Query params:
/// - `render=true`: Include pre-rendered markdown HTML in `text_html` field
#[instrument]
pub async fn get_chat_messages(
    Path(session_id): Path<String>,
    Query(query): Query<ChatMessagesQuery>,
) -> ApiResult<Json<Vec<ChatMessage>>> {
    let messages = if query.render {
        crate::history::get_session_messages_rendered(&session_id).await
    } else {
        crate::history::get_session_messages_async(&session_id).await
    }
    .map_err(|e| ApiError::internal(format!("Failed to get chat messages: {}", e)))?;

    info!(session_id = %session_id, count = messages.len(), render = query.render, "Listed chat messages");
    Ok(Json(messages))
}

// ============================================================================
// AgentRPC Handlers (new unified backend API)
// ============================================================================

use crate::agent_rpc::{
    self, Conversation as RpcConversation, HealthStatus as RpcHealthStatus, Message as RpcMessage,
    SendMessagePart, SessionHandle,
};

/// Request to start a new agent session.
#[derive(Debug, Deserialize)]
pub struct StartAgentSessionRequest {
    /// Working directory for the session
    pub workdir: String,
    /// Model to use (optional)
    pub model: Option<String>,
    /// Agent/mode to use (optional, passed to opencode via --agent flag)
    pub agent: Option<String>,
    /// Session ID to resume (optional)
    pub resume_session_id: Option<String>,
    /// Project ID for shared project sessions (optional)
    pub project_id: Option<String>,
}

/// Request to send a message to an agent session.
#[derive(Debug, Deserialize)]
pub struct SendAgentMessageRequest {
    /// Message text
    pub text: Option<String>,
    /// Structured message parts
    pub parts: Option<Vec<SendMessagePart>>,
    /// Optional file part
    pub file: Option<SendAgentFilePart>,
    /// Optional agent mention part
    pub agent: Option<SendAgentAgentPart>,
    /// Model override (optional)
    pub model: Option<agent_rpc::MessageModel>,
}

/// File part for agent messages.
#[derive(Debug, Deserialize)]
pub struct SendAgentFilePart {
    pub mime: String,
    pub url: String,
    pub filename: Option<String>,
}

/// Agent part for agent messages.
#[derive(Debug, Deserialize)]
pub struct SendAgentAgentPart {
    pub name: String,
    pub id: Option<String>,
}

/// List conversations via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_list_conversations(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<RpcConversation>>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let conversations = backend
        .list_conversations(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list conversations: {}", e)))?;

    info!(user_id = %user.id(), count = conversations.len(), "Listed agent conversations");
    Ok(Json(conversations))
}

/// Get a specific conversation via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_get_conversation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(conversation_id): Path<String>,
) -> ApiResult<Json<RpcConversation>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let conversation = backend
        .get_conversation(user.id(), &conversation_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get conversation: {}", e)))?
        .ok_or_else(|| {
            ApiError::not_found(format!("Conversation {} not found", conversation_id))
        })?;

    Ok(Json(conversation))
}

/// Get messages for a conversation via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_get_messages(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(conversation_id): Path<String>,
) -> ApiResult<Json<Vec<RpcMessage>>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let messages = backend
        .get_messages(user.id(), &conversation_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

    info!(user_id = %user.id(), conversation_id = %conversation_id, count = messages.len(), "Listed agent messages");
    Ok(Json(messages))
}

/// Start a new agent session via AgentBackend.
#[instrument(skip(state, user, request))]
pub async fn agent_start_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<StartAgentSessionRequest>,
) -> ApiResult<(StatusCode, Json<SessionHandle>)> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let opts = agent_rpc::StartSessionOpts {
        model: request.model,
        agent: request.agent,
        resume_session_id: request.resume_session_id,
        project_id: request.project_id,
        env: std::collections::HashMap::new(),
    };

    let workdir = std::path::Path::new(&request.workdir);
    let handle = backend
        .start_session(user.id(), workdir, opts)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start session: {}", e)))?;

    info!(user_id = %user.id(), session_id = %handle.session_id, "Started agent session");
    Ok((StatusCode::CREATED, Json(handle)))
}

/// Send a message to an agent session via AgentBackend.
#[instrument(skip(state, user, request))]
pub async fn agent_send_message(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Json(request): Json<SendAgentMessageRequest>,
) -> ApiResult<StatusCode> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let mut parts = request.parts.unwrap_or_default();
    if let Some(file) = request.file {
        parts.push(SendMessagePart::File {
            mime: file.mime,
            url: file.url,
            filename: file.filename,
        });
    }
    if let Some(agent) = request.agent {
        parts.push(SendMessagePart::Agent {
            name: agent.name,
            id: agent.id,
        });
    }
    if let Some(text) = request.text {
        parts.push(SendMessagePart::Text { text });
    }
    if parts.is_empty() {
        return Err(ApiError::bad_request("message must include text or parts"));
    }

    let send_request = agent_rpc::SendMessageRequest {
        parts,
        model: request.model,
    };

    backend
        .send_message(user.id(), &session_id, send_request)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to send message: {}", e)))?;

    info!(user_id = %user.id(), session_id = %session_id, "Sent message to agent session");
    Ok(StatusCode::ACCEPTED)
}

/// Stop an agent session via AgentBackend.
#[instrument(skip(state, user))]
pub async fn agent_stop_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    backend
        .stop_session(user.id(), &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to stop session: {}", e)))?;

    info!(user_id = %user.id(), session_id = %session_id, "Stopped agent session");
    Ok(StatusCode::NO_CONTENT)
}

/// Get the session URL for an agent session.
#[instrument(skip(state, user))]
pub async fn agent_get_session_url(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<Json<SessionUrlResponse>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let url = backend
        .get_session_url(user.id(), &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get session URL: {}", e)))?;

    Ok(Json(SessionUrlResponse { session_id, url }))
}

/// Response for session URL query.
#[derive(Debug, Serialize)]
pub struct SessionUrlResponse {
    pub session_id: String,
    pub url: Option<String>,
}

/// Health check for the AgentRPC backend.
#[instrument(skip(state))]
pub async fn agent_health(State(state): State<AppState>) -> ApiResult<Json<RpcHealthStatus>> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let health = backend
        .health()
        .await
        .map_err(|e| ApiError::internal(format!("Health check failed: {}", e)))?;

    Ok(Json(health))
}

/// Attach to a session's event stream via AgentBackend.
///
/// Returns an SSE stream of agent events (messages, tool calls, etc.).
#[instrument(skip(state, user))]
pub async fn agent_attach(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let backend = state
        .agent_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("AgentRPC backend not enabled"))?;

    let event_stream = backend
        .attach(user.id(), &session_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to attach to session: {}", e)))?;

    // Convert AgentEvent stream to SSE Event stream
    let sse_stream = tokio_stream::StreamExt::map(event_stream, |result| {
        match result {
            Ok(event) => {
                // Serialize the event to JSON
                match serde_json::to_string(&event) {
                    Ok(json) => Ok(Event::default().data(json)),
                    Err(e) => {
                        warn!("Failed to serialize agent event: {}", e);
                        Ok(Event::default().data(format!(r#"{{"error":"{}"}}"#, e)))
                    }
                }
            }
            Err(e) => {
                warn!("Error in agent event stream: {}", e);
                Ok(Event::default().data(format!(r#"{{"error":"{}"}}"#, e)))
            }
        }
    });

    info!(user_id = %user.id(), session_id = %session_id, "Attached to agent session event stream");
    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

// ============================================================================
// Agent Ask Handler
// ============================================================================

/// Query parameters for session search.
#[derive(Debug, Deserialize)]
pub struct AgentSessionsQuery {
    /// Search query (fuzzy matches on ID and title)
    #[serde(default)]
    pub q: Option<String>,
    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

/// Search for sessions matching a query.
/// 
/// GET /api/agents/sessions?q=query&limit=20
#[instrument(skip(state, user))]
pub async fn agents_search_sessions(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<AgentSessionsQuery>,
) -> ApiResult<Json<Vec<SessionMatch>>> {
    let pi_service = state
        .main_chat_pi
        .as_ref()
        .ok_or_else(|| ApiError::internal("Main Chat Pi service not enabled"))?;

    let sessions = if let Some(q) = &query.q {
        pi_service
            .search_sessions(user.id(), q)
            .map_err(|e| ApiError::internal(format!("Failed to search sessions: {}", e)))?
    } else {
        pi_service
            .list_sessions(user.id())
            .map_err(|e| ApiError::internal(format!("Failed to list sessions: {}", e)))?
    };

    let matches: Vec<SessionMatch> = sessions
        .into_iter()
        .take(query.limit)
        .map(|s| SessionMatch {
            id: s.id,
            title: s.title,
            modified_at: s.modified_at,
        })
        .collect();

    Ok(Json(matches))
}

/// Request body for asking an agent a question.
#[derive(Debug, Deserialize)]
pub struct AgentAskRequest {
    /// Target agent: "main-chat", "session:<id>", or workspace path
    pub target: String,
    /// The question/prompt to send
    pub question: String,
    /// Timeout in seconds (default: 300)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Whether to stream the response
    #[serde(default)]
    pub stream: bool,
}

fn default_timeout() -> u64 {
    300
}

/// Response for non-streaming agent ask.
#[derive(Debug, Serialize)]
pub struct AgentAskResponse {
    pub response: String,
    pub session_id: Option<String>,
}

/// Response when multiple sessions match a query.
#[derive(Debug, Serialize)]
pub struct AgentAskAmbiguousResponse {
    pub error: String,
    pub matches: Vec<SessionMatch>,
}

/// A matching session for disambiguation.
#[derive(Debug, Serialize)]
pub struct SessionMatch {
    pub id: String,
    pub title: Option<String>,
    pub modified_at: i64,
}

/// Parsed target for agent ask.
#[derive(Debug)]
enum AskTarget {
    /// Main chat, optionally with session query
    MainChat { session_query: Option<String> },
    /// Specific Pi session by exact ID
    Session { id: String },
    /// OpenCode session by ID (for chat history sessions)
    OpenCodeSession {
        id: String,
        workspace_path: Option<String>,
    },
}

/// Parse an ask target string into structured form.
/// 
/// Supported formats:
/// - "main", "main-chat", "pi" -> MainChat
/// - "main:query", "pi:query" -> MainChat with session search
/// - "session:id" -> Specific Pi session
/// - "opencode:id" or "opencode:id:workspace_path" -> OpenCode session
/// - Custom assistant name (checked against main chat config)
fn parse_ask_target(target: &str, assistant_name: Option<&str>) -> Result<AskTarget, String> {
    // Check for main chat aliases
    let main_aliases = ["main", "main-chat", "pi"];
    
    // Split on ':' for arguments
    let parts: Vec<&str> = target.splitn(3, ':').collect();
    let base = parts.first().map(|s| *s).unwrap_or("");
    let base_lower = base.to_lowercase();
    
    // Check main chat aliases
    if main_aliases.contains(&base_lower.as_str()) {
        return Ok(AskTarget::MainChat {
            session_query: parts.get(1).map(|s| s.to_string()),
        });
    }
    
    // Check custom assistant name
    if let Some(name) = assistant_name {
        if base_lower == name.to_lowercase() {
            return Ok(AskTarget::MainChat {
                session_query: parts.get(1).map(|s| s.to_string()),
            });
        }
    }
    
    // Check for explicit session: prefix (Pi sessions)
    if base_lower == "session" {
        if let Some(id) = parts.get(1) {
            return Ok(AskTarget::Session { id: id.to_string() });
        } else {
            return Err("session: requires a session ID".to_string());
        }
    }
    
    // Check for opencode: prefix (OpenCode/chat history sessions)
    if base_lower == "opencode" {
        if let Some(id) = parts.get(1) {
            let workspace_path = parts.get(2).map(|s| s.to_string());
            return Ok(AskTarget::OpenCodeSession {
                id: id.to_string(),
                workspace_path,
            });
        } else {
            return Err("opencode: requires a session ID".to_string());
        }
    }
    
    // Could be a direct session ID (for backwards compat)
    // ses_ prefix indicates OpenCode session, others are Pi sessions
    if target.starts_with("ses_") {
        return Ok(AskTarget::OpenCodeSession {
            id: target.to_string(),
            workspace_path: None,
        });
    }
    
    if target.contains('-') {
        return Ok(AskTarget::Session { id: target.to_string() });
    }
    
    Err(format!(
        "Unknown target: {}. Use 'main', 'pi', 'session:<id>', or 'opencode:<id>'",
        target
    ))
}

/// Ask an agent a question and get the response.
///
/// Supports two modes:
/// - Non-streaming: Returns complete response after agent finishes
/// - Streaming: Returns SSE stream of events as they happen
///
/// Target formats:
/// - "main", "main-chat", "pi" - Main chat, active session
/// - "main:query", "pi:query" - Main chat, fuzzy search for session
/// - "<assistant_name>" - Alias for main (e.g., "jarvis")
/// - "session:<id>" - Specific Pi session by ID
/// - "opencode:<id>" - OpenCode chat history session
#[instrument(skip(state, user))]
pub async fn agents_ask(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<AgentAskRequest>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;

    info!(
        user_id = %user.id(),
        target = %req.target,
        question_len = req.question.len(),
        stream = req.stream,
        "Agent ask request"
    );

    // Get assistant name for alias matching
    let assistant_name = if let Some(mc) = state.main_chat.as_ref() {
        mc.get_main_chat_info(user.id())
            .await
            .ok()
            .map(|info| info.name)
    } else {
        None
    };

    // Parse the target
    let parsed_target = parse_ask_target(&req.target, assistant_name.as_deref())
        .map_err(ApiError::bad_request)?;

    // Handle OpenCode sessions differently from Pi sessions
    if let AskTarget::OpenCodeSession { id, workspace_path } = parsed_target {
        return handle_opencode_ask(&state, &req, &id, workspace_path.as_deref()).await;
    }

    // Get the Pi service for Pi-based targets
    let pi_service = state
        .main_chat_pi
        .as_ref()
        .ok_or_else(|| ApiError::internal("Main Chat Pi service not enabled"))?;

    // Resolve to a Pi session
    let session = match parsed_target {
        AskTarget::MainChat { session_query: None } => {
            // Get active session or create new
            pi_service
                .get_or_create_session(user.id())
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get session: {}", e)))?
        }
        AskTarget::MainChat { session_query: Some(query) } => {
            // Search for matching sessions
            let matches = pi_service
                .search_sessions(user.id(), &query)
                .map_err(|e| ApiError::internal(format!("Failed to search sessions: {}", e)))?;

            if matches.is_empty() {
                return Err(ApiError::not_found(format!(
                    "No sessions found matching '{}'",
                    query
                )));
            }

            if matches.len() > 1 {
                // Check if first match is significantly better than second
                // (We'd need scores for this - for now just check exact match)
                let first = &matches[0];
                let is_exact = first.id.to_lowercase() == query.to_lowercase()
                    || first.title.as_ref().map(|t| t.to_lowercase()) == Some(query.to_lowercase());

                if !is_exact {
                    // Ambiguous - return matches for user to choose
                    let response = AgentAskAmbiguousResponse {
                        error: format!("Multiple sessions match '{}'. Please be more specific.", query),
                        matches: matches
                            .into_iter()
                            .take(10)
                            .map(|s| SessionMatch {
                                id: s.id,
                                title: s.title,
                                modified_at: s.modified_at,
                            })
                            .collect(),
                    };
                    return Ok(Json(response).into_response());
                }
            }

            // Use first (best) match
            let session_id = &matches[0].id;
            pi_service
                .resume_session(user.id(), session_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to resume session: {}", e)))?
        }
        AskTarget::Session { id } => {
            // Try exact ID first, then fuzzy search
            match pi_service.resume_session(user.id(), &id).await {
                Ok(session) => session,
                Err(_) => {
                    // Try fuzzy search
                    let matches = pi_service
                        .search_sessions(user.id(), &id)
                        .map_err(|e| ApiError::internal(format!("Failed to search sessions: {}", e)))?;

                    if matches.is_empty() {
                        return Err(ApiError::not_found(format!("Session not found: {}", id)));
                    }

                    if matches.len() > 1 {
                        let response = AgentAskAmbiguousResponse {
                            error: format!("Multiple sessions match '{}'. Please be more specific.", id),
                            matches: matches
                                .into_iter()
                                .take(10)
                                .map(|s| SessionMatch {
                                    id: s.id,
                                    title: s.title,
                                    modified_at: s.modified_at,
                                })
                                .collect(),
                        };
                        return Ok(Json(response).into_response());
                    }

                    pi_service
                        .resume_session(user.id(), &matches[0].id)
                        .await
                        .map_err(|e| ApiError::internal(format!("Failed to resume session: {}", e)))?
                }
            }
        }
        AskTarget::OpenCodeSession { .. } => {
            // Already handled above, this is unreachable
            unreachable!("OpenCodeSession should be handled before this match")
        }
    };

    if req.stream {
        // Streaming mode - return SSE
        use crate::pi::{AssistantMessageEvent, PiEvent};
        use tokio::sync::mpsc;

        let mut event_rx = session.subscribe().await;
        let session_for_prompt = session.clone();
        let question = req.question.clone();
        let timeout_secs = req.timeout_secs;

        // Create a channel to produce SSE events
        let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

        // Spawn task to handle Pi events and send SSE events
        tokio::spawn(async move {
            // Send the prompt
            if let Err(e) = session_for_prompt.prompt(&question).await {
                let json = serde_json::json!({
                    "type": "error",
                    "error": format!("Failed to send prompt: {}", e)
                });
                let _ = tx.send(Ok(Event::default().data(json.to_string()))).await;
                return;
            }

            let mut text_buffer = String::new();

            loop {
                match tokio::time::timeout(
                    Duration::from_secs(timeout_secs),
                    event_rx.recv(),
                )
                .await
                {
                    Ok(Ok(event)) => {
                        match &event {
                            PiEvent::MessageUpdate {
                                assistant_message_event,
                                ..
                            } => match assistant_message_event {
                                AssistantMessageEvent::TextDelta { delta, .. } => {
                                    text_buffer.push_str(delta);
                                    let json = serde_json::json!({
                                        "type": "text",
                                        "data": delta
                                    });
                                    if tx.send(Ok(Event::default().data(json.to_string()))).await.is_err() {
                                        return; // Client disconnected
                                    }
                                }
                                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                                    let json = serde_json::json!({
                                        "type": "thinking",
                                        "data": delta
                                    });
                                    if tx.send(Ok(Event::default().data(json.to_string()))).await.is_err() {
                                        return;
                                    }
                                }
                                _ => {}
                            },
                            PiEvent::AgentEnd { .. } => {
                                let json = serde_json::json!({
                                    "type": "done",
                                    "response": text_buffer
                                });
                                let _ = tx.send(Ok(Event::default().data(json.to_string()))).await;
                                return;
                            }
                            _ => {}
                        }
                    }
                    Ok(Err(_)) => {
                        // Channel closed
                        return;
                    }
                    Err(_) => {
                        // Timeout
                        let json = serde_json::json!({
                            "type": "error",
                            "error": "Timeout waiting for response"
                        });
                        let _ = tx.send(Ok(Event::default().data(json.to_string()))).await;
                        return;
                    }
                }
            }
        });

        // Convert receiver to stream
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);

        Ok(Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response())
    } else {
        // Non-streaming mode - wait for complete response
        use crate::pi::PiEvent;

        let mut event_rx = session.subscribe().await;

        // Send the prompt
        session
            .prompt(&req.question)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to send prompt: {}", e)))?;

        // Collect response
        let mut response_text = String::new();
        let timeout = Duration::from_secs(req.timeout_secs);
        let start = Instant::now();

        loop {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(ApiError::internal("Timeout waiting for agent response"));
            }

            match tokio::time::timeout(remaining, event_rx.recv()).await {
                Ok(Ok(event)) => {
                    match event {
                        PiEvent::MessageUpdate { assistant_message_event, .. } => {
                            use crate::pi::AssistantMessageEvent;
                            if let AssistantMessageEvent::TextDelta { delta, .. } = assistant_message_event {
                                response_text.push_str(&delta);
                            }
                        }
                        PiEvent::AgentEnd { .. } => {
                            break;
                        }
                        _ => {}
                    }
                }
                Ok(Err(_)) => {
                    // Channel closed unexpectedly
                    break;
                }
                Err(_) => {
                    return Err(ApiError::internal("Timeout waiting for agent response"));
                }
            }
        }

        let pi_state = session.get_state().await.ok();
        let session_id = pi_state.and_then(|s| s.session_id);

        Ok(Json(AgentAskResponse {
            response: response_text,
            session_id,
        })
        .into_response())
    }
}

/// Handle asking an OpenCode session (chat history session).
///
/// This sends a message to the OpenCode HTTP server and waits for the response
/// by subscribing to the SSE event stream.
async fn handle_opencode_ask(
    state: &AppState,
    req: &AgentAskRequest,
    session_id: &str,
    provided_workspace_path: Option<&str>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    use reqwest_eventsource::{Event as SseEvent, EventSource};

    // Get workspace path from provided value or look up from chat history
    let workspace_path = if let Some(path) = provided_workspace_path {
        path.to_string()
    } else {
        // Look up session in chat history to get workspace path
        let chat_session = crate::history::get_session(session_id)
            .map_err(|e| ApiError::internal(format!("Failed to lookup session: {}", e)))?
            .ok_or_else(|| ApiError::not_found(format!("Session not found: {}", session_id)))?;
        chat_session.workspace_path
    };

    // Get or create the OpenCode runtime session
    let opencode_session = state
        .sessions
        .get_or_create_opencode_session()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get OpenCode session: {}", e)))?;

    let opencode_port = opencode_session.opencode_port as u16;

    // Build the prompt request
    let prompt_url = format!(
        "http://localhost:{}/session/{}/prompt_async",
        opencode_port, session_id
    );
    let request_body = serde_json::json!({
        "parts": [{"type": "text", "text": &req.question}]
    });

    let client = reqwest::Client::new();

    if req.stream {
        // Streaming mode - subscribe to events and forward them
        use tokio::sync::mpsc;

        let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

        let session_id_owned = session_id.to_string();
        let workspace_path_owned = workspace_path.clone();
        let timeout_secs = req.timeout_secs;

        tokio::spawn(async move {
            // First, connect to the event stream using EventSource
            let event_url = format!("http://localhost:{}/event", opencode_port);
            let request_builder = client
                .get(&event_url)
                .header("Accept", "text/event-stream");
            
            let mut es = match EventSource::new(request_builder) {
                Ok(es) => es,
                Err(e) => {
                    let json = serde_json::json!({
                        "type": "error",
                        "error": format!("Failed to connect to event stream: {}", e)
                    });
                    let _ = tx.send(Ok(Event::default().data(json.to_string()))).await;
                    return;
                }
            };

            // Send the prompt
            let prompt_response = client
                .post(&prompt_url)
                .header("x-opencode-directory", &workspace_path_owned)
                .json(&request_body)
                .send()
                .await;

            if let Err(e) = prompt_response {
                let json = serde_json::json!({
                    "type": "error",
                    "error": format!("Failed to send prompt: {}", e)
                });
                let _ = tx.send(Ok(Event::default().data(json.to_string()))).await;
                return;
            }

            // Process event stream
            let mut text_buffer = String::new();
            let start = Instant::now();

            while let Some(event_result) = futures::StreamExt::next(&mut es).await {
                if start.elapsed() > Duration::from_secs(timeout_secs) {
                    let json = serde_json::json!({
                        "type": "error",
                        "error": "Timeout waiting for response"
                    });
                    let _ = tx.send(Ok(Event::default().data(json.to_string()))).await;
                    return;
                }

                match event_result {
                    Ok(SseEvent::Open) => {}
                    Ok(SseEvent::Message(msg)) => {
                        // Parse the event data
                        if let Ok(event_json) = serde_json::from_str::<serde_json::Value>(&msg.data) {
                            let event_session = event_json
                                .get("properties")
                                .and_then(|p| p.get("sessionID"))
                                .and_then(|s| s.as_str());

                            if event_session != Some(&session_id_owned) {
                                continue; // Skip events from other sessions
                            }

                            let event_type = event_json.get("type").and_then(|t| t.as_str());

                            match event_type {
                                Some("message.part.delta") => {
                                    if let Some(content) = event_json
                                        .get("properties")
                                        .and_then(|p| p.get("content"))
                                        .and_then(|c| c.as_str())
                                    {
                                        text_buffer.push_str(content);
                                        let json = serde_json::json!({
                                            "type": "text",
                                            "data": content
                                        });
                                        if tx.send(Ok(Event::default().data(json.to_string()))).await.is_err() {
                                            return; // Client disconnected
                                        }
                                    }
                                }
                                Some("message.completed") | Some("session.completed") => {
                                    let json = serde_json::json!({
                                        "type": "done",
                                        "response": text_buffer
                                    });
                                    let _ = tx.send(Ok(Event::default().data(json.to_string()))).await;
                                    return;
                                }
                                Some("message.error") | Some("session.error") => {
                                    let error_msg = event_json
                                        .get("properties")
                                        .and_then(|p| p.get("error"))
                                        .and_then(|e| e.as_str())
                                        .unwrap_or("Unknown error");
                                    let json = serde_json::json!({
                                        "type": "error",
                                        "error": error_msg
                                    });
                                    let _ = tx.send(Ok(Event::default().data(json.to_string()))).await;
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        let json = serde_json::json!({
                            "type": "error",
                            "error": format!("Stream error: {:?}", e)
                        });
                        let _ = tx.send(Ok(Event::default().data(json.to_string()))).await;
                        return;
                    }
                }
            }
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response())
    } else {
        // Non-streaming mode - send prompt and collect full response using EventSource
        let event_url = format!("http://localhost:{}/event", opencode_port);
        let request_builder = client
            .get(&event_url)
            .header("Accept", "text/event-stream");
        
        let mut es = EventSource::new(request_builder)
            .map_err(|e| ApiError::internal(format!("Failed to connect to event stream: {}", e)))?;

        // Send the prompt
        let prompt_response = client
            .post(&prompt_url)
            .header("x-opencode-directory", &workspace_path)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to send prompt: {}", e)))?;

        if !prompt_response.status().is_success() {
            let status = prompt_response.status();
            let body = prompt_response.text().await.unwrap_or_default();
            return Err(ApiError::internal(format!(
                "OpenCode returned {}: {}",
                status, body
            )));
        }

        // Process event stream until completion
        let mut response_text = String::new();
        let timeout = Duration::from_secs(req.timeout_secs);
        let start = Instant::now();

        while let Some(event_result) = futures::StreamExt::next(&mut es).await {
            if start.elapsed() > timeout {
                return Err(ApiError::internal("Timeout waiting for agent response"));
            }

            match event_result {
                Ok(SseEvent::Open) => {}
                Ok(SseEvent::Message(msg)) => {
                    if let Ok(event_json) = serde_json::from_str::<serde_json::Value>(&msg.data) {
                        // Check if this event is for our session
                        let event_session = event_json
                            .get("properties")
                            .and_then(|p| p.get("sessionID"))
                            .and_then(|s| s.as_str());

                        if event_session != Some(session_id) {
                            continue;
                        }

                        let event_type = event_json.get("type").and_then(|t| t.as_str());

                        match event_type {
                            Some("message.part.delta") => {
                                if let Some(content) = event_json
                                    .get("properties")
                                    .and_then(|p| p.get("content"))
                                    .and_then(|c| c.as_str())
                                {
                                    response_text.push_str(content);
                                }
                            }
                            Some("message.completed") | Some("session.completed") => {
                                return Ok(Json(AgentAskResponse {
                                    response: response_text,
                                    session_id: Some(session_id.to_string()),
                                })
                                .into_response());
                            }
                            Some("message.error") | Some("session.error") => {
                                let error_msg = event_json
                                    .get("properties")
                                    .and_then(|p| p.get("error"))
                                    .and_then(|e| e.as_str())
                                    .unwrap_or("Unknown error");
                                return Err(ApiError::internal(error_msg.to_string()));
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    return Err(ApiError::internal(format!("Stream error: {:?}", e)));
                }
            }
        }

        // Stream ended - return what we have
        Ok(Json(AgentAskResponse {
            response: response_text,
            session_id: Some(session_id.to_string()),
        })
        .into_response())
    }
}

// ============================================================================
// In-Session Search Handler
// ============================================================================

/// Query parameters for in-session search.
#[derive(Debug, Deserialize)]
pub struct InSessionSearchQuery {
    /// Search query
    pub q: String,
    /// Maximum number of results
    #[serde(default = "default_cass_limit")]
    pub limit: usize,
}

fn default_cass_limit() -> usize {
    20
}

/// Search result from CASS.
#[derive(Debug, Serialize)]
pub struct InSessionSearchResult {
    /// Line number in the source file
    pub line_number: usize,
    /// Match score
    pub score: f64,
    /// Short snippet around the match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Session title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Match type (exact, fuzzy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_type: Option<String>,
    /// Timestamp when the message was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

/// Search within a specific Pi session using CASS.
/// 
/// GET /api/agents/sessions/{session_id}/search?q=query&limit=20
#[instrument(skip(state, user))]
pub async fn agents_session_search(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<InSessionSearchQuery>,
) -> ApiResult<Json<Vec<InSessionSearchResult>>> {
    let pi_service = state
        .main_chat_pi
        .as_ref()
        .ok_or_else(|| ApiError::internal("Main Chat Pi service not enabled"))?;

    let results = pi_service
        .search_in_session(user.id(), &session_id, &query.q, query.limit)
        .await
        .map_err(|e| ApiError::internal(format!("Search failed: {}", e)))?;

    let response: Vec<InSessionSearchResult> = results
        .into_iter()
        .map(|r| InSessionSearchResult {
            line_number: r.line_number,
            score: r.score,
            snippet: r.snippet,
            title: r.title,
            match_type: r.match_type,
            created_at: r.created_at,
        })
        .collect();

    Ok(Json(response))
}

// ============================================================================
// Settings Handlers
// ============================================================================

use crate::settings::{ConfigUpdate, SettingsScope, SettingsService, SettingsValue};
use std::collections::HashMap;

/// Query parameters for settings endpoints.
#[derive(Debug, Deserialize)]
pub struct SettingsQuery {
    /// App to get settings for (e.g., "octo", "mmry")
    pub app: String,
}

/// Get the settings schema for an app, filtered by user permissions.
#[instrument(skip(state, user))]
pub async fn get_settings_schema(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SettingsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = get_settings_service(&state, &query.app)?;
    let scope = user_to_scope(&user);

    let schema = service.get_schema(scope);

    info!(user_id = %user.id(), app = %query.app, scope = ?scope, "Retrieved settings schema");
    Ok(Json(schema))
}

/// Get current settings values for an app.
#[instrument(skip(state, user))]
pub async fn get_settings_values(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SettingsQuery>,
) -> ApiResult<Json<HashMap<String, SettingsValue>>> {
    let service = get_settings_service(&state, &query.app)?;
    let scope = user_to_scope(&user);

    let values = service.get_values(scope).await;

    info!(user_id = %user.id(), app = %query.app, count = values.len(), "Retrieved settings values");
    Ok(Json(values))
}

/// Update settings values for an app.
#[instrument(skip(state, user))]
pub async fn update_settings_values(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SettingsQuery>,
    Json(updates): Json<ConfigUpdate>,
) -> ApiResult<Json<HashMap<String, SettingsValue>>> {
    let service = get_settings_service(&state, &query.app)?;
    let scope = user_to_scope(&user);

    service
        .update_values(updates, scope)
        .await
        .map_err(|e| ApiError::bad_request(format!("Failed to update settings: {}", e)))?;

    // Return updated values
    let values = service.get_values(scope).await;

    info!(user_id = %user.id(), app = %query.app, "Updated settings");
    Ok(Json(values))
}

/// Reload settings from disk (admin only).
#[instrument(skip(state, _admin))]
pub async fn reload_settings(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Query(query): Query<SettingsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = get_settings_service(&state, &query.app)?;

    service
        .reload()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to reload settings: {}", e)))?;

    info!(app = %query.app, "Settings reloaded");
    Ok(Json(serde_json::json!({ "status": "reloaded" })))
}

/// Convert user role to settings scope.
fn user_to_scope(user: &CurrentUser) -> SettingsScope {
    if user.is_admin() {
        SettingsScope::Admin
    } else {
        SettingsScope::User
    }
}

// ============================================================================
// OpenCode Global Config
// ============================================================================

/// Get the global opencode.json config for the current user.
///
/// Returns the contents of ~/.config/opencode/opencode.json
/// In local mode, this is the server user's config.
/// In container mode, this would be per-user (not yet implemented).
#[instrument(skip(_user))]
pub async fn get_global_opencode_config(_user: CurrentUser) -> ApiResult<Json<serde_json::Value>> {
    // Get the config directory path
    let config_path = get_global_opencode_config_path();

    // Read and parse the config file
    match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => {
            // Parse as JSON - strip comments first since opencode.json supports JSONC
            let stripped = strip_json_comments(&content);
            let config: serde_json::Value = serde_json::from_str(&stripped)
                .map_err(|e| ApiError::internal(format!("Failed to parse opencode.json: {}", e)))?;
            Ok(Json(config))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Return empty object if file doesn't exist
            Ok(Json(serde_json::json!({})))
        }
        Err(e) => Err(ApiError::internal(format!(
            "Failed to read opencode.json: {}",
            e
        ))),
    }
}

/// Get the path to the global opencode.json config file.
fn get_global_opencode_config_path() -> std::path::PathBuf {
    // Default: ~/.config/opencode/opencode.json
    if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("opencode").join("opencode.json")
    } else if let Some(home) = dirs::home_dir() {
        home.join(".config").join("opencode").join("opencode.json")
    } else {
        // Fallback
        std::path::PathBuf::from("/etc/opencode/opencode.json")
    }
}

/// Strip single-line (//) and multi-line (/* */) comments from JSON content,
/// and remove trailing commas. This allows parsing JSONC (JSON with comments) files.
fn strip_json_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(c) = chars.next() {
        if escape_next {
            result.push(c);
            escape_next = false;
            continue;
        }

        if in_string {
            result.push(c);
            if c == '\\' {
                escape_next = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                result.push(c);
            }
            '/' => {
                if let Some(&next) = chars.peek() {
                    match next {
                        '/' => {
                            // Single-line comment: skip until end of line
                            chars.next(); // consume the second '/'
                            while let Some(&ch) = chars.peek() {
                                if ch == '\n' {
                                    break;
                                }
                                chars.next();
                            }
                        }
                        '*' => {
                            // Multi-line comment: skip until */
                            chars.next(); // consume the '*'
                            while let Some(ch) = chars.next() {
                                if ch == '*' {
                                    if let Some(&'/') = chars.peek() {
                                        chars.next(); // consume the '/'
                                        break;
                                    }
                                }
                            }
                        }
                        _ => {
                            result.push(c);
                        }
                    }
                } else {
                    result.push(c);
                }
            }
            _ => {
                result.push(c);
            }
        }
    }

    result
}

/// Get the settings service for an app.
fn get_settings_service<'a>(state: &'a AppState, app: &str) -> ApiResult<&'a Arc<SettingsService>> {
    match app {
        "octo" => state.settings_octo.as_ref(),
        "mmry" => state.settings_mmry.as_ref(),
        _ => None,
    }
    .ok_or_else(|| ApiError::not_found(format!("Settings for app '{}' not found", app)))
}

// ============================================================================
// TRX (Issue Tracking) Handlers
// ============================================================================

/// Dependency as returned by trx CLI.
#[derive(Debug, Deserialize)]
struct TrxDependency {
    #[allow(dead_code)]
    issue_id: String,
    depends_on_id: String,
    #[serde(rename = "type")]
    dep_type: String,
    #[allow(dead_code)]
    created_at: String,
}

/// Raw TRX issue as returned by `trx list --json`.
#[derive(Debug, Deserialize)]
struct TrxIssueRaw {
    id: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    status: String,
    priority: i32,
    issue_type: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    closed_at: Option<String>,
    #[serde(default)]
    dependencies: Vec<TrxDependency>,
}

/// TRX issue as returned by API (transformed from raw).
#[derive(Debug, Serialize, Deserialize)]
pub struct TrxIssue {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: String,
    pub priority: i32,
    pub issue_type: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub closed_at: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

impl From<TrxIssueRaw> for TrxIssue {
    fn from(raw: TrxIssueRaw) -> Self {
        // Extract parent_id from dependencies with type "parent_child"
        // Also check "blocks" type where depends_on_id is a prefix of issue_id (hierarchical IDs like octo-k8z1.1 -> octo-k8z1)
        let parent_id = raw
            .dependencies
            .iter()
            .find(|d| {
                d.dep_type == "parent_child"
                    || (d.dep_type == "blocks"
                        && raw.id.starts_with(&format!("{}.", d.depends_on_id)))
            })
            .map(|d| d.depends_on_id.clone());

        // Extract blocked_by from dependencies with type "blocks"
        let blocked_by: Vec<String> = raw
            .dependencies
            .iter()
            .filter(|d| d.dep_type == "blocks")
            .map(|d| d.depends_on_id.clone())
            .collect();

        TrxIssue {
            id: raw.id,
            title: raw.title,
            description: raw.description,
            status: raw.status,
            priority: raw.priority,
            issue_type: raw.issue_type,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
            closed_at: raw.closed_at,
            parent_id,
            labels: Vec::new(),
            blocked_by,
        }
    }
}

/// Request body for creating a TRX issue.
#[derive(Debug, Deserialize)]
pub struct CreateTrxIssueRequest {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_issue_type")]
    pub issue_type: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub parent_id: Option<String>,
}

fn default_issue_type() -> String {
    "task".to_string()
}

fn default_priority() -> i32 {
    2
}

/// Request body for updating a TRX issue.
#[derive(Debug, Deserialize)]
pub struct UpdateTrxIssueRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
}

/// Request body for closing a TRX issue.
#[derive(Debug, Deserialize)]
pub struct CloseTrxIssueRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

/// Query parameters for workspace-based TRX routes.
#[derive(Debug, Deserialize)]
pub struct TrxWorkspaceQuery {
    pub workspace_path: String,
}

/// Validate and resolve a workspace path, ensuring it's within the allowed workspace root
/// or is a valid Main Chat workspace path.
fn validate_workspace_path(state: &AppState, workspace_path: &str) -> Result<PathBuf, ApiError> {
    let workspace_root = state.sessions.workspace_root();
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.clone());

    let requested = PathBuf::from(workspace_path);

    // Resolve the path - if relative, it's relative to workspace root
    let resolved = if requested.is_absolute() {
        requested.clone()
    } else {
        workspace_root.join(&requested)
    };

    // Canonicalize if it exists, otherwise verify parent is valid
    let canonical = if resolved.exists() {
        resolved
            .canonicalize()
            .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?
    } else {
        // Path doesn't exist yet - verify parent is valid
        if let Some(parent) = resolved.parent() {
            if parent.exists() {
                let canonical_parent = parent
                    .canonicalize()
                    .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?;
                if !canonical_parent.starts_with(&canonical_root) {
                    // Check if it's a Main Chat path before rejecting
                    if !is_main_chat_path(state, &canonical_parent) {
                        warn!(
                            "Workspace path parent outside root: {:?} (root: {:?})",
                            parent, canonical_root
                        );
                        return Err(ApiError::bad_request("Workspace path outside allowed root"));
                    }
                }
            }
        }
        resolved
    };

    // Verify the path is under the workspace root or is a Main Chat path
    if !canonical.starts_with(&canonical_root) && !is_main_chat_path(state, &canonical) {
        warn!(
            "Workspace path outside root: {:?} (root: {:?})",
            canonical, canonical_root
        );
        return Err(ApiError::bad_request("Workspace path outside allowed root"));
    }

    Ok(canonical)
}

/// Check if a path is within a Main Chat workspace directory.
fn is_main_chat_path(state: &AppState, path: &std::path::Path) -> bool {
    let Some(main_chat) = state.main_chat.as_ref() else {
        return false;
    };

    // Get the Main Chat workspace root (parent of individual user directories)
    // Main Chat paths are like: data_dir/users/{user_id}/...
    // The service's workspace_dir is data_dir/users
    let main_chat_root = main_chat.workspace_dir();
    let canonical_main_chat_root = main_chat_root
        .canonicalize()
        .unwrap_or_else(|_| main_chat_root.to_path_buf());

    path.starts_with(&canonical_main_chat_root)
}

/// Execute trx command in a validated workspace directory.
async fn exec_trx_command(
    state: &AppState,
    workspace_path: &str,
    args: &[&str],
) -> Result<String, ApiError> {
    use tokio::process::Command;

    // Validate workspace path before executing command
    let validated_path = validate_workspace_path(state, workspace_path)?;

    let output = Command::new("trx")
        .args(args)
        .arg("--json")
        .current_dir(&validated_path)
        .output()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to execute trx: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::internal(format!(
            "trx command failed: {}",
            stderr
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// List TRX issues for a workspace.
#[instrument(skip(state))]
pub async fn list_trx_issues(
    State(state): State<AppState>,
    Query(query): Query<TrxWorkspaceQuery>,
) -> ApiResult<Json<Vec<TrxIssue>>> {
    let output = exec_trx_command(&state, &query.workspace_path, &["list"]).await?;

    // Parse the raw JSON output and transform to API format
    let raw_issues: Vec<TrxIssueRaw> = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    let issues: Vec<TrxIssue> = raw_issues.into_iter().map(TrxIssue::from).collect();

    Ok(Json(issues))
}

/// Get a specific TRX issue.
#[instrument(skip(state))]
pub async fn get_trx_issue(
    State(state): State<AppState>,
    Path(issue_id): Path<String>,
    Query(query): Query<TrxWorkspaceQuery>,
) -> ApiResult<Json<TrxIssue>> {
    let output = exec_trx_command(&state, &query.workspace_path, &["show", &issue_id]).await?;

    let raw_issue: TrxIssueRaw = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    Ok(Json(TrxIssue::from(raw_issue)))
}

/// Create a new TRX issue.
#[instrument(skip(state, request))]
pub async fn create_trx_issue(
    State(state): State<AppState>,
    Query(query): Query<TrxWorkspaceQuery>,
    Json(request): Json<CreateTrxIssueRequest>,
) -> ApiResult<Json<TrxIssue>> {
    let mut args = vec!["create", &request.title, "-t", &request.issue_type, "-p"];
    let priority_str = request.priority.to_string();
    args.push(&priority_str);

    if let Some(ref desc) = request.description {
        args.push("-d");
        args.push(desc);
    }

    if let Some(ref parent) = request.parent_id {
        args.push("--parent");
        args.push(parent);
    }

    let output = exec_trx_command(&state, &query.workspace_path, &args).await?;

    // trx create --json returns the created issue
    let raw_issue: TrxIssueRaw = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    let issue = TrxIssue::from(raw_issue);
    info!(issue_id = %issue.id, "Created TRX issue");
    Ok(Json(issue))
}

/// Update a TRX issue.
#[instrument(skip(state, request))]
pub async fn update_trx_issue(
    State(state): State<AppState>,
    Path(issue_id): Path<String>,
    Query(query): Query<TrxWorkspaceQuery>,
    Json(request): Json<UpdateTrxIssueRequest>,
) -> ApiResult<Json<TrxIssue>> {
    let mut args = vec!["update", &issue_id];

    // Build args based on what's being updated
    let title_arg;
    if let Some(ref title) = request.title {
        args.push("--title");
        title_arg = title.clone();
        args.push(&title_arg);
    }

    let desc_arg;
    if let Some(ref desc) = request.description {
        args.push("--description");
        desc_arg = desc.clone();
        args.push(&desc_arg);
    }

    let status_arg;
    if let Some(ref status) = request.status {
        args.push("--status");
        status_arg = status.clone();
        args.push(&status_arg);
    }

    let priority_arg;
    if let Some(priority) = request.priority {
        args.push("-p");
        priority_arg = priority.to_string();
        args.push(&priority_arg);
    }

    let output = exec_trx_command(&state, &query.workspace_path, &args).await?;

    // Parse the updated issue (trx update --json returns a single issue object)
    let raw_issue: TrxIssueRaw = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    let issue = TrxIssue::from(raw_issue);

    info!(issue_id = %issue.id, "Updated TRX issue");
    Ok(Json(issue))
}

/// Close a TRX issue.
#[instrument(skip(state, request))]
pub async fn close_trx_issue(
    State(state): State<AppState>,
    Path(issue_id): Path<String>,
    Query(query): Query<TrxWorkspaceQuery>,
    Json(request): Json<CloseTrxIssueRequest>,
) -> ApiResult<Json<TrxIssue>> {
    let mut args = vec!["close", &issue_id];

    let reason_arg;
    if let Some(ref reason) = request.reason {
        args.push("-r");
        reason_arg = reason.clone();
        args.push(&reason_arg);
    }

    let output = exec_trx_command(&state, &query.workspace_path, &args).await?;

    // Parse the closed issue (trx close --json returns a single issue object)
    let raw_issue: TrxIssueRaw = serde_json::from_str(&output)
        .map_err(|e| ApiError::internal(format!("Failed to parse trx output: {}", e)))?;

    let issue = TrxIssue::from(raw_issue);

    info!(issue_id = %issue.id, "Closed TRX issue");
    Ok(Json(issue))
}

/// Sync TRX changes (git add and commit .trx/).
#[instrument(skip(state))]
pub async fn sync_trx(
    State(state): State<AppState>,
    Query(query): Query<TrxWorkspaceQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    // Validate workspace path before executing command
    let validated_path = validate_workspace_path(&state, &query.workspace_path)?;

    // Note: trx sync doesn't have JSON output, so we just check for success
    use tokio::process::Command;

    let output = Command::new("trx")
        .args(["sync"])
        .current_dir(&validated_path)
        .output()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to execute trx sync: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::internal(format!("trx sync failed: {}", stderr)));
    }

    info!("TRX synced");
    Ok(Json(serde_json::json!({ "synced": true })))
}

// ============================================================================
// CASS (Coding Agent Session Search) handlers
// ============================================================================

/// Query parameters for cass search.
#[derive(Debug, Deserialize)]
pub struct CassSearchQuery {
    /// Search query string.
    pub q: String,
    /// Agent filter: "all", "pi_agent", "opencode", or comma-separated list.
    #[serde(default = "default_agent_filter")]
    pub agents: String,
    /// Maximum number of results.
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_agent_filter() -> String {
    "all".to_string()
}

fn default_search_limit() -> usize {
    50
}

/// A single search hit from cass.
#[derive(Debug, Serialize, Deserialize)]
pub struct CassSearchHit {
    /// Agent type (pi_agent, opencode, etc.)
    pub agent: String,
    /// Path to the session file.
    pub source_path: String,
    /// Session identifier extracted from path.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Workspace/project directory.
    #[serde(default)]
    pub workspace: Option<String>,
    /// Message ID if available.
    #[serde(default)]
    pub message_id: Option<String>,
    /// Line number in the source file.
    #[serde(default)]
    pub line_number: Option<usize>,
    /// Matched content snippet.
    #[serde(default)]
    pub snippet: Option<String>,
    /// Search relevance score.
    #[serde(default)]
    pub score: Option<f64>,
    /// Timestamp of the message (cass uses created_at).
    #[serde(default, alias = "created_at")]
    pub timestamp: Option<i64>,
    /// Role (user, assistant, system).
    #[serde(default)]
    pub role: Option<String>,
    /// Session/conversation title if available.
    #[serde(default)]
    pub title: Option<String>,
    /// Full content (cass returns this)
    #[serde(default)]
    pub content: Option<String>,
    /// Match type from cass
    #[serde(default)]
    pub match_type: Option<String>,
    /// Origin kind from cass
    #[serde(default)]
    pub origin_kind: Option<String>,
    /// Source ID from cass
    #[serde(default)]
    pub source_id: Option<String>,
}

/// Response from cass search.
#[derive(Debug, Serialize, Deserialize)]
pub struct CassSearchResponse {
    pub hits: Vec<CassSearchHit>,
    /// Total count from cass (field is "count" in cass output)
    #[serde(default, alias = "count")]
    pub total: Option<usize>,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Search across coding agent sessions using cass.
#[instrument(skip(_state))]
pub async fn search_sessions(
    State(_state): State<AppState>,
    Query(query): Query<CassSearchQuery>,
) -> ApiResult<Json<CassSearchResponse>> {
    use tokio::process::Command;

    // Don't search empty queries
    if query.q.trim().is_empty() {
        return Ok(Json(CassSearchResponse {
            hits: vec![],
            total: Some(0),
            elapsed_ms: Some(0),
            cursor: None,
        }));
    }

    // Build cass command
    let args = build_cass_args(&query);

    // Try to find cass in common locations
    let cass_path = std::env::var("CASS_PATH")
        .ok()
        .or_else(|| {
            // Check common locations
            let home = std::env::var("HOME").ok()?;
            let local_bin = format!("{}/.local/bin/cass", home);
            if std::path::Path::new(&local_bin).exists() {
                return Some(local_bin);
            }
            None
        })
        .unwrap_or_else(|| "cass".to_string());

    let output = Command::new(&cass_path)
        .args(&args)
        .env("HOME", std::env::var("HOME").unwrap_or_default())
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::internal(format!(
                    "cass not found at '{}'. Install from: https://github.com/Dicklesworthstone/coding_agent_session_search",
                    cass_path
                ))
            } else {
                ApiError::internal(format!("Failed to execute cass: {}", e))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Check for common errors
        if stderr.contains("index") && stderr.contains("not found") {
            return Err(ApiError::internal(
                "cass index not built. Run 'cass index --full' to build the search index.",
            ));
        }
        return Err(ApiError::internal(format!(
            "cass search failed: {}",
            stderr
        )));
    }

    // Parse cass JSON output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: CassSearchResponse = serde_json::from_str(&stdout).map_err(|e| {
        ApiError::internal(format!(
            "Failed to parse cass output: {}. Output: {}",
            e,
            stdout.chars().take(500).collect::<String>()
        ))
    })?;

    Ok(Json(response))
}

fn build_cass_args(query: &CassSearchQuery) -> Vec<String> {
    let mut args = vec![
        "search".to_string(),
        query.q.clone(),
        "--robot".to_string(),
        "--robot-meta".to_string(),
        "--limit".to_string(),
        query.limit.to_string(),
    ];

    let agents = query.agents.trim();
    if !agents.is_empty() && agents != "all" {
        for agent in agents
            .split(',')
            .map(|agent| agent.trim())
            .filter(|a| !a.is_empty())
        {
            args.push("--agent".to_string());
            args.push(agent.to_string());
        }
    }

    args
}

#[cfg(test)]
mod cass_search_tests {
    use super::{CassSearchQuery, build_cass_args};

    fn base_query() -> CassSearchQuery {
        CassSearchQuery {
            q: "hello".to_string(),
            agents: "all".to_string(),
            limit: 50,
        }
    }

    #[test]
    fn cass_args_skip_all_agent_filter() {
        let query = base_query();
        let args = build_cass_args(&query);
        assert!(!args.contains(&"--agent".to_string()));
    }

    #[test]
    fn cass_args_supports_multiple_agents() {
        let mut query = base_query();
        query.agents = "opencode,pi_agent".to_string();
        let args = build_cass_args(&query);
        assert_eq!(
            args,
            vec![
                "search",
                "hello",
                "--robot",
                "--robot-meta",
                "--limit",
                "50",
                "--agent",
                "opencode",
                "--agent",
                "pi_agent",
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn cass_args_trims_agent_tokens() {
        let mut query = base_query();
        query.agents = " opencode , pi_agent , ".to_string();
        let args = build_cass_args(&query);
        assert!(args.contains(&"opencode".to_string()));
        assert!(args.contains(&"pi_agent".to_string()));
    }
}

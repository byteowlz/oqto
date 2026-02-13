//! Onboarding API handlers.
//!
//! Endpoints for managing user onboarding state:
//! - GET /onboarding - Get current onboarding state
//! - PUT /onboarding - Update onboarding state
//! - POST /onboarding/advance - Advance to next stage
//! - POST /onboarding/unlock/{component} - Unlock a UI component
//! - POST /onboarding/godmode - Activate godmode (skip onboarding)
//! - POST /onboarding/complete - Mark onboarding as complete
//! - POST /onboarding/reset - Reset onboarding state

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{SecondsFormat, Utc};
use rand::prelude::IndexedRandom;
use serde::{Deserialize, Serialize};
use std::path::{Path as FsPath, PathBuf};
use tracing::instrument;
use uuid::Uuid;

use super::error::{ApiError, ApiResult};
use super::state::AppState;
use crate::auth::CurrentUser;
use crate::onboarding::{OnboardingResponse, UnlockComponentRequest, UpdateOnboardingRequest};
use crate::templates::UserTemplateOverrides;
use crate::workspace::meta::{WorkspaceMeta, write_workspace_meta};

/// Get the current onboarding state.
#[instrument(skip(state, user))]
pub async fn get_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .get(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Update the onboarding state.
#[instrument(skip(state, user, request))]
pub async fn update_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<UpdateOnboardingRequest>,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .update(user.id(), request)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Advance to the next onboarding stage.
#[instrument(skip(state, user))]
pub async fn advance_stage(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .advance_stage(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Unlock a UI component.
#[instrument(skip(state, user))]
pub async fn unlock_component(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(component): Path<String>,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .unlock_component(user.id(), UnlockComponentRequest { component })
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Activate godmode (skip onboarding).
#[instrument(skip(state, user))]
pub async fn godmode(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .godmode(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Mark onboarding as complete.
#[instrument(skip(state, user))]
pub async fn complete_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .complete(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

/// Reset onboarding state.
#[instrument(skip(state, user))]
pub async fn reset_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<OnboardingResponse>> {
    let Some(ref service) = state.onboarding else {
        return Err(ApiError::ServiceUnavailable(
            "Onboarding service not configured".into(),
        ));
    };

    let onboarding_state = service
        .reset(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(onboarding_state.into()))
}

#[derive(Debug, Deserialize)]
pub struct BootstrapOnboardingRequest {
    pub display_name: String,
    pub language: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BootstrapOnboardingResponse {
    pub workspace_path: String,
    pub session_id: String,
    pub message: String,
}

const BOOTSTRAP_GREETINGS_EN: &[&str] = &[
    "Welcome to Octo. I will help set up your workspace. What kind of work will you do here?",
    "Welcome. I am here to set up your workspace. What is the main goal for this project?",
    "Welcome to your new workspace. Tell me what you want to build first.",
    "Welcome to Octo. I will guide setup and capture your preferences. What should we focus on?",
    "Welcome. I will help configure your workspace and assistant. What is your first task?",
];

const BOOTSTRAP_GREETINGS_DE: &[&str] = &[
    "Willkommen bei Octo. Ich richte den Workspace ein. Woran werden Sie hier arbeiten?",
    "Willkommen. Ich helfe beim Setup des Workspaces. Was ist das Hauptziel dieses Projekts?",
    "Willkommen in Ihrem neuen Workspace. Was mochten Sie als Erstes bauen?",
    "Willkommen bei Octo. Ich erfasse Ihre Einstellungen. Worauf sollen wir uns konzentrieren?",
    "Willkommen. Ich konfiguriere Workspace und Assistent. Was ist Ihre erste Aufgabe?",
];

/// Bootstrap the default workspace and seed the first Pi session.
#[instrument(skip(state, user, request))]
pub async fn bootstrap_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<BootstrapOnboardingRequest>,
) -> ApiResult<Json<BootstrapOnboardingResponse>> {
    let display_name = request.display_name.trim();
    if display_name.is_empty() {
        return Err(ApiError::BadRequest("display_name is required".into()));
    }

    let language = request
        .language
        .as_ref()
        .map(|lang| lang.trim().to_string())
        .filter(|lang| !lang.is_empty());

    let workspace_root = state.sessions.for_user(user.id()).workspace_root();
    if workspace_root.exists() {
        let mut entries = std::fs::read_dir(&workspace_root)
            .map_err(|e| ApiError::Internal(format!("Failed to read workspace root: {e}")))?;
        if entries.next().is_some() {
            return Err(ApiError::BadRequest("workspace already initialized".into()));
        }
    }

    let workspace_path = workspace_root.join("main");
    std::fs::create_dir_all(&workspace_path)
        .map_err(|e| ApiError::Internal(format!("Failed to create workspace: {e}")))?;

    let meta = WorkspaceMeta {
        display_name: Some(display_name.to_string()),
        language: language.clone(),
        pinned: Some(true),
        bootstrap_pending: Some(true),
    };

    write_workspace_meta(&workspace_path, &meta)
        .map_err(|e| ApiError::Internal(format!("Failed to write workspace metadata: {e}")))?;

    let templates_service = state.onboarding_templates.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Onboarding templates not configured".into())
    })?;

    let overrides = UserTemplateOverrides {
        language: language.clone(),
        ..Default::default()
    };

    let templates = templates_service
        .resolve(Some(&overrides))
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to resolve templates: {e}")))?;

    write_if_missing(&workspace_path.join("ONBOARD.md"), &templates.onboard)?;
    write_if_missing(
        &workspace_path.join("PERSONALITY.md"),
        &templates.personality,
    )?;
    write_if_missing(&workspace_path.join("USER.md"), &templates.user)?;
    write_if_missing(&workspace_path.join("AGENTS.md"), &templates.agents)?;

    let session_id = Uuid::new_v4().to_string();
    let (message, title) = pick_bootstrap_greeting(language.as_deref());
    let workspace_path_str = workspace_path.to_string_lossy().to_string();

    let hstry = state
        .hstry
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("Chat history service not available".into()))?;

    let now = Utc::now();
    let now_ms = now.timestamp_millis();
    let readable_id = crate::wordlist::readable_id_from_session_id(&session_id);
    let metadata_json = serde_json::json!({
        "bootstrap": true,
        "workspace_path": workspace_path_str,
        "language": language.clone().unwrap_or_else(|| "en".to_string()),
    })
    .to_string();

    let agent_message = crate::pi::AgentMessage {
        role: "assistant".to_string(),
        content: serde_json::json!([
            { "type": "text", "text": message }
        ]),
        timestamp: Some(now_ms as u64),
        tool_call_id: None,
        tool_name: None,
        is_error: None,
        api: None,
        provider: None,
        model: None,
        usage: None,
        stop_reason: None,
        extra: std::collections::HashMap::new(),
    };

    let proto_message = crate::hstry::agent_message_to_proto(&agent_message, 0);

    hstry
        .write_conversation(
            &session_id,
            Some(title.to_string()),
            Some(workspace_path_str.clone()),
            None,
            None,
            Some(metadata_json),
            vec![proto_message],
            now_ms,
            Some(now_ms),
            Some("pi".to_string()),
            Some(readable_id),
        )
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to seed chat history: {e}")))?;

    let home_dir = resolve_user_home(&state, user.id())?;
    write_pi_session_file(&home_dir, &workspace_path, &session_id, message, now)?;

    Ok(Json(BootstrapOnboardingResponse {
        workspace_path: workspace_path_str,
        session_id,
        message: message.to_string(),
    }))
}

fn resolve_user_home(state: &AppState, user_id: &str) -> Result<PathBuf, ApiError> {
    if let Some(linux_users) = state.linux_users.as_ref().filter(|cfg| cfg.enabled) {
        let linux_username = linux_users.linux_username(user_id);
        let home_dir = linux_users
            .get_home_dir(user_id)
            .map_err(|e| ApiError::Internal(format!("Failed to resolve user home: {e}")))?
            .unwrap_or_else(|| PathBuf::from(format!("/home/{linux_username}")));
        return Ok(home_dir);
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|e| ApiError::Internal(format!("HOME is not set: {e}")))?;
    Ok(home)
}

fn pick_bootstrap_greeting(language: Option<&str>) -> (&'static str, &'static str) {
    let locale = language.unwrap_or("en").to_lowercase();
    let is_german = locale.starts_with("de");
    let mut rng = rand::rng();
    if is_german {
        let message = BOOTSTRAP_GREETINGS_DE
            .choose(&mut rng)
            .copied()
            .unwrap_or(BOOTSTRAP_GREETINGS_DE[0]);
        return (message, "Willkommen");
    }

    let message = BOOTSTRAP_GREETINGS_EN
        .choose(&mut rng)
        .copied()
        .unwrap_or(BOOTSTRAP_GREETINGS_EN[0]);
    (message, "Welcome")
}

fn write_if_missing(path: &FsPath, contents: &str) -> Result<(), ApiError> {
    if path.exists() {
        return Ok(());
    }
    std::fs::write(path, contents)
        .map_err(|e| ApiError::Internal(format!("Failed to write {}: {e}", path.display())))?;
    Ok(())
}

fn write_pi_session_file(
    home_dir: &FsPath,
    workspace_path: &FsPath,
    session_id: &str,
    message: &str,
    now: chrono::DateTime<Utc>,
) -> Result<(), ApiError> {
    let safe_dir = safe_cwd_dirname(workspace_path);
    let sessions_dir = home_dir.join(".pi/agent/sessions").join(safe_dir);
    std::fs::create_dir_all(&sessions_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to create Pi sessions dir: {e}")))?;

    let timestamp = now.timestamp_millis();
    let filename = format!("{}_{}.jsonl", timestamp, session_id);
    let session_file = sessions_dir.join(filename);

    let header = serde_json::json!({
        "cwd": workspace_path.to_string_lossy(),
        "id": session_id,
        "timestamp": now.to_rfc3339(),
        "type": "session",
    });

    let message_id = nanoid::nanoid!(8);
    let parent_id = nanoid::nanoid!(8);
    let line_timestamp = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let message_entry = serde_json::json!({
        "type": "message",
        "id": message_id,
        "parentId": parent_id,
        "timestamp": line_timestamp,
        "message": {
            "role": "assistant",
            "content": [
                { "type": "text", "text": message }
            ],
            "timestamp": timestamp,
        }
    });

    let mut body = String::new();
    body.push_str(&header.to_string());
    body.push('\n');
    body.push_str(&message_entry.to_string());
    body.push('\n');

    std::fs::write(&session_file, body)
        .map_err(|e| ApiError::Internal(format!("Failed to write session file: {e}")))?;

    Ok(())
}

fn safe_cwd_dirname(cwd: &FsPath) -> String {
    let path_str = cwd.to_string_lossy();
    let safe = path_str
        .strip_prefix('/')
        .unwrap_or(&path_str)
        .replace('/', "-");
    format!("--{}--", safe)
}

/// Check if user needs onboarding (lightweight endpoint).
#[instrument(skip(state, user))]
pub async fn needs_onboarding(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    let Some(ref service) = state.onboarding else {
        // If onboarding service is not configured, user doesn't need onboarding
        return Ok(Json(serde_json::json!({
            "needs_onboarding": false,
            "reason": "service_disabled"
        })));
    };

    let onboarding_state = service
        .get(user.id())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "needs_onboarding": onboarding_state.needs_onboarding(),
        "stage": onboarding_state.stage,
        "completed": onboarding_state.completed,
        "godmode": onboarding_state.godmode
    })))
}

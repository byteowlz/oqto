//! UI control API handlers.
//!
//! Provides HTTP endpoints for agents or admins to control the frontend UI
//! via WebSocket broadcast events.

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::state::AppState;
use crate::ws::{UiSpotlightStep, WsEvent};

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct UiEventResponse {
    pub success: bool,
}

#[derive(Debug, Deserialize)]
pub struct UiNavigateRequest {
    pub path: String,
    #[serde(default)]
    pub replace: bool,
}

#[derive(Debug, Deserialize)]
pub struct UiSessionRequest {
    pub session_id: String,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UiViewRequest {
    pub view: String,
}

#[derive(Debug, Deserialize)]
pub struct UiPaletteRequest {
    #[serde(default = "default_true")]
    pub open: bool,
}

#[derive(Debug, Deserialize)]
pub struct UiPaletteExecRequest {
    pub command: String,
    #[serde(default)]
    pub args: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct UiSpotlightRequest {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UiTourRequest {
    #[serde(default)]
    pub steps: Vec<UiSpotlightStep>,
    #[serde(default)]
    pub start_index: Option<usize>,
    #[serde(default)]
    pub active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UiSidebarRequest {
    #[serde(default)]
    pub collapsed: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UiPanelRequest {
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default)]
    pub collapsed: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UiThemeRequest {
    pub theme: String,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Handlers
// ============================================================================

pub async fn navigate(
    State(state): State<AppState>,
    Json(request): Json<UiNavigateRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiNavigate {
            path: request.path,
            replace: request.replace,
        })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

pub async fn session(
    State(state): State<AppState>,
    Json(request): Json<UiSessionRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiSession {
            session_id: request.session_id,
            mode: request.mode,
        })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

pub async fn view(
    State(state): State<AppState>,
    Json(request): Json<UiViewRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiView { view: request.view })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

pub async fn palette(
    State(state): State<AppState>,
    Json(request): Json<UiPaletteRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiPalette { open: request.open })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

pub async fn palette_exec(
    State(state): State<AppState>,
    Json(request): Json<UiPaletteExecRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiPaletteExec {
            command: request.command,
            args: request.args,
        })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

pub async fn spotlight(
    State(state): State<AppState>,
    Json(request): Json<UiSpotlightRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    let active = request.active.unwrap_or(true);
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiSpotlight {
            target: request.target,
            title: request.title,
            description: request.description,
            action: request.action,
            position: request.position,
            active,
        })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

pub async fn tour(
    State(state): State<AppState>,
    Json(request): Json<UiTourRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    let active = request.active.unwrap_or(true);
    let start_index = request.start_index.map(|v| v as u32);
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiTour {
            steps: request.steps,
            start_index,
            active,
        })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

pub async fn sidebar(
    State(state): State<AppState>,
    Json(request): Json<UiSidebarRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiSidebar {
            collapsed: request.collapsed,
        })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

pub async fn panel(
    State(state): State<AppState>,
    Json(request): Json<UiPanelRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiPanel {
            view: request.view,
            collapsed: request.collapsed,
        })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

pub async fn theme(
    State(state): State<AppState>,
    Json(request): Json<UiThemeRequest>,
) -> Result<Json<UiEventResponse>, StatusCode> {
    state
        .ws_hub
        .broadcast_to_all(WsEvent::UiTheme {
            theme: request.theme,
        })
        .await;
    Ok(Json(UiEventResponse { success: true }))
}

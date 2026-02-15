//! Miscellaneous handlers (health, features, scheduler, feeds, search).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{instrument, warn};

use crate::auth::CurrentUser;
use crate::local::LinuxUsersConfig;
use crate::session_ui::SessionAutoAttachMode;

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Health check endpoint.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
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
    /// Whether the agent-browser integration is enabled.
    pub agent_browser_enabled: bool,
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
        agent_browser_enabled: state.sessions.agent_browser_enabled(),
    })
}

// ============================================================================
// Scheduler + Feed handlers
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SchedulerEntry {
    pub name: String,
    pub status: String,
    pub schedule: String,
    pub command: String,
    #[serde(default)]
    pub next_run: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SchedulerStats {
    pub total: usize,
    pub enabled: usize,
    pub disabled: usize,
}

#[derive(Debug, Serialize)]
pub struct SchedulerOverview {
    pub stats: SchedulerStats,
    pub schedules: Vec<SchedulerEntry>,
}

fn resolve_skdlr_bin(workspace_root: &std::path::Path) -> std::path::PathBuf {
    if let Ok(value) = std::env::var("SKDLR_BIN")
        && !value.is_empty()
    {
        return std::path::PathBuf::from(value);
    }

    let release = workspace_root
        .join("skdlr")
        .join("target")
        .join("release")
        .join("skdlr");
    if release.exists() {
        return release;
    }
    let debug = workspace_root
        .join("skdlr")
        .join("target")
        .join("debug")
        .join("skdlr");
    if debug.exists() {
        return debug;
    }

    std::path::PathBuf::from("skdlr")
}

async fn exec_skdlr_command(
    workspace_root: &PathBuf,
    args: &[&str],
    linux_users: Option<&LinuxUsersConfig>,
    user_id: &str,
) -> Result<String, ApiError> {
    let bin = resolve_skdlr_bin(workspace_root);
    let skdlr_config = PathBuf::from("/etc/octo/skdlr-agent.toml");
    let mut full_args: Vec<&str> = Vec::new();

    if skdlr_config.exists() {
        full_args.push("--config");
        full_args.push(
            skdlr_config
                .to_str()
                .unwrap_or("/etc/octo/skdlr-agent.toml"),
        );
    }

    full_args.extend_from_slice(args);

    let output = if let Some(linux_users) = linux_users.filter(|cfg| cfg.enabled) {
        let linux_username = linux_users.linux_username(user_id);
        let home_dir = linux_users
            .get_home_dir(user_id)
            .map_err(|e| ApiError::internal(format!("Failed to resolve linux user home: {e}")))?
            .unwrap_or_else(|| PathBuf::from(format!("/home/{}", linux_username)));
        let xdg_config = home_dir.join(".config");
        let xdg_data = home_dir.join(".local/share");

        let mut cmd = Command::new("sudo");
        cmd.arg("-n")
            .arg("-u")
            .arg(&linux_username)
            .arg("--")
            .arg("env")
            .arg("SKDLR_OCTO_MODE=1")
            .arg(format!("XDG_CONFIG_HOME={}", xdg_config.display()))
            .arg(format!("XDG_DATA_HOME={}", xdg_data.display()))
            .arg(bin)
            .args(&full_args)
            .current_dir(workspace_root);

        cmd.output()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to execute skdlr: {}", e)))?
    } else {
        let mut cmd = Command::new(bin);
        if skdlr_config.exists() {
            cmd.env("SKDLR_OCTO_MODE", "1");
        }
        cmd.args(&full_args)
            .current_dir(workspace_root)
            .output()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to execute skdlr: {}", e)))?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::internal(format!(
            "skdlr command failed: {}",
            stderr
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_skdlr_list(output: &str) -> Vec<SchedulerEntry> {
    let mut schedules = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("NAME")
            || trimmed.starts_with('-')
            || trimmed.starts_with("No schedules")
        {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let name = parts[0].to_string();
        let status = parts[1].to_string();
        let sched_type = parts[2];

        let (schedule, command) = match sched_type {
            "cron" => {
                if parts.len() < 8 {
                    (parts[3..].join(" "), String::new())
                } else {
                    (
                        parts[3..8].join(" "),
                        if parts.len() > 8 {
                            parts[8..].join(" ")
                        } else {
                            String::new()
                        },
                    )
                }
            }
            "once" => {
                if parts.len() < 6 {
                    (parts[3..].join(" "), String::new())
                } else {
                    (
                        parts[3..6].join(" "),
                        if parts.len() > 6 {
                            parts[6..].join(" ")
                        } else {
                            String::new()
                        },
                    )
                }
            }
            _ => {
                // Fallback for unknown format
                if parts.len() >= 7 {
                    (
                        parts[2..7].join(" "),
                        if parts.len() > 7 {
                            parts[7..].join(" ")
                        } else {
                            String::new()
                        },
                    )
                } else {
                    (parts[2..].join(" "), String::new())
                }
            }
        };

        schedules.push(SchedulerEntry {
            name,
            status,
            schedule,
            command,
            next_run: None,
        });
    }

    schedules
}

fn parse_skdlr_next(output: &str) -> HashMap<String, String> {
    let mut next_runs = HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("NAME")
            || trimmed.starts_with('-')
            || trimmed.starts_with("No upcoming runs")
        {
            continue;
        }

        if line.len() >= 21 {
            let name = line.get(0..20).unwrap_or("").trim();
            let next_run = line.get(21..).unwrap_or("").trim();
            if !name.is_empty() && !next_run.is_empty() {
                next_runs.insert(name.to_string(), next_run.to_string());
            }
        }
    }

    next_runs
}

/// Scheduler overview (skdlr) for the dashboard.
#[instrument(skip(state, user))]
pub async fn scheduler_overview(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<SchedulerOverview>> {
    let workspace_root = state.sessions.for_user(user.id()).workspace_root();
    let list_output = exec_skdlr_command(
        &workspace_root,
        &["list"],
        state.linux_users.as_ref(),
        user.id(),
    )
    .await?;
    let next_output = exec_skdlr_command(
        &workspace_root,
        &["next"],
        state.linux_users.as_ref(),
        user.id(),
    )
    .await
    .unwrap_or_default();

    let mut schedules = parse_skdlr_list(&list_output);
    let next_runs = parse_skdlr_next(&next_output);

    for schedule in &mut schedules {
        if let Some(next) = next_runs.get(&schedule.name) {
            schedule.next_run = Some(next.clone());
        }
    }

    let enabled = schedules
        .iter()
        .filter(|s| s.status.eq_ignore_ascii_case("enabled"))
        .count();
    let disabled = schedules
        .iter()
        .filter(|s| s.status.eq_ignore_ascii_case("disabled"))
        .count();
    let stats = SchedulerStats {
        total: schedules.len(),
        enabled,
        disabled,
    };

    Ok(Json(SchedulerOverview { stats, schedules }))
}

/// Delete a scheduled job by name.
#[instrument(skip(state))]
pub async fn scheduler_delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let workspace_root = state.sessions.for_user(user.id()).workspace_root();
    exec_skdlr_command(
        &workspace_root,
        &["remove", &name],
        state.linux_users.as_ref(),
        user.id(),
    )
    .await?;

    Ok(Json(serde_json::json!({ "deleted": name })))
}

#[derive(Debug, Deserialize)]
pub struct FeedFetchQuery {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct FeedFetchResponse {
    pub url: String,
    pub content: String,
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Fetch an RSS/Atom feed and return raw XML for client-side parsing.
#[instrument(skip(_state))]
pub async fn fetch_feed(
    State(_state): State<AppState>,
    Query(query): Query<FeedFetchQuery>,
) -> ApiResult<Json<FeedFetchResponse>> {
    let url =
        reqwest::Url::parse(&query.url).map_err(|_| ApiError::bad_request("Invalid feed URL"))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(ApiError::bad_request("Feed URL must be http or https"));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| ApiError::internal(format!("Failed to build HTTP client: {}", e)))?;

    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("Feed fetch failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(ApiError::internal(format!(
            "Feed request failed with status {}",
            response.status()
        )));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let max_bytes = 1_000_000usize;
    if let Some(length) = response.content_length()
        && length as usize > max_bytes
    {
        return Err(ApiError::bad_request("Feed payload too large"));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ApiError::internal(format!("Feed read failed: {}", e)))?;

    if bytes.len() > max_bytes {
        return Err(ApiError::bad_request("Feed payload too large"));
    }

    let content = String::from_utf8_lossy(&bytes).to_string();
    Ok(Json(FeedFetchResponse {
        url: query.url,
        content,
        content_type,
    }))
}

// ============================================================================
// CodexBar (AI subscription usage) handlers
// ============================================================================

/// Fetch CodexBar usage from the CLI (if available on PATH).
#[instrument]
pub async fn codexbar_usage() -> ApiResult<Json<serde_json::Value>> {
    let candidates: [&[&str]; 3] = [
        &[
            "usage",
            "--provider",
            "all",
            "--source",
            "cli",
            "--format",
            "json",
        ],
        &["usage", "--provider", "all", "--source", "cli", "--json"],
        &["usage", "--provider", "all", "--source", "cli"],
    ];

    let mut last_error: Option<String> = None;

    for args in candidates {
        let output = match tokio::time::timeout(
            Duration::from_secs(20),
            Command::new("codexbar").args(args).output(),
        )
        .await
        {
            Ok(result) => result.map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    ApiError::not_found("codexbar not available")
                } else {
                    ApiError::internal(format!("Failed to execute codexbar: {}", e))
                }
            })?,
            Err(_) => {
                return Err(ApiError::internal(
                    "codexbar timed out while fetching usage",
                ));
            }
        };

        if !output.status.success() {
            warn!("codexbar returned non-zero exit status");
        }

        match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            Ok(payload) => return Ok(Json(payload)),
            Err(err) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                last_error = Some(format!(
                    "Failed to parse codexbar JSON output: {} ({})",
                    err, stderr
                ));
            }
        }
    }

    Err(ApiError::internal(last_error.unwrap_or_else(|| {
        "Failed to parse codexbar output".to_string()
    })))
}

// ============================================================================
// HSTRY search handlers
// ============================================================================

/// Query parameters for session search.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Search query string.
    pub q: String,
    /// Agent filter: "all", "pi_agent", or comma-separated list.
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

/// A single search hit from hstry.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchHit {
    /// Agent type (pi_agent, etc.)
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
    /// Timestamp of the message (ms since epoch).
    #[serde(default, alias = "created_at")]
    pub timestamp: Option<i64>,
    /// Role (user, assistant, system).
    #[serde(default)]
    pub role: Option<String>,
    /// Session/conversation title if available.
    #[serde(default)]
    pub title: Option<String>,
    /// Full content
    #[serde(default)]
    pub content: Option<String>,
    /// Match type
    #[serde(default)]
    pub match_type: Option<String>,
    /// Origin kind
    #[serde(default)]
    pub origin_kind: Option<String>,
    /// Source ID
    #[serde(default)]
    pub source_id: Option<String>,
}

/// Response from hstry search.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    /// Total count
    #[serde(default, alias = "count")]
    pub total: Option<usize>,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Search across coding agent sessions using hstry.
#[instrument(skip(_state))]
pub async fn search_sessions(
    State(_state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<SearchResponse>> {
    // Don't search empty queries
    if query.q.trim().is_empty() {
        return Ok(Json(SearchResponse {
            hits: vec![],
            total: Some(0),
            elapsed_ms: Some(0),
            cursor: None,
        }));
    }

    let hits = crate::history::search_hstry(&query.q, query.limit)
        .await
        .map_err(|e| ApiError::internal(format!("hstry search failed: {e}")))?;

    let allowed_sources = parse_agent_filters(&query.agents);
    let mut results = Vec::new();
    for hit in hits {
        if let Some(ref allowed) = allowed_sources
            && !allowed.contains(&hit.source_id)
        {
            continue;
        }

        let timestamp = hit
            .created_at
            .or(hit.conv_updated_at)
            .map(|dt| dt.timestamp_millis())
            .or_else(|| Some(hit.conv_created_at.timestamp_millis()));

        let session_id = hit
            .external_id
            .clone()
            .unwrap_or_else(|| hit.conversation_id.clone());

        let source_path = hit
            .source_path
            .clone()
            .unwrap_or_else(|| format!("hstry:{}:{}", hit.source_id, hit.conversation_id));

        results.push(SearchHit {
            agent: map_source_id_to_agent(&hit.source_id),
            source_path,
            session_id: Some(session_id),
            workspace: hit.workspace.clone(),
            message_id: None,
            line_number: Some((hit.message_idx.max(0) as usize) + 1),
            snippet: Some(hit.snippet.clone()),
            score: Some(f64::from(hit.score)),
            timestamp,
            role: Some(hit.role.clone()),
            title: hit.title.clone(),
            content: Some(hit.content.clone()),
            match_type: None,
            origin_kind: Some(hit.source_adapter.clone()),
            source_id: Some(hit.source_id.clone()),
        });

        if results.len() >= query.limit {
            break;
        }
    }

    Ok(Json(SearchResponse {
        total: Some(results.len()),
        hits: results,
        elapsed_ms: None,
        cursor: None,
    }))
}

fn parse_agent_filters(agents: &str) -> Option<std::collections::HashSet<String>> {
    let agents = agents.trim();
    if agents.is_empty() || agents.eq_ignore_ascii_case("all") {
        return None;
    }

    let mut filters = std::collections::HashSet::new();
    for agent in agents
        .split(',')
        .map(|agent| agent.trim())
        .filter(|agent| !agent.is_empty())
    {
        filters.insert(map_agent_filter_to_source_id(agent));
    }

    if filters.is_empty() {
        None
    } else {
        Some(filters)
    }
}

fn map_agent_filter_to_source_id(agent: &str) -> String {
    match agent {
        "pi_agent" => "pi".to_string(),
        other => other.to_string(),
    }
}

fn map_source_id_to_agent(source_id: &str) -> String {
    match source_id {
        "pi" => "pi_agent".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
pub mod hstry_search_tests {
    use super::{map_agent_filter_to_source_id, parse_agent_filters};

    #[test]
    fn hstry_filters_skip_all() {
        assert!(parse_agent_filters("all").is_none());
    }

    #[test]
    fn hstry_filters_support_multiple() {
        let filters = parse_agent_filters("pi_agent,custom_agent").expect("filters");
        assert!(filters.contains("pi_agent"));
        assert!(filters.contains("pi"));
    }

    #[test]
    fn hstry_filters_trim_tokens() {
        let filters = parse_agent_filters(" pi_agent , custom_agent , ").expect("filters");
        assert!(filters.contains("pi_agent"));
        assert!(filters.contains("pi"));
    }

    #[test]
    fn hstry_filter_maps_pi_agent() {
        assert_eq!(map_agent_filter_to_source_id("pi_agent"), "pi");
    }
}

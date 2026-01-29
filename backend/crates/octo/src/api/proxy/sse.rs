//! Server-Sent Events (SSE) proxy handlers.
//!
//! Handles proxying SSE streams from opencode and sub-agents.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{Response, StatusCode},
};
use log::{debug, error};

use crate::auth::CurrentUser;

use super::super::state::AppState;
use super::builder::{
    DEFAULT_SSE_TIMEOUT, ensure_session_active_for_proxy, get_opencode_session,
    get_session_for_user,
};
use super::handlers::OpencodeEventQuery;

// ============================================================================
// SSE Response Builder
// ============================================================================

/// Build an SSE response from a reqwest response stream.
fn build_sse_response(
    stream: impl futures::Stream<Item = Result<axum::body::Bytes, reqwest::Error>> + Send + 'static,
) -> Result<Response<Body>, StatusCode> {
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("X-Accel-Buffering", "no") // Disable nginx buffering if present
        .body(body)
        .map_err(|e| {
            error!("Failed to build SSE response: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// Connect to an SSE endpoint with retry logic.
async fn connect_sse_with_retry(
    client: &reqwest::Client,
    target_url: &str,
    service_name: &str,
) -> Result<reqwest::Response, StatusCode> {
    let start = tokio::time::Instant::now();
    let timeout = DEFAULT_SSE_TIMEOUT;
    let mut attempts: u32 = 0;

    loop {
        attempts += 1;
        match client
            .get(target_url)
            .header("Accept", "text/event-stream")
            .send()
            .await
        {
            Ok(res) => return Ok(res),
            Err(err) => {
                // Only retry connection-level failures.
                if !err.is_connect() || start.elapsed() >= timeout {
                    error!(
                        "Failed to connect to {} SSE after {} attempts over {:?}: {:?}",
                        service_name, attempts, timeout, err
                    );
                    return Err(StatusCode::BAD_GATEWAY);
                }

                let backoff_ms = (attempts.min(20) as u64) * 100;
                let backoff = tokio::time::Duration::from_millis(backoff_ms);
                debug!(
                    "{} SSE not ready yet (attempt {}): {}; retrying in {:?}",
                    service_name, attempts, err, backoff
                );
                tokio::time::sleep(backoff).await;
            }
        }
    }
}

// ============================================================================
// Opencode SSE Handlers
// ============================================================================

/// SSE events proxy for a specific session's opencode server.
pub async fn proxy_opencode_events(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<OpencodeEventQuery>,
) -> Result<Response<Body>, StatusCode> {
    // Validate the requested session exists for this user
    let _requested = get_session_for_user(&state, &user, &session_id).await?;

    // Get the primary opencode session
    let opencode_session = get_opencode_session(&state, &user).await?;
    let opencode_session_id = opencode_session.id.clone();
    let opencode_session =
        ensure_session_active_for_proxy(&state, user.id(), &opencode_session_id, opencode_session)
            .await?;

    let target_url = if let Some(directory) = query.directory.as_deref() {
        format!(
            "http://localhost:{}/event?directory={}",
            opencode_session.opencode_port,
            urlencoding::encode(directory)
        )
    } else {
        format!("http://localhost:{}/event", opencode_session.opencode_port)
    };
    debug!("Proxying SSE events from {}", target_url);

    let client = reqwest::Client::new();
    let response = connect_sse_with_retry(&client, &target_url, "opencode").await?;

    if !response.status().is_success() {
        error!("Opencode SSE returned status: {}", response.status());
        return Err(StatusCode::BAD_GATEWAY);
    }

    build_sse_response(response.bytes_stream())
}

/// SSE events stream for opencode global events.
///
/// Proxies to opencode's /global/event endpoint which provides events for all
/// directories/sessions. The SDK expects this endpoint to receive real-time
/// updates about sessions, messages, permissions, etc.
pub async fn opencode_events(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Response<Body>, StatusCode> {
    let opencode_session = get_opencode_session(&state, &user).await?;
    let opencode_session_id = opencode_session.id.clone();
    let opencode_session =
        ensure_session_active_for_proxy(&state, user.id(), &opencode_session_id, opencode_session)
            .await?;

    let target_url = format!(
        "http://localhost:{}/global/event",
        opencode_session.opencode_port
    );
    debug!("Proxying global SSE events from {}", target_url);

    let client = reqwest::Client::new();
    let response = connect_sse_with_retry(&client, &target_url, "opencode global").await?;

    if !response.status().is_success() {
        error!("Opencode global SSE returned status: {}", response.status());
        return Err(StatusCode::BAD_GATEWAY);
    }

    build_sse_response(response.bytes_stream())
}

// ============================================================================
// Sub-Agent SSE Handler
// ============================================================================

/// SSE events proxy for a specific agent's opencode server.
///
/// Routes: /session/{session_id}/agent/{agent_id}/code/event
pub async fn proxy_opencode_agent_events(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((session_id, agent_id)): Path<(String, String)>,
    Query(query): Query<OpencodeEventQuery>,
) -> Result<Response<Body>, StatusCode> {
    // Validate the requested session exists for this user
    let _requested = get_session_for_user(&state, &user, &session_id).await?;

    // Get the primary opencode session
    let opencode_session = get_opencode_session(&state, &user).await?;
    let opencode_session_id = opencode_session.id.clone();
    let _opencode_session =
        ensure_session_active_for_proxy(&state, user.id(), &opencode_session_id, opencode_session)
            .await?;

    // Resolve the agent's port
    let port = state
        .agents
        .get_agent_port(&opencode_session_id, &agent_id)
        .await
        .map_err(|e| {
            error!(
                "Failed to get agent port for {}/{}: {:?}",
                opencode_session_id, agent_id, e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            log::warn!(
                "Agent {} not found or not running in session {}",
                agent_id,
                opencode_session_id
            );
            StatusCode::NOT_FOUND
        })?;

    let target_url = if let Some(directory) = query.directory.as_deref() {
        format!(
            "http://localhost:{}/event?directory={}",
            port,
            urlencoding::encode(directory)
        )
    } else {
        format!("http://localhost:{}/event", port)
    };
    debug!(
        "Proxying agent SSE events from {} (agent: {})",
        target_url, agent_id
    );

    let client = reqwest::Client::new();
    let response =
        connect_sse_with_retry(&client, &target_url, &format!("agent {}", agent_id)).await?;

    if !response.status().is_success() {
        error!(
            "Agent {} SSE returned status: {}",
            agent_id,
            response.status()
        );
        return Err(StatusCode::BAD_GATEWAY);
    }

    build_sse_response(response.bytes_stream())
}

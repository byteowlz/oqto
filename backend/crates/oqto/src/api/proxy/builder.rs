//! Generic proxy builder and shared utilities.
//!
//! Provides common infrastructure for HTTP and WebSocket proxying:
//! - Session lookup and validation
//! - Retry logic with exponential backoff
//! - Request forwarding with body limits
//! - Error handling and status code mapping

use axum::{
    body::Body,
    http::{HeaderMap, Request, Response, StatusCode, Uri},
};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use log::{debug, error, warn};
use std::time::Duration;

use crate::auth::CurrentUser;
use crate::session::{Session, SessionStatus};

use super::super::state::AppState;

/// Default timeout for proxy requests during startup.
pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

/// Default timeout for SSE proxy connections during startup.
pub const DEFAULT_SSE_TIMEOUT: Duration = Duration::from_secs(20);

/// Default timeout for WebSocket connections.
pub const DEFAULT_WS_TIMEOUT: Duration = Duration::from_secs(60);

// ============================================================================
// Session Lookup Helpers
// ============================================================================

/// Get a session by ID, validating it belongs to the user.
pub async fn get_session_for_user(
    state: &AppState,
    user: &CurrentUser,
    session_id: &str,
) -> Result<Session, StatusCode> {
    state
        .sessions
        .for_user(user.id())
        .get_session(session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)
}

/// Get a session by ID without user validation (for internal use).
pub async fn get_session_by_id(state: &AppState, session_id: &str) -> Result<Session, StatusCode> {
    state
        .sessions
        .get_session(session_id)
        .await
        .map_err(|e| {
            error!("Failed to get session {}: {:?}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)
}

/// Get or create an IO session for a workspace path.
pub async fn get_io_session_for_workspace(
    state: &AppState,
    user: &CurrentUser,
    workspace_path: &str,
) -> Result<Session, StatusCode> {
    state
        .sessions
        .for_user(user.id())
        .get_or_create_io_session_for_workspace(workspace_path)
        .await
        .map_err(|e| {
            error!(
                "Failed to get IO session for workspace {}: {:?}",
                workspace_path, e
            );
            StatusCode::SERVICE_UNAVAILABLE
        })
}

/// Ensure a session is active for IO proxy requests (fileserver, terminal).
///
/// For Stopped sessions, attempts to resume for IO only (lighter weight).
/// Other statuses pass through unchanged.
pub async fn ensure_session_for_io_proxy(
    state: &AppState,
    user_id: &str,
    session_id: &str,
    session: Session,
) -> Result<Session, StatusCode> {
    if session.status != SessionStatus::Stopped {
        return Ok(session);
    }

    warn!(
        "Session {} is stopped; attempting to resume for IO proxy request",
        session_id
    );
    match state
        .sessions
        .for_user(user_id)
        .resume_session_for_io(session_id)
        .await
    {
        Ok(resumed) => Ok(resumed),
        Err(err) => {
            error!("Failed to resume session {}: {:?}", session_id, err);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

// ============================================================================
// Body Limit Enforcement
// ============================================================================

/// Enforce a body size limit based on Content-Length header.
pub fn enforce_proxy_body_limit(
    headers: &HeaderMap,
    max_body_bytes: usize,
) -> Result<(), StatusCode> {
    if let Some(value) = headers.get(axum::http::header::CONTENT_LENGTH) {
        let length = value
            .to_str()
            .ok()
            .and_then(|raw| raw.parse::<usize>().ok())
            .ok_or(StatusCode::BAD_REQUEST)?;
        if length > max_body_bytes {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
    }
    Ok(())
}

// ============================================================================
// HTTP Proxy Core
// ============================================================================

/// Proxy an HTTP request to a target port with retry logic.
///
/// # Arguments
/// * `client` - The hyper client to use
/// * `req` - The incoming request
/// * `target_port` - The localhost port to proxy to
/// * `target_path` - The path on the target server
/// * `retry_on_connect` - Whether to retry connection errors (for starting services)
/// * `max_body_bytes` - Maximum body size to accept
pub async fn proxy_http_request(
    client: Client<HttpConnector, Body>,
    req: Request<Body>,
    target_port: u16,
    target_path: &str,
    retry_on_connect: bool,
    max_body_bytes: usize,
) -> Result<Response<Body>, StatusCode> {
    proxy_http_request_with_query(
        client,
        req,
        target_port,
        target_path,
        retry_on_connect,
        None,
        max_body_bytes,
    )
    .await
}

/// Proxy an HTTP request with optional query string override.
pub async fn proxy_http_request_with_query(
    client: Client<HttpConnector, Body>,
    req: Request<Body>,
    target_port: u16,
    target_path: &str,
    retry_on_connect: bool,
    query_override: Option<&str>,
    max_body_bytes: usize,
) -> Result<Response<Body>, StatusCode> {
    let query = req.uri().query().unwrap_or("");
    let query = query_override.unwrap_or(query);
    let mut target_uri = format!("http://localhost:{}/{}", target_port, target_path);
    if !query.is_empty() {
        target_uri.push('?');
        target_uri.push_str(query);
    }

    debug!("Proxying request to {}", target_uri);

    let uri: Uri = target_uri.parse().map_err(|e| {
        error!("Invalid target URI {}: {:?}", target_uri, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (parts, body) = req.into_parts();

    // Log content-type for debugging multipart issues
    if let Some(ct) = parts.headers.get(axum::http::header::CONTENT_TYPE) {
        warn!("Proxy request Content-Type: {:?}", ct);
    }
    if let Some(cl) = parts.headers.get(axum::http::header::CONTENT_LENGTH) {
        warn!("Proxy request Content-Length header: {:?}", cl);
    }

    enforce_proxy_body_limit(&parts.headers, max_body_bytes)?;
    let body_bytes = axum::body::to_bytes(body, max_body_bytes)
        .await
        .map_err(|e| {
            if e.to_string().contains("length limit") {
                warn!(
                    "Proxy request body exceeded limit of {} bytes",
                    max_body_bytes
                );
                return StatusCode::PAYLOAD_TOO_LARGE;
            }
            error!("Failed to buffer proxy request body: {:?}", e);
            StatusCode::BAD_GATEWAY
        })?;

    warn!("Proxy request body size: {} bytes", body_bytes.len());

    let start = tokio::time::Instant::now();
    let timeout = DEFAULT_STARTUP_TIMEOUT;
    let mut attempts: u32 = 0;

    let response = loop {
        attempts += 1;
        let mut forwarded = Request::builder()
            .method(parts.method.clone())
            .uri(uri.clone())
            .version(parts.version)
            .body(Body::from(body_bytes.clone()))
            .map_err(|e| {
                error!("Failed to build proxy request: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        *forwarded.headers_mut() = parts.headers.clone();

        // Update Content-Length to match actual body size (important for multipart uploads
        // where the original Content-Length may not match after buffering).
        // Also remove Transfer-Encoding since we're sending a fixed-length body.
        forwarded
            .headers_mut()
            .remove(axum::http::header::TRANSFER_ENCODING);
        let new_content_length = body_bytes.len().to_string();
        warn!(
            "Setting Content-Length to {} (was: {:?})",
            new_content_length,
            parts.headers.get(axum::http::header::CONTENT_LENGTH)
        );
        forwarded.headers_mut().insert(
            axum::http::header::CONTENT_LENGTH,
            axum::http::HeaderValue::from_str(&new_content_length)
                .expect("content-length is valid"),
        );

        // Ensure Host header matches the target authority.
        if let Some(authority) = forwarded.uri().authority() {
            let value = axum::http::HeaderValue::from_str(authority.as_str()).map_err(|e| {
                error!("Invalid Host header value {}: {:?}", authority.as_str(), e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            forwarded
                .headers_mut()
                .insert(axum::http::header::HOST, value);
        }

        match client.request(forwarded).await {
            Ok(res) => break res,
            Err(err) => {
                if retry_on_connect && err.is_connect() && start.elapsed() < timeout {
                    let backoff_ms = (attempts.min(20) as u64) * 100;
                    let backoff = Duration::from_millis(backoff_ms);
                    debug!(
                        "Proxy target not ready yet (attempt {}): {}; retrying in {:?}",
                        attempts, err, backoff
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                error!("Proxy request failed: {:?}", err);
                return Err(if retry_on_connect && err.is_connect() {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::BAD_GATEWAY
                });
            }
        }
    };

    // Convert hyper response to axum response
    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}

// ============================================================================
// Query String Utilities
// ============================================================================

/// Build a fileserver query string, adding directory parameter.
pub fn build_fileserver_query(workspace_path: &str, query: Option<&str>) -> String {
    let mut pairs: Vec<String> = Vec::new();
    if let Some(query) = query {
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            if pair.starts_with("workspace_path=") || pair.starts_with("directory=") {
                continue;
            }
            pairs.push(pair.to_string());
        }
    }
    pairs.push(format!("directory={}", urlencoding::encode(workspace_path)));
    pairs.join("&")
}

/// Build an mmry query string, filtering out workspace_path and store params.
pub fn build_mmry_query(query: Option<&str>) -> String {
    let mut pairs: Vec<String> = Vec::new();
    if let Some(query) = query {
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            if pair.starts_with("workspace_path=") || pair.starts_with("store=") {
                continue;
            }
            pairs.push(pair.to_string());
        }
    }
    pairs.join("&")
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn enforce_proxy_body_limit_allows_small() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            axum::http::HeaderValue::from_static("10"),
        );
        assert!(enforce_proxy_body_limit(&headers, 20).is_ok());
    }

    #[test]
    fn enforce_proxy_body_limit_rejects_large() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            axum::http::HeaderValue::from_static("128"),
        );
        assert_eq!(
            enforce_proxy_body_limit(&headers, 64).unwrap_err(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[test]
    fn enforce_proxy_body_limit_rejects_invalid_length() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            axum::http::HeaderValue::from_static("nope"),
        );
        assert_eq!(
            enforce_proxy_body_limit(&headers, 64).unwrap_err(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn build_fileserver_query_adds_directory() {
        let result = build_fileserver_query("/home/user/project", None);
        assert_eq!(result, "directory=%2Fhome%2Fuser%2Fproject");
    }

    #[test]
    fn build_fileserver_query_preserves_other_params() {
        let result = build_fileserver_query("/home/user/project", Some("foo=bar&baz=qux"));
        assert!(result.contains("foo=bar"));
        assert!(result.contains("baz=qux"));
        assert!(result.contains("directory="));
    }

    #[test]
    fn build_fileserver_query_removes_directory() {
        let result = build_fileserver_query("/home/user/project", Some("directory=old&foo=bar"));
        assert!(!result.contains("directory=old"));
        assert!(result.contains("foo=bar"));
    }
}

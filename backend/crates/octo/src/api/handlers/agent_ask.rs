//! Agent ask handlers.
//!
//! Handles "ask" requests to agents via Pi and OpenCode.

use std::convert::Infallible;
use std::time::{Duration, Instant};

use axum::{
    Json,
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

use crate::auth::CurrentUser;

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

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
    let base = parts.first().copied().unwrap_or("");
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
        return Ok(AskTarget::Session {
            id: target.to_string(),
        });
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
    let parsed_target =
        parse_ask_target(&req.target, assistant_name.as_deref()).map_err(ApiError::bad_request)?;

    // Handle OpenCode sessions differently from Pi sessions
    if let AskTarget::OpenCodeSession { id, workspace_path } = parsed_target {
        return handle_opencode_ask(&state, &user, &req, &id, workspace_path.as_deref()).await;
    }

    // Get the Pi service for Pi-based targets
    let pi_service = state
        .main_chat_pi
        .as_ref()
        .ok_or_else(|| ApiError::internal("Main Chat Pi service not enabled"))?;

    // Resolve to a Pi session
    let session = match parsed_target {
        AskTarget::MainChat {
            session_query: None,
        } => {
            // Get active session or create new
            pi_service
                .get_or_create_session(user.id())
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get session: {}", e)))?
        }
        AskTarget::MainChat {
            session_query: Some(query),
        } => {
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
                        error: format!(
                            "Multiple sessions match '{}'. Please be more specific.",
                            query
                        ),
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
                    let matches = pi_service.search_sessions(user.id(), &id).map_err(|e| {
                        ApiError::internal(format!("Failed to search sessions: {}", e))
                    })?;

                    if matches.is_empty() {
                        return Err(ApiError::not_found(format!("Session not found: {}", id)));
                    }

                    if matches.len() > 1 {
                        let response = AgentAskAmbiguousResponse {
                            error: format!(
                                "Multiple sessions match '{}'. Please be more specific.",
                                id
                            ),
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
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to resume session: {}", e))
                        })?
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
                match tokio::time::timeout(Duration::from_secs(timeout_secs), event_rx.recv()).await
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
                                    if tx
                                        .send(Ok(Event::default().data(json.to_string())))
                                        .await
                                        .is_err()
                                    {
                                        return; // Client disconnected
                                    }
                                }
                                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                                    let json = serde_json::json!({
                                        "type": "thinking",
                                        "data": delta
                                    });
                                    if tx
                                        .send(Ok(Event::default().data(json.to_string())))
                                        .await
                                        .is_err()
                                    {
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
                Ok(Ok(event)) => match event {
                    PiEvent::MessageUpdate {
                        assistant_message_event,
                        ..
                    } => {
                        use crate::pi::AssistantMessageEvent;
                        if let AssistantMessageEvent::TextDelta { delta, .. } =
                            assistant_message_event
                        {
                            response_text.push_str(&delta);
                        }
                    }
                    PiEvent::AgentEnd { .. } => {
                        break;
                    }
                    _ => {}
                },
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
    user: &CurrentUser,
    req: &AgentAskRequest,
    session_id: &str,
    provided_workspace_path: Option<&str>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    use reqwest_eventsource::{Event as SseEvent, EventSource};

    // Get workspace path from provided value or look up from chat history
    let workspace_path = if let Some(path) = provided_workspace_path {
        // Ensure this workspace path is valid for the authenticated user.
        state
            .sessions
            .for_user(user.id())
            .validate_workspace_path(path)
            .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?
            .to_string_lossy()
            .to_string()
    } else {
        // Look up session in chat history to get workspace path
        let chat_session = crate::history::get_session(session_id)
            .map_err(|e| ApiError::internal(format!("Failed to lookup session: {}", e)))?
            .ok_or_else(|| ApiError::not_found(format!("Session not found: {}", session_id)))?;
        state
            .sessions
            .for_user(user.id())
            .validate_workspace_path(&chat_session.workspace_path)
            .map_err(|e| ApiError::bad_request(format!("Invalid workspace path: {}", e)))?
            .to_string_lossy()
            .to_string()
    };

    // Get or create the OpenCode runtime session
    let opencode_session = state
        .sessions
        .for_user(user.id())
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
            let request_builder = client.get(&event_url).header("Accept", "text/event-stream");

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
                        if let Ok(event_json) = serde_json::from_str::<serde_json::Value>(&msg.data)
                        {
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
                                        if tx
                                            .send(Ok(Event::default().data(json.to_string())))
                                            .await
                                            .is_err()
                                        {
                                            return; // Client disconnected
                                        }
                                    }
                                }
                                Some("message.completed") | Some("session.completed") => {
                                    let json = serde_json::json!({
                                        "type": "done",
                                        "response": text_buffer
                                    });
                                    let _ =
                                        tx.send(Ok(Event::default().data(json.to_string()))).await;
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
                                    let _ =
                                        tx.send(Ok(Event::default().data(json.to_string()))).await;
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
        let request_builder = client.get(&event_url).header("Accept", "text/event-stream");

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

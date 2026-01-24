//! WebSocket handler for client connections.

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::{Request, Uri},
    response::Response,
};
use futures::{SinkExt, StreamExt};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::Duration;

use crate::api::{ApiError, AppState};
use crate::auth::CurrentUser;
use crate::session::SessionStatus;

use super::hub::WsHub;
use super::types::{SessionSubscription, WsCommand, WsEvent};

/// Ping interval for keepalive.
const PING_INTERVAL_SECS: u64 = 30;

/// WebSocket upgrade handler.
///
/// GET /api/ws
pub async fn ws_handler(
    State(state): State<AppState>,
    user: CurrentUser,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let user_id = user.id().to_string();
    info!("WebSocket upgrade request from user {}", user_id);

    // Get the hub from app state
    let hub = state.ws_hub.clone();

    Ok(ws.on_upgrade(move |socket| handle_ws_connection(socket, hub, user_id, state)))
}

async fn ensure_session_ready_for_ws(
    state: &AppState,
    user_id: &str,
    session: crate::session::Session,
) -> anyhow::Result<crate::session::Session> {
    match session.status {
        SessionStatus::Running => {
            if !is_opencode_healthy(state.http_client.clone(), session.opencode_port as u16).await {
                warn!(
                    "Opencode for session {} is unreachable; attempting restart",
                    session.id
                );
                state
                    .sessions
                    .for_user(user_id)
                    .stop_session(&session.id)
                    .await?;
                let resumed = state
                    .sessions
                    .for_user(user_id)
                    .resume_session(&session.id)
                    .await?;
                Ok(resumed)
            } else {
                Ok(session)
            }
        }
        SessionStatus::Starting | SessionStatus::Pending => Ok(session),
        SessionStatus::Stopped => {
            let resumed = state
                .sessions
                .for_user(user_id)
                .resume_session(&session.id)
                .await?;
            Ok(resumed)
        }
        SessionStatus::Stopping | SessionStatus::Failed => anyhow::bail!(
            "Session {} is not active (status={:?})",
            session.id,
            session.status
        ),
    }
}

async fn is_opencode_healthy(client: Client<HttpConnector, Body>, port: u16) -> bool {
    let uri = match format!("http://localhost:{}/session", port).parse::<Uri>() {
        Ok(uri) => uri,
        Err(_) => return false,
    };
    let req = match Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
    {
        Ok(req) => req,
        Err(_) => return false,
    };

    match tokio::time::timeout(Duration::from_secs(2), client.request(req)).await {
        Ok(Ok(resp)) => resp.status().is_success(),
        _ => false,
    }
}

/// Handle a WebSocket connection.
async fn handle_ws_connection(
    socket: WebSocket,
    hub: Arc<WsHub>,
    user_id: String,
    state: AppState,
) {
    let (mut sender, mut receiver) = socket.split();

    // Register connection with hub
    let (mut event_rx, conn_id) = hub.register_connection(&user_id);

    // Send connected message
    let connected_json = match serde_json::to_string(&WsEvent::Connected) {
        Ok(json) => json,
        Err(e) => {
            error!("Failed to serialize connected event for {}: {}", user_id, e);
            hub.unregister_connection(&user_id, conn_id);
            return;
        }
    };

    if let Err(e) = sender.send(Message::Text(connected_json.into())).await {
        error!(
            "Failed to send connected message to user {}: {}",
            user_id, e
        );
        hub.unregister_connection(&user_id, conn_id);
        return;
    }

    // Subscribe to hub events for this user
    let mut hub_events = hub.subscribe_events();

    // Spawn task to send events to client
    let user_id_send = user_id.clone();
    let hub_send = hub.clone();
    let send_task = tokio::spawn(async move {
        // Ping ticker
        let mut ping_interval = tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));

        loop {
            tokio::select! {
                // Events from the per-connection channel
                Some(event) = event_rx.recv() => {
                    let json = match serde_json::to_string(&event) {
                        Ok(j) => j,
                        Err(e) => {
                            warn!("Failed to serialize event: {}", e);
                            continue;
                        }
                    };
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }

                // Events from the hub broadcast channel (session events)
                Ok((session_id, event)) = hub_events.recv() => {
                    // Only forward events for sessions this user is subscribed to
                    if hub_send.is_subscribed(&user_id_send, &session_id) {
                        let json = match serde_json::to_string(&event) {
                            Ok(j) => j,
                            Err(e) => {
                                warn!("Failed to serialize hub event: {}", e);
                                continue;
                            }
                        };
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }

                // Periodic ping
                _ = ping_interval.tick() => {
                    match serde_json::to_string(&WsEvent::Ping) {
                        Ok(ping_json) => {
                            if sender.send(Message::Text(ping_json.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to serialize ping event: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    });

    // Process incoming messages
    while let Some(msg_result) = receiver.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                let text_str = text.to_string();
                match serde_json::from_str::<WsCommand>(&text_str) {
                    Ok(cmd) => {
                        let cmd_session_id = cmd.session_id().map(|id| id.to_string());
                        if let Err(e) = handle_command(&hub, &state, &user_id, cmd).await {
                            warn!("Failed to handle command from user {}: {}", user_id, e);
                            // Send error to user
                            hub.send_to_user(
                                &user_id,
                                WsEvent::Error {
                                    message: e.to_string(),
                                    session_id: cmd_session_id,
                                },
                            )
                            .await;
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to parse command from user {}: {} - {}",
                            user_id, e, text_str
                        );
                    }
                }
            }
            Ok(Message::Binary(_)) => {
                // Binary messages not supported for commands
                debug!("Received binary message from user {}, ignoring", user_id);
            }
            Ok(Message::Ping(_data)) => {
                // Respond to ping with pong - handled by axum
                debug!("Received ping from user {}", user_id);
            }
            Ok(Message::Pong(_)) => {
                debug!("Received pong from user {}", user_id);
            }
            Ok(Message::Close(_)) => {
                info!("User {} closed WebSocket connection", user_id);
                break;
            }
            Err(e) => {
                warn!("WebSocket error for user {}: {}", user_id, e);
                break;
            }
        }
    }

    // Clean up
    send_task.abort();

    // Unsubscribe from all sessions
    for session_id in hub.user_subscriptions(&user_id) {
        hub.unsubscribe_session(&user_id, &session_id);
    }

    hub.unregister_connection(&user_id, conn_id);
    info!("WebSocket connection closed for user {}", user_id);
}

/// Handle a command from a client.
async fn handle_command(
    hub: &WsHub,
    state: &AppState,
    user_id: &str,
    cmd: WsCommand,
) -> anyhow::Result<()> {
    match cmd {
        WsCommand::Pong => {
            // Pong received, connection is alive
            Ok(())
        }

        WsCommand::Subscribe { session_id } => {
            // Get session info
            let session = state
                .sessions
                .for_user(user_id)
                .get_session(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            let session = ensure_session_ready_for_ws(state, user_id, session).await?;

            let subscription = SessionSubscription {
                session_id: session_id.clone(),
                workspace_path: session.workspace_path.clone(),
                opencode_port: session.opencode_port as u16,
            };

            hub.subscribe_session(user_id, subscription).await?;

            // Send initial session state
            hub.send_to_user(
                user_id,
                WsEvent::SessionUpdated {
                    session_id: session_id.clone(),
                    status: session.status.to_string(),
                    workspace_path: session.workspace_path.clone(),
                },
            )
            .await;

            Ok(())
        }

        WsCommand::Unsubscribe { session_id } => {
            hub.unsubscribe_session(user_id, &session_id);
            Ok(())
        }

        WsCommand::SendMessage {
            session_id,
            message,
            attachments: _attachments,
        } => {
            // Verify user is subscribed
            if !hub.is_subscribed(user_id, &session_id) {
                anyhow::bail!("Not subscribed to session");
            }

            // Get session
            let session = state
                .sessions
                .get_session(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            // Get opencode session
            let opencode_session = state
                .sessions
                .for_user(user_id)
                .get_or_create_opencode_session()
                .await?;

            // Send message via HTTP to opencode
            let client = reqwest::Client::new();
            let url = format!(
                "http://localhost:{}/session/{}/prompt_async",
                opencode_session.opencode_port, session_id
            );

            let request_body = serde_json::json!({
                "parts": [{"type": "text", "text": message}]
            });

            // Add directory header for workspace scoping
            let response = client
                .post(&url)
                .header("x-opencode-directory", &session.workspace_path)
                .json(&request_body)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to send message: {} - {}", status, body);
            }

            Ok(())
        }

        WsCommand::SendParts { session_id, parts } => {
            // Verify user is subscribed
            if !hub.is_subscribed(user_id, &session_id) {
                anyhow::bail!("Not subscribed to session");
            }

            // Get session
            let session = state
                .sessions
                .for_user(user_id)
                .get_session(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            // Get opencode session
            let opencode_session = state
                .sessions
                .for_user(user_id)
                .get_or_create_opencode_session()
                .await?;

            // Send parts via HTTP to opencode
            let client = reqwest::Client::new();
            let url = format!(
                "http://localhost:{}/session/{}/prompt_async",
                opencode_session.opencode_port, session_id
            );

            // Convert WS parts to opencode prompt parts format
            let opencode_parts: Vec<serde_json::Value> = parts
                .iter()
                .map(|p| match p {
                    super::types::MessagePart::Text { text } => {
                        serde_json::json!({"type": "text", "text": text})
                    }
                    super::types::MessagePart::Image { url } => {
                        serde_json::json!({"type": "image", "url": url})
                    }
                    super::types::MessagePart::File { path } => {
                        serde_json::json!({"type": "file", "path": path})
                    }
                })
                .collect();

            let response = client
                .post(&url)
                .header("x-opencode-directory", &session.workspace_path)
                .json(&serde_json::json!({ "parts": opencode_parts }))
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to send parts: {} - {}", status, body);
            }

            Ok(())
        }

        WsCommand::Abort { session_id } => {
            // Verify user is subscribed
            if !hub.is_subscribed(user_id, &session_id) {
                anyhow::bail!("Not subscribed to session");
            }

            // Get session
            let session = state
                .sessions
                .for_user(user_id)
                .get_session(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            // Get opencode session
            let opencode_session = state
                .sessions
                .for_user(user_id)
                .get_or_create_opencode_session()
                .await?;

            // Send abort via HTTP to opencode
            let client = reqwest::Client::new();
            let url = format!(
                "http://localhost:{}/session/{}/abort",
                opencode_session.opencode_port, session_id
            );

            let response = client
                .post(&url)
                .header("x-opencode-directory", &session.workspace_path)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to abort: {} - {}", status, body);
            }

            Ok(())
        }

        WsCommand::PermissionReply {
            session_id,
            permission_id,
            granted,
        } => {
            // Verify user is subscribed
            if !hub.is_subscribed(user_id, &session_id) {
                anyhow::bail!("Not subscribed to session");
            }

            // Get session
            let session = state
                .sessions
                .for_user(user_id)
                .get_session(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            // Get opencode session
            let opencode_session = state
                .sessions
                .for_user(user_id)
                .get_or_create_opencode_session()
                .await?;

            // Send permission reply via HTTP to opencode
            let client = reqwest::Client::new();
            let url = format!(
                "http://localhost:{}/session/{}/permissions/{}",
                opencode_session.opencode_port, session_id, permission_id
            );

            let result = if granted { "granted" } else { "denied" };
            let response = client
                .put(&url)
                .header("x-opencode-directory", &session.workspace_path)
                .json(&serde_json::json!({ "result": result }))
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to reply to permission: {} - {}", status, body);
            }

            Ok(())
        }

        WsCommand::QuestionReply {
            session_id,
            request_id,
            answers,
        } => {
            // Verify user is subscribed
            if !hub.is_subscribed(user_id, &session_id) {
                anyhow::bail!("Not subscribed to session");
            }

            // Get session
            let session = state
                .sessions
                .for_user(user_id)
                .get_session(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            // Get opencode session
            let opencode_session = state
                .sessions
                .for_user(user_id)
                .get_or_create_opencode_session()
                .await?;

            // Send question reply via HTTP to opencode
            let client = reqwest::Client::new();
            let url = format!(
                "http://localhost:{}/question/{}/reply",
                opencode_session.opencode_port, request_id
            );

            let response = client
                .post(&url)
                .header("x-opencode-directory", &session.workspace_path)
                .json(&serde_json::json!({ "answers": answers }))
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to reply to question: {} - {}", status, body);
            }

            Ok(())
        }

        WsCommand::QuestionReject {
            session_id,
            request_id,
        } => {
            // Verify user is subscribed
            if !hub.is_subscribed(user_id, &session_id) {
                anyhow::bail!("Not subscribed to session");
            }

            // Get session
            let session = state
                .sessions
                .for_user(user_id)
                .get_session(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            // Get opencode session
            let opencode_session = state
                .sessions
                .for_user(user_id)
                .get_or_create_opencode_session()
                .await?;

            // Send question reject via HTTP to opencode
            let client = reqwest::Client::new();
            let url = format!(
                "http://localhost:{}/question/{}/reject",
                opencode_session.opencode_port, request_id
            );

            let response = client
                .post(&url)
                .header("x-opencode-directory", &session.workspace_path)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to reject question: {} - {}", status, body);
            }

            Ok(())
        }

        WsCommand::RefreshSession { session_id } => {
            // Verify user is subscribed
            if !hub.is_subscribed(user_id, &session_id) {
                anyhow::bail!("Not subscribed to session");
            }

            // Get session info and send update
            let session = state
                .sessions
                .for_user(user_id)
                .get_session(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            let opencode_session = state
                .sessions
                .for_user(user_id)
                .get_or_create_opencode_session()
                .await?;

            hub.send_to_user(
                user_id,
                WsEvent::SessionUpdated {
                    session_id,
                    status: opencode_session.status.to_string(),
                    workspace_path: session.workspace_path,
                },
            )
            .await;

            Ok(())
        }

        WsCommand::GetMessages {
            session_id,
            after_id: _after_id,
        } => {
            // This is a pull-based request - the client wants messages
            // We'll fetch them and send via the MessageUpdated event

            // Verify user is subscribed
            if !hub.is_subscribed(user_id, &session_id) {
                anyhow::bail!("Not subscribed to session");
            }

            // Get session
            let session = state
                .sessions
                .for_user(user_id)
                .get_session(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            // Get opencode session
            let opencode_session = state
                .sessions
                .for_user(user_id)
                .get_or_create_opencode_session()
                .await?;

            // Fetch messages from opencode
            let client = reqwest::Client::new();
            let url = format!(
                "http://localhost:{}/session/{}/message",
                opencode_session.opencode_port, session_id
            );

            let response = client
                .get(&url)
                .header("x-opencode-directory", &session.workspace_path)
                .send()
                .await?;

            if response.status().is_success() {
                let messages: serde_json::Value = response.json().await?;

                // Send each message as an update
                if let Some(msgs) = messages.as_array() {
                    for msg in msgs {
                        hub.send_to_user(
                            user_id,
                            WsEvent::MessageUpdated {
                                session_id: session_id.clone(),
                                message: msg.clone(),
                            },
                        )
                        .await;
                    }
                }
            }

            Ok(())
        }

        WsCommand::A2uiAction {
            session_id,
            surface_id,
            request_id,
            action_name,
            source_component_id,
            context,
        } => {
            tracing::info!(
                session_id = %session_id,
                surface_id = %surface_id,
                action_name = %action_name,
                source_component_id = %source_component_id,
                "Received A2UI action"
            );

            // Convert context from Value to HashMap<String, Value>
            let context_map: std::collections::HashMap<String, serde_json::Value> = context
                .as_object()
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();

            // If this is a blocking request, forward to the pending request handler
            if let Some(ref req_id) = request_id {
                let handled = crate::api::a2ui::handle_a2ui_action(
                    &state.pending_a2ui_requests,
                    req_id,
                    action_name.clone(),
                    source_component_id.clone(),
                    context_map,
                );

                if handled {
                    tracing::info!(request_id = %req_id, "A2UI blocking request resolved");
                }

                // Send resolved event to frontend
                hub.send_to_user(
                    user_id,
                    WsEvent::A2uiActionResolved {
                        session_id: session_id.clone(),
                        request_id: req_id.clone(),
                    },
                )
                .await;
            }

            let _ = surface_id; // Silence unused warning

            Ok(())
        }
    }
}

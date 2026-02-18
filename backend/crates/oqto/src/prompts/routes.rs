//! HTTP and WebSocket routes for the prompt system.

use crate::prompts::{PromptAction, PromptManager, PromptMessage, PromptRequest};
use axum::{
    Json, Router,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Shared state for prompt routes.
#[derive(Clone)]
pub struct PromptState {
    pub manager: Arc<PromptManager>,
}

/// Create prompt routes.
pub fn prompt_routes(manager: Arc<PromptManager>) -> Router {
    let state = PromptState { manager };

    Router::new()
        // Public API routes
        .route("/api/prompts", get(list_prompts))
        .route("/api/prompts/{id}", get(get_prompt).post(respond_to_prompt))
        .route("/api/prompts/ws", get(websocket_handler))
        // Internal routes (for oqto-guard, oqto-ssh-proxy)
        .route("/internal/prompt", post(create_prompt))
        .with_state(state)
}

/// List all pending prompts.
async fn list_prompts(State(state): State<PromptState>) -> impl IntoResponse {
    let prompts = state.manager.list_pending().await;
    Json(prompts)
}

/// Get a specific prompt.
async fn get_prompt(State(state): State<PromptState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.manager.get(&id).await {
        Some(prompt) => Ok(Json(prompt)),
        None => Err((StatusCode::NOT_FOUND, "Prompt not found")),
    }
}

/// Request body for responding to a prompt.
#[derive(Debug, Deserialize)]
struct RespondRequest {
    action: PromptAction,
}

/// Response for prompt operations.
#[derive(Debug, Serialize)]
struct PromptOpResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Respond to a prompt.
async fn respond_to_prompt(
    State(state): State<PromptState>,
    Path(id): Path<String>,
    Json(req): Json<RespondRequest>,
) -> impl IntoResponse {
    info!("Responding to prompt {}: {:?}", id, req.action);

    match state.manager.respond(&id, req.action).await {
        Ok(()) => (
            StatusCode::OK,
            Json(PromptOpResponse {
                success: true,
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(PromptOpResponse {
                success: false,
                error: Some(e.to_string()),
            }),
        ),
    }
}

/// Internal endpoint for creating prompts (used by oqto-guard, oqto-ssh-proxy).
///
/// This endpoint blocks until the user responds or the prompt times out.
async fn create_prompt(
    State(state): State<PromptState>,
    Json(req): Json<PromptRequest>,
) -> impl IntoResponse {
    info!(
        "Internal prompt request: {} wants {} access to {}",
        req.source, req.prompt_type, req.resource
    );

    match state.manager.request(req).await {
        Ok(response) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "action": response.action,
                "responded_at": response.responded_at,
            })),
        ),
        Err(e) => (
            StatusCode::REQUEST_TIMEOUT,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string(),
            })),
        ),
    }
}

/// WebSocket handler for real-time prompt updates.
async fn websocket_handler(
    State(state): State<PromptState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, state))
}

/// Handle a WebSocket connection.
async fn handle_websocket(mut socket: WebSocket, state: PromptState) {
    info!("Prompt WebSocket connected");

    // Send initial sync of pending prompts
    let pending = state.manager.list_pending().await;
    let sync_msg = PromptMessage::Sync { prompts: pending };
    if let Ok(json) = serde_json::to_string(&sync_msg)
        && socket.send(Message::Text(json.into())).await.is_err()
    {
        error!("Failed to send initial sync");
        return;
    }

    // Subscribe to prompt updates
    let mut rx = state.manager.subscribe();

    loop {
        tokio::select! {
            // Broadcast messages from manager
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if let Ok(json) = serde_json::to_string(&msg)
                            && socket.send(Message::Text(json.into())).await.is_err() {
                                debug!("WebSocket send failed, client disconnected");
                                break;
                            }
                    }
                    Err(e) => {
                        warn!("Broadcast receive error: {}", e);
                        break;
                    }
                }
            }

            // Messages from client (for future use - e.g., responding via WebSocket)
            result = socket.recv() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        // Handle client messages (e.g., respond to prompt)
                        if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                            match client_msg {
                                ClientMessage::Respond { prompt_id, action } => {
                                    if let Err(e) = state.manager.respond(&prompt_id, action).await {
                                        warn!("Failed to respond to prompt: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        debug!("Client sent close");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        debug!("WebSocket closed");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    info!("Prompt WebSocket disconnected");
}

/// Messages from the client.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Respond {
        prompt_id: String,
        action: PromptAction,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_list_prompts_empty() {
        let manager = Arc::new(PromptManager::new());
        let app = prompt_routes(manager);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/prompts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}

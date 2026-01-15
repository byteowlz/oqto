//! Test harness for sending mock messages and events to the frontend.
//!
//! This module provides endpoints for testing interactive features like:
//! - A2UI surfaces
//! - Permission dialogs
//! - Question dialogs
//! - Message streaming
//!
//! These endpoints are only available in development mode.

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::state::AppState;
use crate::ws::WsEvent;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to send a mock event to a session.
#[derive(Debug, Deserialize)]
pub struct MockEventRequest {
    /// Session ID to send the event to.
    pub session_id: String,
    /// The event to send.
    pub event: MockEvent,
}

/// Mock events that can be sent for testing.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MockEvent {
    /// Mock permission request.
    PermissionRequest {
        permission_id: String,
        permission_type: String,
        title: String,
        #[serde(default)]
        pattern: Option<Value>,
        #[serde(default)]
        metadata: Option<Value>,
    },

    /// Mock question request.
    QuestionRequest {
        request_id: String,
        questions: Value,
        #[serde(default)]
        tool: Option<Value>,
    },

    /// Mock A2UI surface.
    A2uiSurface {
        surface_id: String,
        messages: Value,
        #[serde(default)]
        blocking: bool,
        #[serde(default)]
        request_id: Option<String>,
    },

    /// Mock text delta (streaming).
    TextDelta { message_id: String, delta: String },

    /// Mock session busy.
    SessionBusy,

    /// Mock session idle.
    SessionIdle,

    /// Mock agent connected.
    AgentConnected,

    /// Mock agent disconnected.
    AgentDisconnected { reason: String },
}

/// Response for mock event endpoint.
#[derive(Debug, Serialize)]
pub struct MockEventResponse {
    pub success: bool,
    pub message: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// Send a mock event to a session.
///
/// POST /api/test/event
///
/// This is only available in development mode.
pub async fn send_mock_event(
    State(state): State<AppState>,
    Json(request): Json<MockEventRequest>,
) -> Result<Json<MockEventResponse>, (StatusCode, String)> {
    // Check if in development mode
    if !is_dev_mode() {
        return Err((
            StatusCode::FORBIDDEN,
            "Test harness is only available in development mode".to_string(),
        ));
    }

    let session_id = request.session_id.clone();
    let event = convert_mock_event(request.session_id, request.event);

    // Send event to all users subscribed to this session
    let hub = state.ws_hub.clone();
    hub.send_to_session(&session_id, event).await;

    Ok(Json(MockEventResponse {
        success: true,
        message: format!("Mock event sent to session {}", session_id),
    }))
}

/// Request to send mock A2UI messages.
#[derive(Debug, Deserialize)]
pub struct MockA2uiRequest {
    /// Session ID.
    pub session_id: String,
    /// A2UI messages (JSONL format as array).
    pub messages: Vec<Value>,
    /// Whether this is a blocking request.
    #[serde(default)]
    pub blocking: bool,
}

/// Send mock A2UI messages to a session.
///
/// POST /api/test/a2ui
pub async fn send_mock_a2ui(
    State(state): State<AppState>,
    Json(request): Json<MockA2uiRequest>,
) -> Result<Json<MockEventResponse>, (StatusCode, String)> {
    if !is_dev_mode() {
        return Err((
            StatusCode::FORBIDDEN,
            "Test harness is only available in development mode".to_string(),
        ));
    }

    let session_id = request.session_id.clone();
    let surface_id = format!("test_surface_{}", uuid::Uuid::new_v4());

    let event = WsEvent::A2uiSurface {
        session_id: session_id.clone(),
        surface_id: surface_id.clone(),
        messages: serde_json::to_value(&request.messages).unwrap_or(Value::Array(vec![])),
        blocking: request.blocking,
        request_id: if request.blocking {
            Some(format!("test_req_{}", uuid::Uuid::new_v4()))
        } else {
            None
        },
    };

    let hub = state.ws_hub.clone();
    let connected_users = hub.connected_user_count();

    // Broadcast to all connected users for testing
    hub.broadcast_to_all(event).await;

    Ok(Json(MockEventResponse {
        success: true,
        message: format!(
            "A2UI surface {} broadcast to {} connected users (session {})",
            surface_id, connected_users, session_id
        ),
    }))
}

/// Sample A2UI surfaces for quick testing.
#[derive(Debug, Deserialize)]
pub struct SampleA2uiRequest {
    /// Session ID.
    pub session_id: String,
    /// Sample type: "simple_text", "button_group", "form", "card".
    pub sample: String,
}

/// Send a sample A2UI surface.
///
/// POST /api/test/a2ui/sample
pub async fn send_sample_a2ui(
    State(state): State<AppState>,
    Json(request): Json<SampleA2uiRequest>,
) -> Result<Json<MockEventResponse>, (StatusCode, String)> {
    if !is_dev_mode() {
        return Err((
            StatusCode::FORBIDDEN,
            "Test harness is only available in development mode".to_string(),
        ));
    }

    let messages = get_sample_a2ui(&request.sample);
    let session_id = request.session_id.clone();
    let surface_id = format!("sample_{}", request.sample);

    let event = WsEvent::A2uiSurface {
        session_id: session_id.clone(),
        surface_id: surface_id.clone(),
        messages: serde_json::to_value(&messages).unwrap_or(Value::Array(vec![])),
        blocking: request.sample == "question",
        request_id: if request.sample == "question" {
            Some(format!("sample_req_{}", uuid::Uuid::new_v4()))
        } else {
            None
        },
    };

    let hub = state.ws_hub.clone();
    let connected_users = hub.connected_user_count();

    // Broadcast to all connected users for testing
    hub.broadcast_to_all(event).await;

    Ok(Json(MockEventResponse {
        success: true,
        message: format!(
            "Sample A2UI '{}' broadcast to {} connected users (session {})",
            request.sample, connected_users, session_id
        ),
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if running in development mode.
fn is_dev_mode() -> bool {
    std::env::var("OCTO_DEV_MODE")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
        || std::env::var("RUST_ENV")
            .map(|v| v == "development")
            .unwrap_or(false)
        || cfg!(debug_assertions)
}

/// Convert a MockEvent to a WsEvent.
fn convert_mock_event(session_id: String, mock: MockEvent) -> WsEvent {
    match mock {
        MockEvent::PermissionRequest {
            permission_id,
            permission_type,
            title,
            pattern,
            metadata,
        } => WsEvent::PermissionRequest {
            session_id,
            permission_id,
            permission_type,
            title,
            pattern,
            metadata,
        },

        MockEvent::QuestionRequest {
            request_id,
            questions,
            tool,
        } => WsEvent::QuestionRequest {
            session_id,
            request_id,
            questions,
            tool,
        },

        MockEvent::A2uiSurface {
            surface_id,
            messages,
            blocking,
            request_id,
        } => WsEvent::A2uiSurface {
            session_id,
            surface_id,
            messages,
            blocking,
            request_id,
        },

        MockEvent::TextDelta { message_id, delta } => WsEvent::TextDelta {
            session_id,
            message_id,
            delta,
        },

        MockEvent::SessionBusy => WsEvent::SessionBusy { session_id },

        MockEvent::SessionIdle => WsEvent::SessionIdle { session_id },

        MockEvent::AgentConnected => WsEvent::AgentConnected { session_id },

        MockEvent::AgentDisconnected { reason } => {
            WsEvent::AgentDisconnected { session_id, reason }
        }
    }
}

/// Get sample A2UI messages for testing.
fn get_sample_a2ui(sample_type: &str) -> Vec<Value> {
    match sample_type {
        "simple_text" => vec![
            serde_json::json!({
                "surfaceUpdate": {
                    "surfaceId": "test",
                    "components": [
                        {
                            "id": "root",
                            "component": {
                                "Column": {
                                    "children": { "explicitList": ["title", "content"] }
                                }
                            }
                        },
                        {
                            "id": "title",
                            "component": {
                                "Text": {
                                    "text": { "literalString": "Hello from A2UI!" },
                                    "usageHint": "h2"
                                }
                            }
                        },
                        {
                            "id": "content",
                            "component": {
                                "Text": {
                                    "text": { "literalString": "This is a test A2UI surface rendered in Octo." },
                                    "usageHint": "body"
                                }
                            }
                        }
                    ]
                }
            }),
            serde_json::json!({
                "beginRendering": {
                    "surfaceId": "test",
                    "root": "root"
                }
            }),
        ],

        "button_group" => vec![
            serde_json::json!({
                "surfaceUpdate": {
                    "surfaceId": "test",
                    "components": [
                        {
                            "id": "root",
                            "component": {
                                "Column": {
                                    "children": { "explicitList": ["question", "buttons"] }
                                }
                            }
                        },
                        {
                            "id": "question",
                            "component": {
                                "Text": {
                                    "text": { "literalString": "Which option do you prefer?" },
                                    "usageHint": "body"
                                }
                            }
                        },
                        {
                            "id": "buttons",
                            "component": {
                                "Row": {
                                    "children": { "explicitList": ["btn1", "btn2", "btn3"] },
                                    "distribution": "spaceEvenly"
                                }
                            }
                        },
                        {
                            "id": "btn1",
                            "component": {
                                "Button": {
                                    "child": "btn1_text",
                                    "action": { "name": "select", "context": [{ "key": "value", "value": { "literalString": "option1" } }] }
                                }
                            }
                        },
                        { "id": "btn1_text", "component": { "Text": { "text": { "literalString": "Option 1" } } } },
                        {
                            "id": "btn2",
                            "component": {
                                "Button": {
                                    "child": "btn2_text",
                                    "action": { "name": "select", "context": [{ "key": "value", "value": { "literalString": "option2" } }] },
                                    "primary": true
                                }
                            }
                        },
                        { "id": "btn2_text", "component": { "Text": { "text": { "literalString": "Option 2" } } } },
                        {
                            "id": "btn3",
                            "component": {
                                "Button": {
                                    "child": "btn3_text",
                                    "action": { "name": "select", "context": [{ "key": "value", "value": { "literalString": "option3" } }] }
                                }
                            }
                        },
                        { "id": "btn3_text", "component": { "Text": { "text": { "literalString": "Option 3" } } } }
                    ]
                }
            }),
            serde_json::json!({
                "beginRendering": {
                    "surfaceId": "test",
                    "root": "root"
                }
            }),
        ],

        "form" => vec![
            serde_json::json!({
                "surfaceUpdate": {
                    "surfaceId": "test",
                    "components": [
                        {
                            "id": "root",
                            "component": {
                                "Card": { "child": "form_content" }
                            }
                        },
                        {
                            "id": "form_content",
                            "component": {
                                "Column": {
                                    "children": { "explicitList": ["title", "name_field", "email_field", "submit_btn"] }
                                }
                            }
                        },
                        {
                            "id": "title",
                            "component": {
                                "Text": {
                                    "text": { "literalString": "Contact Form" },
                                    "usageHint": "h3"
                                }
                            }
                        },
                        {
                            "id": "name_field",
                            "component": {
                                "TextField": {
                                    "label": { "literalString": "Name" },
                                    "text": { "path": "/form/name" },
                                    "textFieldType": "shortText"
                                }
                            }
                        },
                        {
                            "id": "email_field",
                            "component": {
                                "TextField": {
                                    "label": { "literalString": "Email" },
                                    "text": { "path": "/form/email" },
                                    "textFieldType": "shortText"
                                }
                            }
                        },
                        {
                            "id": "submit_btn",
                            "component": {
                                "Button": {
                                    "child": "submit_text",
                                    "primary": true,
                                    "action": {
                                        "name": "submit_form",
                                        "context": [
                                            { "key": "name", "value": { "path": "/form/name" } },
                                            { "key": "email", "value": { "path": "/form/email" } }
                                        ]
                                    }
                                }
                            }
                        },
                        { "id": "submit_text", "component": { "Text": { "text": { "literalString": "Submit" } } } }
                    ]
                }
            }),
            serde_json::json!({
                "dataModelUpdate": {
                    "surfaceId": "test",
                    "path": "/form",
                    "contents": [
                        { "key": "name", "valueString": "" },
                        { "key": "email", "valueString": "" }
                    ]
                }
            }),
            serde_json::json!({
                "beginRendering": {
                    "surfaceId": "test",
                    "root": "root"
                }
            }),
        ],

        "question" => vec![
            serde_json::json!({
                "surfaceUpdate": {
                    "surfaceId": "test",
                    "components": [
                        {
                            "id": "root",
                            "component": {
                                "Column": {
                                    "children": { "explicitList": ["question", "choices"] }
                                }
                            }
                        },
                        {
                            "id": "question",
                            "component": {
                                "Text": {
                                    "text": { "literalString": "Which framework would you like to use?" },
                                    "usageHint": "body"
                                }
                            }
                        },
                        {
                            "id": "choices",
                            "component": {
                                "MultipleChoice": {
                                    "selections": { "literalArray": [] },
                                    "options": [
                                        { "label": { "literalString": "React" }, "value": "react" },
                                        { "label": { "literalString": "Vue" }, "value": "vue" },
                                        { "label": { "literalString": "Svelte" }, "value": "svelte" },
                                        { "label": { "literalString": "Angular" }, "value": "angular" }
                                    ],
                                    "maxAllowedSelections": 1
                                }
                            }
                        }
                    ]
                }
            }),
            serde_json::json!({
                "beginRendering": {
                    "surfaceId": "test",
                    "root": "root"
                }
            }),
        ],

        _ => vec![
            serde_json::json!({
                "surfaceUpdate": {
                    "surfaceId": "test",
                    "components": [
                        {
                            "id": "root",
                            "component": {
                                "Text": {
                                    "text": { "literalString": "Unknown sample type" },
                                    "usageHint": "body"
                                }
                            }
                        }
                    ]
                }
            }),
            serde_json::json!({
                "beginRendering": {
                    "surfaceId": "test",
                    "root": "root"
                }
            }),
        ],
    }
}

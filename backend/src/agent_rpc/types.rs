//! Common types for the AgentRPC interface.

use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::collections::HashMap;

/// A conversation (chat session) from opencode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Unique conversation ID (e.g., "ses_xxx")
    pub id: String,
    /// Human-readable title
    pub title: Option<String>,
    /// Parent conversation ID (for child/branched sessions)
    pub parent_id: Option<String>,
    /// Working directory for this conversation
    pub workspace_path: String,
    /// Project name (derived from workspace path)
    pub project_name: String,
    /// Creation timestamp (milliseconds since epoch)
    pub created_at: i64,
    /// Last update timestamp (milliseconds since epoch)
    pub updated_at: i64,
    /// Whether this is currently active/running
    pub is_active: bool,
    /// OpenCode version
    pub version: Option<String>,
}

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID
    pub id: String,
    /// Conversation/session ID
    pub session_id: String,
    /// Role: "user" or "assistant"
    pub role: String,
    /// Message parts (text, tool calls, etc.)
    pub parts: Vec<MessagePart>,
    /// Creation timestamp
    pub created_at: i64,
    /// Completion timestamp (for assistant messages)
    pub completed_at: Option<i64>,
    /// Model used (for assistant messages)
    pub model: Option<MessageModel>,
    /// Token usage
    pub tokens: Option<TokenUsage>,
}

/// A part of a message (text, tool call, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessagePart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool")]
    Tool {
        tool: String,
        #[serde(rename = "callID")]
        call_id: Option<String>,
        state: Option<ToolState>,
    },
    #[serde(rename = "step-start")]
    StepStart,
    #[serde(rename = "step-finish")]
    StepFinish {
        reason: Option<String>,
        cost: Option<f64>,
        tokens: Option<TokenUsage>,
    },
    #[serde(other)]
    Unknown,
}

/// Tool execution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolState {
    pub status: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageModel {
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(rename = "modelID")]
    pub model_id: String,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub reasoning: Option<i64>,
    pub cache: Option<TokenCache>,
}

/// Token cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCache {
    pub read: Option<i64>,
    pub write: Option<i64>,
}

/// Options for starting a session.
#[derive(Debug, Clone, Default)]
pub struct StartSessionOpts {
    /// Model to use (provider/model format)
    pub model: Option<String>,
    /// Agent to use (passed to opencode via --agent flag)
    pub agent: Option<String>,
    /// Session ID to resume (if any)
    pub resume_session_id: Option<String>,
    /// Project ID for shared project sessions.
    /// When set, the session runs as the project's Linux user instead of
    /// the requesting user's Linux user, enabling multi-user access.
    pub project_id: Option<String>,
    /// Additional environment variables
    pub env: HashMap<String, String>,
}

/// Handle to a running session.
#[derive(Debug, Clone, Serialize)]
pub struct SessionHandle {
    /// Platform session ID (from octo database)
    pub session_id: String,
    /// OpenCode session ID (may differ from platform ID)
    pub opencode_session_id: Option<String>,
    /// Base URL for the opencode API
    pub api_url: String,
    /// Port for the opencode API
    pub opencode_port: u16,
    /// Port for ttyd terminal
    pub ttyd_port: u16,
    /// Port for fileserver
    pub fileserver_port: u16,
    /// Working directory
    pub workdir: String,
    /// Whether this is a newly created session or resumed
    pub is_new: bool,
}

/// Request to send a message.
#[derive(Debug, Clone)]
pub struct SendMessageRequest {
    /// Message content parts
    pub parts: Vec<SendMessagePart>,
    /// Model override
    pub model: Option<MessageModel>,
}

/// Part of a message to send.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SendMessagePart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "file")]
    File {
        mime: String,
        url: String,
        filename: Option<String>,
    },
    #[serde(rename = "agent")]
    Agent { name: String, id: Option<String> },
}

/// Health status of the backend.
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    /// Whether the backend is healthy
    pub healthy: bool,
    /// Backend mode (local/container)
    pub mode: String,
    /// Version info
    pub version: Option<String>,
    /// Additional details
    pub details: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // =========================================================================
    // Conversation Tests
    // =========================================================================

    #[test]
    fn test_conversation_creation() {
        let conv = Conversation {
            id: "conv_123".to_string(),
            title: Some("My Session".to_string()),
            parent_id: None,
            workspace_path: "/home/user/project".to_string(),
            project_name: "project".to_string(),
            created_at: 1700000000000,
            updated_at: 1700000001000,
            is_active: true,
            version: Some("1.0.0".to_string()),
        };

        assert_eq!(conv.id, "conv_123");
        assert_eq!(conv.title, Some("My Session".to_string()));
        assert!(conv.is_active);
    }

    #[test]
    fn test_conversation_clone() {
        let conv = Conversation {
            id: "conv_456".to_string(),
            title: None,
            parent_id: Some("conv_parent".to_string()),
            workspace_path: "/workspace".to_string(),
            project_name: "test".to_string(),
            created_at: 1700000000000,
            updated_at: 1700000001000,
            is_active: false,
            version: None,
        };

        let cloned = conv.clone();
        assert_eq!(cloned.id, conv.id);
        assert_eq!(cloned.parent_id, conv.parent_id);
    }

    #[test]
    fn test_conversation_serialization() {
        let conv = Conversation {
            id: "conv_789".to_string(),
            title: Some("Test".to_string()),
            parent_id: None,
            workspace_path: "/home/test".to_string(),
            project_name: "test".to_string(),
            created_at: 1700000000000,
            updated_at: 1700000001000,
            is_active: true,
            version: Some("2.0.0".to_string()),
        };

        let json = serde_json::to_string(&conv).unwrap();
        assert!(json.contains("conv_789"));
        assert!(json.contains("Test"));
        assert!(json.contains("is_active"));

        // Deserialize back
        let parsed: Conversation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, conv.id);
        assert_eq!(parsed.title, conv.title);
    }

    // =========================================================================
    // Message Tests
    // =========================================================================

    #[test]
    fn test_message_creation() {
        let msg = Message {
            id: "msg_001".to_string(),
            session_id: "ses_001".to_string(),
            role: "user".to_string(),
            parts: vec![MessagePart::Text {
                text: "Hello world".to_string(),
            }],
            created_at: 1700000000000,
            completed_at: None,
            model: None,
            tokens: None,
        };

        assert_eq!(msg.id, "msg_001");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.parts.len(), 1);
    }

    #[test]
    fn test_message_with_model_and_tokens() {
        let msg = Message {
            id: "msg_002".to_string(),
            session_id: "ses_001".to_string(),
            role: "assistant".to_string(),
            parts: vec![MessagePart::Text {
                text: "Hello!".to_string(),
            }],
            created_at: 1700000000000,
            completed_at: Some(1700000001000),
            model: Some(MessageModel {
                provider_id: "anthropic".to_string(),
                model_id: "claude-3-5-sonnet".to_string(),
            }),
            tokens: Some(TokenUsage {
                input: Some(100),
                output: Some(50),
                reasoning: None,
                cache: Some(TokenCache {
                    read: Some(80),
                    write: Some(20),
                }),
            }),
        };

        assert!(msg.completed_at.is_some());
        assert!(msg.model.is_some());
        assert!(msg.tokens.is_some());

        let model = msg.model.unwrap();
        assert_eq!(model.provider_id, "anthropic");

        let tokens = msg.tokens.unwrap();
        assert_eq!(tokens.input, Some(100));
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message {
            id: "msg_ser".to_string(),
            session_id: "ses_ser".to_string(),
            role: "user".to_string(),
            parts: vec![
                MessagePart::Text {
                    text: "Test message".to_string(),
                },
                MessagePart::StepStart,
            ],
            created_at: 1700000000000,
            completed_at: None,
            model: None,
            tokens: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("msg_ser"));
        assert!(json.contains("Test message"));
    }

    // =========================================================================
    // MessagePart Tests
    // =========================================================================

    #[test]
    fn test_message_part_text() {
        let part = MessagePart::Text {
            text: "Hello".to_string(),
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("Hello"));

        let parsed: MessagePart = serde_json::from_str(&json).unwrap();
        match parsed {
            MessagePart::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_message_part_tool() {
        let part = MessagePart::Tool {
            tool: "bash".to_string(),
            call_id: Some("call_123".to_string()),
            state: Some(ToolState {
                status: Some("completed".to_string()),
                input: Some(serde_json::json!({"command": "ls -la"})),
                output: Some("file1\nfile2".to_string()),
                title: Some("List files".to_string()),
                metadata: None,
            }),
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"tool\""));
        assert!(json.contains("bash"));
        assert!(json.contains("call_123"));

        let parsed: MessagePart = serde_json::from_str(&json).unwrap();
        match parsed {
            MessagePart::Tool {
                tool,
                call_id,
                state,
            } => {
                assert_eq!(tool, "bash");
                assert_eq!(call_id, Some("call_123".to_string()));
                assert!(state.is_some());
            }
            _ => panic!("Expected Tool variant"),
        }
    }

    #[test]
    fn test_message_part_step_start() {
        let part = MessagePart::StepStart;

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"step-start\""));

        let parsed: MessagePart = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, MessagePart::StepStart));
    }

    #[test]
    fn test_message_part_step_finish() {
        let part = MessagePart::StepFinish {
            reason: Some("completed".to_string()),
            cost: Some(0.05),
            tokens: Some(TokenUsage {
                input: Some(200),
                output: Some(100),
                reasoning: Some(50),
                cache: None,
            }),
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"step-finish\""));
        assert!(json.contains("completed"));
        assert!(json.contains("0.05"));
    }

    #[test]
    fn test_message_part_unknown_deserialization() {
        // Unknown types should deserialize to Unknown variant
        let json = r#"{"type":"future-type","data":"something"}"#;
        let parsed: MessagePart = serde_json::from_str(json).unwrap();
        assert!(matches!(parsed, MessagePart::Unknown));
    }

    // =========================================================================
    // ToolState Tests
    // =========================================================================

    #[test]
    fn test_tool_state_full() {
        let state = ToolState {
            status: Some("running".to_string()),
            input: Some(serde_json::json!({"key": "value"})),
            output: Some("output text".to_string()),
            title: Some("Tool title".to_string()),
            metadata: Some(serde_json::json!({"extra": 123})),
        };

        assert_eq!(state.status, Some("running".to_string()));
        assert!(state.input.is_some());

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("running"));
        assert!(json.contains("Tool title"));
    }

    #[test]
    fn test_tool_state_minimal() {
        let state = ToolState {
            status: None,
            input: None,
            output: None,
            title: None,
            metadata: None,
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: ToolState = serde_json::from_str(&json).unwrap();

        assert!(parsed.status.is_none());
        assert!(parsed.input.is_none());
    }

    // =========================================================================
    // MessageModel Tests
    // =========================================================================

    #[test]
    fn test_message_model() {
        let model = MessageModel {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        };

        assert_eq!(model.provider_id, "openai");
        assert_eq!(model.model_id, "gpt-4");

        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("providerID")); // Check serde rename
        assert!(json.contains("modelID"));

        let parsed: MessageModel = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider_id, model.provider_id);
    }

    // =========================================================================
    // TokenUsage Tests
    // =========================================================================

    #[test]
    fn test_token_usage_full() {
        let tokens = TokenUsage {
            input: Some(1000),
            output: Some(500),
            reasoning: Some(200),
            cache: Some(TokenCache {
                read: Some(800),
                write: Some(100),
            }),
        };

        assert_eq!(tokens.input, Some(1000));
        assert!(tokens.cache.is_some());

        let cache = tokens.cache.unwrap();
        assert_eq!(cache.read, Some(800));
    }

    #[test]
    fn test_token_usage_minimal() {
        let tokens = TokenUsage {
            input: None,
            output: None,
            reasoning: None,
            cache: None,
        };

        let json = serde_json::to_string(&tokens).unwrap();
        let parsed: TokenUsage = serde_json::from_str(&json).unwrap();

        assert!(parsed.input.is_none());
    }

    // =========================================================================
    // StartSessionOpts Tests
    // =========================================================================

    #[test]
    fn test_start_session_opts_default() {
        let opts = StartSessionOpts::default();

        assert!(opts.model.is_none());
        assert!(opts.agent.is_none());
        assert!(opts.resume_session_id.is_none());
        assert!(opts.project_id.is_none());
        assert!(opts.env.is_empty());
    }

    #[test]
    fn test_start_session_opts_with_values() {
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "secret".to_string());

        let opts = StartSessionOpts {
            model: Some("anthropic/claude-3-opus".to_string()),
            agent: Some("coding-agent".to_string()),
            resume_session_id: Some("ses_old".to_string()),
            project_id: Some("proj_123".to_string()),
            env,
        };

        assert_eq!(opts.model, Some("anthropic/claude-3-opus".to_string()));
        assert_eq!(opts.agent, Some("coding-agent".to_string()));
        assert_eq!(opts.project_id, Some("proj_123".to_string()));
        assert_eq!(opts.env.get("API_KEY").unwrap(), "secret");
    }

    #[test]
    fn test_start_session_opts_debug() {
        let opts = StartSessionOpts {
            model: Some("test-model".to_string()),
            ..Default::default()
        };

        let debug_str = format!("{:?}", opts);
        assert!(debug_str.contains("test-model"));
    }

    // =========================================================================
    // SessionHandle Tests
    // =========================================================================

    #[test]
    fn test_session_handle_creation() {
        let handle = SessionHandle {
            session_id: "ses_handle".to_string(),
            opencode_session_id: Some("opencode_123".to_string()),
            api_url: "http://localhost:41820".to_string(),
            opencode_port: 41820,
            ttyd_port: 41821,
            fileserver_port: 41822,
            workdir: "/home/user/project".to_string(),
            is_new: true,
        };

        assert_eq!(handle.session_id, "ses_handle");
        assert!(handle.is_new);
        assert_eq!(handle.opencode_port, 41820);
    }

    #[test]
    fn test_session_handle_serialization() {
        let handle = SessionHandle {
            session_id: "ses_ser".to_string(),
            opencode_session_id: None,
            api_url: "http://localhost:8080".to_string(),
            opencode_port: 8080,
            ttyd_port: 8081,
            fileserver_port: 8082,
            workdir: "/workspace".to_string(),
            is_new: false,
        };

        let json = serde_json::to_string(&handle).unwrap();
        assert!(json.contains("ses_ser"));
        assert!(json.contains("8080"));
        assert!(json.contains("is_new"));
    }

    #[test]
    fn test_session_handle_clone() {
        let handle = SessionHandle {
            session_id: "ses_clone".to_string(),
            opencode_session_id: Some("oc_123".to_string()),
            api_url: "http://example.com".to_string(),
            opencode_port: 9000,
            ttyd_port: 9001,
            fileserver_port: 9002,
            workdir: "/data".to_string(),
            is_new: true,
        };

        let cloned = handle.clone();
        assert_eq!(cloned.session_id, handle.session_id);
        assert_eq!(cloned.opencode_session_id, handle.opencode_session_id);
    }

    // =========================================================================
    // SendMessageRequest Tests
    // =========================================================================

    #[test]
    fn test_send_message_request_text_only() {
        let request = SendMessageRequest {
            parts: vec![SendMessagePart::Text {
                text: "Hello, Claude!".to_string(),
            }],
            model: None,
        };

        assert_eq!(request.parts.len(), 1);
        assert!(request.model.is_none());
    }

    #[test]
    fn test_send_message_request_with_model() {
        let request = SendMessageRequest {
            parts: vec![SendMessagePart::Text {
                text: "Test".to_string(),
            }],
            model: Some(MessageModel {
                provider_id: "anthropic".to_string(),
                model_id: "claude-3-5-sonnet".to_string(),
            }),
        };

        assert!(request.model.is_some());
        let model = request.model.unwrap();
        assert_eq!(model.provider_id, "anthropic");
    }

    #[test]
    fn test_send_message_request_multiple_parts() {
        let request = SendMessageRequest {
            parts: vec![
                SendMessagePart::Text {
                    text: "Check this file:".to_string(),
                },
                SendMessagePart::File {
                    mime: "text/plain".to_string(),
                    url: "file:///path/to/file.txt".to_string(),
                    filename: Some("file.txt".to_string()),
                },
                SendMessagePart::Agent {
                    name: "code-review".to_string(),
                    id: Some("agent_123".to_string()),
                },
            ],
            model: None,
        };

        assert_eq!(request.parts.len(), 3);
    }

    // =========================================================================
    // SendMessagePart Tests
    // =========================================================================

    #[test]
    fn test_send_message_part_text() {
        let part = SendMessagePart::Text {
            text: "Hello".to_string(),
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"text\""));

        let parsed: SendMessagePart = serde_json::from_str(&json).unwrap();
        match parsed {
            SendMessagePart::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_send_message_part_file() {
        let part = SendMessagePart::File {
            mime: "image/png".to_string(),
            url: "https://example.com/image.png".to_string(),
            filename: Some("screenshot.png".to_string()),
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"file\""));
        assert!(json.contains("image/png"));

        let parsed: SendMessagePart = serde_json::from_str(&json).unwrap();
        match parsed {
            SendMessagePart::File {
                mime,
                url,
                filename,
            } => {
                assert_eq!(mime, "image/png");
                assert_eq!(url, "https://example.com/image.png");
                assert_eq!(filename, Some("screenshot.png".to_string()));
            }
            _ => panic!("Expected File variant"),
        }
    }

    #[test]
    fn test_send_message_part_agent() {
        let part = SendMessagePart::Agent {
            name: "custom-agent".to_string(),
            id: None,
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"agent\""));
        assert!(json.contains("custom-agent"));

        let parsed: SendMessagePart = serde_json::from_str(&json).unwrap();
        match parsed {
            SendMessagePart::Agent { name, id } => {
                assert_eq!(name, "custom-agent");
                assert!(id.is_none());
            }
            _ => panic!("Expected Agent variant"),
        }
    }

    // =========================================================================
    // HealthStatus Tests
    // =========================================================================

    #[test]
    fn test_health_status_healthy() {
        let status = HealthStatus {
            healthy: true,
            mode: "local".to_string(),
            version: Some("1.0.0".to_string()),
            details: Some("All systems operational".to_string()),
        };

        assert!(status.healthy);
        assert_eq!(status.mode, "local");
    }

    #[test]
    fn test_health_status_unhealthy() {
        let status = HealthStatus {
            healthy: false,
            mode: "container".to_string(),
            version: None,
            details: Some("Container runtime unavailable".to_string()),
        };

        assert!(!status.healthy);
        assert!(status.version.is_none());
    }

    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus {
            healthy: true,
            mode: "local".to_string(),
            version: Some("2.0.0".to_string()),
            details: None,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("local"));
        assert!(json.contains("2.0.0"));
    }

    #[test]
    fn test_health_status_clone() {
        let status = HealthStatus {
            healthy: true,
            mode: "mock".to_string(),
            version: Some("test".to_string()),
            details: Some("details".to_string()),
        };

        let cloned = status.clone();
        assert_eq!(cloned.healthy, status.healthy);
        assert_eq!(cloned.mode, status.mode);
    }

    #[test]
    fn test_health_status_debug() {
        let status = HealthStatus {
            healthy: true,
            mode: "debug-test".to_string(),
            version: None,
            details: None,
        };

        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("debug-test"));
        assert!(debug_str.contains("healthy"));
    }
}

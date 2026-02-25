//! Pi RPC protocol types.
//!
//! Based on the pi-mono RPC documentation.
//! See: https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/rpc.md

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// Commands (sent to pi via stdin)
// ============================================================================

/// Base command structure sent to pi.
/// See: https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/rpc.md
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PiCommand {
    // ========================================================================
    // Prompting
    // ========================================================================
    /// Send a user prompt to the agent.
    Prompt {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageContent>>,
        #[serde(rename = "streamingBehavior", skip_serializing_if = "Option::is_none")]
        streaming_behavior: Option<String>, // "steer" or "followUp"
    },
    /// Queue a steering message to interrupt the agent mid-run.
    Steer {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        message: String,
    },
    /// Queue a follow-up message for after the agent finishes.
    FollowUp {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        message: String,
    },
    /// Abort the current agent operation.
    Abort {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    // ========================================================================
    // Session Management
    // ========================================================================
    /// Start a fresh session.
    NewSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(rename = "parentSession", skip_serializing_if = "Option::is_none")]
        parent_session: Option<String>,
    },
    /// Switch to a different session file.
    SwitchSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(rename = "sessionPath")]
        session_path: String,
    },
    /// Set session display name.
    SetSessionName {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        name: String,
    },
    /// Export session to HTML.
    ExportHtml {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(rename = "outputPath", skip_serializing_if = "Option::is_none")]
        output_path: Option<String>,
    },

    // ========================================================================
    // State Queries
    // ========================================================================
    /// Get current session state.
    GetState {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// Get all messages in the conversation.
    GetMessages {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// Get the last assistant message text.
    GetLastAssistantText {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// Get token usage and cost statistics.
    GetSessionStats {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// Get available commands (extensions, templates, skills).
    GetCommands {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    // ========================================================================
    // Model Configuration
    // ========================================================================
    /// Switch to a specific model.
    SetModel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        provider: String,
        #[serde(rename = "modelId")]
        model_id: String,
    },
    /// Cycle to the next available model.
    CycleModel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// List all configured models.
    GetAvailableModels {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    // ========================================================================
    // Thinking Configuration
    // ========================================================================
    /// Set the reasoning/thinking level.
    SetThinkingLevel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        level: String, // "off", "minimal", "low", "medium", "high", "xhigh"
    },
    /// Cycle through thinking levels.
    CycleThinkingLevel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    // ========================================================================
    // Queue Modes
    // ========================================================================
    /// Set steering message delivery mode.
    SetSteeringMode {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        mode: String, // "all" or "one-at-a-time"
    },
    /// Set follow-up message delivery mode.
    SetFollowUpMode {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        mode: String, // "all" or "one-at-a-time"
    },

    // ========================================================================
    // Compaction
    // ========================================================================
    /// Manually compact conversation context.
    Compact {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(rename = "customInstructions", skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
    },
    /// Enable/disable automatic compaction.
    SetAutoCompaction {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        enabled: bool,
    },

    // ========================================================================
    // Retry
    // ========================================================================
    /// Enable/disable automatic retry on transient errors.
    SetAutoRetry {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        enabled: bool,
    },
    /// Abort an in-progress retry.
    AbortRetry {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    // ========================================================================
    // Forking
    // ========================================================================
    /// Fork from a previous user message.
    Fork {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(rename = "entryId")]
        entry_id: String,
    },
    /// Get user messages available for forking.
    GetForkMessages {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    // ========================================================================
    // Bash Execution
    // ========================================================================
    /// Execute a shell command.
    Bash {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        command: String,
    },
    /// Abort a running bash command.
    AbortBash {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    // ========================================================================
    // Extension UI
    // ========================================================================
    /// Respond to an extension UI dialog request.
    ExtensionUiResponse {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        confirmed: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cancelled: Option<bool>,
    },
}

/// Image content for prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageContent {
    #[serde(rename = "type")]
    pub content_type: String, // "image"
    pub source: ImageSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 {
        #[serde(rename = "mediaType")]
        media_type: String,
        data: String,
    },
    Url {
        url: String,
    },
}

// ============================================================================
// Responses (received from pi via stdout)
// ============================================================================

/// Response to a command.
#[derive(Debug, Clone, Deserialize)]
pub struct PiResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ============================================================================
// Events (streamed from pi via stdout during operation)
// ============================================================================

/// Events streamed from pi during agent operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum PiEvent {
    /// Agent begins processing.
    AgentStart,
    /// Agent completes.
    AgentEnd { messages: Vec<AgentMessage> },
    /// New turn begins.
    TurnStart,
    /// Turn completes.
    TurnEnd {
        message: AgentMessage,
        #[serde(rename = "toolResults", default)]
        tool_results: Vec<ToolResultMessage>,
    },
    /// Message begins.
    MessageStart { message: AgentMessage },
    /// Streaming update.
    MessageUpdate {
        message: AgentMessage,
        #[serde(rename = "assistantMessageEvent")]
        assistant_message_event: AssistantMessageEvent,
    },
    /// Message completes.
    MessageEnd { message: AgentMessage },
    /// Tool begins execution.
    ToolExecutionStart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        args: Value,
    },
    /// Tool execution progress.
    ToolExecutionUpdate {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        args: Value,
        #[serde(rename = "partialResult")]
        partial_result: ToolResult,
    },
    /// Tool completes.
    ToolExecutionEnd {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        result: ToolResult,
        #[serde(rename = "isError")]
        is_error: bool,
    },
    /// Extension UI request (RPC mode).
    ExtensionUiRequest(ExtensionUiRequest),
    /// Auto-compaction begins.
    AutoCompactionStart { reason: String },
    /// Auto-compaction completes.
    AutoCompactionEnd {
        result: Option<CompactionResult>,
        aborted: bool,
        #[serde(rename = "willRetry")]
        will_retry: bool,
    },
    /// Auto-retry begins.
    AutoRetryStart {
        attempt: u32,
        #[serde(rename = "maxAttempts")]
        max_attempts: u32,
        #[serde(rename = "delayMs")]
        delay_ms: u64,
        #[serde(rename = "errorMessage")]
        error_message: String,
    },
    /// Auto-retry completes.
    AutoRetryEnd {
        success: bool,
        attempt: u32,
        #[serde(rename = "finalError")]
        final_error: Option<String>,
    },
    /// Hook threw an error.
    HookError {
        #[serde(rename = "hookPath")]
        hook_path: String,
        event: String,
        error: String,
    },
    /// Unknown event type (forward-compatible).
    #[serde(other)]
    Unknown,
}

/// Streaming delta events for assistant messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantMessageEvent {
    Start {
        partial: Value,
    },
    TextStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        partial: Value,
    },
    TextDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
        partial: Value,
    },
    TextEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        content: String,
        partial: Value,
    },
    ThinkingStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        partial: Value,
    },
    ThinkingDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
        partial: Value,
    },
    ThinkingEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        /// The thinking content (Pi sends this as "content" not "thinking")
        #[serde(alias = "thinking")]
        content: String,
        partial: Value,
    },
    ToolcallStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        partial: Value,
    },
    ToolcallDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
        partial: Value,
    },
    ToolcallEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        #[serde(rename = "toolCall")]
        tool_call: ToolCall,
        partial: Value,
    },
    Done {
        reason: String, // "stop", "length", "toolUse"
        #[serde(default)]
        message: Option<AgentMessage>,
    },
    Error {
        reason: String, // "aborted", "error"
        #[serde(default)]
        error: Option<AgentMessage>,
    },
    /// Unknown assistant event type (forward-compatible).
    #[serde(other)]
    Unknown,
}

// ============================================================================
// Message Types
// ============================================================================

/// Agent message (can be user, assistant, or tool result).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: String,
    #[serde(default)]
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    /// Tool call id for tool-result messages.
    #[serde(rename = "toolCallId", skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool name for tool-result messages.
    #[serde(rename = "toolName", skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Whether the tool result is an error.
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    // Assistant-specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    #[serde(rename = "stopReason", skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// Preserve unknown fields for forward-compatibility.
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    #[serde(rename = "cacheRead", default)]
    pub cache_read: u64,
    #[serde(rename = "cacheWrite", default)]
    pub cache_write: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<TokenCost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCost {
    pub input: f64,
    pub output: f64,
    #[serde(rename = "cacheRead", default)]
    pub cache_read: f64,
    #[serde(rename = "cacheWrite", default)]
    pub cache_write: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    pub role: String, // "toolResult"
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    pub content: Vec<ContentBlock>,
    #[serde(rename = "isError")]
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContentBlock {
    Text { text: String },
    Thinking { thinking: String },
    ToolCall(ToolCall),
    Image { source: ImageSource },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub summary: String,
    #[serde(rename = "firstKeptEntryId")]
    pub first_kept_entry_id: String,
    #[serde(rename = "tokensBefore")]
    pub tokens_before: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

// ============================================================================
// State Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiState {
    pub model: Option<PiModel>,
    #[serde(rename = "thinkingLevel")]
    pub thinking_level: String,
    #[serde(rename = "isStreaming")]
    pub is_streaming: bool,
    #[serde(rename = "isCompacting")]
    pub is_compacting: bool,
    #[serde(rename = "steeringMode")]
    pub steering_mode: String,
    #[serde(rename = "followUpMode")]
    pub follow_up_mode: String,
    #[serde(rename = "sessionFile")]
    pub session_file: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "sessionName")]
    pub session_name: Option<String>,
    #[serde(rename = "autoCompactionEnabled")]
    pub auto_compaction_enabled: bool,
    #[serde(rename = "messageCount")]
    pub message_count: u64,
    #[serde(rename = "pendingMessageCount")]
    pub pending_message_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiModel {
    pub id: String,
    pub name: String,
    pub api: String,
    pub provider: String,
    #[serde(rename = "baseUrl", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub reasoning: bool,
    pub input: Vec<String>,
    #[serde(rename = "contextWindow")]
    pub context_window: u64,
    #[serde(rename = "maxTokens")]
    pub max_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<ModelCost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    #[serde(rename = "cacheRead", default)]
    pub cache_read: f64,
    #[serde(rename = "cacheWrite", default)]
    pub cache_write: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    #[serde(rename = "sessionFile")]
    pub session_file: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "userMessages")]
    pub user_messages: u64,
    #[serde(rename = "assistantMessages")]
    pub assistant_messages: u64,
    #[serde(rename = "toolCalls")]
    pub tool_calls: u64,
    #[serde(rename = "toolResults")]
    pub tool_results: u64,
    #[serde(rename = "totalMessages")]
    pub total_messages: u64,
    pub tokens: SessionTokens,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionUiRequest {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(rename = "statusKey", default)]
    pub status_key: Option<String>,
    #[serde(rename = "statusText", default)]
    pub status_text: Option<String>,
    #[serde(rename = "widgetKey", default)]
    pub widget_key: Option<String>,
    #[serde(rename = "widgetLines", default)]
    pub widget_lines: Option<Vec<String>>,
    #[serde(rename = "widgetPlacement", default)]
    pub widget_placement: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub prefill: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(rename = "notifyType", default)]
    pub notify_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTokens {
    pub input: u64,
    pub output: u64,
    #[serde(rename = "cacheRead")]
    pub cache_read: u64,
    #[serde(rename = "cacheWrite")]
    pub cache_write: u64,
    pub total: u64,
}

// ============================================================================
// Parsed message from stdout (can be response or event)
// ============================================================================

/// Message received from pi stdout - either a response or an event.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum PiMessage {
    Response(PiResponse),
    Event(PiEvent),
}

impl PiMessage {
    /// Parse a JSON line from pi stdout.
    pub fn parse(line: &str) -> Result<Self, serde_json::Error> {
        // First, check if it's a response
        let value: Value = serde_json::from_str(line)?;

        if let Some(msg_type) = value.get("type").and_then(|v| v.as_str())
            && msg_type == "response"
        {
            let response: PiResponse = serde_json::from_value(value)?;
            return Ok(PiMessage::Response(response));
        }

        // Otherwise, try to parse as an event
        let event: PiEvent = serde_json::from_value(value)?;
        Ok(PiMessage::Event(event))
    }

    /// Parse a line that may contain one or more concatenated JSON objects.
    ///
    /// Pi sometimes flushes multiple JSON objects on a single line when the
    /// output buffer fills up mid-write (e.g. `{...}{...}\n`). A plain
    /// `serde_json::from_str` fails with "trailing characters" in that case.
    /// This method uses `serde_json::StreamDeserializer` to yield every
    /// valid object on the line.
    pub fn parse_all(line: &str) -> Vec<Result<Self, String>> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // Fast path: try single parse first (vast majority of lines)
        match Self::parse(trimmed) {
            Ok(msg) => return vec![Ok(msg)],
            Err(ref e) if !e.to_string().contains("trailing characters") => {
                // Not a concatenation issue -- genuine parse error
                return vec![Err(format!("{e}"))];
            }
            Err(_) => {} // trailing characters -- fall through to stream parse
        }

        // Slow path: multiple JSON objects concatenated on one line.
        // Use StreamDeserializer to split them.
        let stream = serde_json::Deserializer::from_str(trimmed).into_iter::<serde_json::Value>();
        let mut results = Vec::new();
        for value_result in stream {
            match value_result {
                Ok(value) => {
                    if let Some(msg_type) = value.get("type").and_then(|v| v.as_str()) {
                        if msg_type == "response" {
                            match serde_json::from_value::<PiResponse>(value) {
                                Ok(resp) => results.push(Ok(PiMessage::Response(resp))),
                                Err(e) => results.push(Err(format!("response parse: {e}"))),
                            }
                        } else {
                            match serde_json::from_value::<PiEvent>(value) {
                                Ok(evt) => results.push(Ok(PiMessage::Event(evt))),
                                Err(e) => results.push(Err(format!("event parse: {e}"))),
                            }
                        }
                    } else {
                        match serde_json::from_value::<PiEvent>(value) {
                            Ok(evt) => results.push(Ok(PiMessage::Event(evt))),
                            Err(e) => results.push(Err(format!("event parse (no type): {e}"))),
                        }
                    }
                }
                Err(e) => {
                    results.push(Err(format!("json stream: {e}")));
                    break; // Stream deserializer can't recover from errors
                }
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal valid MessageStart event (simplest PiEvent variant)
    fn make_event() -> String {
        r#"{"type":"message_start","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#.to_string()
    }

    #[test]
    fn test_parse_all_single_object() {
        let line = make_event();
        let results = PiMessage::parse_all(&line);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok(), "Got: {:?}", results[0]);
    }

    #[test]
    fn test_parse_all_concatenated_objects() {
        // Simulate what Pi does when buffer fills at 4096 bytes
        let obj = make_event();
        let concat = format!("{obj}{obj}");

        let results = PiMessage::parse_all(&concat);
        assert_eq!(results.len(), 2, "Should parse both concatenated objects");
        assert!(results[0].is_ok(), "First: {:?}", results[0]);
        assert!(results[1].is_ok(), "Second: {:?}", results[1]);
    }

    #[test]
    fn test_parse_all_empty() {
        assert!(PiMessage::parse_all("").is_empty());
        assert!(PiMessage::parse_all("   ").is_empty());
    }

    #[test]
    fn test_parse_all_invalid_json() {
        let results = PiMessage::parse_all("not json at all");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
    }

    #[test]
    fn test_parse_all_response() {
        let line =
            r#"{"type":"response","success":true,"id":"get_state","data":{"sessionId":"abc"}}"#;
        let results = PiMessage::parse_all(line);
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], Ok(PiMessage::Response(_))));
    }

    #[test]
    fn test_parse_all_three_objects() {
        let obj = make_event();
        let concat = format!("{obj}{obj}{obj}");
        let results = PiMessage::parse_all(&concat);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn test_parse_all_mixed_response_and_event() {
        let event = make_event();
        let response = r#"{"type":"response","success":true,"id":"test"}"#;
        let concat = format!("{event}{response}");
        let results = PiMessage::parse_all(&concat);
        assert_eq!(results.len(), 2);
        assert!(matches!(results[0], Ok(PiMessage::Event(_))));
        assert!(matches!(results[1], Ok(PiMessage::Response(_))));
    }
}

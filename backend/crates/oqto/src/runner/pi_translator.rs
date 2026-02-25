//! Translator from native Pi events to canonical protocol events.
//!
//! This module converts Pi's JSONL events (from stdout) into canonical
//! `EventPayload` variants defined in `oqto-protocol`. It uses the
//! `SessionState` state machine to track agent lifecycle and ensure
//! correct state transitions.
//!
//! ## Design Rules
//!
//! 1. `agent_start`/`agent_end` are authoritative idle/working transitions.
//! 2. Extension `setStatus` (via `oqto_phase`) only refines phase WITHIN working state.
//! 3. Native events (tool_execution_start, auto_compaction_start) also refine phase.
//! 4. One Pi event can produce 0..N canonical events (returned as `Vec`).
//! 5. The translator is stateful: it holds a `SessionState` and a current message ID.

use serde_json::Value;

use oqto_protocol::Part;
use oqto_protocol::events::{
    AgentPhase, CommandResponse, CompactReason, EventPayload, InputRequest, NotifyLevel,
    ToolCallInfo,
};
use oqto_protocol::messages::{Message, Role, StopReason, Usage};

use crate::pi::{
    AgentMessage, AssistantMessageEvent, CompactionResult, ContentBlock, ExtensionUiRequest,
    ImageSource, PiEvent,
};
use oqto_protocol::runner::SessionState;

// ============================================================================
// Translator
// ============================================================================

/// Translates native Pi events into canonical protocol events.
///
/// Holds per-session state needed for translation: the `SessionState`
/// state machine and the current streaming message ID.
pub struct PiTranslator {
    /// The canonical state machine.
    pub state: SessionState,

    /// Current streaming assistant message ID (set on MessageStart, cleared on MessageEnd).
    current_message_id: Option<String>,

    /// Counter for generating deterministic message IDs within a session.
    message_counter: u64,

    /// Client-generated ID for the pending user message.
    /// Set by the runner before sending a prompt, consumed when translating
    /// the user message in agent_end.
    pending_client_id: Option<String>,

    /// Whether any streaming occurred during the current agent turn.
    /// Set true on MessageStart, cleared on AgentEnd. Used to suppress
    /// the redundant `Messages` event from `agent_end` when the frontend
    /// already received all content via streaming events.
    streaming_occurred: bool,

    /// True when Pi is retrying a failed LLM request (between auto_retry_start
    /// and auto_retry_end). During this period, agent_end/agent_start cycles
    /// are suppressed to avoid flickering idle->working transitions on the
    /// frontend.
    in_retry_cycle: bool,
}

impl Default for PiTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl PiTranslator {
    pub fn new() -> Self {
        Self {
            state: SessionState::default(),
            current_message_id: None,
            message_counter: 0,
            pending_client_id: None,
            streaming_occurred: false,
            in_retry_cycle: false,
        }
    }

    /// Set the client ID for the next user message.
    /// Called by the runner before sending a prompt command.
    pub fn set_pending_client_id(&mut self, client_id: Option<String>) {
        self.pending_client_id = client_id;
    }

    /// Take and clear the pending client ID.
    fn take_pending_client_id(&mut self) -> Option<String> {
        self.pending_client_id.take()
    }

    /// Get the current message ID, or generate one if not set.
    fn ensure_message_id(&mut self) -> String {
        if let Some(ref id) = self.current_message_id {
            id.clone()
        } else {
            self.message_counter += 1;
            let id = format!("msg_{}", self.message_counter);
            self.current_message_id = Some(id.clone());
            id
        }
    }

    /// Translate a native Pi event into zero or more canonical events.
    pub fn translate(&mut self, event: &PiEvent) -> Vec<EventPayload> {
        match event {
            PiEvent::AgentStart => self.on_agent_start(),
            PiEvent::AgentEnd { messages } => self.on_agent_end(messages),
            PiEvent::TurnStart => vec![],
            PiEvent::TurnEnd { .. } => vec![],
            PiEvent::MessageStart { message } => self.on_message_start(message),
            PiEvent::MessageUpdate {
                assistant_message_event,
                message,
            } => self.on_message_update(assistant_message_event, message),
            PiEvent::MessageEnd { message } => self.on_message_end(message),
            PiEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
            } => self.on_tool_start(tool_call_id, tool_name, args),
            PiEvent::ToolExecutionUpdate {
                tool_call_id,
                tool_name,
                partial_result,
                ..
            } => self.on_tool_progress(tool_call_id, tool_name, partial_result),
            PiEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
            } => self.on_tool_end(tool_call_id, tool_name, result, *is_error),
            PiEvent::ExtensionUiRequest(req) => self.on_extension_ui(req),
            PiEvent::AutoCompactionStart { reason } => self.on_compaction_start(reason),
            PiEvent::AutoCompactionEnd {
                result,
                aborted,
                will_retry,
            } => self.on_compaction_end(result.is_some() && !aborted, *will_retry, result.as_ref()),
            PiEvent::AutoRetryStart {
                attempt,
                max_attempts,
                delay_ms,
                error_message,
            } => self.on_retry_start(*attempt, *max_attempts, *delay_ms, error_message),
            PiEvent::AutoRetryEnd {
                success,
                attempt,
                final_error,
            } => self.on_retry_end(*success, *attempt, final_error.clone()),
            PiEvent::HookError {
                hook_path,
                event,
                error,
            } => self.on_hook_error(hook_path, event, error),
            PiEvent::Unknown => vec![],
        }
    }

    // -- Lifecycle events --

    fn on_agent_start(&mut self) -> Vec<EventPayload> {
        if self.in_retry_cycle {
            // During retries, Pi emits agent_end -> agent_start for each attempt.
            // Suppress the agent_start -> AgentWorking(Generating) event to avoid
            // an idle->working flicker on the frontend. The retry.start event
            // already emitted AgentWorking(Retrying).
            return vec![];
        }
        let event = self.state.on_agent_start();
        vec![event]
    }

    fn on_agent_end(&mut self, messages: &[AgentMessage]) -> Vec<EventPayload> {
        if self.in_retry_cycle {
            // During retries, suppress the agent_end -> AgentIdle transition
            // to avoid flickering. The retry cycle will end with either
            // retry.end(success) -> AgentWorking(Generating) or
            // retry.end(failure) -> AgentError.
            self.streaming_occurred = false;
            self.current_message_id = None;
            return vec![];
        }

        let mut events = Vec::new();

        // Take the pending client_id - it should be attached to the last user message
        // in this turn (the one that triggered agent_start).
        let pending_client_id = self.take_pending_client_id();

        // Only emit the Messages payload when streaming did NOT occur.
        //
        // When streaming was active, the frontend already received all content
        // via streaming events (text_delta, tool events, stream.message_end).
        // Emitting a redundant Messages event here would cause the frontend's
        // mergeServerMessages to append duplicate messages with different IDs
        // (each call to pi_agent_message_to_canonical generates a new UUID).
        // This manifests as the user's prompt text appearing inside the
        // agent's response bubble.
        //
        // The Messages event is still emitted for non-streaming turns (e.g.
        // reconnection, or turns where Pi completed without streaming events).
        let had_streaming = self.streaming_occurred;
        self.streaming_occurred = false;

        if !had_streaming && !messages.is_empty() {
            // Find the last user message index to attach client_id
            let last_user_idx = messages
                .iter()
                .enumerate()
                .rev()
                .find(|(_, m)| m.role == "user" || m.role == "human")
                .map(|(i, _)| i);

            let canonical_messages: Vec<Message> = messages
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    // Attach client_id only to the last user message
                    let client_id = if Some(i) == last_user_idx {
                        pending_client_id.clone()
                    } else {
                        None
                    };
                    pi_agent_message_to_canonical(m, i as u32, client_id)
                })
                .collect();
            events.push(EventPayload::Messages {
                messages: canonical_messages,
            });
        }

        // Clear current message tracking.
        self.current_message_id = None;

        // Transition state.
        let idle_event = self.state.on_agent_end();
        events.push(idle_event);

        events
    }

    // -- Streaming events --

    fn on_message_start(&mut self, message: &AgentMessage) -> Vec<EventPayload> {
        // Only emit streaming events for assistant messages. Pi sends
        // message_start/end for every message including user echoes
        // (steer) and tool-result messages (role "user"/"toolResult").
        // Those are already represented by tool.start/tool.end events
        // and the optimistic user message on the frontend. Forwarding
        // them causes raw tool output to leak into the UI as text.
        let is_assistant = message.role == "assistant" || message.role == "agent";
        if !is_assistant {
            // Still mark streaming as occurred (Pi did produce events),
            // but don't track a message_id so on_message_end will also
            // suppress the corresponding StreamMessageEnd.
            self.streaming_occurred = true;
            return vec![];
        }

        self.message_counter += 1;
        let msg_id = format!("msg_{}", self.message_counter);
        self.current_message_id = Some(msg_id.clone());
        self.streaming_occurred = true;

        vec![EventPayload::StreamMessageStart {
            message_id: msg_id,
            role: message.role.clone(),
        }]
    }

    fn on_message_update(
        &mut self,
        ame: &AssistantMessageEvent,
        _message: &AgentMessage,
    ) -> Vec<EventPayload> {
        let msg_id = self.ensure_message_id();

        match ame {
            AssistantMessageEvent::TextDelta {
                delta,
                content_index,
                ..
            } => vec![EventPayload::StreamTextDelta {
                message_id: msg_id,
                delta: delta.clone(),
                content_index: *content_index,
            }],

            AssistantMessageEvent::ThinkingDelta {
                delta,
                content_index,
                ..
            } => vec![EventPayload::StreamThinkingDelta {
                message_id: msg_id,
                delta: delta.clone(),
                content_index: *content_index,
            }],

            AssistantMessageEvent::ToolcallStart { .. } => {
                // We don't have the tool info yet (name, id) -- just the index.
                // The real info comes in ToolcallEnd. Suppress this placeholder
                // to avoid creating an "Unknown Tool" card on the frontend that
                // can never be matched to the real tool call.
                vec![]
            }

            AssistantMessageEvent::ToolcallDelta {
                delta,
                content_index,
                ..
            } => vec![EventPayload::StreamToolCallDelta {
                message_id: msg_id,
                tool_call_id: String::new(),
                delta: delta.clone(),
                content_index: *content_index,
            }],

            AssistantMessageEvent::ToolcallEnd {
                tool_call,
                content_index,
                ..
            } => vec![EventPayload::StreamToolCallEnd {
                message_id: msg_id,
                tool_call_id: tool_call.id.clone(),
                tool_call: ToolCallInfo {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    input: tool_call.arguments.clone(),
                },
                content_index: *content_index,
            }],

            AssistantMessageEvent::Done { reason, .. } => {
                vec![EventPayload::StreamDone {
                    reason: pi_stop_reason(reason),
                }]
            }

            AssistantMessageEvent::Error { reason, error } => {
                let error_text = error
                    .as_ref()
                    .and_then(|m| extract_text_content(&m.content))
                    .unwrap_or_else(|| reason.clone());

                vec![EventPayload::AgentError {
                    error: error_text,
                    recoverable: reason == "aborted",
                    phase: Some(AgentPhase::Generating),
                }]
            }

            // Start/End markers for individual content blocks - no canonical equivalent needed.
            AssistantMessageEvent::Start { .. }
            | AssistantMessageEvent::TextStart { .. }
            | AssistantMessageEvent::TextEnd { .. }
            | AssistantMessageEvent::ThinkingStart { .. }
            | AssistantMessageEvent::ThinkingEnd { .. }
            | AssistantMessageEvent::Unknown => vec![],
        }
    }

    fn on_message_end(&mut self, message: &AgentMessage) -> Vec<EventPayload> {
        // If no current message is being tracked, this message_end is for a
        // non-assistant message that was suppressed in on_message_start.
        if self.current_message_id.is_none() {
            return vec![];
        }

        let idx = self.message_counter.saturating_sub(1) as u32;
        // message_end is for assistant messages during streaming - no client_id needed
        let canonical = pi_agent_message_to_canonical(message, idx, None);

        // Clear current message tracking.
        self.current_message_id = None;

        vec![EventPayload::StreamMessageEnd { message: canonical }]

        // Note: when stopReason == "error", Pi follows up with auto_retry_start
        // which transitions to AgentWorking(Retrying). If all retries fail,
        // auto_retry_end emits AgentError(recoverable=false). The error
        // information is also available in the canonical message's stop_reason
        // and metadata.errorMessage fields for display purposes.
    }

    // -- Tool execution events --

    fn on_tool_start(
        &mut self,
        tool_call_id: &str,
        tool_name: &str,
        args: &Value,
    ) -> Vec<EventPayload> {
        let mut events = Vec::new();

        // Update state machine.
        let phase_event = self
            .state
            .on_native_phase(AgentPhase::ToolRunning, Some(tool_name.to_string()));
        events.push(phase_event);

        events.push(EventPayload::ToolStart {
            tool_call_id: tool_call_id.to_string(),
            name: tool_name.to_string(),
            input: Some(args.clone()),
        });

        events
    }

    fn on_tool_progress(
        &mut self,
        tool_call_id: &str,
        tool_name: &str,
        partial_result: &crate::pi::ToolResult,
    ) -> Vec<EventPayload> {
        vec![EventPayload::ToolProgress {
            tool_call_id: tool_call_id.to_string(),
            name: tool_name.to_string(),
            partial_output: serde_json::to_value(partial_result).unwrap_or_default(),
        }]
    }

    fn on_tool_end(
        &mut self,
        tool_call_id: &str,
        tool_name: &str,
        result: &crate::pi::ToolResult,
        is_error: bool,
    ) -> Vec<EventPayload> {
        let mut events = Vec::new();

        // Back to generating after tool completes.
        let phase_event = self.state.on_native_phase(AgentPhase::Generating, None);
        events.push(phase_event);

        events.push(EventPayload::ToolEnd {
            tool_call_id: tool_call_id.to_string(),
            name: tool_name.to_string(),
            output: serde_json::to_value(result).unwrap_or_default(),
            is_error,
            duration_ms: None,
        });

        events
    }

    // -- Extension UI events --

    fn on_extension_ui(&mut self, req: &ExtensionUiRequest) -> Vec<EventPayload> {
        // Check if this is an oqto_phase status update from our bridge extension.
        if req.method == "setStatus"
            && let Some(ref key) = req.status_key
        {
            if key == "oqto_phase" {
                return self.on_oqto_phase_status(req.status_text.as_deref());
            }
            if key == "oqto_title_changed" {
                return self.on_title_changed(req.status_text.as_deref());
            }
        }

        // Other extension UI requests map to input_needed events.
        match req.method.as_str() {
            "select" => {
                vec![EventPayload::AgentInputNeeded {
                    request: InputRequest::Select {
                        request_id: req.id.clone(),
                        title: req.title.clone().unwrap_or_default(),
                        options: req.options.clone().unwrap_or_default(),
                        timeout: req.timeout,
                    },
                }]
            }
            "confirm" => {
                vec![EventPayload::AgentInputNeeded {
                    request: InputRequest::Confirm {
                        request_id: req.id.clone(),
                        title: req.title.clone().unwrap_or_default(),
                        message: req.message.clone().unwrap_or_default(),
                        timeout: req.timeout,
                    },
                }]
            }
            "input" => {
                vec![EventPayload::AgentInputNeeded {
                    request: InputRequest::Input {
                        request_id: req.id.clone(),
                        title: req.title.clone().unwrap_or_default(),
                        placeholder: req.placeholder.clone(),
                        timeout: req.timeout,
                    },
                }]
            }
            "notify" => {
                let level = match req.notify_type.as_deref() {
                    Some("error") => NotifyLevel::Error,
                    Some("warning") | Some("warn") => NotifyLevel::Warning,
                    _ => NotifyLevel::Info,
                };
                vec![EventPayload::Notify {
                    level,
                    message: req.message.clone().unwrap_or_default(),
                }]
            }
            "setStatus" => {
                // Non-oqto_phase status updates pass through as Status events.
                vec![EventPayload::Status {
                    key: req.status_key.clone().unwrap_or_default(),
                    text: req.status_text.clone(),
                }]
            }
            _ => vec![],
        }
    }

    fn on_oqto_phase_status(&mut self, status_text: Option<&str>) -> Vec<EventPayload> {
        match status_text {
            Some(text) if !text.is_empty() => {
                if let Some((phase, detail)) = AgentPhase::from_extension_status(text) {
                    self.state
                        .on_extension_phase(Some(phase), detail)
                        .into_iter()
                        .collect()
                } else {
                    vec![]
                }
            }
            // Clear status -> fall back to generating.
            _ => self
                .state
                .on_extension_phase(None, None)
                .into_iter()
                .collect(),
        }
    }

    fn on_title_changed(&mut self, status_text: Option<&str>) -> Vec<EventPayload> {
        let Some(raw_name) = status_text.filter(|s| !s.is_empty()) else {
            return vec![];
        };
        let parsed = crate::pi::session_parser::ParsedTitle::parse(raw_name);
        let title = parsed.display_title().to_string();
        if title.is_empty() {
            return vec![];
        }
        vec![EventPayload::SessionTitleChanged {
            title,
            readable_id: parsed.readable_id.map(|s| s.to_string()),
        }]
    }

    // -- Compaction events --

    fn on_compaction_start(&mut self, reason: &str) -> Vec<EventPayload> {
        let mut events = Vec::new();

        let phase_event = self.state.on_native_phase(AgentPhase::Compacting, None);
        events.push(phase_event);

        let compact_reason = if reason.contains("overflow") {
            CompactReason::Overflow
        } else {
            CompactReason::Threshold
        };
        events.push(EventPayload::CompactStart {
            reason: compact_reason,
        });

        events
    }

    fn on_compaction_end(
        &mut self,
        success: bool,
        will_retry: bool,
        result: Option<&CompactionResult>,
    ) -> Vec<EventPayload> {
        let mut events = Vec::new();

        events.push(EventPayload::CompactEnd {
            success,
            will_retry,
            error: None,
            summary: result.map(|r| r.summary.clone()),
            tokens_before: result.map(|r| r.tokens_before),
        });

        // If not retrying, transition back to generating (still within agent working).
        if !will_retry {
            let phase_event = self.state.on_native_phase(AgentPhase::Generating, None);
            events.push(phase_event);
        }

        events
    }

    // -- Retry events --

    fn on_retry_start(
        &mut self,
        attempt: u32,
        max_attempts: u32,
        delay_ms: u64,
        error: &str,
    ) -> Vec<EventPayload> {
        self.in_retry_cycle = true;

        let mut events = Vec::new();

        let phase_event = self.state.on_native_phase(AgentPhase::Retrying, None);
        events.push(phase_event);

        events.push(EventPayload::RetryStart {
            attempt,
            max_attempts,
            delay_ms,
            error: error.to_string(),
        });

        events
    }

    fn on_retry_end(
        &mut self,
        success: bool,
        attempt: u32,
        final_error: Option<String>,
    ) -> Vec<EventPayload> {
        self.in_retry_cycle = false;

        let mut events = Vec::new();

        events.push(EventPayload::RetryEnd {
            success,
            attempt,
            final_error: final_error.clone(),
        });

        if success {
            // On success, back to generating.
            let phase_event = self.state.on_native_phase(AgentPhase::Generating, None);
            events.push(phase_event);
        } else {
            // All retries exhausted -- emit a non-recoverable error so the
            // frontend surfaces it to the user. Without this, the error is
            // silently swallowed and the agent just goes idle.
            let error_text = final_error
                .unwrap_or_else(|| "LLM request failed after all retries".to_string());
            events.push(EventPayload::AgentError {
                error: error_text,
                recoverable: false,
                phase: Some(AgentPhase::Generating),
            });
        }

        events
    }

    // -- Hook errors --

    fn on_hook_error(&self, hook_path: &str, event: &str, error: &str) -> Vec<EventPayload> {
        vec![EventPayload::Notify {
            level: NotifyLevel::Warning,
            message: format!("Hook error in {hook_path} ({event}): {error}"),
        }]
    }
}

// ============================================================================
// Pi response translator
// ============================================================================

/// Translate a Pi command response into a canonical CommandResponse event.
///
/// Special handling for commands that emit config events:
/// - `set_model` -> `ConfigModelChanged` event
/// - `set_thinking_level` -> `ConfigThinkingLevelChanged` event
pub fn pi_response_to_canonical(
    pi_response: &crate::pi::PiResponse,
    cmd_name: &str,
) -> EventPayload {
    // Special case: set_thinking_level emits ConfigThinkingLevelChanged event
    if cmd_name == "set_thinking_level"
        && let Some(data) = &pi_response.data
        && let Some(level) = data.get("level")
    {
        let level_str = level.as_str().unwrap_or("").to_string();
        return EventPayload::ConfigThinkingLevelChanged { level: level_str };
    }

    // Special case: set_model emits ConfigModelChanged event
    if cmd_name == "set_model"
        && let Some(data) = &pi_response.data
        && let Some(model) = data.get("model")
        && let Some(provider) = data.get("provider")
    {
        let model_id = model.as_str().unwrap_or("").to_string();
        let provider_str = provider.as_str().unwrap_or("").to_string();
        return EventPayload::ConfigModelChanged {
            provider: provider_str,
            model_id,
        };
    }

    // Default: wrap as CommandResponse
    EventPayload::Response(CommandResponse {
        id: pi_response.id.clone().unwrap_or_default(),
        cmd: cmd_name.to_string(),
        success: pi_response.success,
        data: pi_response.data.clone(),
        error: pi_response.error.clone(),
    })
}

// ============================================================================
// Message conversion
// ============================================================================

/// Convert a Pi `AgentMessage` to a canonical `Message`.
///
/// The `client_id` parameter is used for optimistic message matching. The frontend
/// sends a client-generated ID with the prompt; this ID is included in the resulting
/// user message so the frontend can correlate its optimistic message with the
/// persisted version.
pub fn pi_agent_message_to_canonical(
    msg: &AgentMessage,
    idx: u32,
    client_id: Option<String>,
) -> Message {
    let role = match msg.role.as_str() {
        "user" | "human" => Role::User,
        "assistant" | "agent" => Role::Assistant,
        "system" => Role::System,
        "tool" | "toolResult" => Role::Tool,
        _ => Role::User,
    };

    let timestamp = msg
        .timestamp
        .map(|t| t as i64)
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    let parts = pi_content_to_parts(&msg.content, msg);
    let id = format!("msg_{}", uuid::Uuid::new_v4().simple());

    Message {
        id,
        idx,
        role,
        client_id,
        sender: None,
        parts,
        created_at: timestamp,
        model: msg.model.clone(),
        provider: msg.provider.clone(),
        stop_reason: msg.stop_reason.as_deref().map(pi_stop_reason),
        usage: msg.usage.as_ref().map(|u| Usage {
            input_tokens: u.input,
            output_tokens: u.output,
            cache_read_tokens: if u.cache_read > 0 {
                Some(u.cache_read)
            } else {
                None
            },
            cache_write_tokens: if u.cache_write > 0 {
                Some(u.cache_write)
            } else {
                None
            },
            cost_usd: u.cost.as_ref().map(|c| c.total),
        }),
        tool_call_id: msg.tool_call_id.clone(),
        tool_name: msg.tool_name.clone(),
        is_error: msg.is_error,
        metadata: if msg.extra.is_empty() {
            None
        } else {
            serde_json::to_value(&msg.extra).ok()
        },
    }
}

/// Convert Pi content (string or array of content blocks) to canonical Parts.
fn pi_content_to_parts(content: &Value, msg: &AgentMessage) -> Vec<Part> {
    let mut parts = Vec::new();

    let is_tool_result = msg.role == "tool" || msg.role == "toolResult";

    // For tool result messages, only emit a ToolResult part.
    // The text content is redundant with the tool_result output and would
    // leak into the chat as visible text if included.
    if is_tool_result {
        let tool_call_id = msg
            .tool_call_id
            .clone()
            .unwrap_or_else(|| format!("tc_{}", uuid::Uuid::new_v4().simple()));
        parts.push(Part::tool_result(
            tool_call_id,
            Some(content.clone()),
            msg.is_error.unwrap_or(false),
        ));
    } else {
        match content {
            Value::String(text) => {
                parts.push(Part::text(text));
            }
            Value::Array(blocks) => {
                for block in blocks {
                    if let Ok(cb) = serde_json::from_value::<ContentBlock>(block.clone()) {
                        match cb {
                            ContentBlock::Text { text } => {
                                parts.push(Part::text(&text));
                            }
                            ContentBlock::Thinking { thinking } => {
                                parts.push(Part::thinking(&thinking));
                            }
                            ContentBlock::ToolCall(tc) => {
                                parts.push(Part::tool_call(&tc.id, &tc.name, Some(tc.arguments)));
                            }
                            ContentBlock::Image { source } => {
                                parts.push(pi_image_to_part(&source));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    parts
}

/// Convert a Pi image source to a canonical Image part.
fn pi_image_to_part(source: &ImageSource) -> Part {
    use hstry_core::parts::MediaSource;

    let media_source = match source {
        ImageSource::Url { url } => MediaSource::url(url),
        ImageSource::Base64 { media_type, data } => MediaSource::base64(data, media_type),
    };

    Part::Image {
        id: format!("part_{}", uuid::Uuid::new_v4().simple()),
        source: media_source,
        alt: None,
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a Pi stop reason string to canonical StopReason.
fn pi_stop_reason(reason: &str) -> StopReason {
    match reason {
        "stop" | "end_turn" | "completed" => StopReason::Stop,
        "length" | "max_tokens" => StopReason::Length,
        "toolUse" | "tool_use" => StopReason::ToolUse,
        "error" => StopReason::Error,
        "aborted" | "abort" => StopReason::Aborted,
        _ => StopReason::Stop,
    }
}

/// Extract text content from a Pi message value.
fn extract_text_content(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => {
            let texts: Vec<String> = blocks
                .iter()
                .filter_map(|b| {
                    if let Some(obj) = b.as_object()
                        && obj.get("type").and_then(|t| t.as_str()) == Some("text")
                    {
                        return obj.get("text").and_then(|t| t.as_str()).map(String::from);
                    }
                    None
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n\n"))
            }
        }
        _ => None,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi::{TokenCost, TokenUsage};

    #[test]
    fn test_agent_lifecycle() {
        let mut t = PiTranslator::new();

        // Start
        let events = t.translate(&PiEvent::AgentStart);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            EventPayload::AgentWorking {
                phase: AgentPhase::Generating,
                ..
            }
        ));
        assert!(t.state.is_working());

        // End
        let events = t.translate(&PiEvent::AgentEnd { messages: vec![] });
        assert!(events.iter().any(|e| matches!(e, EventPayload::AgentIdle)));
        assert!(t.state.is_idle());
    }

    #[test]
    fn test_text_streaming() {
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        // MessageStart
        let msg = make_assistant_message();
        let events = t.translate(&PiEvent::MessageStart {
            message: msg.clone(),
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], EventPayload::StreamMessageStart { .. }));

        // TextDelta
        let events = t.translate(&PiEvent::MessageUpdate {
            message: msg.clone(),
            assistant_message_event: AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "Hello".to_string(),
                partial: Value::Null,
            },
        });
        assert_eq!(events.len(), 1);
        if let EventPayload::StreamTextDelta { delta, .. } = &events[0] {
            assert_eq!(delta, "Hello");
        } else {
            panic!("Expected StreamTextDelta");
        }

        // ThinkingDelta
        let events = t.translate(&PiEvent::MessageUpdate {
            message: msg.clone(),
            assistant_message_event: AssistantMessageEvent::ThinkingDelta {
                content_index: 0,
                delta: "Hmm...".to_string(),
                partial: Value::Null,
            },
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            EventPayload::StreamThinkingDelta { .. }
        ));
    }

    #[test]
    fn test_tool_execution_lifecycle() {
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        // ToolExecutionStart -> phase change + tool.start
        let events = t.translate(&PiEvent::ToolExecutionStart {
            tool_call_id: "tc_1".to_string(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({"command": "ls"}),
        });
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            EventPayload::AgentWorking {
                phase: AgentPhase::ToolRunning,
                ..
            }
        ));
        assert!(matches!(events[1], EventPayload::ToolStart { .. }));

        // ToolExecutionEnd -> phase change + tool.end
        let events = t.translate(&PiEvent::ToolExecutionEnd {
            tool_call_id: "tc_1".to_string(),
            tool_name: "bash".to_string(),
            result: crate::pi::ToolResult {
                content: vec![],
                details: None,
            },
            is_error: false,
        });
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            EventPayload::AgentWorking {
                phase: AgentPhase::Generating,
                ..
            }
        ));
        assert!(matches!(events[1], EventPayload::ToolEnd { .. }));
    }

    #[test]
    fn test_extension_oqto_phase() {
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        let req = ExtensionUiRequest {
            id: "ext_1".to_string(),
            method: "setStatus".to_string(),
            title: None,
            message: None,
            options: None,
            timeout: None,
            status_key: Some("oqto_phase".to_string()),
            status_text: Some("tool_running:bash".to_string()),
            widget_key: None,
            widget_lines: None,
            widget_placement: None,
            text: None,
            prefill: None,
            placeholder: None,
            notify_type: None,
        };

        let events = t.translate(&PiEvent::ExtensionUiRequest(req));
        assert_eq!(events.len(), 1);
        if let EventPayload::AgentWorking { phase, detail } = &events[0] {
            assert_eq!(*phase, AgentPhase::ToolRunning);
            assert_eq!(detail.as_deref(), Some("bash"));
        } else {
            panic!("Expected AgentWorking");
        }
    }

    #[test]
    fn test_extension_phase_ignored_when_idle() {
        let mut t = PiTranslator::new();
        // State is Initializing (default), not Working.

        let req = ExtensionUiRequest {
            id: "ext_1".to_string(),
            method: "setStatus".to_string(),
            title: None,
            message: None,
            options: None,
            timeout: None,
            status_key: Some("oqto_phase".to_string()),
            status_text: Some("generating".to_string()),
            widget_key: None,
            widget_lines: None,
            widget_placement: None,
            text: None,
            prefill: None,
            placeholder: None,
            notify_type: None,
        };

        let events = t.translate(&PiEvent::ExtensionUiRequest(req));
        assert!(events.is_empty());
    }

    #[test]
    fn test_compaction_lifecycle() {
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        // Start
        let events = t.translate(&PiEvent::AutoCompactionStart {
            reason: "threshold".to_string(),
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EventPayload::CompactStart { .. }))
        );
        assert!(matches!(
            t.state,
            SessionState::Working {
                phase: AgentPhase::Compacting,
                ..
            }
        ));

        // End (success, no retry)
        let events = t.translate(&PiEvent::AutoCompactionEnd {
            result: Some(crate::pi::CompactionResult {
                summary: "Compacted".to_string(),
                first_kept_entry_id: "entry_1".to_string(),
                tokens_before: 10000,
                details: None,
            }),
            aborted: false,
            will_retry: false,
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EventPayload::CompactEnd { .. }))
        );
        // Should return to generating.
        assert!(matches!(
            t.state,
            SessionState::Working {
                phase: AgentPhase::Generating,
                ..
            }
        ));
    }

    #[test]
    fn test_retry_lifecycle() {
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        let events = t.translate(&PiEvent::AutoRetryStart {
            attempt: 1,
            max_attempts: 3,
            delay_ms: 1000,
            error_message: "rate limited".to_string(),
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EventPayload::RetryStart { .. }))
        );
        assert!(matches!(
            t.state,
            SessionState::Working {
                phase: AgentPhase::Retrying,
                ..
            }
        ));

        let events = t.translate(&PiEvent::AutoRetryEnd {
            success: true,
            attempt: 1,
            final_error: None,
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EventPayload::RetryEnd { success: true, .. }))
        );
        assert!(matches!(
            t.state,
            SessionState::Working {
                phase: AgentPhase::Generating,
                ..
            }
        ));
    }

    #[test]
    fn test_hook_error() {
        let t = PiTranslator::new();
        let events = t.on_hook_error("hooks/pre-commit", "agent_start", "syntax error");
        assert_eq!(events.len(), 1);
        if let EventPayload::Notify { level, message } = &events[0] {
            assert_eq!(*level, NotifyLevel::Warning);
            assert!(message.contains("syntax error"));
        } else {
            panic!("Expected Notify");
        }
    }

    #[test]
    fn test_message_conversion() {
        let msg = AgentMessage {
            role: "assistant".to_string(),
            content: Value::String("Hello!".to_string()),
            timestamp: Some(1700000000000),
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: Some("anthropic".to_string()),
            provider: Some("anthropic".to_string()),
            model: Some("claude-sonnet-4-20250514".to_string()),
            usage: Some(TokenUsage {
                input: 100,
                output: 50,
                cache_read: 10,
                cache_write: 5,
                cost: Some(TokenCost {
                    input: 0.003,
                    output: 0.015,
                    cache_read: 0.0003,
                    cache_write: 0.00375,
                    total: 0.02205,
                }),
            }),
            stop_reason: Some("stop".to_string()),
            extra: Default::default(),
        };

        let canonical = pi_agent_message_to_canonical(&msg, 0, None);

        assert_eq!(canonical.role, Role::Assistant);
        assert_eq!(canonical.idx, 0);
        assert_eq!(canonical.model.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(canonical.provider.as_deref(), Some("anthropic"));
        assert_eq!(canonical.stop_reason, Some(StopReason::Stop));
        assert_eq!(canonical.created_at, 1700000000000);

        let usage = canonical.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, Some(10));
        assert_eq!(usage.cache_write_tokens, Some(5));
        assert!((usage.cost_usd.unwrap() - 0.02205).abs() < 0.0001);

        assert_eq!(canonical.parts.len(), 1);
    }

    #[test]
    fn test_tool_result_message_conversion() {
        let msg = AgentMessage {
            role: "tool".to_string(),
            content: serde_json::json!([{"type": "text", "text": "file.rs contents..."}]),
            timestamp: Some(1700000000000),
            tool_call_id: Some("tc_123".to_string()),
            tool_name: Some("read".to_string()),
            is_error: Some(false),
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
            extra: Default::default(),
        };

        let canonical = pi_agent_message_to_canonical(&msg, 1, None);

        assert_eq!(canonical.role, Role::Tool);
        assert_eq!(canonical.tool_call_id.as_deref(), Some("tc_123"));
        assert_eq!(canonical.tool_name.as_deref(), Some("read"));
        assert_eq!(canonical.is_error, Some(false));
        // Should have text part + tool_result part
        assert!(canonical.parts.len() >= 2);
    }

    #[test]
    fn test_agent_end_with_messages() {
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        let messages = vec![
            AgentMessage {
                role: "user".to_string(),
                content: Value::String("Hello".to_string()),
                timestamp: Some(1700000000000),
                tool_call_id: None,
                tool_name: None,
                is_error: None,
                api: None,
                provider: None,
                model: None,
                usage: None,
                stop_reason: None,
                extra: Default::default(),
            },
            make_assistant_message(),
        ];

        let events = t.translate(&PiEvent::AgentEnd { messages });

        // Should have Messages event + AgentIdle
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EventPayload::Messages { .. }))
        );
        assert!(events.iter().any(|e| matches!(e, EventPayload::AgentIdle)));
    }

    #[test]
    fn test_agent_end_suppresses_messages_after_streaming() {
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        // Simulate streaming: MessageStart sets streaming_occurred = true
        let msg = make_assistant_message();
        t.translate(&PiEvent::MessageStart {
            message: msg.clone(),
        });

        let messages = vec![
            AgentMessage {
                role: "user".to_string(),
                content: Value::String("Hello".to_string()),
                timestamp: Some(1700000000000),
                tool_call_id: None,
                tool_name: None,
                is_error: None,
                api: None,
                provider: None,
                model: None,
                usage: None,
                stop_reason: None,
                extra: Default::default(),
            },
            make_assistant_message(),
        ];

        let events = t.translate(&PiEvent::AgentEnd { messages });

        // Messages event should NOT be emitted after streaming (to avoid
        // duplicate/echoed messages on the frontend).
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, EventPayload::Messages { .. })),
            "Messages event should be suppressed when streaming occurred"
        );
        // AgentIdle should still be emitted.
        assert!(events.iter().any(|e| matches!(e, EventPayload::AgentIdle)));
    }

    #[test]
    fn test_extension_input_dialogs() {
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        // Select dialog
        let req = ExtensionUiRequest {
            id: "ext_1".to_string(),
            method: "select".to_string(),
            title: Some("Choose".to_string()),
            message: None,
            options: Some(vec!["a".to_string(), "b".to_string()]),
            timeout: Some(30000),
            status_key: None,
            status_text: None,
            widget_key: None,
            widget_lines: None,
            widget_placement: None,
            text: None,
            prefill: None,
            placeholder: None,
            notify_type: None,
        };

        let events = t.translate(&PiEvent::ExtensionUiRequest(req));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            EventPayload::AgentInputNeeded {
                request: InputRequest::Select { .. }
            }
        ));
    }

    #[test]
    fn test_pi_response_translation() {
        let response = crate::pi::PiResponse {
            success: true,
            id: Some("cmd_1".to_string()),
            data: Some(serde_json::json!({"model": "claude-3-opus"})),
            error: None,
        };

        let event = pi_response_to_canonical(&response, "set_model");
        if let EventPayload::Response(resp) = event {
            assert_eq!(resp.id, "cmd_1");
            assert_eq!(resp.cmd, "set_model");
            assert!(resp.success);
            assert!(resp.data.is_some());
        } else {
            panic!("Expected Response event");
        }
    }

    #[test]
    fn test_stop_reason_mapping() {
        assert_eq!(pi_stop_reason("stop"), StopReason::Stop);
        assert_eq!(pi_stop_reason("end_turn"), StopReason::Stop);
        assert_eq!(pi_stop_reason("length"), StopReason::Length);
        assert_eq!(pi_stop_reason("toolUse"), StopReason::ToolUse);
        assert_eq!(pi_stop_reason("tool_use"), StopReason::ToolUse);
        assert_eq!(pi_stop_reason("error"), StopReason::Error);
        assert_eq!(pi_stop_reason("aborted"), StopReason::Aborted);
        assert_eq!(pi_stop_reason("unknown"), StopReason::Stop);
    }

    #[test]
    fn test_non_assistant_messages_suppressed() {
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        // User message (steer echo) should produce no streaming events
        let user_msg = AgentMessage {
            role: "user".to_string(),
            content: Value::String("Hello".to_string()),
            timestamp: Some(1700000000000),
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
            extra: Default::default(),
        };
        let events = t.translate(&PiEvent::MessageStart {
            message: user_msg.clone(),
        });
        assert!(events.is_empty(), "User message_start should be suppressed");

        let events = t.translate(&PiEvent::MessageEnd { message: user_msg });
        assert!(events.is_empty(), "User message_end should be suppressed");

        // Tool result message should also be suppressed
        let tool_msg = AgentMessage {
            role: "toolResult".to_string(),
            content: serde_json::json!([{"type": "text", "text": "file contents"}]),
            timestamp: Some(1700000000000),
            tool_call_id: Some("tc_1".to_string()),
            tool_name: Some("read".to_string()),
            is_error: Some(false),
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
            extra: Default::default(),
        };
        let events = t.translate(&PiEvent::MessageStart {
            message: tool_msg.clone(),
        });
        assert!(
            events.is_empty(),
            "Tool result message_start should be suppressed"
        );

        let events = t.translate(&PiEvent::MessageEnd { message: tool_msg });
        assert!(
            events.is_empty(),
            "Tool result message_end should be suppressed"
        );

        // Assistant message should still produce events
        let assistant_msg = make_assistant_message();
        let events = t.translate(&PiEvent::MessageStart {
            message: assistant_msg.clone(),
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], EventPayload::StreamMessageStart { .. }));

        let events = t.translate(&PiEvent::MessageEnd {
            message: assistant_msg,
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], EventPayload::StreamMessageEnd { .. }));
    }

    #[test]
    fn test_tool_use_flow_with_suppressed_messages() {
        // Simulate a full tool-using conversation:
        // 1. user echo (suppressed)
        // 2. assistant thinking + tool_use
        // 3. tool execution
        // 4. tool result message (suppressed)
        // 5. assistant text response
        let mut t = PiTranslator::new();
        t.translate(&PiEvent::AgentStart);

        // 1. User echo
        let user_msg = AgentMessage {
            role: "user".to_string(),
            content: Value::String("read README.md".to_string()),
            ..make_empty_message()
        };
        assert!(
            t.translate(&PiEvent::MessageStart {
                message: user_msg.clone()
            })
            .is_empty()
        );
        assert!(
            t.translate(&PiEvent::MessageEnd { message: user_msg })
                .is_empty()
        );

        // 2. Assistant with tool_use
        let asst_msg = make_assistant_message();
        let events = t.translate(&PiEvent::MessageStart {
            message: asst_msg.clone(),
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], EventPayload::StreamMessageStart { .. }));

        let events = t.translate(&PiEvent::MessageEnd { message: asst_msg });
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], EventPayload::StreamMessageEnd { .. }));

        // 3. Tool execution
        let events = t.translate(&PiEvent::ToolExecutionStart {
            tool_call_id: "tc_1".to_string(),
            tool_name: "read".to_string(),
            args: serde_json::json!({"path": "README.md"}),
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EventPayload::ToolStart { .. }))
        );

        let events = t.translate(&PiEvent::ToolExecutionEnd {
            tool_call_id: "tc_1".to_string(),
            tool_name: "read".to_string(),
            result: crate::pi::ToolResult {
                content: vec![],
                details: None,
            },
            is_error: false,
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EventPayload::ToolEnd { .. }))
        );

        // 4. Tool result message (suppressed)
        let tool_msg = AgentMessage {
            role: "toolResult".to_string(),
            content: serde_json::json!("file contents here"),
            tool_call_id: Some("tc_1".to_string()),
            tool_name: Some("read".to_string()),
            ..make_empty_message()
        };
        assert!(
            t.translate(&PiEvent::MessageStart {
                message: tool_msg.clone()
            })
            .is_empty()
        );
        assert!(
            t.translate(&PiEvent::MessageEnd { message: tool_msg })
                .is_empty()
        );

        // 5. Final assistant response
        let final_msg = make_assistant_message();
        let events = t.translate(&PiEvent::MessageStart {
            message: final_msg.clone(),
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], EventPayload::StreamMessageStart { .. }));

        let events = t.translate(&PiEvent::MessageEnd { message: final_msg });
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], EventPayload::StreamMessageEnd { .. }));

        // streaming_occurred should be true (assistant messages streamed)
        assert!(t.streaming_occurred);
    }

    // Helper to create a minimal empty message for struct update syntax.
    fn make_empty_message() -> AgentMessage {
        AgentMessage {
            role: String::new(),
            content: Value::Null,
            timestamp: None,
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
            extra: Default::default(),
        }
    }

    // Helper to create a minimal assistant message.
    fn make_assistant_message() -> AgentMessage {
        AgentMessage {
            role: "assistant".to_string(),
            content: Value::String("Hi there!".to_string()),
            timestamp: Some(1700000000000),
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: Some("anthropic".to_string()),
            provider: Some("anthropic".to_string()),
            model: Some("claude-sonnet-4-20250514".to_string()),
            usage: None,
            stop_reason: Some("stop".to_string()),
            extra: Default::default(),
        }
    }
}

/**
 * Canonical protocol types for Oqto agent communication.
 *
 * These types mirror the Rust `oqto-protocol` crate. The Rust types are the
 * source of truth; this file is kept in sync manually.
 *
 * The frontend speaks only the canonical protocol. It does not know or care
 * which agent harness is running. The runner translates native agent events
 * into canonical format.
 */

// ============================================================================
// Parts (atomic content units within a message)
// ============================================================================

/** Tool execution status. */
export type ToolStatus = "pending" | "running" | "success" | "error";

/** Source for media content. Tagged union on `source` field. */
export type MediaSource =
	| { source: "url"; url: string; mimeType?: string }
	| { source: "attachmentRef"; attachmentId: string; mimeType?: string }
	| { source: "base64"; data: string; mimeType: string };

/** A range within a file. */
export type FileRange = {
	startLine?: number;
	endLine?: number;
};

/** A content part within a message. Tagged union on `type` field. */
export type Part =
	// --- Core ---
	| { type: "text"; id: string; text: string; format?: "markdown" | "plain" }
	| { type: "thinking"; id: string; text: string }
	| {
			type: "tool_call";
			id: string;
			toolCallId: string;
			name: string;
			input?: unknown;
			status: ToolStatus;
	  }
	| {
			type: "tool_result";
			id: string;
			toolCallId: string;
			name?: string;
			output?: unknown;
			isError: boolean;
			durationMs?: number;
	  }
	// --- Media ---
	| {
			type: "file_ref";
			id: string;
			uri: string;
			label?: string;
			range?: FileRange;
	  }
	| ({ type: "image"; id: string; alt?: string } & MediaSource)
	| ({
			type: "audio";
			id: string;
			durationSec?: number;
			transcript?: string;
	  } & MediaSource)
	| ({ type: "video"; id: string; durationSec?: number } & MediaSource)
	| ({
			type: "attachment";
			id: string;
			filename?: string;
			sizeBytes?: number;
	  } & MediaSource)
	// --- Extensions ---
	| {
			type: `x-${string}`;
			id: string;
			payload?: unknown;
			meta?: Record<string, unknown>;
	  };

// ============================================================================
// Identity (who produced a message)
// ============================================================================

/** The type of message sender. */
export type SenderType = "user" | "agent" | "system";

/**
 * Who produced a message. Used for multi-user workspaces and delegation.
 *
 * Omitted for simple single-user conversations where the sender is
 * implied by the role. When present, the frontend renders a labeled bubble
 * with the sender's name and a distinct color.
 *
 * For the LLM, the backend inlines identity as `[name]: content` in the
 * user message text so the agent understands who is speaking.
 */
export type Sender = {
	type: SenderType;
	/** Stable identifier: user ID for humans, session ID for agents. */
	id: string;
	/** Human-readable display name (e.g. "Alice", "pi:ses_abc"). */
	name: string;
	/** Which runner the sender is on (agents only). */
	runner_id?: string;
	/** Which session the sender is from (agents only, for delegation). */
	session_id?: string;
};

// ============================================================================
// Messages (persistent conversation content)
// ============================================================================

/** Message role. */
export type Role = "user" | "assistant" | "system" | "tool";

/** Why generation stopped. */
export type StopReason = "stop" | "length" | "tool_use" | "error" | "aborted";

/** Token usage for a message. */
export type Usage = {
	input_tokens: number;
	output_tokens: number;
	cache_read_tokens?: number;
	cache_write_tokens?: number;
	cost_usd?: number;
};

/** A conversation message. Stored in hstry, rendered by the frontend. */
export type Message = {
	id: string;
	idx: number;
	role: Role;
	/** Who produced this message. Omitted when implied by role (single-user). */
	sender?: Sender;
	parts: Part[];
	created_at: number;

	// Assistant-specific
	model?: string;
	provider?: string;
	stop_reason?: StopReason;
	usage?: Usage;

	// Tool-result-specific
	tool_call_id?: string;
	tool_name?: string;
	is_error?: boolean;

	// Forward-compatible extras
	metadata?: Record<string, unknown>;
};

// ============================================================================
// Events (ephemeral real-time signals)
// ============================================================================

/** What the agent is currently doing. */
export type AgentPhase =
	| "generating"
	| "thinking"
	| "tool_running"
	| "compacting"
	| "retrying"
	| "initializing";

/** Process health information from the runner. */
export type ProcessHealth = {
	alive: boolean;
	pid?: number;
	rss_bytes?: number;
	cpu_pct?: number;
	uptime_s?: number;
};

/** Agent input request types. */
export type InputRequest =
	| {
			type: "select";
			request_id: string;
			title: string;
			options: string[];
			timeout?: number;
	  }
	| {
			type: "confirm";
			request_id: string;
			title: string;
			message: string;
			timeout?: number;
	  }
	| {
			type: "input";
			request_id: string;
			title: string;
			placeholder?: string;
			timeout?: number;
	  }
	| {
			type: "permission";
			request_id: string;
			title: string;
			description?: string;
			metadata?: unknown;
	  };

/** Completed tool call info. */
export type ToolCallInfo = {
	id: string;
	name: string;
	input: unknown;
};

/** Reason for compaction. */
export type CompactReason = "threshold" | "overflow";

/** Notification severity level. */
export type NotifyLevel = "info" | "warning" | "error";

/** State of an active Pi session on the runner. */
export type ActivePiSession = {
	session_id: string;
	state:
		| "starting"
		| "idle"
		| "streaming"
		| "compacting"
		| "aborting"
		| "stopping";
	cwd: string;
	provider?: string;
	model?: string;
	last_activity: number;
	subscriber_count: number;
};

/** Response to a command. */
export type CommandResponse = {
	id: string;
	cmd: string;
	success: boolean;
	data?: unknown;
	error?: string;
};

/** All canonical event payloads. Tagged union on `event` field. */
export type EventPayload =
	// Session lifecycle
	| { event: "session.created"; resumed: boolean; harness: string }
	| { event: "session.closed"; reason?: string }
	| { event: "session.heartbeat"; process: ProcessHealth }
	// Agent state
	| { event: "agent.idle" }
	| { event: "agent.working"; phase: AgentPhase; detail?: string }
	| {
			event: "agent.error";
			error: string;
			recoverable: boolean;
			phase?: AgentPhase;
	  }
	| { event: "agent.input_needed"; request: InputRequest }
	| { event: "agent.input_resolved"; request_id: string }
	// Streaming
	| { event: "stream.message_start"; message_id: string; role: string }
	| {
			event: "stream.text_delta";
			message_id: string;
			delta: string;
			content_index: number;
	  }
	| {
			event: "stream.thinking_delta";
			message_id: string;
			delta: string;
			content_index: number;
	  }
	| {
			event: "stream.tool_call_start";
			message_id: string;
			tool_call_id: string;
			name: string;
			content_index: number;
	  }
	| {
			event: "stream.tool_call_delta";
			message_id: string;
			tool_call_id: string;
			delta: string;
			content_index: number;
	  }
	| {
			event: "stream.tool_call_end";
			message_id: string;
			tool_call_id: string;
			tool_call: ToolCallInfo;
			content_index: number;
	  }
	| { event: "stream.message_end"; message: Message }
	| { event: "stream.done"; reason: StopReason }
	// Tool execution
	| { event: "tool.start"; tool_call_id: string; name: string; input?: unknown }
	| {
			event: "tool.progress";
			tool_call_id: string;
			name: string;
			partial_output: unknown;
	  }
	| {
			event: "tool.end";
			tool_call_id: string;
			name: string;
			output: unknown;
			is_error: boolean;
			duration_ms?: number;
	  }
	// Auto-recovery
	| {
			event: "retry.start";
			attempt: number;
			max_attempts: number;
			delay_ms: number;
			error: string;
	  }
	| {
			event: "retry.end";
			success: boolean;
			attempt: number;
			final_error?: string;
	  }
	| { event: "compact.start"; reason: CompactReason }
	| {
			event: "compact.end";
			success: boolean;
			will_retry: boolean;
			error?: string;
			summary?: string;
			tokens_before?: number;
	  }
	// Config changes
	| { event: "config.model_changed"; provider: string; model_id: string }
	| { event: "config.thinking_level_changed"; level: string }
	// Notifications
	| { event: "notify"; level: NotifyLevel; message: string }
	| { event: "status"; key: string; text: string | null }
	// Delegation
	| {
			event: "delegate.start";
			request_id: string;
			target_session_id: string;
			target_runner_id: string;
			mode: DelegateMode;
	  }
	| { event: "delegate.delta"; request_id: string; delta: string }
	| {
			event: "delegate.end";
			request_id: string;
			response: Message;
			responder: Sender;
			duration_ms?: number;
	  }
	| {
			event: "delegate.error";
			request_id: string;
			error: string;
			code: DelegateErrorCode;
	  }
	// Messages sync
	| { event: "messages"; messages: Message[] }
	| { event: "persisted"; message_count: number }
	// Command response (fields are flattened, not nested under "response")
	| ({ event: "response" } & CommandResponse);

/** A canonical event with routing metadata. */
export type Event = {
	session_id: string;
	runner_id: string;
	ts: number;
} & EventPayload;

// ============================================================================
// Commands (frontend -> backend -> runner)
// ============================================================================

/** Configuration for creating a new agent session. */
export type SessionConfig = {
	harness: string;
	cwd?: string;
	provider?: string;
	model?: string;
	continue_session?: string;
};

/** Image attachment for prompts. */
export type ImageAttachment = {
	data: string;
	media_type: string;
};

/** All canonical command payloads. Tagged union on `cmd` field. */
export type CommandPayload =
	// Session lifecycle
	| { cmd: "session.create"; config: SessionConfig }
	| { cmd: "session.close" }
	| { cmd: "session.new"; parent_session?: string }
	| { cmd: "session.switch"; session_path: string }
	// Agent commands
	| { cmd: "prompt"; message: string; images?: ImageAttachment[] }
	| { cmd: "steer"; message: string }
	| { cmd: "follow_up"; message: string }
	| { cmd: "abort" }
	| {
			cmd: "input_response";
			request_id: string;
			value?: string;
			confirmed?: boolean;
			cancelled?: boolean;
	  }
	// Query commands
	| { cmd: "get_state" }
	| { cmd: "get_messages" }
	| { cmd: "get_stats" }
	| { cmd: "get_models"; workdir?: string }
	| { cmd: "get_commands" }
	| { cmd: "get_fork_points" }
	| { cmd: "list_sessions" }
	// Configuration commands
	| { cmd: "set_model"; provider: string; model_id: string }
	| { cmd: "cycle_model" }
	| { cmd: "set_thinking_level"; level: string }
	| { cmd: "cycle_thinking_level" }
	| { cmd: "set_auto_compaction"; enabled: boolean }
	| { cmd: "set_auto_retry"; enabled: boolean }
	| { cmd: "compact"; instructions?: string }
	| { cmd: "abort_retry" }
	| { cmd: "set_session_name"; name: string }
	// Forking
	| { cmd: "fork"; entry_id: string }
	// Delegation
	| { cmd: "delegate"; request: DelegateRequest }
	| { cmd: "delegate.cancel"; request_id: string };

/** A canonical command with routing metadata. */
export type Command = {
	id?: string;
	session_id: string;
	runner_id?: string;
} & CommandPayload;

// ============================================================================
// Delegation (cross-agent communication)
// ============================================================================

/** Whether to block for the response or fire-and-forget. */
export type DelegateMode = "sync" | "async";

/** Delegation error categories. */
export type DelegateErrorCode =
	| "target_not_found"
	| "permission_denied"
	| "timeout"
	| "target_error"
	| "cancelled"
	| "runner_unreachable";

/** Request to delegate a message to another agent session. */
export type DelegateRequest = {
	target_session_id: string;
	target_runner_id?: string;
	message: string;
	mode: DelegateMode;
	/** Sandbox profile to apply to the target for this delegation. */
	sandbox_profile?: string;
	/** Max wait time (ms). Default: 300000 (5 min). */
	timeout_ms?: number;
	/** Max output tokens for the delegated response. */
	max_tokens?: number;
	/** Opaque context passed through to the target. */
	context?: unknown;
};

/** Permission policy for delegation between sessions. */
export type DelegationPermission = {
	source: string;
	target: string;
	effect: "allow" | "deny";
	required_sandbox?: string;
	max_depth: number;
	allow_async: boolean;
	description?: string;
};

/**
 * Routing decision for a delegation request (runner-side).
 *
 * The runner checks whether the target session is local before deciding
 * whether to handle delegation directly or escalate to the backend.
 * - "local": Target session is managed by this runner -- handle directly.
 * - "escalate": Target session is on another runner -- forward to backend.
 */
export type DelegateRouting = "local" | "escalate";

/**
 * Escalation request from runner to backend for cross-runner delegation.
 *
 * Sent when the runner receives a delegation command for a session it
 * doesn't manage. The backend routes the request to the correct runner.
 */
export type DelegateEscalation = {
	source_session_id: string;
	request: DelegateRequest;
	correlation_id: string;
};

// ============================================================================
// Mux channel (WebSocket multiplexer)
// ============================================================================

/** WebSocket multiplexer channels. */
export type CanonicalChannel =
	| "agent" // Canonical agent protocol (replaces "pi" and "session")
	| "files" // File operations
	| "terminal" // Terminal I/O
	| "hstry" // History queries
	| "trx" // Issue tracking
	| "system"; // Connection lifecycle

/** Wire format for agent channel messages. */
export type AgentChannelMessage = {
	channel: "agent";
} & (Command | Event);

// ============================================================================
// Helper: extract event type
// ============================================================================

/** Extract the `event` discriminator from an EventPayload. */
export type EventType = EventPayload["event"];

/** Extract the `cmd` discriminator from a CommandPayload. */
export type CommandType = CommandPayload["cmd"];

/** Helper to narrow an EventPayload by its event type. */
export type EventOfType<T extends EventType> = Extract<
	EventPayload,
	{ event: T }
>;

/** Helper to narrow a CommandPayload by its cmd type. */
export type CommandOfType<T extends CommandType> = Extract<
	CommandPayload,
	{ cmd: T }
>;

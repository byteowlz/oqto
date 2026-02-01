/**
 * Type definitions for the multiplexed WebSocket protocol.
 *
 * This file defines the TypeScript types that match the Rust types in
 * backend/crates/octo/src/api/ws_multiplexed.rs
 */

// ============================================================================
// Channel Types
// ============================================================================

/** Supported channels for multiplexed WebSocket */
export type Channel =
	| "pi"
	| "files"
	| "terminal"
	| "hstry"
	| "session"
	| "trx"
	| "system";

// ============================================================================
// Commands (Frontend -> Backend)
// ============================================================================

/** Base command structure with optional correlation ID */
export type WsCommandBase = {
	/** Optional correlation ID for matching responses */
	id?: string;
};

/** Thinking level options */
export type ThinkingLevel = "off" | "minimal" | "low" | "medium" | "high" | "xhigh";

export type Attachment = {
	type: string;
	url?: string;
	data?: string;
	media_type?: string;
	filename?: string;
};

export type MessagePart =
	| { type: "text"; text: string }
	| { type: "image"; url: string }
	| { type: "file"; path: string };

/** Pi channel commands */
export type PiWsCommand =
	// === Session Lifecycle ===
	| ({
			channel: "pi";
			type: "create_session";
			session_id: string;
			config?: PiSessionConfig;
	  } & WsCommandBase)
	| ({ channel: "pi"; type: "close_session"; session_id: string } & WsCommandBase)
	| ({
			channel: "pi";
			type: "new_session";
			session_id: string;
			parent_session?: string;
	  } & WsCommandBase)
	| ({
			channel: "pi";
			type: "switch_session";
			session_id: string;
			session_path: string;
	  } & WsCommandBase)
	| ({ channel: "pi"; type: "list_sessions" } & WsCommandBase)
	| ({ channel: "pi"; type: "subscribe"; session_id: string } & WsCommandBase)
	| ({ channel: "pi"; type: "unsubscribe"; session_id: string } & WsCommandBase)

	// === Prompting ===
	| ({
			channel: "pi";
			type: "prompt";
			session_id: string;
			message: string;
	  } & WsCommandBase)
	| ({
			channel: "pi";
			type: "steer";
			session_id: string;
			message: string;
	  } & WsCommandBase)
	| ({
			channel: "pi";
			type: "follow_up";
			session_id: string;
			message: string;
	  } & WsCommandBase)
	| ({ channel: "pi"; type: "abort"; session_id: string } & WsCommandBase)

	// === State & Messages ===
	| ({ channel: "pi"; type: "get_state"; session_id: string } & WsCommandBase)
	| ({ channel: "pi"; type: "get_messages"; session_id: string } & WsCommandBase)
	| ({ channel: "pi"; type: "get_session_stats"; session_id: string } & WsCommandBase)
	| ({ channel: "pi"; type: "get_last_assistant_text"; session_id: string } & WsCommandBase)

	// === Model Management ===
	| ({
			channel: "pi";
			type: "set_model";
			session_id: string;
			provider: string;
			model_id: string;
	  } & WsCommandBase)
	| ({ channel: "pi"; type: "cycle_model"; session_id: string } & WsCommandBase)
	| ({ channel: "pi"; type: "get_available_models"; session_id: string } & WsCommandBase)

	// === Thinking Level ===
	| ({
			channel: "pi";
			type: "set_thinking_level";
			session_id: string;
			level: ThinkingLevel;
	  } & WsCommandBase)
	| ({ channel: "pi"; type: "cycle_thinking_level"; session_id: string } & WsCommandBase)

	// === Compaction ===
	| ({
			channel: "pi";
			type: "compact";
			session_id: string;
			instructions?: string;
	  } & WsCommandBase)
	| ({
			channel: "pi";
			type: "set_auto_compaction";
			session_id: string;
			enabled: boolean;
	  } & WsCommandBase)

	// === Queue Modes ===
	| ({
			channel: "pi";
			type: "set_steering_mode";
			session_id: string;
			mode: "all" | "one-at-a-time";
	  } & WsCommandBase)
	| ({
			channel: "pi";
			type: "set_follow_up_mode";
			session_id: string;
			mode: "all" | "one-at-a-time";
	  } & WsCommandBase)

	// === Retry ===
	| ({
			channel: "pi";
			type: "set_auto_retry";
			session_id: string;
			enabled: boolean;
	  } & WsCommandBase)
	| ({ channel: "pi"; type: "abort_retry"; session_id: string } & WsCommandBase)

	// === Forking ===
	| ({
			channel: "pi";
			type: "fork";
			session_id: string;
			entry_id: string;
	  } & WsCommandBase)
	| ({ channel: "pi"; type: "get_fork_messages"; session_id: string } & WsCommandBase)

	// === Session Metadata ===
	| ({
			channel: "pi";
			type: "set_session_name";
			session_id: string;
			name: string;
	  } & WsCommandBase)
	| ({
			channel: "pi";
			type: "export_html";
			session_id: string;
			output_path?: string;
	  } & WsCommandBase)

	// === Commands/Skills ===
	| ({ channel: "pi"; type: "get_commands"; session_id: string } & WsCommandBase)

	// === Bash ===
	| ({
			channel: "pi";
			type: "bash";
			session_id: string;
			command: string;
	  } & WsCommandBase)
	| ({ channel: "pi"; type: "abort_bash"; session_id: string } & WsCommandBase)

	// === Extension UI ===
	| ({
			channel: "pi";
			type: "extension_ui_response";
			session_id: string;
			request_id: string;
			value?: string;
			confirmed?: boolean;
			cancelled?: boolean;
	  } & WsCommandBase);

/** Pi session configuration */
export type PiSessionConfig = {
	/** Session scope: "main" for default chat, "workspace" for workspace sessions */
	scope?: "main" | "workspace";
	/** Working directory for Pi (ignored if scope="main") */
	cwd?: string;
	/** Provider (anthropic, openai, etc.) */
	provider?: string;
	/** Model ID */
	model?: string;
	/** Session file to continue from */
	continue_session?: string;
};

/** Files channel commands */
export type FilesWsCommand =
	| ({
			channel: "files";
			type: "tree";
			path: string;
			depth?: number;
			include_hidden?: boolean;
			workspace_path?: string;
	  } & WsCommandBase)
	| ({
			channel: "files";
			type: "read";
			path: string;
			workspace_path?: string;
	  } & WsCommandBase)
	| ({
			channel: "files";
			type: "write";
			path: string;
			content: string;
			create_parents?: boolean;
			workspace_path?: string;
	  } & WsCommandBase)
	| ({
			channel: "files";
			type: "list";
			path: string;
			include_hidden?: boolean;
			workspace_path?: string;
	  } & WsCommandBase)
	| ({
			channel: "files";
			type: "stat";
			path: string;
			workspace_path?: string;
	  } & WsCommandBase)
	| ({
			channel: "files";
			type: "delete";
			path: string;
			recursive?: boolean;
			workspace_path?: string;
	  } & WsCommandBase)
	| ({
			channel: "files";
			type: "create_directory";
			path: string;
			create_parents?: boolean;
			workspace_path?: string;
	  } & WsCommandBase)
	| ({
			channel: "files";
			type: "rename";
			from: string;
			to: string;
			workspace_path?: string;
	  } & WsCommandBase)
	| ({
			channel: "files";
			type: "copy";
			from: string;
			to: string;
			overwrite?: boolean;
			workspace_path?: string;
	  } & WsCommandBase)
	| ({
			channel: "files";
			type: "move";
			from: string;
			to: string;
			overwrite?: boolean;
			workspace_path?: string;
	  } & WsCommandBase);

/** Terminal channel commands */
export type TerminalWsCommand =
	| ({
			channel: "terminal";
			type: "open";
			terminal_id?: string;
			workspace_path?: string;
			session_id?: string;
			cols: number;
			rows: number;
	  } & WsCommandBase)
	| ({
			channel: "terminal";
			type: "input";
			terminal_id: string;
			data: string;
	  } & WsCommandBase)
	| ({
			channel: "terminal";
			type: "resize";
			terminal_id: string;
			cols: number;
			rows: number;
	  } & WsCommandBase)
	| ({
			channel: "terminal";
			type: "close";
			terminal_id: string;
	  } & WsCommandBase);

/** History channel commands */
export type HstryWsCommand = {
	channel: "hstry";
	type: "query";
	session_id?: string;
	query?: string;
	limit?: number;
} & WsCommandBase;

export type TrxIssueInput = {
	title: string;
	description?: string;
	issue_type?: string;
	priority?: number;
	parent_id?: string;
};

export type TrxIssueUpdate = {
	title?: string;
	description?: string;
	status?: string;
	priority?: number;
};

export type TrxWsCommand =
	| ({
			channel: "trx";
			type: "list";
			workspace_path: string;
	  } & WsCommandBase)
	| ({
			channel: "trx";
			type: "create";
			workspace_path: string;
			data: TrxIssueInput;
	  } & WsCommandBase)
	| ({
			channel: "trx";
			type: "update";
			workspace_path: string;
			issue_id: string;
			data: TrxIssueUpdate;
	  } & WsCommandBase)
	| ({
			channel: "trx";
			type: "close";
			workspace_path: string;
			issue_id: string;
			reason?: string;
	  } & WsCommandBase)
	| ({
			channel: "trx";
			type: "sync";
			workspace_path: string;
	  } & WsCommandBase);

/** Session channel commands (legacy session WS protocol over mux) */
export type SessionWsCommand =
	| ({ channel: "session"; type: "pong" } & WsCommandBase)
	| ({ channel: "session"; type: "subscribe"; session_id: string } & WsCommandBase)
	| ({ channel: "session"; type: "unsubscribe"; session_id: string } & WsCommandBase)
	| ({
			channel: "session";
			type: "send_message";
			session_id: string;
			message: string;
			attachments?: Attachment[];
	  } & WsCommandBase)
	| ({
			channel: "session";
			type: "send_parts";
			session_id: string;
			parts: MessagePart[];
	  } & WsCommandBase)
	| ({ channel: "session"; type: "abort"; session_id: string } & WsCommandBase)
	| ({
			channel: "session";
			type: "permission_reply";
			session_id: string;
			permission_id: string;
			granted: boolean;
	  } & WsCommandBase)
	| ({
			channel: "session";
			type: "question_reply";
			session_id: string;
			request_id: string;
			answers: unknown;
	  } & WsCommandBase)
	| ({
			channel: "session";
			type: "question_reject";
			session_id: string;
			request_id: string;
	  } & WsCommandBase)
	| ({
			channel: "session";
			type: "refresh_session";
			session_id: string;
	  } & WsCommandBase)
	| ({
			channel: "session";
			type: "get_messages";
			session_id: string;
			limit?: number;
	  } & WsCommandBase);

/** All possible WebSocket commands */
export type WsCommand =
	| PiWsCommand
	| FilesWsCommand
	| TerminalWsCommand
	| HstryWsCommand
	| TrxWsCommand
	| SessionWsCommand;

export type WsSessionCommand = SessionWsCommand;

// ============================================================================
// Events (Backend -> Frontend)
// ============================================================================

/** Base event structure */
export type WsEventBase = {
	/** Correlation ID (if responding to a command) */
	id?: string;
};

/** Tool use event data */
export type ToolUseData = {
	id: string;
	name: string;
	input: unknown;
};

/** Tool result event data */
export type ToolResultData = {
	id: string;
	name?: string;
	content: unknown;
	is_error?: boolean;
};

export type PiCommandInfo = {
	name: string;
	description?: string | null;
	type: string;
};

/** Pi session info */
export type PiSessionInfo = {
	session_id: string;
	state: string;
	last_activity: number;
	subscriber_count: number;
};

/** Pi channel events */
export type PiWsEvent =
	| ({ channel: "pi"; type: "session_created"; session_id: string } & WsEventBase)
	| ({ channel: "pi"; type: "session_closed"; session_id: string } & WsEventBase)
	| ({
			channel: "pi";
			type: "sessions";
			sessions: PiSessionInfo[];
	  } & WsEventBase)
	| ({
			channel: "pi";
			type: "state";
			session_id: string;
			state: unknown;
	  } & WsEventBase)
	| { channel: "pi"; type: "message_start"; session_id: string; role: string }
	| { channel: "pi"; type: "text"; session_id: string; data: string }
	| { channel: "pi"; type: "thinking"; session_id: string; data: string }
	| { channel: "pi"; type: "tool_use"; session_id: string; data: ToolUseData }
	| { channel: "pi"; type: "tool_start"; session_id: string; data: ToolUseData }
	| {
			channel: "pi";
			type: "tool_result";
			session_id: string;
			data: ToolResultData;
	  }
	| { channel: "pi"; type: "done"; session_id: string }
	| ({
			channel: "pi";
			type: "error";
			session_id: string;
			error: string;
	  } & WsEventBase)
	| {
			channel: "pi";
			type: "persisted";
			session_id: string;
			message_count: number;
	  }
	| ({
			channel: "pi";
			type: "model_changed";
			session_id: string;
			provider: string;
			model_id: string;
	  } & WsEventBase)
	| ({
			channel: "pi";
			type: "available_models";
			session_id: string;
			models: unknown[];
	  } & WsEventBase)
	| ({
			channel: "pi";
			type: "messages";
			session_id: string;
			messages: unknown[];
	  } & WsEventBase)
	| ({
			channel: "pi";
			type: "stats";
			session_id: string;
			stats: unknown;
	  } & WsEventBase)
	| ({
			channel: "pi";
			type: "commands";
			session_id: string;
			commands: PiCommandInfo[];
	  } & WsEventBase)
	| ({
			channel: "pi";
			type: "last_assistant_text";
			session_id: string;
			text: string | null;
	  } & WsEventBase)
	| ({
			channel: "pi";
			type: "fork_messages";
			session_id: string;
			messages: unknown[];
	  } & WsEventBase)
	| ({
			channel: "pi";
			type: "thinking_level_changed";
			session_id: string;
			level: string;
	  } & WsEventBase);

export type DirEntry = {
	name: string;
	is_dir: boolean;
	is_symlink: boolean;
	size: number;
	modified_at: number;
};

export type FileTreeNode = {
	name: string;
	path: string;
	type: "file" | "directory";
	size?: number;
	modified?: number;
	children?: FileTreeNode[];
};

/** Files channel events */
export type FilesWsEvent =
	| ({
			channel: "files";
			type: "tree_result";
			path: string;
			entries: FileTreeNode[];
	  } & WsEventBase)
	| ({
			channel: "files";
			type: "read_result";
			path: string;
			content: string;
			size?: number;
			truncated?: boolean;
	  } & WsEventBase)
	| ({
			channel: "files";
			type: "write_result";
			path: string;
			success: boolean;
	  } & WsEventBase)
	| ({
			channel: "files";
			type: "list_result";
			path: string;
			entries: DirEntry[];
	  } & WsEventBase)
	| ({
			channel: "files";
			type: "stat_result";
			path: string;
			stat: unknown;
	  } & WsEventBase)
	| ({
			channel: "files";
			type: "delete_result";
			path: string;
			success: boolean;
	  } & WsEventBase)
	| ({
			channel: "files";
			type: "create_directory_result";
			path: string;
			success: boolean;
	  } & WsEventBase)
	| ({
			channel: "files";
			type: "rename_result";
			from: string;
			to: string;
			success: boolean;
	  } & WsEventBase)
	| ({
			channel: "files";
			type: "copy_result";
			from: string;
			to: string;
			success: boolean;
	  } & WsEventBase)
	| ({
			channel: "files";
			type: "move_result";
			from: string;
			to: string;
			success: boolean;
	  } & WsEventBase)
	| ({ channel: "files"; type: "error"; error: string } & WsEventBase);

export type TerminalWsEvent =
	| ({
			channel: "terminal";
			type: "opened";
			terminal_id: string;
	  } & WsEventBase)
	| {
			channel: "terminal";
			type: "output";
			terminal_id: string;
			data_base64: string;
	  }
	| ({ channel: "terminal"; type: "exit"; terminal_id: string } & WsEventBase)
	| ({
			channel: "terminal";
			type: "error";
			terminal_id?: string;
			error: string;
	  } & WsEventBase);

export type HstryWsEvent =
	| ({
			channel: "hstry";
			type: "result";
			data: unknown;
	  } & WsEventBase)
	| ({ channel: "hstry"; type: "error"; error: string } & WsEventBase);

export type TrxIssue = {
	id: string;
	title: string;
	description?: string;
	status: string;
	priority: number;
	issue_type: string;
	created_at: string;
	updated_at: string;
	closed_at?: string;
	parent_id?: string;
	labels: string[];
	blocked_by: string[];
};

export type TrxWsEvent =
	| ({
			channel: "trx";
			type: "list_result";
			issues: TrxIssue[];
	  } & WsEventBase)
	| ({
			channel: "trx";
			type: "issue_result";
			issue: TrxIssue;
	  } & WsEventBase)
	| ({
			channel: "trx";
			type: "sync_result";
			success: boolean;
	  } & WsEventBase)
	| ({ channel: "trx"; type: "error"; error: string } & WsEventBase);

/** System channel events */
export type SystemWsEvent =
	| { channel: "system"; type: "connected" }
	| { channel: "system"; type: "error"; error: string }
	| { channel: "system"; type: "ping" };

/** All possible WebSocket events */
export type WsEvent =
	| PiWsEvent
	| FilesWsEvent
	| TerminalWsEvent
	| HstryWsEvent
	| TrxWsEvent
	| SystemWsEvent;

// ============================================================================
// Handler Types
// ============================================================================

/** Event handler function type */
export type WsEventHandler<E = WsEvent> = (event: E) => void;

/** Connection state */
export type WsMuxConnectionState =
	| "disconnected"
	| "connecting"
	| "connected"
	| "reconnecting"
	| "failed";

/** Connection state handler */
export type ConnectionStateHandler = (state: WsMuxConnectionState) => void;

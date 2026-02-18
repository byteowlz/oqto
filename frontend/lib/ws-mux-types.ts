/**
 * Type definitions for the multiplexed WebSocket protocol.
 *
 * This file defines the TypeScript types that match the Rust types in
 * backend/crates/oqto/src/api/ws_multiplexed.rs
 */

// ============================================================================
// Channel Types
// ============================================================================

/** Supported channels for multiplexed WebSocket */
export type Channel =
	| "agent"
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
export type ThinkingLevel =
	| "off"
	| "minimal"
	| "low"
	| "medium"
	| "high"
	| "xhigh";

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

/** Agent channel commands use canonical protocol types from canonical-types.ts.
 * Import { Command } from "@/lib/canonical-types" for the full type.
 * On the wire: { channel: "agent", session_id, cmd, ...payload }
 */
export type AgentWsCommand = {
	channel: "agent";
	id?: string;
	session_id: string;
	runner_id?: string;
	cmd: string;
	[key: string]: unknown;
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
	| ({
			channel: "session";
			type: "subscribe";
			session_id: string;
	  } & WsCommandBase)
	| ({
			channel: "session";
			type: "unsubscribe";
			session_id: string;
	  } & WsCommandBase)
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
	| AgentWsCommand
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

/**
 * Agent channel events (canonical protocol).
 *
 * These are canonical events from the agent runtime (Pi, Claude Code, etc.)
 * The `event` field discriminates the event type.
 * Command responses have `event: "response"` with CommandResponse fields
 * flattened into the top level (id, cmd, success, data?, error?).
 */
export type AgentWsEvent = {
	channel: "agent";
	session_id: string;
	runner_id: string;
	ts: number;
	event: string;
	[key: string]: unknown;
};

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
	| AgentWsEvent
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

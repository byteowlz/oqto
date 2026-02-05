/**
 * Shared types for Pi chat hooks.
 */

import type { PiAgentMessage, PiState } from "@/lib/control-plane-client";

/** Pi streaming event types */
export type PiEventType =
	| "connected"
	| "state"
	| "message_start"
	| "message"
	| "text"
	| "thinking"
	| "tool_use"
	| "tool_start"
	| "tool_result"
	| "done"
	| "error"
	| "compaction";

/** Pi streaming event */
export type PiStreamEvent = {
	type: PiEventType;
	data?: unknown;
	/** Session ID for validation - ensures messages belong to the active session */
	session_id?: string | null;
};

/** Message part for display */
export type PiMessagePart =
	| { type: "text"; content: string }
	| { type: "tool_use"; id: string; name: string; input: unknown }
	| {
			type: "tool_result";
			id: string;
			name?: string;
			content: unknown;
			isError?: boolean;
	  }
	| { type: "thinking"; content: string }
	| { type: "compaction"; content: string }
	| { type: "error"; content: string }
	| {
			type: "image";
			id: string;
			source: string;
			data?: string;
			url?: string;
			mimeType?: string;
	  };

/** Display message with parts */
export type PiDisplayMessage = {
	id: string;
	role: "user" | "assistant" | "system";
	parts: PiMessagePart[];
	timestamp: number;
	isStreaming?: boolean;
	usage?: PiAgentMessage["usage"];
};

/** Send mode for messages */
export type PiSendMode = "prompt" | "steer" | "follow_up";

/** Options for sending messages */
export type PiSendOptions = {
	mode?: PiSendMode;
	queueIfStreaming?: boolean;
	/** Force a specific session id (used to bind a pending chat to a real session). */
	sessionId?: string;
};

/** Hook options */
export type UsePiChatOptions = {
	/** Auto-connect on mount */
	autoConnect?: boolean;
	/** Workspace path */
	workspacePath?: string | null;
	/** Storage key prefix for cached messages */
	storageKeyPrefix?: string;
	/** Selected Pi session ID (disk-backed Default Chat session) */
	selectedSessionId?: string | null;
	/** Notify when a new session becomes active (e.g. /new) */
	onSelectedSessionIdChange?: (id: string | null) => void;
	/** Callback when message stream completes */
	onMessageComplete?: (message: PiDisplayMessage) => void;
	/** Callback on error */
	onError?: (error: Error) => void;
};

/** Hook return type */
export type UsePiChatReturn = {
	/** Current Pi state */
	state: PiState | null;
	/** Display messages */
	messages: PiDisplayMessage[];
	/** Whether connected to WebSocket */
	isConnected: boolean;
	/** Whether currently streaming a response */
	isStreaming: boolean;
	/** Whether awaiting the first response event */
	isAwaitingResponse: boolean;
	/** Current error if any */
	error: Error | null;
	/** Send a message */
	send: (message: string, options?: PiSendOptions) => Promise<void>;
	/** Append a local assistant message (no Pi call) */
	appendLocalAssistantMessage: (content: string) => void;
	/** Abort current stream */
	abort: () => Promise<void>;
	/** Compact the session context */
	compact: (customInstructions?: string) => Promise<void>;
	/** Start new session (clear history) */
	newSession: () => Promise<void>;
	/** Reset session - restarts Pi process to reload PERSONALITY.md and USER.md */
	resetSession: () => Promise<void>;
	/** Reload messages from server */
	refresh: () => Promise<void>;
	/** Connect to WebSocket */
	connect: () => void;
	/** Disconnect from WebSocket */
	disconnect: () => void;
};

/** Raw Pi message from backend */
export type RawPiMessage = {
	id?: string;
	role: string;
	content: unknown;
	/** Canonical message parts (array of Part objects from octo-protocol). */
	parts?: unknown[];
	timestamp?: number;
	created_at?: number;
	created_at_ms?: number;
	createdAtMs?: number;
	parts_json?: string;
	partsJson?: string;
	usage?: PiAgentMessage["usage"];
	toolCallId?: string;
	tool_call_id?: string;
	toolName?: string;
	tool_name?: string;
	isError?: boolean;
	is_error?: boolean;
};

/** Batched update state for token streaming - reduces per-token React updates */
export type BatchedUpdateState = {
	rafId: number | null;
	lastFlushTime: number;
	pendingUpdate: boolean;
};

/** Session message cache entry */
export type SessionMessageCacheEntry = {
	messages: PiDisplayMessage[];
	timestamp: number;
	version: number;
};

/** WebSocket connection state cache */
export type WsConnectionState = {
	ws: WebSocket | null;
	isConnected: boolean;
	sessionStarted: boolean;
	mainSessionInit: Promise<PiState | null> | null;
	listeners: Set<(connected: boolean) => void>;
};

/** Scroll position cache state */
export type ScrollCache = {
	positions: Map<string, number | null>;
	initialized: Set<string>;
};

/** Session message cache state */
export type SessionMessageCache = {
	messagesBySession: Map<string, SessionMessageCacheEntry>;
	initialized: boolean;
	lastWriteTime: Map<string, number>;
	pendingWrite: Map<string, ReturnType<typeof setTimeout>>;
};

// Re-export PiState for convenience
export type { PiState } from "@/lib/control-plane-client";

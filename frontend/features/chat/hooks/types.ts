/**
 * Shared types for chat hooks.
 *
 * Display types are built on top of the canonical protocol types from
 * `@/lib/canonical-types`. The canonical `Part` union covers persistent
 * content; `DisplayPart` extends it with ephemeral display-only variants
 * (compaction notices, inline errors). `DisplayMessage` wraps canonical
 * parts with UI-specific metadata (streaming state, client correlation).
 */

import type { Part, ToolStatus, Usage } from "@/lib/canonical-types";
import type { AgentState } from "@/lib/control-plane-client";

// ============================================================================
// Display-only part variants (not persisted, frontend-only)
// ============================================================================

/** Context-compaction notice shown inline in the chat. */
export type CompactionPart = { type: "compaction"; id: string; text: string };

/** Inline error notice (e.g. stream failure). */
export type ErrorPart = { type: "error"; id: string; text: string };

/**
 * A display part is either a canonical Part or a display-only variant.
 *
 * Renderers should handle all canonical `Part` types plus the two
 * display-only extensions (`compaction`, `error`).
 */
export type DisplayPart = Part | CompactionPart | ErrorPart;

// ============================================================================
// Display message
// ============================================================================

/** A message ready for rendering in the chat UI. */
export type DisplayMessage = {
	id: string;
	role: "user" | "assistant" | "system";
	parts: DisplayPart[];
	timestamp: number;
	/** True while the assistant is still streaming tokens for this message. */
	isStreaming?: boolean;
	/** Token usage / cost metadata. */
	usage?: Usage;
	/** Client-generated ID for optimistic message matching.
	 * Used to correlate frontend optimistic messages with server-confirmed versions. */
	clientId?: string;
	/** Model ID used for this message (from hstry or streaming). */
	model?: string | null;
	/** Provider ID used for this message (from hstry or streaming). */
	provider?: string | null;
};

/** Send mode for messages */
export type SendMode = "prompt" | "steer" | "follow_up";

/** Options for sending messages */
export type SendOptions = {
	mode?: SendMode;
	queueIfStreaming?: boolean;
	/** Force a specific session id (used to bind a pending chat to a real session). */
	sessionId?: string;
};

/** Hook options */
export type UseChatOptions = {
	/** Auto-connect on mount */
	autoConnect?: boolean;
	/** Workspace path */
	workspacePath?: string | null;
	/** Storage key prefix for cached messages */
	storageKeyPrefix?: string;
	/** Selected session ID (disk-backed Default Chat session) */
	selectedSessionId?: string | null;
	/** Notify when a new session becomes active (e.g. /new) */
	onSelectedSessionIdChange?: (id: string | null) => void;
	/** Callback when message stream completes */
	onMessageComplete?: (message: DisplayMessage) => void;
	/** Callback on error */
	onError?: (error: Error) => void;
	/** Callback when session title changes (from auto-rename or server event) */
	onTitleChanged?: (
		sessionId: string,
		title: string,
		readableId?: string | null,
	) => void;
};

/** Hook return type */
export type UseChatReturn = {
	/** Current agent state */
	state: AgentState | null;
	/** Display messages */
	messages: DisplayMessage[];
	/** Whether connected to WebSocket */
	isConnected: boolean;
	/** Whether currently streaming a response */
	isStreaming: boolean;
	/** Whether awaiting the first response event */
	isAwaitingResponse: boolean;
	/** Current error if any */
	error: Error | null;
	/** Send a message */
	send: (message: string, options?: SendOptions) => Promise<void>;
	/** Append a local assistant message (no agent call) */
	appendLocalAssistantMessage: (content: string) => void;
	/** Abort current stream */
	abort: () => Promise<void>;
	/** Compact the session context */
	compact: (customInstructions?: string) => Promise<void>;
	/** Start new session (clear history) */
	newSession: () => Promise<void>;
	/** Reset session - restarts agent process to reload PERSONALITY.md and USER.md */
	resetSession: () => Promise<void>;
	/** Reload messages from server */
	refresh: () => Promise<void>;
	/** Connect to WebSocket */
	connect: () => void;
	/** Disconnect from WebSocket */
	disconnect: () => void;
};

/**
 * Raw message from backend (hstry serializable, Pi JSONL, or canonical).
 *
 * The `usage` field accepts any shape because different sources use different
 * field names (canonical: input_tokens/output_tokens, Pi: input/output).
 * Display code normalizes at render time.
 */
export type RawMessage = {
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
	// biome-ignore lint/suspicious/noExplicitAny: usage comes from multiple sources with different shapes
	usage?: any;
	toolCallId?: string;
	tool_call_id?: string;
	toolName?: string;
	tool_name?: string;
	isError?: boolean;
	is_error?: boolean;
	/** Client-generated ID for optimistic message matching */
	client_id?: string;
	clientId?: string;
	/** Model ID from hstry ChatMessage */
	model_id?: string | null;
	model?: string | null;
	/** Provider ID from hstry ChatMessage */
	provider_id?: string | null;
	provider?: string | null;
};

/** Batched update state for token streaming - reduces per-token React updates */
export type BatchedUpdateState = {
	rafId: number | null;
	lastFlushTime: number;
	pendingUpdate: boolean;
};

/** Session message cache entry */
export type SessionMessageCacheEntry = {
	messages: DisplayMessage[];
	timestamp: number;
	version: number;
};

/** WebSocket connection state cache */
export type WsConnectionState = {
	ws: WebSocket | null;
	isConnected: boolean;
	sessionStarted: boolean;
	mainSessionInit: Promise<AgentState | null> | null;
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

// Re-export AgentState for convenience
export type { AgentState } from "@/lib/control-plane-client";

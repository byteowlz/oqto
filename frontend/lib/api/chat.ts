/**
 * Chat History API
 * Reads Pi chat history from disk (no running opencode needed)
 */

import type {
	OpenCodeMessage,
	OpenCodeMessageWithParts,
	OpenCodePart,
} from "../opencode-client";
import { authFetch, controlPlaneApiUrl, readApiError } from "./client";

const normalizeWorkspacePathValue = (path?: string | null): string | null => {
	if (!path || path === "global" || path.startsWith("global/")) return null;
	return path;
};

// ============================================================================
// Chat History Types (from disk, no running opencode needed)
// ============================================================================

/** A Pi session read directly from disk */
export type ChatSession = {
	/** Session ID (UUID filename) */
	id: string;
	/** Human-readable ID (set by auto-rename extension, if available) */
	readable_id: string | null;
	/** Session title */
	title: string | null;
	/** Parent session ID (for child sessions) */
	parent_id: string | null;
	/** Workspace/project path */
	workspace_path: string | null;
	/** Project name (derived from path) */
	project_name: string | null;
	/** Created timestamp (ms since epoch) */
	created_at: number;
	/** Updated timestamp (ms since epoch) */
	updated_at: number;
	/** OpenCode version that created this session */
	version: string | null;
	/** Whether this session is a child session */
	is_child: boolean;
	/** Path to the session JSON file (for loading messages) */
	source_path: string | null;
	/** Last used model ID (from hstry conversation) */
	model?: string | null;
	/** Last used provider ID (from hstry conversation) */
	provider?: string | null;
};

/** Chat sessions grouped by workspace/project */
export type GroupedChatHistory = {
	workspace_path: string;
	project_name: string;
	sessions: ChatSession[];
};

/** Query parameters for listing chat history */
export type ChatHistoryQuery = {
	/** Filter by workspace path */
	workspace?: string;
	/** Include child sessions (default: false) */
	include_children?: boolean;
	/** Maximum number of sessions to return */
	limit?: number;
};

/** Request to update a chat session */
export type UpdateChatSessionRequest = {
	title?: string;
};

// ============================================================================
// Chat Message Types (from disk, no running opencode needed)
// ============================================================================

/** A single part of a chat message */
export type ChatMessagePart = {
	id: string;
	part_type: string;
	/** Text content (for text parts) */
	text: string | null;
	/** Tool name (for tool parts) */
	tool_name: string | null;
	/** Tool call id (for tool parts) */
	tool_call_id?: string | null;
	/** Tool input (for tool parts) */
	tool_input: unknown | null;
	/** Tool output (for tool parts) */
	tool_output: string | null;
	/** Tool status (for tool parts) */
	tool_status: string | null;
	/** Tool title/summary (for tool parts) */
	tool_title: string | null;
};

/** A chat message with its content parts */
export type ChatMessage = {
	id: string;
	session_id: string;
	role: string;
	created_at: number;
	completed_at: number | null;
	parent_id: string | null;
	model_id: string | null;
	provider_id: string | null;
	agent: string | null;
	summary_title: string | null;
	tokens_input: number | null;
	tokens_output: number | null;
	cost: number | null;
	/** Client-generated ID for optimistic message matching */
	client_id?: string | null;
	/** Message content parts */
	parts: ChatMessagePart[];
};

// ============================================================================
// Chat History API (reads from disk, no running opencode needed)
// ============================================================================

/** List all chat sessions. */
export async function listChatHistory(
	query: ChatHistoryQuery = {},
): Promise<ChatSession[]> {
	const url = new URL(
		controlPlaneApiUrl("/api/chat-history"),
		window.location.origin,
	);
	if (query.workspace) url.searchParams.set("workspace", query.workspace);
	if (query.include_children) url.searchParams.set("include_children", "true");
	if (query.limit) url.searchParams.set("limit", query.limit.toString());

	const res = await authFetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = (await res.json()) as ChatSession[];
	return data.map((session) => ({
		...session,
		workspace_path: normalizeWorkspacePathValue(session.workspace_path),
	}));
}

/** List chat sessions grouped by workspace/project */
export async function listChatHistoryGrouped(
	query: ChatHistoryQuery = {},
): Promise<GroupedChatHistory[]> {
	const url = new URL(
		controlPlaneApiUrl("/api/chat-history/grouped"),
		window.location.origin,
	);
	if (query.workspace) url.searchParams.set("workspace", query.workspace);
	if (query.include_children) url.searchParams.set("include_children", "true");
	if (query.limit) url.searchParams.set("limit", query.limit.toString());

	const res = await authFetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = await res.json();
	return data.map((group: GroupedChatHistory) => ({
		...group,
		workspace_path: normalizeWorkspacePathValue(group.workspace_path),
		sessions: group.sessions.map((session: ChatSession) => ({
			...session,
			workspace_path: normalizeWorkspacePathValue(session.workspace_path),
		})),
	}));
}

/** Get a specific chat session by ID */
export async function getChatSession(sessionId: string): Promise<ChatSession> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/chat-history/${sessionId}`),
		{
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Update a chat session (e.g., rename) */
export async function updateChatSession(
	sessionId: string,
	updates: UpdateChatSessionRequest,
): Promise<ChatSession> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/chat-history/${sessionId}`),
		{
			method: "PATCH",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(updates),
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get all messages for a chat session */
export async function getChatMessages(
	sessionId: string,
): Promise<ChatMessage[]> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/chat-history/${sessionId}/messages`),
		{
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

// ============================================================================
// Message Format Conversion (disk format -> opencode format)
// ============================================================================

/** Convert a ChatMessage (from disk) to OpenCodeMessageWithParts (for rendering) */
export function convertChatMessageToOpenCode(
	msg: ChatMessage,
): OpenCodeMessageWithParts {
	// Convert parts
	const parts: OpenCodePart[] = msg.parts.map((part) => ({
		id: part.id,
		sessionID: msg.session_id,
		messageID: msg.id,
		type: part.part_type as OpenCodePart["type"],
		text: part.text ?? undefined,
		tool: part.tool_name ?? undefined,
		state: part.tool_name
			? {
					status:
						(part.tool_status as
							| "pending"
							| "running"
							| "completed"
							| "error") ?? "completed",
					input: part.tool_input as Record<string, unknown> | undefined,
					output: part.tool_output ?? undefined,
					title: part.tool_title ?? undefined,
				}
			: undefined,
	}));

	// Build message info based on role
	const info: OpenCodeMessage =
		msg.role === "user"
			? {
					id: msg.id,
					sessionID: msg.session_id,
					role: "user" as const,
					time: { created: msg.created_at },
					agent: msg.agent ?? undefined,
					model:
						msg.provider_id && msg.model_id
							? {
									providerID: msg.provider_id,
									modelID: msg.model_id,
								}
							: undefined,
				}
			: {
					id: msg.id,
					sessionID: msg.session_id,
					role: "assistant" as const,
					time: {
						created: msg.created_at,
						completed: msg.completed_at ?? undefined,
					},
					parentID: msg.parent_id ?? "",
					modelID: msg.model_id ?? "",
					providerID: msg.provider_id ?? "",
					cost: msg.cost ?? undefined,
					tokens:
						msg.tokens_input != null || msg.tokens_output != null
							? {
									input: msg.tokens_input ?? 0,
									output: msg.tokens_output ?? 0,
									reasoning: 0,
									cache: { read: 0, write: 0 },
								}
							: undefined,
				};

	return { info, parts };
}

/** Convert an array of ChatMessages to OpenCodeMessageWithParts */
export function convertChatMessagesToOpenCode(
	messages: ChatMessage[],
): OpenCodeMessageWithParts[] {
	return messages.map(convertChatMessageToOpenCode);
}

/**
 * Chat History API
 * Reads Pi chat history from disk (from hstry)
 */

import type { AgentMessage, AgentAgentMessageWithParts, MessagePart } from "../agent-client";
import type { Message, Part, Role } from "@/lib/canonical-types";
import { authFetch, controlPlaneApiUrl, readApiError } from "./client";

const normalizeWorkspacePathValue = (path?: string | null): string | null => {
	if (!path || path === "global" || path.startsWith("global/")) return null;
	return path;
};

// ============================================================================
// Chat History Types (from disk, from hstry)
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
	/** Version that created this session */
	version: string | null;
	/** Whether this session is a child session */
	is_child: boolean;
	/** Path to the session JSON file (for loading messages) */
	source_path: string | null;
	/** Last used model ID (from hstry conversation) */
	model?: string | null;
	/** Last used provider ID (from hstry conversation) */
	provider?: string | null;
	/** Session stats (tokens, cost) */
	stats?: {
		tokens_in: number;
		tokens_out: number;
		cache_read: number;
		cache_write: number;
		cost_usd: number;
	} | null;
	/** If set, this session belongs to a shared workspace (client-side only) */
	shared_workspace_id?: string | null;
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
	/** If set, list sessions from this shared workspace instead of personal */
	shared_workspace_id?: string;
};

/** Request to update a chat session */
export type UpdateChatSessionRequest = {
	title?: string;
};

// ============================================================================
// Chat Message Types (canonical)
// ============================================================================

export type ChatMessagePart = Part;
export type ChatMessage = Message;

// ============================================================================
// Chat History API (reads from disk, from hstry)
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
	if (query.shared_workspace_id) url.searchParams.set("shared_workspace_id", query.shared_workspace_id);

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
	shared_workspace_id?: string,
): Promise<ChatSession> {
	const params = new URLSearchParams();
	if (shared_workspace_id) params.set("shared_workspace_id", shared_workspace_id);
	const qs = params.toString();
	const url = controlPlaneApiUrl(`/api/chat-history/${sessionId}${qs ? `?${qs}` : ""}`);
	const res = await authFetch(url,
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

/** Get all messages for a chat session.
 * Pass shared_workspace_id to route to the shared workspace's runner. */
export async function deleteChatSessionApi(
	sessionId: string,
	shared_workspace_id?: string,
): Promise<void> {
	const params = new URLSearchParams();
	if (shared_workspace_id) params.set("shared_workspace_id", shared_workspace_id);
	const qs = params.toString();
	const url = controlPlaneApiUrl(`/api/chat-history/${sessionId}${qs ? `?${qs}` : ""}`);
	const res = await authFetch(url, {
		method: "DELETE",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

export type BackfillChatHistoryResponse = {
	scanned_files: number;
	repaired_conversations: number;
	skipped_files: number;
	failed_files: number;
};

export async function triggerChatHistoryBackfill(opts: {
	workspace?: string;
	shared_workspace_id?: string;
	limit?: number;
} = {}): Promise<BackfillChatHistoryResponse> {
	const url = new URL(
		controlPlaneApiUrl("/api/chat-history/backfill"),
		window.location.origin,
	);
	if (opts.workspace) url.searchParams.set("workspace", opts.workspace);
	if (opts.shared_workspace_id) {
		url.searchParams.set("shared_workspace_id", opts.shared_workspace_id);
	}
	if (typeof opts.limit === "number") {
		url.searchParams.set("limit", String(opts.limit));
	}

	const res = await authFetch(url.toString(), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function getChatMessages(
	sessionId: string,
	shared_workspace_id?: string,
): Promise<ChatMessage[]> {
	const params = new URLSearchParams();
	if (shared_workspace_id) params.set("shared_workspace_id", shared_workspace_id);
	const qs = params.toString();
	const url = controlPlaneApiUrl(`/api/chat-history/${sessionId}/messages${qs ? `?${qs}` : ""}`);
	const res = await authFetch(url, {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

// ============================================================================
// Message Format Conversion (canonical -> agent-client format)
// ============================================================================

const normalizeRoleForAgent = (role: Role): "user" | "assistant" =>
	role === "user" ? "user" : "assistant";

const formatToolOutput = (output: unknown): string => {
	if (output == null) return "";
	if (typeof output === "string") return output;
	try {
		return JSON.stringify(output, null, 2);
	} catch {
		return String(output);
	}
};

const mapToolStatus = (
	status: "pending" | "running" | "success" | "error" | undefined,
): "pending" | "running" | "completed" | "error" => {
	switch (status) {
		case "pending":
			return "pending";
		case "running":
			return "running";
		case "error":
			return "error";
		case "success":
		default:
			return "completed";
	}
};

const canonicalPartToAgentPart = (
	part: ChatMessagePart,
	messageId: string,
	sessionId: string,
): MessagePart => {
	switch (part.type) {
		case "text":
			return {
				id: part.id,
				sessionID: sessionId,
				messageID: messageId,
				type: "text",
				text: part.text,
			};
		case "thinking":
			return {
				id: part.id,
				sessionID: sessionId,
				messageID: messageId,
				type: "reasoning",
				text: part.text,
			};
		case "tool_call":
			return {
				id: part.id,
				sessionID: sessionId,
				messageID: messageId,
				type: "tool",
				tool: part.name,
				callID: part.toolCallId,
				state: {
					status: mapToolStatus(part.status),
					input: (part.input ?? undefined) as Record<string, unknown> | undefined,
					title: part.name,
				},
			};
		case "tool_result":
			return {
				id: part.id,
				sessionID: sessionId,
				messageID: messageId,
				type: "tool",
				tool: part.name ?? "tool",
				callID: part.toolCallId,
				state: {
					status: part.isError ? "error" : "completed",
					output: formatToolOutput(part.output),
					title: part.name ?? "Tool Result",
				},
			};
		case "file_ref":
			return {
				id: part.id,
				sessionID: sessionId,
				messageID: messageId,
				type: "file",
				url: part.uri,
				filename: part.label ?? undefined,
			};
		default:
			return {
				id: part.id,
				sessionID: sessionId,
				messageID: messageId,
				type: "text",
				text: JSON.stringify(part),
			};
	}
};

/** Convert a ChatMessage (canonical) to AgentMessageWithParts (for rendering). */
export function convertChatMessageToAgent(
	msg: ChatMessage,
	sessionId: string,
): AgentMessageWithParts {
	const parts: MessagePart[] = msg.parts.map((part) =>
		canonicalPartToAgentPart(part, msg.id, sessionId),
	);

	const role = normalizeRoleForAgent(msg.role as Role);

	const info: AgentMessage =
		role === "user"
			? {
					id: msg.id,
					sessionID: sessionId,
					role: "user",
					time: { created: msg.created_at },
					model:
						msg.model && msg.provider
							? { providerID: msg.provider, modelID: msg.model }
							: undefined,
				}
			: {
					id: msg.id,
					sessionID: sessionId,
					role: "assistant",
					time: { created: msg.created_at },
					parentID: "",
					modelID: msg.model ?? "",
					providerID: msg.provider ?? "",
					cost: msg.usage?.cost_usd ?? undefined,
					tokens: msg.usage
						? {
								input: msg.usage.input_tokens,
								output: msg.usage.output_tokens,
								reasoning: 0,
								cache: {
									read: msg.usage.cache_read_tokens ?? 0,
									write: msg.usage.cache_write_tokens ?? 0,
								},
							}
						: undefined,
				};

	return { info, parts };
}

/** Convert an array of ChatMessages to AgentMessageWithParts */
export function convertChatMessagesToAgent(
	messages: ChatMessage[],
	sessionId: string,
): AgentMessageWithParts[] {
	return messages.map((message) =>
		convertChatMessageToAgent(message, sessionId),
	);
}

export const convertChatMessagesToCanonical = convertChatMessagesToAgent;

/**
 * Default Chat API
 * Pi agent runtime for Default Chat, assistants, history, streaming
 */

import {
	authFetch,
	controlPlaneApiUrl,
	getAuthToken,
	readApiError,
} from "./client";
import { toAbsoluteWsUrl } from "@/lib/url";

const normalizeWorkspacePathValue = (path?: string | null): string | null => {
	if (!path || path === "global" || path.startsWith("global/")) return null;
	return path;
};

// ============================================================================
// Default Chat Types
// ============================================================================

/** History entry type */
export type DefaultChatHistoryType =
	| "summary"
	| "decision"
	| "handoff"
	| "insight";

/** History entry from Default Chat */
export type DefaultChatHistoryEntry = {
	id: number;
	ts: string;
	type: DefaultChatHistoryType;
	content: string;
	session_id?: string;
	meta?: Record<string, unknown>;
	created_at: string;
};

/** Default Chat session (legacy DB-backed, kept for history/exports) */
export type DefaultChatSession = {
	id: number;
	session_id: string;
	title?: string;
	started_at: string;
	ended_at?: string;
	message_count: number;
};

/** Pi session file entry (disk-backed; used for Default Chat sessions list) */
export type PiSessionFile = {
	id: string;
	started_at: string;
	size: number;
	modified_at: number;
	title?: string;
	/** Human-readable ID (e.g., "cold-lamp-verb") */
	readable_id?: string;
	parent_id?: string;
	message_count: number;
	workspace_path?: string;
	session_dir?: string;
};

/** Message loaded from a Pi session JSONL file */
export type PiSessionMessage = {
	id: string;
	role: "user" | "assistant" | "system" | "toolResult";
	content: unknown;
	toolCallId?: string;
	toolName?: string;
	isError?: boolean;
	timestamp: number;
	usage?: unknown;
};

/** Default Chat assistant info */
export type DefaultChatAssistantInfo = {
	name: string;
	user_id: string;
	path: string;
	session_count: number;
	history_count: number;
	created_at?: string;
};

/** Pi session status */
export type DefaultChatPiStatus = {
	exists: boolean;
	session_active: boolean;
};

/** Pi model info */
export type PiModelInfo = {
	id: string;
	provider: string;
	name: string;
	context_window: number;
	max_tokens: number;
};

export type PiPromptCommandInfo = {
	name: string;
	description: string;
};

/** Pi session state */
export type PiState = {
	model: PiModelInfo | null;
	thinking_level: string;
	is_streaming: boolean;
	is_compacting: boolean;
	session_id: string | null;
	session_name?: string | null;
	message_count: number;
	auto_compaction_enabled: boolean;
};

/** Pi session stats */
export type PiSessionStats = {
	session_id: string | null;
	user_messages: number;
	assistant_messages: number;
	tool_calls: number;
	total_messages: number;
	tokens: {
		input: number;
		output: number;
		cache_read: number;
		cache_write: number;
		total: number;
	};
	cost: number;
};

/** Pi agent message */
export type PiAgentMessage = {
	role: string;
	content: unknown;
	timestamp?: number;
	api?: string;
	provider?: string;
	model?: string;
	usage?: {
		input: number;
		output: number;
		cacheRead: number;
		cacheWrite: number;
		cost?: {
			input: number;
			output: number;
			cacheRead: number;
			cacheWrite: number;
			total: number;
		};
	};
	stopReason?: string;
};

/** Pi compaction result */
export type PiCompactionResult = {
	summary: string;
	firstKeptEntryId: string;
	tokensBefore: number;
	details?: unknown;
};

/** Chat message stored in main_chat.db for persistent display history */
export type DefaultChatDbMessage = {
	id: number;
	role: "user" | "assistant" | "system";
	/** JSON array of message parts (text, thinking, tool_use, tool_result) */
	content: string;
	pi_session_id: string | null;
	timestamp: number;
	created_at: string;
};

/** Search result from Pi session search */
export type PiSearchHit = {
	agent: string;
	source_path: string;
	session_id: string;
	message_id?: string;
	line_number: number;
	snippet?: string;
	score: number;
	timestamp?: number;
	role?: string;
	title?: string;
};

/** Search response from Pi session search */
export type PiSearchResponse = {
	hits: PiSearchHit[];
	total: number;
};

/** In-session search result from hstry */
export type InSessionSearchResult = {
	/** Line number in the source file */
	line_number: number;
	/** Match score */
	score: number;
	/** Short snippet around the match */
	snippet?: string;
	/** Session title */
	title?: string;
	/** Match type (exact, fuzzy) */
	match_type?: string;
	/** Timestamp when the message was created */
	created_at?: number;
	/** Message ID for direct navigation */
	message_id?: string;
};

// ============================================================================
// Default Chat Assistant API
// ============================================================================

/** List all Default Chat assistants for the current user */
export async function listDefaultChatAssistants(): Promise<string[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/main"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = await res.json();
	if (data.exists && data.info?.name) {
		return [data.info.name];
	}
	return [];
}

/** Get info about a specific assistant */
export async function getDefaultChatAssistant(
	name: string,
): Promise<DefaultChatAssistantInfo> {
	const res = await authFetch(controlPlaneApiUrl("/api/main"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = await res.json();
	if (!data.exists || !data.info) {
		throw new Error("Default Chat not found");
	}
	return {
		...data.info,
		path: normalizeWorkspacePathValue(data.info.path),
	};
}

/** Create a new Default Chat assistant */
export async function createDefaultChatAssistant(
	name: string,
): Promise<DefaultChatAssistantInfo> {
	const res = await authFetch(controlPlaneApiUrl("/api/main"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ name }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Update the Default Chat assistant name */
export async function updateDefaultChatAssistant(
	name: string,
): Promise<DefaultChatAssistantInfo> {
	const res = await authFetch(controlPlaneApiUrl("/api/main"), {
		method: "PATCH",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ name }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Delete a Default Chat assistant */
export async function deleteDefaultChatAssistant(name: string): Promise<void> {
	const res = await authFetch(controlPlaneApiUrl("/api/main"), {
		method: "DELETE",
		credentials: "include",
	});
	if (res.status === 404) return;
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Get recent history for an assistant */
export async function getDefaultChatHistory(
	name: string,
	limit = 20,
): Promise<DefaultChatHistoryEntry[]> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/main/history?limit=${limit}`),
		{ credentials: "include" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Add a history entry */
export async function addDefaultChatHistory(
	name: string,
	entry: {
		type: DefaultChatHistoryType;
		content: string;
		session_id?: string;
		meta?: Record<string, unknown>;
	},
): Promise<DefaultChatHistoryEntry> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/history"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(entry),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** List sessions for an assistant (legacy DB-backed sessions table) */
export async function listDefaultChatSessions(
	name: string,
): Promise<DefaultChatSession[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/sessions"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** List Pi sessions from disk (used for Default Chat sessions list) */
export async function listDefaultChatPiSessions(): Promise<PiSessionFile[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/sessions"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = await res.json();
	return data.map((session: PiSessionFile) => ({
		...session,
		workspace_path: normalizeWorkspacePathValue(session.workspace_path),
	}));
}

/** Rename a Pi session (update title) */
export async function renamePiSession(
	sessionId: string,
	title: string,
): Promise<PiSessionFile> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/main/pi/sessions/${sessionId}`),
		{
			method: "PATCH",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ title }),
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Delete a Pi session (soft delete) */
export async function deleteDefaultChatPiSession(sessionId: string): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/main/pi/sessions/${sessionId}`),
		{
			method: "DELETE",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Search Default Chat Pi sessions for message content */
export async function searchDefaultChatPiSessions(
	query: string,
	limit = 50,
): Promise<PiSearchResponse> {
	const url = new URL(
		controlPlaneApiUrl("/api/main/pi/sessions/search"),
		window.location.origin,
	);
	url.searchParams.set("q", query);
	url.searchParams.set("limit", limit.toString());
	const res = await authFetch(url.toString(), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Start a brand new Pi session (creates new session file) */
export async function newDefaultChatPiSessionFile(): Promise<PiState> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/sessions"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Load messages from a specific Pi session file */
export async function getDefaultChatPiSessionMessages(
	sessionId: string,
): Promise<PiSessionMessage[]> {
	const res = await authFetch(
		controlPlaneApiUrl(
			`/api/main/pi/sessions/${encodeURIComponent(sessionId)}`,
		),
		{ credentials: "include" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Resume/switch the active Pi session */
export async function resumeDefaultChatPiSession(
	sessionId: string,
): Promise<PiState> {
	const res = await authFetch(
		controlPlaneApiUrl(
			`/api/main/pi/sessions/${encodeURIComponent(sessionId)}`,
		),
		{ method: "POST", credentials: "include" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Search within a specific Pi session using hstry */
export async function searchInPiSession(
	sessionId: string,
	query: string,
	limit = 20,
): Promise<InSessionSearchResult[]> {
	const url = new URL(
		controlPlaneApiUrl(
			`/api/agents/sessions/${encodeURIComponent(sessionId)}/search`,
		),
		window.location.origin,
	);
	url.searchParams.set("q", query);
	url.searchParams.set("limit", limit.toString());

	const res = await authFetch(url.toString(), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Register a new session with the assistant */
export async function registerDefaultChatSession(
	name: string,
	session: { session_id: string; title?: string },
): Promise<DefaultChatSession> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/sessions"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(session),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get the latest session for an assistant */
export async function getLatestDefaultChatSession(
	name: string,
): Promise<DefaultChatSession | null> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/sessions/latest"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Export history as JSONL */
export async function exportDefaultChatHistory(name: string): Promise<string> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/export"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = await res.json();
	return data.jsonl ?? "";
}

// ============================================================================
// Default Chat Pi API (Pi agent runtime for Default Chat)
// ============================================================================

/** Check Pi session status */
export async function getDefaultChatPiStatus(): Promise<DefaultChatPiStatus> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/status"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Start or get Pi session */
export async function startDefaultChatPiSession(): Promise<PiState> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/session"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get Pi session state */

function mainPiUrl(path: string, sessionId: string): string {
	const url = new URL(controlPlaneApiUrl(path), window.location.origin);
	url.searchParams.set("session_id", sessionId);
	return url.toString();
}

export async function getDefaultChatPiState(sessionId: string): Promise<PiState> {
	const res = await authFetch(mainPiUrl("/api/main/pi/state", sessionId), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Send a prompt to Pi */
export async function sendDefaultChatPiPrompt(
	sessionId: string,
	message: string,
): Promise<void> {
	const res = await authFetch(mainPiUrl("/api/main/pi/prompt", sessionId), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ message }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Abort current Pi operation */
export async function abortDefaultChatPi(sessionId: string): Promise<void> {
	const res = await authFetch(mainPiUrl("/api/main/pi/abort", sessionId), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Get Pi messages */
export async function getDefaultChatPiMessages(
	sessionId: string,
): Promise<PiAgentMessage[]> {
	const res = await authFetch(mainPiUrl("/api/main/pi/messages", sessionId), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Compact Pi session */

export async function compactDefaultChatPi(
	sessionId: string,
	customInstructions?: string,
): Promise<PiCompactionResult> {
	const res = await authFetch(mainPiUrl("/api/main/pi/compact", sessionId), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ custom_instructions: customInstructions }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Set Pi session model */
export async function setDefaultChatPiModel(
	sessionId: string,
	provider: string,
	modelId: string,
): Promise<PiState> {
	const res = await authFetch(mainPiUrl("/api/main/pi/model", sessionId), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ provider, model_id: modelId }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get available Pi models */
export async function getDefaultChatPiModels(sessionId: string): Promise<PiModelInfo[]> {
	const res = await authFetch(mainPiUrl("/api/main/pi/models", sessionId), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = (await res.json()) as { models?: PiModelInfo[] };
	return data.models ?? [];
}

/** Get available Pi prompt commands (slash templates). */
export async function getDefaultChatPiCommands(
	sessionId: string,
): Promise<PiPromptCommandInfo[]> {
	const res = await authFetch(mainPiUrl("/api/main/pi/commands", sessionId), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = (await res.json()) as { commands?: PiPromptCommandInfo[] };
	return data.commands ?? [];
}

/** Start new Pi session (clear history) */
export async function newDefaultChatPiSession(): Promise<PiState> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/new"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Reset Pi session - restarts the process to reload PERSONALITY.md and USER.md */
export async function resetDefaultChatPiSession(sessionId: string): Promise<PiState> {
	const res = await authFetch(mainPiUrl("/api/main/pi/reset", sessionId), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get Pi session stats */
export async function getDefaultChatPiStats(sessionId: string): Promise<PiSessionStats> {
	const res = await authFetch(mainPiUrl("/api/main/pi/stats", sessionId), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Close Pi session */
export async function closeDefaultChatPiSession(): Promise<void> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/session"), {
		method: "DELETE",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Get persistent chat history from database (survives Pi session restarts) */
export async function getDefaultChatPiHistory(
	sessionId?: string,
): Promise<DefaultChatDbMessage[]> {
	const url = sessionId
		? controlPlaneApiUrl(
				`/api/main/pi/history?session_id=${encodeURIComponent(sessionId)}`,
			)
		: controlPlaneApiUrl("/api/main/pi/history");
	const res = await authFetch(url, {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

// ============================================================================
// Workspace Pi API
// ============================================================================

/** Set workspace Pi session model */
export async function setWorkspacePiModel(
	workspacePath: string,
	sessionId: string,
	provider: string,
	modelId: string,
): Promise<PiState> {
	const url = new URL(
		controlPlaneApiUrl("/api/pi/workspace/model"),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);
	url.searchParams.set("session_id", sessionId);

	const res = await authFetch(url.toString(), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ provider, model_id: modelId }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get available workspace Pi models */
export async function getWorkspacePiModels(
	workspacePath: string,
	sessionId: string,
): Promise<PiModelInfo[]> {
	const url = new URL(
		controlPlaneApiUrl("/api/pi/workspace/models"),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);
	url.searchParams.set("session_id", sessionId);

	const res = await authFetch(url.toString(), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = (await res.json()) as { models?: PiModelInfo[] };
	return data.models ?? [];
}

/** Start a new workspace Pi session */
export async function newWorkspacePiSession(
	workspacePath: string,
): Promise<PiState> {
	const res = await authFetch(
		controlPlaneApiUrl("/api/pi/workspace/sessions"),
		{
			method: "POST",
			credentials: "include",
			headers: {
				"Content-Type": "application/json",
			},
			body: JSON.stringify({ workspace_path: workspacePath }),
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Resume a workspace Pi session */
export async function resumeWorkspacePiSession(
	workspacePath: string,
	sessionId: string,
): Promise<PiState> {
	const url = new URL(
		controlPlaneApiUrl(`/api/pi/workspace/sessions/${sessionId}/resume`),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);

	const res = await authFetch(url.toString(), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get workspace Pi session state */
export async function getWorkspacePiState(
	workspacePath: string,
	sessionId: string,
): Promise<PiState> {
	const url = new URL(
		controlPlaneApiUrl("/api/pi/workspace/state"),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);
	url.searchParams.set("session_id", sessionId);

	const res = await authFetch(url.toString(), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get messages from a workspace Pi session */
export async function getWorkspacePiSessionMessages(
	workspacePath: string,
	sessionId: string,
): Promise<PiSessionMessage[]> {
	const url = new URL(
		controlPlaneApiUrl(`/api/pi/workspace/sessions/${sessionId}/messages`),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);

	const res = await authFetch(url.toString(), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Abort a workspace Pi session */
export async function abortWorkspacePiSession(
	workspacePath: string,
	sessionId: string,
): Promise<void> {
	const url = new URL(
		controlPlaneApiUrl(`/api/pi/workspace/sessions/${sessionId}/abort`),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);

	const res = await authFetch(url.toString(), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Delete a workspace Pi session (soft delete) */
export async function deleteWorkspacePiSession(
	workspacePath: string,
	sessionId: string,
): Promise<void> {
	const url = new URL(
		controlPlaneApiUrl(`/api/pi/workspace/sessions/${sessionId}`),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);

	const res = await authFetch(url.toString(), {
		method: "DELETE",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Create WebSocket connection to a workspace Pi session for streaming events */
export function createWorkspacePiWebSocket(
	workspacePath: string,
	sessionId: string,
): WebSocket {
	let wsUrl = toAbsoluteWsUrl(controlPlaneApiUrl("/api/pi/workspace/ws"));
	const params = new URLSearchParams();
	params.set("workspace_path", workspacePath);
	params.set("session_id", sessionId);

	// Add auth token as query parameter for WebSocket auth
	const token = getAuthToken();
	if (token) {
		params.set("token", token);
	}
	wsUrl = `${wsUrl}?${params.toString()}`;
	return new WebSocket(wsUrl);
}

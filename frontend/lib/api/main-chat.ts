/**
 * Main Chat API
 * Pi agent runtime for Main Chat, assistants, history, streaming
 */

import {
	authFetch,
	controlPlaneApiUrl,
	getAuthToken,
	getControlPlaneBaseUrl,
	readApiError,
} from "./client";

// ============================================================================
// Main Chat Types
// ============================================================================

/** History entry type */
export type MainChatHistoryType =
	| "summary"
	| "decision"
	| "handoff"
	| "insight";

/** History entry from Main Chat */
export type MainChatHistoryEntry = {
	id: number;
	ts: string;
	type: MainChatHistoryType;
	content: string;
	session_id?: string;
	meta?: Record<string, unknown>;
	created_at: string;
};

/** Main Chat session (legacy DB-backed, kept for history/exports) */
export type MainChatSession = {
	id: number;
	session_id: string;
	title?: string;
	started_at: string;
	ended_at?: string;
	message_count: number;
};

/** Pi session file entry (disk-backed; used for Main Chat sessions list) */
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

/** Main Chat assistant info */
export type MainChatAssistantInfo = {
	name: string;
	user_id: string;
	path: string;
	session_count: number;
	history_count: number;
	created_at?: string;
};

/** Pi session status */
export type MainChatPiStatus = {
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
export type MainChatDbMessage = {
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
// Main Chat Assistant API
// ============================================================================

/** List all Main Chat assistants for the current user */
export async function listMainChatAssistants(): Promise<string[]> {
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
export async function getMainChatAssistant(
	name: string,
): Promise<MainChatAssistantInfo> {
	const res = await authFetch(controlPlaneApiUrl("/api/main"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = await res.json();
	if (!data.exists || !data.info) {
		throw new Error("Main Chat not found");
	}
	return data.info;
}

/** Create a new Main Chat assistant */
export async function createMainChatAssistant(
	name: string,
): Promise<MainChatAssistantInfo> {
	const res = await authFetch(controlPlaneApiUrl("/api/main"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ name }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Update the Main Chat assistant name */
export async function updateMainChatAssistant(
	name: string,
): Promise<MainChatAssistantInfo> {
	const res = await authFetch(controlPlaneApiUrl("/api/main"), {
		method: "PATCH",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ name }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Delete a Main Chat assistant */
export async function deleteMainChatAssistant(name: string): Promise<void> {
	const res = await authFetch(controlPlaneApiUrl("/api/main"), {
		method: "DELETE",
		credentials: "include",
	});
	if (res.status === 404) return;
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Get recent history for an assistant */
export async function getMainChatHistory(
	name: string,
	limit = 20,
): Promise<MainChatHistoryEntry[]> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/main/history?limit=${limit}`),
		{ credentials: "include" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Add a history entry */
export async function addMainChatHistory(
	name: string,
	entry: {
		type: MainChatHistoryType;
		content: string;
		session_id?: string;
		meta?: Record<string, unknown>;
	},
): Promise<MainChatHistoryEntry> {
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
export async function listMainChatSessions(
	name: string,
): Promise<MainChatSession[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/sessions"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** List Pi sessions from disk (used for Main Chat sessions list) */
export async function listMainChatPiSessions(): Promise<PiSessionFile[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/sessions"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
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

/** Search Main Chat Pi sessions for message content */
export async function searchMainChatPiSessions(
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
export async function newMainChatPiSessionFile(): Promise<PiState> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/sessions"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Load messages from a specific Pi session file */
export async function getMainChatPiSessionMessages(
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
export async function resumeMainChatPiSession(
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
export async function registerMainChatSession(
	name: string,
	session: { session_id: string; title?: string },
): Promise<MainChatSession> {
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
export async function getLatestMainChatSession(
	name: string,
): Promise<MainChatSession | null> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/sessions/latest"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Export history as JSONL */
export async function exportMainChatHistory(name: string): Promise<string> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/export"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = await res.json();
	return data.jsonl ?? "";
}

// ============================================================================
// Main Chat Pi API (Pi agent runtime for Main Chat)
// ============================================================================

/** Check Pi session status */
export async function getMainChatPiStatus(): Promise<MainChatPiStatus> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/status"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Start or get Pi session */
export async function startMainChatPiSession(): Promise<PiState> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/session"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get Pi session state */
export async function getMainChatPiState(): Promise<PiState> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/state"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Send a prompt to Pi */
export async function sendMainChatPiPrompt(message: string): Promise<void> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/prompt"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ message }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Abort current Pi operation */
export async function abortMainChatPi(): Promise<void> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/abort"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Get Pi messages */
export async function getMainChatPiMessages(): Promise<PiAgentMessage[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/messages"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Compact Pi session */
export async function compactMainChatPi(
	customInstructions?: string,
): Promise<PiCompactionResult> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/compact"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ custom_instructions: customInstructions }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Set Pi session model */
export async function setMainChatPiModel(
	provider: string,
	modelId: string,
): Promise<PiState> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/model"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ provider, model_id: modelId }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get available Pi models */
export async function getMainChatPiModels(): Promise<PiModelInfo[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/models"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = (await res.json()) as { models?: PiModelInfo[] };
	return data.models ?? [];
}

/** Get available Pi prompt commands (slash templates). */
export async function getMainChatPiCommands(): Promise<PiPromptCommandInfo[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/commands"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = (await res.json()) as { commands?: PiPromptCommandInfo[] };
	return data.commands ?? [];
}

/** Start new Pi session (clear history) */
export async function newMainChatPiSession(): Promise<PiState> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/new"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Reset Pi session - restarts the process to reload PERSONALITY.md and USER.md */
export async function resetMainChatPiSession(): Promise<PiState> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/reset"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get Pi session stats */
export async function getMainChatPiStats(): Promise<PiSessionStats> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/stats"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Close Pi session */
export async function closeMainChatPiSession(): Promise<void> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/session"), {
		method: "DELETE",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Create WebSocket connection to Pi for streaming events */
export function createMainChatPiWebSocket(): WebSocket {
	const baseUrl = getControlPlaneBaseUrl();
	let wsUrl: string;
	if (baseUrl) {
		// Direct connection to control plane - no /api prefix needed
		wsUrl = `${baseUrl.replace(/^http/, "ws")}/main/pi/ws`;
	} else {
		// Proxied via frontend dev server - use /api prefix
		wsUrl = `${window.location.origin.replace(/^http/, "ws")}/api/main/pi/ws`;
	}
	// Add auth token as query parameter for WebSocket auth
	const token = getAuthToken();
	if (token) {
		wsUrl = `${wsUrl}?token=${encodeURIComponent(token)}`;
	}
	return new WebSocket(wsUrl);
}

/** Get persistent chat history from database (survives Pi session restarts) */
export async function getMainChatPiHistory(
	sessionId?: string,
): Promise<MainChatDbMessage[]> {
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

/** Clear persistent chat history */
export async function clearMainChatPiHistory(): Promise<{ deleted: number }> {
	const res = await authFetch(controlPlaneApiUrl("/api/main/pi/history"), {
		method: "DELETE",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Add a session separator to history (marks new conversation start) */
export async function addMainChatPiSeparator(): Promise<MainChatDbMessage> {
	const res = await authFetch(
		controlPlaneApiUrl("/api/main/pi/history/separator"),
		{
			method: "POST",
			credentials: "include",
		},
	);
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

/** Create WebSocket connection to a workspace Pi session for streaming events */
export function createWorkspacePiWebSocket(
	workspacePath: string,
	sessionId: string,
): WebSocket {
	const baseUrl = getControlPlaneBaseUrl();
	let wsUrl: string;
	if (baseUrl) {
		// Direct connection to control plane - no /api prefix needed
		wsUrl = `${baseUrl.replace(/^http/, "ws")}/pi/workspace/ws`;
	} else {
		// Proxied via frontend dev server - use /api prefix
		wsUrl = `${window.location.origin.replace(/^http/, "ws")}/api/pi/workspace/ws`;
	}
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

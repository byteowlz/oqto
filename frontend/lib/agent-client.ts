import { getAuthHeaders } from "./control-plane-client";

// Message cache for client-side caching
type MessageCache = {
	messages: AgentMessageWithParts[];
	timestamp: number;
	sessionId: string;
};

const messageCache = new Map<string, MessageCache>();
const MESSAGE_CACHE_TTL = 30_000; // 30 seconds

type AgentRequestOptions = {
	directory?: string;
};

// Session type matching actual API response
export type AgentSession = {
	id: string;
	title: string;
	time: {
		created: number;
		updated: number;
	};
	parentID?: string | null;
	version?: string;
	projectID?: string;
	directory?: string;
	summary?: {
		additions: number | null;
		deletions: number | null;
		files: number;
	};
};

// Part types
export type MessagePart = {
	id: string;
	sessionID: string;
	messageID: string;
	type:
		| "text"
		| "tool"
		| "file"
		| "reasoning"
		| "step-start"
		| "step-finish"
		| "snapshot"
		| "patch"
		| "agent"
		| "retry"
		| "compaction"
		| "subtask";
	text?: string;
	tool?: string;
	callID?: string;
	mime?: string;
	url?: string;
	filename?: string;
	state?: {
		status: "pending" | "running" | "completed" | "error";
		input?: Record<string, unknown>;
		output?: string;
		title?: string;
		time?: { start: number; end?: number };
	};
	time?: { start?: number; end?: number };
	metadata?: Record<string, unknown>;
};

// Message types
export type UserMessage = {
	id: string;
	sessionID: string;
	role: "user";
	time: { created: number };
	agent?: string;
	model?: { providerID: string; modelID: string };
};

export type AssistantMessage = {
	id: string;
	sessionID: string;
	role: "assistant";
	time: { created: number; completed?: number };
	parentID: string;
	modelID: string;
	providerID: string;
	cost?: number;
	tokens?: {
		input: number;
		output: number;
		reasoning: number;
		cache: { read: number; write: number };
	};
	error?: { type: string; message?: string };
};

export type AgentMessage = UserMessage | AssistantMessage;

// API response for messages endpoint
export type AgentMessageWithParts = {
	info: AgentMessage;
	parts: MessagePart[];
};

const trimTrailingSlash = (value: string) => value.replace(/\/$/, "");

const base = (agentBaseUrl: string) => {
	if (!agentBaseUrl) throw new Error("Agent base URL is not configured");
	return trimTrailingSlash(agentBaseUrl);
};

const withDirectory = (url: string, directory?: string) => {
	if (!directory) return url;
	const joiner = url.includes("?") ? "&" : "?";
	return `${url}${joiner}directory=${encodeURIComponent(directory)}`;
};

async function handleResponse<T>(res: Response): Promise<T> {
	const contentType = res.headers.get("content-type") || "";

	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `Request failed with ${res.status}`);
	}

	// Check if response is JSON
	if (!contentType.includes("application/json")) {
		const text = await res.text();
		throw new Error(
			`Expected JSON response but got ${contentType}: ${text.slice(0, 100)}...`,
		);
	}

	return res.json();
}

export async function fetchSessions(
	agentBaseUrl: string,
	options?: AgentRequestOptions,
): Promise<AgentSession[]> {
	const res = await fetch(
		withDirectory(`${base(agentBaseUrl)}/session`, options?.directory),
		{
			cache: "no-store",
			credentials: "include",
		},
	);
	return handleResponse<AgentSession[]>(res);
}

export async function fetchMessages(
	agentBaseUrl: string,
	sessionId: string,
	options?: { skipCache?: boolean; directory?: string },
): Promise<AgentMessageWithParts[]> {
	const cacheKey = `${agentBaseUrl}:${sessionId}:${options?.directory ?? ""}`;

	// Check cache unless explicitly skipped
	if (!options?.skipCache) {
		const cached = messageCache.get(cacheKey);
		if (cached && Date.now() - cached.timestamp < MESSAGE_CACHE_TTL) {
			return cached.messages;
		}
	}

	// Fetch from server
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/message`,
			options?.directory,
		),
		{ cache: "no-store", credentials: "include" },
	);
	const messages = await handleResponse<AgentMessageWithParts[]>(res);

	// Update cache
	messageCache.set(cacheKey, {
		messages,
		timestamp: Date.now(),
		sessionId,
	});

	return messages;
}

// Invalidate cache for a session (call this when SSE events indicate changes)
export function invalidateMessageCache(
	agentBaseUrl: string,
	sessionId: string,
	directory?: string,
): void {
	const cacheKey = `${agentBaseUrl}:${sessionId}:${directory ?? ""}`;
	messageCache.delete(cacheKey);
}

// Clear all message cache
export function clearMessageCache(): void {
	messageCache.clear();
}

export async function sendMessage(
	agentBaseUrl: string,
	sessionId: string,
	content: string,
	model?: { providerID: string; modelID: string },
	options?: AgentRequestOptions,
) {
	// Correct endpoint is /message with POST, body contains parts array
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/message`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				model,
				parts: [{ type: "text", text: content }],
			}),
			credentials: "include",
		},
	);
	return handleResponse<AgentMessageWithParts>(res);
}

export async function sendMessageAsync(
	agentBaseUrl: string,
	sessionId: string,
	content: string,
	model?: { providerID: string; modelID: string },
	options?: AgentRequestOptions,
) {
	// Async version - returns immediately, use SSE for updates
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/prompt_async`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				model,
				parts: [{ type: "text", text: content }],
			}),
			credentials: "include",
		},
	);
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `Request failed with ${res.status}`);
	}
	// Returns 204 No Content
	return true;
}

export type MessagePartInput =
	| { type: "text"; text: string }
	| { type: "agent"; name: string; id?: string }
	| { type: "file"; mime: string; url: string; filename?: string };

export async function sendPartsAsync(
	agentBaseUrl: string,
	sessionId: string,
	parts: MessagePartInput[],
	model?: { providerID: string; modelID: string },
	options?: AgentRequestOptions,
) {
	const body: Record<string, unknown> = { parts };
	if (model) body.model = model;

	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/prompt_async`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(body),
			credentials: "include",
		},
	);
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `Request failed with ${res.status}`);
	}
	return true;
}

export async function abortSession(
	agentBaseUrl: string,
	sessionId: string,
	options?: AgentRequestOptions,
): Promise<boolean> {
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/abort`,
			options?.directory,
		),
		{
			method: "POST",
			credentials: "include",
		},
	);
	return handleResponse<boolean>(res);
}

export type AgentInfo = {
	id: string;
	name?: string;
	description?: string;
	mode?: "primary" | "subagent" | string;
	model?: { providerID: string; modelID: string };
};

export async function fetchAgents(
	agentBaseUrl: string,
	options?: AgentRequestOptions,
): Promise<AgentInfo[]> {
	const res = await fetch(
		withDirectory(`${base(agentBaseUrl)}/agent`, options?.directory),
		{
			cache: "no-store",
			credentials: "include",
		},
	);
	return handleResponse<AgentInfo[]>(res);
}

// Command definition from agent config
export type AgentCommand = {
	template: string;
	description?: string;
	agent?: string;
	model?: string;
	subtask?: boolean;
};

// Config response from agent API
export type AgentConfig = {
	model?: string;
	agent?: string;
	command?: Record<string, AgentCommand>;
	// Other config fields we don't need for now
};

export type ProviderModel = {
	id: string;
	name?: string;
	limit?: {
		context?: number;
	};
};

export type Provider = {
	id: string;
	name?: string;
	models?: Record<string, ProviderModel>;
};

export type ProvidersResponse = {
	providers?: Provider[];
	all?: Provider[];
	default?: {
		providerID?: string;
		modelID?: string;
	};
};

export async function fetchConfig(
	agentBaseUrl: string,
	options?: AgentRequestOptions,
): Promise<AgentConfig> {
	const res = await fetch(
		withDirectory(`${base(agentBaseUrl)}/config`, options?.directory),
		{
			cache: "no-store",
			credentials: "include",
		},
	);
	return handleResponse<AgentConfig>(res);
}

export async function fetchProviders(
	agentBaseUrl: string,
	options?: AgentRequestOptions,
): Promise<ProvidersResponse> {
	const providersUrl = withDirectory(
		`${base(agentBaseUrl)}/config/providers`,
		options?.directory,
	);
	try {
		const res = await fetch(providersUrl, {
			cache: "no-store",
			credentials: "include",
			headers: getAuthHeaders(),
		});
		if (res.ok) {
			return handleResponse<ProvidersResponse>(res);
		}
	} catch {
		// Fall back to /provider
	}

	const fallbackUrl = withDirectory(
		`${base(agentBaseUrl)}/provider`,
		options?.directory,
	);
	const res = await fetch(fallbackUrl, {
		cache: "no-store",
		credentials: "include",
		headers: getAuthHeaders(),
	});
	return handleResponse<ProvidersResponse>(res);
}

// Command from the /command list endpoint (includes built-in + custom commands)
export type AgentCommandInfo = {
	name: string;
	description?: string;
	agent?: string;
	model?: string;
	template: string;
	subtask?: boolean;
};

export async function fetchCommands(
	agentBaseUrl: string,
	options?: AgentRequestOptions,
): Promise<AgentCommandInfo[]> {
	const res = await fetch(
		withDirectory(`${base(agentBaseUrl)}/command`, options?.directory),
		{
			cache: "no-store",
			credentials: "include",
		},
	);
	return handleResponse<AgentCommandInfo[]>(res);
}

export async function runShellCommand(
	agentBaseUrl: string,
	sessionId: string,
	command: string,
	agent: string,
	model?: { providerID: string; modelID: string },
	options?: AgentRequestOptions,
): Promise<AgentMessageWithParts> {
	const body: Record<string, unknown> = { command, agent };
	if (model) body.model = model;

	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/shell`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(body),
			credentials: "include",
		},
	);
	return handleResponse<AgentMessageWithParts>(res);
}

export async function runShellCommandAsync(
	agentBaseUrl: string,
	sessionId: string,
	command: string,
	agent: string,
	model?: { providerID: string; modelID: string },
	options?: AgentRequestOptions,
): Promise<boolean> {
	const body: Record<string, unknown> = { command, agent };
	if (model) body.model = model;

	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/shell`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(body),
			credentials: "include",
		},
	);
	if (!res.ok) {
		let errorMsg = `Request failed with ${res.status}`;
		try {
			const data = await res.json();
			errorMsg =
				data.message || data.error || data.name || JSON.stringify(data);
		} catch {
			const text = await res.text().catch(() => res.statusText);
			errorMsg = text || errorMsg;
		}
		throw new Error(errorMsg);
	}
	return true;
}

// Send a slash command to the agent (e.g., /init, /undo, /redo, /share, /help, or custom commands)
// Command should be the name without the slash (e.g., "init", "help")
// Arguments is the string after the command name (e.g., for "/test foo bar", args would be "foo bar")
export async function sendCommandAsync(
	agentBaseUrl: string,
	sessionId: string,
	command: string,
	args = "",
	options?: AgentRequestOptions,
): Promise<boolean> {
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/command`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ command, arguments: args }),
			credentials: "include",
		},
	);
	if (!res.ok) {
		let errorMsg = `Request failed with ${res.status}`;
		try {
			const data = await res.json();
			errorMsg =
				data.message || data.error || data.name || JSON.stringify(data);
		} catch {
			const text = await res.text().catch(() => res.statusText);
			errorMsg = text || errorMsg;
		}
		throw new Error(errorMsg);
	}
	return true;
}

export async function createSession(
	agentBaseUrl: string,
	title?: string,
	parentID?: string,
	options?: AgentRequestOptions,
): Promise<AgentSession> {
	const res = await fetch(
		withDirectory(`${base(agentBaseUrl)}/session`, options?.directory),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ title, parentID }),
			credentials: "include",
		},
	);
	return handleResponse<AgentSession>(res);
}

export async function deleteSession(
	agentBaseUrl: string,
	sessionId: string,
	options?: AgentRequestOptions,
): Promise<void> {
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}`,
			options?.directory,
		),
		{
			method: "DELETE",
			credentials: "include",
		},
	);
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `Request failed with ${res.status}`);
	}
}

export async function updateSession(
	agentBaseUrl: string,
	sessionId: string,
	updates: { title?: string },
	options?: AgentRequestOptions,
): Promise<AgentSession> {
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}`,
			options?.directory,
		),
		{
			method: "PATCH",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(updates),
			credentials: "include",
		},
	);
	return handleResponse<AgentSession>(res);
}

/**
 * Fork a session at a specific message point.
 * Creates a new session with all conversation history up to (and including) the specified message.
 * If no messageID is provided, forks from the current end of the conversation.
 */
export async function forkSession(
	agentBaseUrl: string,
	sessionId: string,
	messageId?: string,
	options?: AgentRequestOptions,
): Promise<AgentSession> {
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/fork`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ messageID: messageId }),
			credentials: "include",
		},
	);
	return handleResponse<AgentSession>(res);
}

// Permission types for tool execution approval
// Matches permission type
export type Permission = {
	id: string;
	type: string; // Permission type (e.g., "bash", "edit", "webfetch")
	pattern?: string | string[];
	sessionID: string;
	messageID?: string;
	callID?: string;
	title: string;
	metadata: Record<string, unknown>;
	time: {
		created: number;
	};
};

export type PermissionResponse = "yes" | "no" | "always" | "never";

// Question types for user question/multiple choice selection
// Matches question types
export type QuestionOption = {
	label: string; // Display text (1-5 words, concise)
	description: string; // Explanation of choice
};

export type QuestionInfo = {
	question: string; // Complete question
	header: string; // Very short label (max 12 chars)
	options: QuestionOption[]; // Available choices
	multiple?: boolean; // Allow selecting multiple choices
};

export type QuestionRequest = {
	id: string; // Request ID (question_*)
	sessionID: string;
	questions: QuestionInfo[];
	tool?: {
		messageID: string;
		callID: string;
	};
};

export type QuestionAnswer = string[]; // Array of selected labels

export type QuestionReply = {
	answers: QuestionAnswer[]; // User answers in order of questions
};

// Respond to a permission request
export async function respondToPermission(
	agentBaseUrl: string,
	sessionId: string,
	permissionId: string,
	response: PermissionResponse,
	options?: AgentRequestOptions,
): Promise<void> {
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/session/${sessionId}/permission/${permissionId}`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ response }),
			credentials: "include",
		},
	);
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `Request failed with ${res.status}`);
	}
}

// Fetch pending permissions for a session
export async function fetchPermissions(
	agentBaseUrl: string,
	sessionId: string,
	options?: AgentRequestOptions,
): Promise<Permission[]> {
	const url = withDirectory(
		`${base(agentBaseUrl)}/session/${sessionId}/permission`,
		options?.directory,
	);
	console.log("[Permission] Fetching permissions from:", url);
	const res = await fetch(url, { cache: "no-store", credentials: "include" });
	// If endpoint doesn't exist or returns error, return empty array
	if (!res.ok) {
		console.log("[Permission] Fetch failed with status:", res.status);
		return [];
	}
	return handleResponse<Permission[]>(res);
}

// Fetch pending questions
export async function fetchQuestions(
	agentBaseUrl: string,
	options?: AgentRequestOptions,
): Promise<QuestionRequest[]> {
	const url = withDirectory(
		`${base(agentBaseUrl)}/question`,
		options?.directory,
	);
	const res = await fetch(url, { cache: "no-store", credentials: "include" });
	if (!res.ok) {
		console.log("[Question] Fetch failed with status:", res.status);
		return [];
	}
	return handleResponse<QuestionRequest[]>(res);
}

// Reply to a question request
export async function replyToQuestion(
	agentBaseUrl: string,
	requestId: string,
	answers: QuestionAnswer[],
	options?: AgentRequestOptions,
): Promise<void> {
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/question/${requestId}/reply`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ answers }),
			credentials: "include",
		},
	);
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `Request failed with ${res.status}`);
	}
}

// Reject a question request
export async function rejectQuestion(
	agentBaseUrl: string,
	requestId: string,
	options?: AgentRequestOptions,
): Promise<void> {
	const res = await fetch(
		withDirectory(
			`${base(agentBaseUrl)}/question/${requestId}/reject`,
			options?.directory,
		),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `Request failed with ${res.status}`);
	}
}

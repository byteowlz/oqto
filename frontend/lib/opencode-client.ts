// Message cache for client-side caching
type MessageCache = {
	messages: OpenCodeMessageWithParts[];
	timestamp: number;
	sessionId: string;
};

const messageCache = new Map<string, MessageCache>();
const MESSAGE_CACHE_TTL = 30_000; // 30 seconds - cache is invalidated by SSE events anyway

type OpencodeRequestOptions = {
	directory?: string;
};

// Session type matching actual API response
export type OpenCodeSession = {
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
export type OpenCodePart = {
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
export type OpenCodeUserMessage = {
	id: string;
	sessionID: string;
	role: "user";
	time: { created: number };
	agent?: string;
	model?: { providerID: string; modelID: string };
};

export type OpenCodeAssistantMessage = {
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

export type OpenCodeMessage = OpenCodeUserMessage | OpenCodeAssistantMessage;

// API response for messages endpoint
export type OpenCodeMessageWithParts = {
	info: OpenCodeMessage;
	parts: OpenCodePart[];
};

const trimTrailingSlash = (value: string) => value.replace(/\/$/, "");

const base = (opencodeBaseUrl: string) => {
	if (!opencodeBaseUrl) throw new Error("OpenCode base URL is not configured");
	return trimTrailingSlash(opencodeBaseUrl);
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
	opencodeBaseUrl: string,
	options?: OpencodeRequestOptions,
): Promise<OpenCodeSession[]> {
	const res = await fetch(
		withDirectory(`${base(opencodeBaseUrl)}/session`, options?.directory),
		{
			cache: "no-store",
			credentials: "include",
		},
	);
	return handleResponse<OpenCodeSession[]>(res);
}

export async function fetchMessages(
	opencodeBaseUrl: string,
	sessionId: string,
	options?: { skipCache?: boolean; directory?: string },
): Promise<OpenCodeMessageWithParts[]> {
	const cacheKey = `${opencodeBaseUrl}:${sessionId}:${options?.directory ?? ""}`;

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
			`${base(opencodeBaseUrl)}/session/${sessionId}/message`,
			options?.directory,
		),
		{ cache: "no-store", credentials: "include" },
	);
	const messages = await handleResponse<OpenCodeMessageWithParts[]>(res);

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
	opencodeBaseUrl: string,
	sessionId: string,
	directory?: string,
): void {
	const cacheKey = `${opencodeBaseUrl}:${sessionId}:${directory ?? ""}`;
	messageCache.delete(cacheKey);
}

// Clear all message cache
export function clearMessageCache(): void {
	messageCache.clear();
}

export async function sendMessage(
	opencodeBaseUrl: string,
	sessionId: string,
	content: string,
	model?: { providerID: string; modelID: string },
	options?: OpencodeRequestOptions,
) {
	// Correct endpoint is /message with POST, body contains parts array
	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}/message`,
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
	return handleResponse<OpenCodeMessageWithParts>(res);
}

export async function sendMessageAsync(
	opencodeBaseUrl: string,
	sessionId: string,
	content: string,
	model?: { providerID: string; modelID: string },
	options?: OpencodeRequestOptions,
) {
	// Async version - returns immediately, use SSE for updates
	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}/prompt_async`,
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

export type OpenCodePartInput =
	| { type: "text"; text: string }
	| { type: "agent"; name: string; id?: string }
	| { type: "file"; mime: string; url: string; filename?: string };

export async function sendPartsAsync(
	opencodeBaseUrl: string,
	sessionId: string,
	parts: OpenCodePartInput[],
	model?: { providerID: string; modelID: string },
	options?: OpencodeRequestOptions,
) {
	const body: Record<string, unknown> = { parts };
	if (model) body.model = model;

	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}/prompt_async`,
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
	opencodeBaseUrl: string,
	sessionId: string,
	options?: OpencodeRequestOptions,
): Promise<boolean> {
	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}/abort`,
			options?.directory,
		),
		{
			method: "POST",
			credentials: "include",
		},
	);
	return handleResponse<boolean>(res);
}

export type OpenCodeAgent = {
	id: string;
	name?: string;
	description?: string;
	mode?: "primary" | "subagent" | string;
	model?: { providerID: string; modelID: string };
};

export async function fetchAgents(
	opencodeBaseUrl: string,
	options?: OpencodeRequestOptions,
): Promise<OpenCodeAgent[]> {
	const res = await fetch(
		withDirectory(`${base(opencodeBaseUrl)}/agent`, options?.directory),
		{
			cache: "no-store",
			credentials: "include",
		},
	);
	return handleResponse<OpenCodeAgent[]>(res);
}

// Command definition from opencode config
export type OpenCodeCommand = {
	template: string;
	description?: string;
	agent?: string;
	model?: string;
	subtask?: boolean;
};

// Config response from opencode
export type OpenCodeConfig = {
	model?: string;
	agent?: string;
	command?: Record<string, OpenCodeCommand>;
	// Other config fields we don't need for now
};

export type OpenCodeProviderModel = {
	id: string;
	name?: string;
	limit?: {
		context?: number;
	};
};

export type OpenCodeProvider = {
	id: string;
	name?: string;
	models?: Record<string, OpenCodeProviderModel>;
};

export type OpenCodeProvidersResponse = {
	providers?: OpenCodeProvider[];
	all?: OpenCodeProvider[];
	default?: {
		providerID?: string;
		modelID?: string;
	};
};

export async function fetchConfig(
	opencodeBaseUrl: string,
	options?: OpencodeRequestOptions,
): Promise<OpenCodeConfig> {
	const res = await fetch(
		withDirectory(`${base(opencodeBaseUrl)}/config`, options?.directory),
		{
			cache: "no-store",
			credentials: "include",
		},
	);
	return handleResponse<OpenCodeConfig>(res);
}

export async function fetchProviders(
	opencodeBaseUrl: string,
	options?: OpencodeRequestOptions,
): Promise<OpenCodeProvidersResponse> {
	const providersUrl = withDirectory(
		`${base(opencodeBaseUrl)}/config/providers`,
		options?.directory,
	);
	try {
		const res = await fetch(providersUrl, {
			cache: "no-store",
			credentials: "include",
		});
		if (res.ok) {
			return handleResponse<OpenCodeProvidersResponse>(res);
		}
	} catch {
		// Fall back to /provider
	}

	const fallbackUrl = withDirectory(
		`${base(opencodeBaseUrl)}/provider`,
		options?.directory,
	);
	const res = await fetch(fallbackUrl, {
		cache: "no-store",
		credentials: "include",
	});
	return handleResponse<OpenCodeProvidersResponse>(res);
}

// Command from the /command list endpoint (includes built-in + custom commands)
export type OpenCodeCommandInfo = {
	name: string;
	description?: string;
	agent?: string;
	model?: string;
	template: string;
	subtask?: boolean;
};

export async function fetchCommands(
	opencodeBaseUrl: string,
	options?: OpencodeRequestOptions,
): Promise<OpenCodeCommandInfo[]> {
	const res = await fetch(
		withDirectory(`${base(opencodeBaseUrl)}/command`, options?.directory),
		{
			cache: "no-store",
			credentials: "include",
		},
	);
	return handleResponse<OpenCodeCommandInfo[]>(res);
}

export async function runShellCommand(
	opencodeBaseUrl: string,
	sessionId: string,
	command: string,
	agent: string,
	model?: { providerID: string; modelID: string },
	options?: OpencodeRequestOptions,
): Promise<OpenCodeMessageWithParts> {
	const body: Record<string, unknown> = { command, agent };
	if (model) body.model = model;

	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}/shell`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(body),
			credentials: "include",
		},
	);
	return handleResponse<OpenCodeMessageWithParts>(res);
}

export async function runShellCommandAsync(
	opencodeBaseUrl: string,
	sessionId: string,
	command: string,
	agent: string,
	model?: { providerID: string; modelID: string },
	options?: OpencodeRequestOptions,
): Promise<boolean> {
	const body: Record<string, unknown> = { command, agent };
	if (model) body.model = model;

	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}/shell`,
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

// Send a slash command to opencode (e.g., /init, /undo, /redo, /share, /help, or custom commands)
// Command should be the name without the slash (e.g., "init", "help")
// Arguments is the string after the command name (e.g., for "/test foo bar", args would be "foo bar")
export async function sendCommandAsync(
	opencodeBaseUrl: string,
	sessionId: string,
	command: string,
	args = "",
	options?: OpencodeRequestOptions,
): Promise<boolean> {
	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}/command`,
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
	opencodeBaseUrl: string,
	title?: string,
	parentID?: string,
	options?: OpencodeRequestOptions,
): Promise<OpenCodeSession> {
	const res = await fetch(
		withDirectory(`${base(opencodeBaseUrl)}/session`, options?.directory),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ title, parentID }),
			credentials: "include",
		},
	);
	return handleResponse<OpenCodeSession>(res);
}

export async function deleteSession(
	opencodeBaseUrl: string,
	sessionId: string,
	options?: OpencodeRequestOptions,
): Promise<void> {
	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}`,
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
	opencodeBaseUrl: string,
	sessionId: string,
	updates: { title?: string },
	options?: OpencodeRequestOptions,
): Promise<OpenCodeSession> {
	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}`,
			options?.directory,
		),
		{
			method: "PATCH",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(updates),
			credentials: "include",
		},
	);
	return handleResponse<OpenCodeSession>(res);
}

/**
 * Fork a session at a specific message point.
 * Creates a new session with all conversation history up to (and including) the specified message.
 * If no messageID is provided, forks from the current end of the conversation.
 */
export async function forkSession(
	opencodeBaseUrl: string,
	sessionId: string,
	messageId?: string,
	options?: OpencodeRequestOptions,
): Promise<OpenCodeSession> {
	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}/fork`,
			options?.directory,
		),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ messageID: messageId }),
			credentials: "include",
		},
	);
	return handleResponse<OpenCodeSession>(res);
}

export type EventCallback = (event: {
	type: string;
	properties: unknown;
}) => void;

// Permission types for tool execution approval
// Matches OpenCode SDK Permission type
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

// Respond to a permission request
export async function respondToPermission(
	opencodeBaseUrl: string,
	sessionId: string,
	permissionId: string,
	response: PermissionResponse,
	options?: OpencodeRequestOptions,
): Promise<void> {
	const res = await fetch(
		withDirectory(
			`${base(opencodeBaseUrl)}/session/${sessionId}/permission/${permissionId}`,
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
	opencodeBaseUrl: string,
	sessionId: string,
	options?: OpencodeRequestOptions,
): Promise<Permission[]> {
	const url = withDirectory(
		`${base(opencodeBaseUrl)}/session/${sessionId}/permission`,
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

type SessionStatusMap = Record<string, { status: string }>;

function tryParseJson(value: string): unknown | null {
	try {
		return JSON.parse(value) as unknown;
	} catch {
		return null;
	}
}

// Subscribe to events.
// Prefer SSE (/event) and fall back to polling (/session/status) with a conservative interval.
function extractWorkspaceSessionIdFromOpencodeBaseUrl(
	opencodeBaseUrl: string,
): string | null {
	const match = opencodeBaseUrl.match(/\/session\/([^/]+)\/code(?:\/)?$/);
	return match ? match[1] : null;
}

export function subscribeToEvents(
	opencodeBaseUrl: string,
	callback: EventCallback,
	directControlPlaneUrl?: string,
	options?: OpencodeRequestOptions,
) {
	let active = true;
	const statusBySession: Record<string, string> = {};
	let transportMode: "sse" | "polling" = "sse";

	let pollTimeout: ReturnType<typeof setTimeout> | null = null;
	let pollDelayMs = 2000;
	const minPollDelayMs = 2000;
	const maxPollDelayMs = 15000;

	const stopPolling = () => {
		if (pollTimeout) clearTimeout(pollTimeout);
		pollTimeout = null;
	};

	const setTransportMode = (mode: "sse" | "polling", reason: string) => {
		if (!active) return;
		if (transportMode === mode) return;
		transportMode = mode;
		callback({ type: "transport.mode", properties: { mode, reason } });
	};

	const emitStatusTransitions = (status: SessionStatusMap) => {
		for (const [sessionId, sessionStatus] of Object.entries(status)) {
			const prevStatus = statusBySession[sessionId];
			const currentStatus = sessionStatus.status;

			if (prevStatus !== currentStatus) {
				if (currentStatus === "idle") {
					callback({ type: "session.idle", properties: { sessionId } });
				} else if (currentStatus === "busy") {
					callback({ type: "session.busy", properties: { sessionId } });
				}
				callback({ type: "message.updated", properties: { sessionId } });
			}

			statusBySession[sessionId] = currentStatus;
		}
	};

	const poll = async () => {
		if (!active) return;

		try {
			const statusBase = (() => {
				const direct = trimTrailingSlash(directControlPlaneUrl ?? "");
				const sessionId =
					extractWorkspaceSessionIdFromOpencodeBaseUrl(opencodeBaseUrl);
				if (direct && sessionId) return `${direct}/session/${sessionId}/code`;
				return base(opencodeBaseUrl);
			})();

			const statusUrl = new URL(
				`${statusBase}/session/status`,
				typeof window === "undefined"
					? "http://localhost"
					: window.location.href,
			);
			if (options?.directory) {
				statusUrl.searchParams.set("directory", options.directory);
			}
			const res = await fetch(statusUrl.toString(), {
				cache: "no-store",
				credentials: "include",
			});
			if (res.ok) {
				const status = (await res.json()) as SessionStatusMap;
				emitStatusTransitions(status);
				pollDelayMs = minPollDelayMs;
			} else {
				if (res.status === 503) {
					const sessionId =
						extractWorkspaceSessionIdFromOpencodeBaseUrl(opencodeBaseUrl);
					if (sessionId) {
						callback({
							type: "session.unavailable",
							properties: { sessionId },
						});
					}
				}
				pollDelayMs = Math.min(maxPollDelayMs, Math.round(pollDelayMs * 1.5));
			}
		} catch {
			pollDelayMs = Math.min(maxPollDelayMs, Math.round(pollDelayMs * 1.5));
		}

		if (!active) return;
		pollTimeout = setTimeout(poll, pollDelayMs);
	};

	let eventSource: EventSource | null = null;
	// Use direct control plane URL for SSE to avoid Next.js proxy buffering issues.
	// CORS is configured on the backend to allow localhost:3000.
	const sseUrl = (() => {
		const direct = trimTrailingSlash(directControlPlaneUrl ?? "");
		const sessionId =
			extractWorkspaceSessionIdFromOpencodeBaseUrl(opencodeBaseUrl);
		const sseBase =
			direct && sessionId
				? `${direct}/session/${sessionId}/code`
				: base(opencodeBaseUrl);
		const url = new URL(
			`${sseBase}/event`,
			typeof window === "undefined" ? "http://localhost" : window.location.href,
		);
		if (options?.directory) {
			url.searchParams.set("directory", options.directory);
		}
		return url.toString();
	})();

	const startSse = () => {
		console.log("[SSE] Starting SSE connection to:", sseUrl);
		try {
			eventSource = new EventSource(sseUrl, { withCredentials: true });
		} catch (err) {
			console.error("[SSE] EventSource constructor failed:", err);
			eventSource = null;
			setTransportMode("polling", "eventsource_constructor_failed");
			poll();
			return;
		}

		eventSource.onopen = () => {
			console.log("[SSE] Connection opened");
			pollDelayMs = minPollDelayMs;
			stopPolling();
			setTransportMode("sse", "connected");
		};

		eventSource.onmessage = (event) => {
			const parsed = tryParseJson(event.data);

			if (
				parsed &&
				typeof parsed === "object" &&
				parsed !== null &&
				"type" in parsed
			) {
				const typed = parsed as { type: string; properties?: unknown };
				// Log permission events fully for debugging
				if (typed.type.startsWith("permission")) {
					console.log("[SSE] Permission event:", typed.type, typed.properties);
				} else if (
					typed.type !== "message.updated" &&
					typed.type !== "message.part.updated"
				) {
					console.log("[SSE] Event:", typed.type);
				}
				callback({ type: typed.type, properties: typed.properties ?? typed });
				return;
			}

			console.log("[SSE] Untyped message:", event.data?.substring?.(0, 100));
			callback({
				type: "message.updated",
				properties: parsed ?? { raw: event.data },
			});
		};

		eventSource.onerror = (err) => {
			console.error("[SSE] Connection error:", err);
			// If SSE isn't available (dev proxy/config), fall back to polling.
			if (eventSource) {
				eventSource.close();
				eventSource = null;
			}
			setTransportMode("polling", "eventsource_error");
			if (!pollTimeout) poll();
		};
	};

	startSse();

	return () => {
		active = false;
		stopPolling();
		if (eventSource) {
			eventSource.close();
			eventSource = null;
		}
	};
}

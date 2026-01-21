import { isTauri } from "@/lib/tauri-fetch-polyfill";
import { toAbsoluteWsUrl } from "@/lib/url";

// ============================================================================
// Token Storage (for Tauri/mobile where cookies don't work)
// ============================================================================

const AUTH_TOKEN_KEY = "octo:authToken";

export function getAuthToken(): string | null {
	if (typeof window === "undefined") return null;
	return localStorage.getItem(AUTH_TOKEN_KEY);
}

export function setAuthToken(token: string | null): void {
	if (typeof window === "undefined") return;
	if (token) {
		localStorage.setItem(AUTH_TOKEN_KEY, token);
		// Also set as cookie for WebSocket auth (browsers can't set headers on WS)
		// Use SameSite=Lax to allow cross-origin requests from same site
		// eslint-disable-next-line unicorn/no-document-cookie -- CookieStore is not widely supported.
		document.cookie = `auth_token=${encodeURIComponent(token)}; path=/; SameSite=Lax`;
	} else {
		localStorage.removeItem(AUTH_TOKEN_KEY);
		// Clear the cookie
		// eslint-disable-next-line unicorn/no-document-cookie -- CookieStore is not widely supported.
		document.cookie =
			"auth_token=; path=/; expires=Thu, 01 Jan 1970 00:00:00 GMT";
	}
}

/**
 * Get auth headers for requests.
 * Uses Bearer token if available (works for both Tauri and browser).
 */
export function getAuthHeaders(): Record<string, string> {
	const token = getAuthToken();
	if (!token) return {};
	return { Authorization: `Bearer ${token}` };
}

/**
 * Authenticated fetch - automatically includes auth headers for Tauri
 */
async function authFetch(
	input: RequestInfo | URL,
	init?: RequestInit,
): Promise<Response> {
	const headers = {
		...getAuthHeaders(),
		...(init?.headers instanceof Headers
			? Object.fromEntries(init.headers.entries())
			: (init?.headers as Record<string, string> | undefined)),
	};
	return fetch(input, {
		...init,
		headers,
		credentials: "include",
	});
}

// ============================================================================
// Auth Types
// ============================================================================

export type UserInfo = {
	id: string;
	name: string;
	email: string;
	role: string;
};

export type LoginRequest = {
	username: string;
	password: string;
};

export type LoginResponse = {
	token: string;
	user: UserInfo;
};

export type RegisterRequest = {
	username: string;
	email: string;
	password: string;
	invite_code: string;
	display_name?: string;
};

export type RegisterResponse = {
	token: string;
	user: UserInfo;
};

// ============================================================================
// Session Types
// ============================================================================

export type WorkspaceSessionStatus =
	| "pending"
	| "starting"
	| "running"
	| "stopping"
	| "stopped"
	| "failed";

/** Workspace mode for a persona */
export type WorkspaceMode = "default_only" | "ask" | "any";

/** Persona metadata from persona.toml */
export type Persona = {
	/** Unique identifier (directory name) */
	id: string;
	/** Display name of the persona */
	name: string;
	/** Short description of what this persona does */
	description: string;
	/** Accent color for UI (hex color, e.g., "#6366f1") */
	color?: string | null;
	/** Path to avatar image (relative to persona directory) */
	avatar?: string | null;
	/** Whether this is the default persona */
	is_default: boolean;
	/** opencode agent ID to use */
	agent_id: string;
	/** Default working directory (optional) */
	default_workdir?: string | null;
	/** Workspace mode (default_only, ask, or any) */
	workspace_mode: WorkspaceMode;
	/** If true, has own directory with opencode.json. If false, wrapper for existing agent */
	standalone: boolean;
	/** If true, can work on external projects. If false, only works in own persona directory */
	project_access: boolean;
};

export type WorkspaceSession = {
	id: string;
	container_id: string | null;
	container_name: string;
	user_id: string;
	workspace_path: string;
	persona_path: string | null;
	image: string;
	opencode_port: number;
	fileserver_port: number;
	ttyd_port: number;
	status: WorkspaceSessionStatus;
	created_at: string;
	started_at: string | null;
	stopped_at: string | null;
	error_message: string | null;
	/** Persona metadata (if session has a persona_path with persona.toml) */
	persona?: Persona | null;
};

export type ProjectLogo = {
	/** Path relative to project root (e.g., "logo/project_logo_white.svg") */
	path: string;
	/** Logo variant (e.g., "white", "black", "white_on_black") */
	variant: string;
};

export type WorkspaceDirEntry = {
	name: string;
	path: string;
	type: "directory";
	/** Project logo if found in logo/ directory */
	logo?: ProjectLogo;
};

export type ProjectTemplateEntry = {
	name: string;
	path: string;
	description?: string;
};

export type ListProjectTemplatesResponse = {
	/** Whether templates are configured (repo_path is set). */
	configured: boolean;
	/** List of available templates. */
	templates: ProjectTemplateEntry[];
};

export type CreateProjectFromTemplateRequest = {
	template_path: string;
	project_path: string;
	shared?: boolean;
};

export type CreateWorkspaceSessionRequest = {
	workspace_path?: string;
	image?: string;
	/** Persona ID to use for this session */
	persona_id?: string;
	env?: Record<string, string>;
};

type ApiErrorResponse = {
	error?: string;
};

const trimTrailingSlash = (value: string) => value.replace(/\/$/, "");
const controlPlaneStorageKey = "octo:controlPlaneUrl";

const env =
	(import.meta as ImportMeta & { env?: Record<string, string | undefined> })
		.env ?? (typeof process !== "undefined" ? process.env : {});

function normalizeControlPlaneUrl(value: string | null | undefined): string {
	if (!value) return "";
	return trimTrailingSlash(value.trim());
}

/** Normalize session status to lowercase (backend may return e.g. "Running" instead of "running") */
function normalizeSession(session: WorkspaceSession): WorkspaceSession {
	return {
		...session,
		status: session.status.toLowerCase() as WorkspaceSessionStatus,
	};
}

export function getControlPlaneBaseUrl(): string {
	if (typeof window !== "undefined") {
		try {
			const stored = window.localStorage.getItem(controlPlaneStorageKey);
			const normalized = normalizeControlPlaneUrl(stored);
			if (normalized) return normalized;
		} catch (err) {
			console.warn("[control-plane] Failed to read stored base URL:", err);
		}
	}
	return normalizeControlPlaneUrl(env.VITE_CONTROL_PLANE_URL ?? "");
}

export function setControlPlaneBaseUrl(value: string | null): void {
	if (typeof window === "undefined") return;
	const normalized = normalizeControlPlaneUrl(value ?? "");
	try {
		if (normalized) {
			window.localStorage.setItem(controlPlaneStorageKey, normalized);
		} else {
			window.localStorage.removeItem(controlPlaneStorageKey);
		}
	} catch (err) {
		console.warn("[control-plane] Failed to store base URL:", err);
	}
}

export function controlPlaneDirectBaseUrl(): string {
	return getControlPlaneBaseUrl();
}

export function controlPlaneApiUrl(path: string): string {
	const base = getControlPlaneBaseUrl();
	const normalizedPath = path.startsWith("/") ? path : `/${path}`;
	if (base) {
		const stripped = normalizedPath.startsWith("/api")
			? normalizedPath.replace(/^\/api/, "")
			: normalizedPath;
		return `${base}${stripped}`;
	}
	if (normalizedPath.startsWith("/api")) return normalizedPath;
	return `/api${normalizedPath}`;
}

async function readApiError(res: Response): Promise<string> {
	const contentType = res.headers.get("content-type") ?? "";
	if (contentType.includes("application/json")) {
		const parsed = (await res
			.json()
			.catch(() => null)) as ApiErrorResponse | null;
		if (parsed?.error) return parsed.error;
	}
	return (await res.text().catch(() => res.statusText)) || res.statusText;
}

// ============================================================================
// Features API
// ============================================================================

/** Per-visualizer voice settings from backend */
export type VisualizerVoiceConfig = {
	voice: string;
	speed: number;
};

/** Voice configuration from backend */
export type VoiceFeatureConfig = {
	stt_url: string;
	tts_url: string;
	vad_timeout_ms: number;
	default_voice: string;
	default_speed: number;
	auto_language_detect: boolean;
	tts_muted: boolean;
	continuous_mode: boolean;
	default_visualizer: string;
	interrupt_word_count: number;
	interrupt_backoff_ms: number;
	visualizer_voices: Record<string, VisualizerVoiceConfig>;
};

export type SessionAutoAttachMode = "off" | "attach" | "resume";

export type Features = {
	mmry_enabled: boolean;
	session_auto_attach?: SessionAutoAttachMode;
	session_auto_attach_scan?: boolean;
	/** Voice configuration (present if voice mode is enabled) */
	voice?: VoiceFeatureConfig | null;
	/** Use WebSocket for real-time events instead of SSE */
	websocket_events?: boolean;
};

export async function getFeatures(): Promise<Features> {
	const res = await authFetch(controlPlaneApiUrl("/api/features"), {
		credentials: "include",
	});
	if (!res.ok) {
		// Return defaults if endpoint not available
		return {
			mmry_enabled: false,
			voice: null,
			session_auto_attach: "off",
			session_auto_attach_scan: false,
		};
	}
	return res.json();
}

// ============================================================================
// Dashboard APIs
// ============================================================================

export type SchedulerEntry = {
	name: string;
	status: string;
	schedule: string;
	command: string;
	next_run?: string | null;
};

export type SchedulerOverview = {
	stats: {
		total: number;
		enabled: number;
		disabled: number;
	};
	schedules: SchedulerEntry[];
};

export async function getSchedulerOverview(): Promise<SchedulerOverview> {
	const res = await authFetch(controlPlaneApiUrl("/api/scheduler/overview"), {
		credentials: "include",
	});
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

export type FeedFetchResponse = {
	url: string;
	content: string;
	content_type?: string | null;
};

export async function fetchFeed(url: string): Promise<FeedFetchResponse> {
	const endpoint = controlPlaneApiUrl(
		`/api/feeds/fetch?url=${encodeURIComponent(url)}`,
	);
	const res = await authFetch(endpoint, { credentials: "include" });
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

export type CodexBarUsagePayload = {
	provider: string;
	account?: string | null;
	version?: string | null;
	source?: string | null;
	status?: {
		indicator?: string | null;
		description?: string | null;
		updatedAt?: string | null;
		url?: string | null;
	} | null;
	usage?: {
		primary?: {
			usedPercent?: number | null;
			windowMinutes?: number | null;
			resetsAt?: string | null;
		} | null;
		secondary?: {
			usedPercent?: number | null;
			windowMinutes?: number | null;
			resetsAt?: string | null;
		} | null;
		updatedAt?: string | null;
		accountEmail?: string | null;
		accountOrganization?: string | null;
		loginMethod?: string | null;
	} | null;
	credits?: {
		remaining?: number | null;
		updatedAt?: string | null;
	} | null;
	error?: {
		message?: string | null;
	} | null;
};

export async function getCodexBarUsage(): Promise<
	CodexBarUsagePayload[] | null
> {
	const res = await authFetch(controlPlaneApiUrl("/api/codexbar/usage"), {
		credentials: "include",
	});
	if (res.status === 404) return null;
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

// ============================================================================
// Auth API
// ============================================================================

export async function login(request: LoginRequest): Promise<LoginResponse> {
	const url = controlPlaneApiUrl("/api/auth/login");
	const options: RequestInit = {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	};

	// Retry logic for transient network errors (e.g., ERR_CONNECTION_REFUSED on first attempt)
	const maxRetries = 2;
	let lastError: Error | undefined;

	for (let attempt = 0; attempt <= maxRetries; attempt++) {
		try {
			const res = await fetch(url, options);
			if (!res.ok) throw new Error(await readApiError(res));
			const data: LoginResponse = await res.json();
			// Store token for Tauri/mobile
			if (data.token) {
				setAuthToken(data.token);
			}
			return data;
		} catch (error) {
			lastError = error instanceof Error ? error : new Error(String(error));
			// Only retry on network errors, not HTTP errors
			const isNetworkError =
				lastError.message.includes("Failed to fetch") ||
				lastError.message.includes("NetworkError") ||
				lastError.message.includes("network") ||
				lastError.name === "TypeError"; // fetch throws TypeError on network failure

			if (!isNetworkError || attempt === maxRetries) {
				throw lastError;
			}

			// Wait before retrying (50ms, then 100ms)
			await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
		}
	}

	throw lastError ?? new Error("Login failed");
}

export async function register(
	request: RegisterRequest,
): Promise<RegisterResponse> {
	const url = controlPlaneApiUrl("/api/auth/register");
	const options: RequestInit = {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	};

	// Retry logic for transient network errors
	const maxRetries = 2;
	let lastError: Error | undefined;

	for (let attempt = 0; attempt <= maxRetries; attempt++) {
		try {
			const res = await fetch(url, options);
			if (!res.ok) throw new Error(await readApiError(res));
			const data: RegisterResponse = await res.json();
			// Store token for Tauri/mobile
			if (data.token) {
				setAuthToken(data.token);
			}
			return data;
		} catch (error) {
			lastError = error instanceof Error ? error : new Error(String(error));
			// Only retry on network errors, not HTTP errors
			const isNetworkError =
				lastError.message.includes("Failed to fetch") ||
				lastError.message.includes("NetworkError") ||
				lastError.message.includes("network") ||
				lastError.name === "TypeError";

			if (!isNetworkError || attempt === maxRetries) {
				throw lastError;
			}

			// Wait before retrying
			await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
		}
	}

	throw lastError ?? new Error("Registration failed");
}

export async function logout(): Promise<void> {
	const res = await authFetch(controlPlaneApiUrl("/api/auth/logout"), {
		method: "POST",
		headers: { ...getAuthHeaders() },
		credentials: "include",
	});
	// Clear token regardless of response
	setAuthToken(null);
	if (!res.ok) throw new Error(await readApiError(res));
}

export async function getCurrentUser(): Promise<UserInfo | null> {
	const res = await authFetch(controlPlaneApiUrl("/api/me"), {
		credentials: "include",
	});
	if (res.status === 401) return null;
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** @deprecated Use login() instead */
export async function devLogin(): Promise<boolean> {
	try {
		await login({ username: "dev", password: "devpassword123" });
		return true;
	} catch {
		return false;
	}
}

export async function listWorkspaceSessions(): Promise<WorkspaceSession[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/sessions"), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const sessions: WorkspaceSession[] = await res.json();
	return sessions.map(normalizeSession);
}

/** Project/workspace directory entry */
export type ProjectEntry = {
	name: string;
	path: string;
	type: "directory";
	/** Project logo if found in logo/ directory */
	logo?: ProjectLogo;
};

/** List available projects (directories in workspace_dir) */
export async function listProjects(): Promise<ProjectEntry[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/projects"), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function createWorkspaceSession(
	request: CreateWorkspaceSessionRequest = {},
): Promise<WorkspaceSession> {
	const res = await authFetch(controlPlaneApiUrl("/api/sessions"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = (await res.json()) as
		| { session?: WorkspaceSession }
		| WorkspaceSession;
	if ("id" in data) return normalizeSession(data);
	if (data.session && "id" in data.session)
		return normalizeSession(data.session);
	throw new Error("Unexpected create session response");
}

/** Get or create a session - handles auto-resume and auto-upgrade */
export async function getOrCreateWorkspaceSession(
	request: CreateWorkspaceSessionRequest = {},
): Promise<WorkspaceSession> {
	const res = await authFetch(
		controlPlaneApiUrl("/api/sessions/get-or-create"),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(request),
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	const session: WorkspaceSession = await res.json();
	return normalizeSession(session);
}

/** Get or create a session for a specific workspace path.
 * This is the preferred way to resume a session from chat history.
 * It will find an existing running session for the workspace or create a new one,
 * enforcing LRU cap by stopping oldest idle session if needed.
 */
export async function getOrCreateSessionForWorkspace(
	workspacePath: string,
): Promise<WorkspaceSession> {
	const res = await authFetch(
		controlPlaneApiUrl("/api/sessions/get-or-create-for-workspace"),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ workspace_path: workspacePath }),
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	const session: WorkspaceSession = await res.json();
	return normalizeSession(session);
}

/**
 * Get a workspace session by ID or alias.
 *
 * The sessionIdOrAlias can be either:
 * - A full session UUID (e.g., "6a03da55-2757-4d71-b421-af929bc4aef5")
 * - A readable alias (e.g., "foxy-geek")
 */
export async function getWorkspaceSession(
	sessionIdOrAlias: string,
): Promise<WorkspaceSession | null> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/sessions/${sessionIdOrAlias}`),
		{
			credentials: "include",
		},
	);
	if (res.status === 404) return null;
	if (!res.ok) throw new Error(await readApiError(res));
	const session: WorkspaceSession = await res.json();
	return normalizeSession(session);
}

/** Touch session activity to prevent idle timeout */
export async function touchSessionActivity(sessionId: string): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/sessions/${sessionId}/activity`),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

export async function stopWorkspaceSession(sessionId: string): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/sessions/${sessionId}/stop`),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

export async function resumeWorkspaceSession(
	sessionId: string,
): Promise<WorkspaceSession> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/sessions/${sessionId}/resume`),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	const data = (await res.json()) as
		| { session?: WorkspaceSession }
		| WorkspaceSession;
	if ("id" in data) return data;
	if (data.session && "id" in data.session) return data.session;
	throw new Error("Unexpected resume session response");
}

export async function deleteWorkspaceSession(sessionId: string): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/sessions/${sessionId}`),
		{
			method: "DELETE",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

/**
 * Restart a workspace session by stopping and resuming it.
 * This is useful for applying config changes that require a restart.
 * Waits for the session to be fully running before returning.
 */
export async function restartWorkspaceSession(
	sessionId: string,
): Promise<WorkspaceSession> {
	await stopWorkspaceSession(sessionId);
	const session = await resumeWorkspaceSession(sessionId);

	// Wait for session to be fully running (poll every 500ms, max 30s)
	const maxAttempts = 60;
	for (let i = 0; i < maxAttempts; i++) {
		const current = await getWorkspaceSession(sessionId);
		if (current?.status === "running") {
			return current;
		}
		if (current?.status === "failed" || current?.status === "error") {
			const errorMsg = current.error_message || current.status;
			throw new Error(`Session failed to restart: ${errorMsg}`);
		}
		await new Promise((resolve) => setTimeout(resolve, 500));
	}

	// Return what we have even if not fully running yet
	return session;
}

export type SessionUpdateInfo = {
	update_available: boolean;
	current_digest: string | null;
	latest_digest: string | null;
};

export async function checkSessionUpdate(
	sessionId: string,
): Promise<SessionUpdateInfo> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/sessions/${sessionId}/update`),
		{
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function upgradeWorkspaceSession(
	sessionId: string,
): Promise<WorkspaceSession> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/sessions/${sessionId}/upgrade`),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

// ============================================================================
// Persona API
// ============================================================================

/** List all available personas */
export async function listPersonas(): Promise<Persona[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/personas"), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get a specific persona by ID */
export async function getPersona(personaId: string): Promise<Persona> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/personas/${personaId}`),
		{
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

// ============================================================================
// Workspace Projects API
// ============================================================================

export async function listWorkspaceDirectories(
	path = ".",
): Promise<WorkspaceDirEntry[]> {
	const url = new URL(
		controlPlaneApiUrl("/api/projects"),
		window.location.origin,
	);
	url.searchParams.set("path", path);
	const res = await authFetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function listProjectTemplates(): Promise<ListProjectTemplatesResponse> {
	const url = new URL(
		controlPlaneApiUrl("/api/projects/templates"),
		window.location.origin,
	);
	const res = await authFetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function createProjectFromTemplate(
	payload: CreateProjectFromTemplateRequest,
): Promise<WorkspaceDirEntry> {
	const url = new URL(
		controlPlaneApiUrl("/api/projects/templates"),
		window.location.origin,
	);
	const res = await authFetch(url.toString(), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(payload),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/**
 * Get the URL for a project logo.
 * @param projectPath - The project path (relative to workspace root, e.g., "octo" or "subfolder/project")
 * @param logoPath - The logo path relative to project root (e.g., "logo/project_logo_white.svg")
 */
export function getProjectLogoUrl(
	projectPath: string,
	logoPath: string,
): string {
	// Combine project path and logo path
	// The path should be relative to workspace root, not absolute
	const fullPath = `${projectPath}/${logoPath}`;
	return controlPlaneApiUrl(`/api/projects/logo/${fullPath}`);
}

// ============================================================================
// Chat History Types (from disk, no running opencode needed)
// ============================================================================

/** A chat session read directly from OpenCode's storage on disk */
export type ChatSession = {
	/** Session ID (e.g., "ses_xxx") */
	id: string;
	/** Human-readable ID (e.g., "cold-lamp") - deterministically generated from session ID */
	readable_id: string;
	/** Session title */
	title: string | null;
	/** Parent session ID (for child sessions) */
	parent_id: string | null;
	/** Workspace/project path */
	workspace_path: string;
	/** Project name (derived from path) */
	project_name: string;
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

// ============================================================================
// Chat History API (reads from disk, no running opencode needed)
// ============================================================================

/** List all chat sessions from OpenCode history */
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
	return res.json();
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
	return res.json();
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

/** Request to update a chat session */
export type UpdateChatSessionRequest = {
	title?: string;
};

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
	/** Message content parts */
	parts: ChatMessagePart[];
};

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

import type {
	OpenCodeMessage,
	OpenCodeMessageWithParts,
	OpenCodePart,
} from "./opencode-client";

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

// ============================================================================
// Proxy URLs
// ============================================================================

export function opencodeProxyBaseUrl(sessionId: string) {
	return controlPlaneApiUrl(`/api/session/${sessionId}/code`);
}

export function terminalProxyPath(sessionId: string) {
	return `/session/${sessionId}/term`;
}

export function fileserverProxyBaseUrl(sessionId: string) {
	return controlPlaneApiUrl(`/api/session/${sessionId}/files`);
}

export function fileserverWorkspaceBaseUrl() {
	return controlPlaneApiUrl("/api/workspace/files");
}

export function mainChatFilesBaseUrl() {
	return controlPlaneApiUrl("/api/main/files");
}

export function workspaceFileUrl(workspacePath: string, path: string): string {
	const baseUrl = fileserverWorkspaceBaseUrl();
	const origin =
		typeof window !== "undefined" ? window.location.origin : "http://localhost";
	const url = new URL(`${baseUrl}/file`, origin);
	url.searchParams.set("path", path);
	url.searchParams.set("workspace_path", workspacePath);
	return url.toString();
}

export function terminalWorkspaceProxyPath(workspacePath: string) {
	return `/workspace/term?workspace_path=${encodeURIComponent(workspacePath)}`;
}

export function memoriesWorkspaceBaseUrl(workspacePath: string) {
	return controlPlaneApiUrl(
		`/api/workspace/memories?workspace_path=${encodeURIComponent(workspacePath)}`,
	);
}

export function voiceProxyWsUrl(kind: "stt" | "tts"): string {
	let wsUrl = toAbsoluteWsUrl(controlPlaneApiUrl(`/api/voice/${kind}`));
	// Add auth token for WebSocket authentication
	const token = getAuthToken();
	if (token) {
		const separator = wsUrl.includes("?") ? "&" : "?";
		wsUrl = `${wsUrl}${separator}token=${encodeURIComponent(token)}`;
	}
	return wsUrl;
}

export function browserStreamWsUrl(sessionId: string): string {
	let wsUrl = toAbsoluteWsUrl(
		controlPlaneApiUrl(`/api/session/${sessionId}/browser/stream`),
	);
	const token = getAuthToken();
	if (token) {
		const separator = wsUrl.includes("?") ? "&" : "?";
		wsUrl = `${wsUrl}${separator}token=${encodeURIComponent(token)}`;
	}
	return wsUrl;
}

// ============================================================================
// Workspace Config (opencode.json)
// ============================================================================

/** Tool permission action */
export type PermissionAction = "ask" | "allow" | "deny";

/** Permission rule - can be a simple action or an object with pattern-specific rules */
export type PermissionRule =
	| PermissionAction
	| Record<string, PermissionAction>;

/** Permission configuration for tools - can be a global action or per-tool config */
export type PermissionConfig =
	| PermissionAction
	| {
			[toolName: string]: PermissionRule;
	  };

/** Compaction settings */
export interface CompactionConfig {
	auto?: boolean;
	prune?: boolean;
}

/** Share mode for session sharing */
export type ShareMode = "manual" | "auto" | "disabled";

/** Full OpenCode workspace configuration (opencode.json) */
export interface WorkspaceConfig {
	/** Model in format "provider/model" */
	model?: string;
	/** Default agent to use */
	default_agent?: string;
	/** Share mode */
	share?: ShareMode;
	/** Compaction settings */
	compaction?: CompactionConfig;
	/** Instruction file paths */
	instructions?: string[];
	/** Tool permissions */
	permission?: PermissionConfig;
	/** Disabled tools (legacy, prefer permission) */
	disabled_tools?: string[];
	/** MCP servers configuration */
	mcp?: Record<string, unknown>;
	/** Custom providers configuration */
	providers?: Record<string, unknown>;
}

/**
 * Read the global opencode.json from ~/.config/opencode/opencode.json.
 * Returns null if the file doesn't exist or can't be parsed.
 */
export async function getGlobalOpencodeConfig(): Promise<WorkspaceConfig | null> {
	try {
		const res = await authFetch(controlPlaneApiUrl("/api/opencode/config"), {
			credentials: "include",
		});
		if (!res.ok) return null;

		const config = await res.json();
		return config as WorkspaceConfig;
	} catch {
		return null;
	}
}

/**
 * Read opencode.json from the workspace root.
 * Returns null if the file doesn't exist or can't be parsed.
 */
export async function getWorkspaceConfig(
	sessionId: string,
): Promise<WorkspaceConfig | null> {
	try {
		const res = await authFetch(
			`${fileserverProxyBaseUrl(sessionId)}/file?path=opencode.json`,
			{ credentials: "include" },
		);
		if (!res.ok) return null;

		const config = await res.json();
		return config as WorkspaceConfig;
	} catch {
		return null;
	}
}

/**
 * Save opencode.json to the workspace root.
 * Creates the file if it doesn't exist.
 */
export async function saveWorkspaceConfig(
	sessionId: string,
	config: WorkspaceConfig,
): Promise<void> {
	const res = await authFetch(
		`${fileserverProxyBaseUrl(sessionId)}/file?path=opencode.json`,
		{
			method: "PUT",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(config, null, 2),
			credentials: "include",
		},
	);
	if (!res.ok) {
		const error = await readApiError(res);
		throw new Error(`Failed to save config: ${error}`);
	}
}

// ============================================================================
// Settings Types and API
// ============================================================================

/** A settings value with metadata */
export type SettingsValue = {
	/** The current value */
	value: unknown;
	/** Whether this value is explicitly set in config (vs default) */
	is_configured: boolean;
	/** The default value from schema (if any) */
	default?: unknown;
};

/** Map of dotted paths to settings values */
export type SettingsValues = Record<string, SettingsValue>;

/** Request to update settings */
export type SettingsUpdateRequest = {
	values: Record<string, unknown>;
};

/** Get the JSON schema for an app's settings (filtered by user permissions) */
export async function getSettingsSchema(app: string): Promise<unknown> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/settings/schema?app=${encodeURIComponent(app)}`),
		{
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get current settings values for an app */
export async function getSettingsValues(app: string): Promise<SettingsValues> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/settings?app=${encodeURIComponent(app)}`),
		{
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Update settings values for an app */
export async function updateSettingsValues(
	app: string,
	updates: SettingsUpdateRequest,
): Promise<SettingsValues> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/settings?app=${encodeURIComponent(app)}`),
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

/** Reload settings from disk (admin only) */
export async function reloadSettings(app: string): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/settings/reload?app=${encodeURIComponent(app)}`),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

// ============================================================================
// Main Chat API
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
	message_count: number;
};

/** Message loaded from a Pi session JSONL file */
export type PiSessionMessage = {
	id: string;
	role: "user" | "assistant" | "system";
	content: unknown;
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

/** In-session search result from CASS */
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

/** Search within a specific Pi session using CASS */
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
// CASS (Coding Agent Session Search)
// ============================================================================

/** Agent filter for search */
export type CassAgentFilter = "all" | "pi_agent" | "opencode" | string;

/** Search query parameters */
export type CassSearchQuery = {
	/** Search query string */
	q: string;
	/** Agent filter: "all", "pi_agent", "opencode", or comma-separated */
	agents?: CassAgentFilter;
	/** Maximum results to return */
	limit?: number;
};

/** A single search hit from cass */
export type CassSearchHit = {
	/** Agent type (pi_agent, opencode, etc.) */
	agent: string;
	/** Path to the session file */
	source_path: string;
	/** Session identifier extracted from path */
	session_id?: string;
	/** Workspace/project directory */
	workspace?: string;
	/** Message ID if available */
	message_id?: string;
	/** Line number in the source file */
	line_number?: number;
	/** Matched content snippet */
	snippet?: string;
	/** Search relevance score */
	score?: number;
	/** Timestamp of the message (milliseconds since epoch) */
	timestamp?: number;
	/** Role (user, assistant, system) */
	role?: string;
	/** Session/conversation title if available */
	title?: string;
	/** Full content from cass */
	content?: string;
	/** Match type */
	match_type?: string;
};

/** Response from cass search */
export type CassSearchResponse = {
	hits: CassSearchHit[];
	total?: number;
	elapsed_ms?: number;
};

/**
 * Search across coding agent sessions using cass.
 * Searches both Main Chat (pi_agent) and OpenCode sessions.
 */
export async function searchSessions(
	query: CassSearchQuery,
): Promise<CassSearchResponse> {
	const url = new URL(
		controlPlaneApiUrl("/api/search"),
		window.location.origin,
	);
	url.searchParams.set("q", query.q);
	if (query.agents) url.searchParams.set("agents", query.agents);
	if (query.limit) url.searchParams.set("limit", query.limit.toString());

	const res = await authFetch(url.toString(), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

// ============================================================================
// Agent Ask API (Cross-agent communication via @@mentions)
// ============================================================================

/** Request to ask another agent a question */
export type AgentAskRequest = {
	/** Target: "main-chat", "session:<id>", or assistant name */
	target: string;
	/** The question to ask */
	question: string;
	/** Timeout in seconds (default 300) */
	timeout_secs?: number;
	/** Whether to stream the response */
	stream?: boolean;
};

/** Response from asking an agent (non-streaming) */
export type AgentAskResponse = {
	response: string;
	session_id?: string;
};

/** Error when multiple sessions match */
export type AgentAskAmbiguousError = {
	error: string;
	matches: Array<{
		id: string;
		title?: string;
		modified_at: number;
	}>;
};

/**
 * Ask another agent a question.
 * Returns the agent's response after it finishes processing.
 *
 * Target formats:
 * - "main-chat" or "pi" - Main chat assistant
 * - "session:<id>" - Specific session by ID
 * - Custom assistant name (e.g., "jarvis")
 */
export async function askAgent(
	request: AgentAskRequest,
): Promise<AgentAskResponse> {
	const res = await authFetch(controlPlaneApiUrl("/api/agents/ask"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});

	if (!res.ok) {
		const errorText = await res.text();
		// Check if it's an ambiguous response
		try {
			const errorJson = JSON.parse(errorText);
			if (errorJson.matches) {
				throw new AgentAskAmbiguousException(errorJson);
			}
		} catch (e) {
			if (e instanceof AgentAskAmbiguousException) throw e;
		}
		throw new Error(errorText || `Agent ask failed: ${res.status}`);
	}

	return res.json();
}

/** Exception thrown when multiple sessions match the target */
export class AgentAskAmbiguousException extends Error {
	public matches: AgentAskAmbiguousError["matches"];

	constructor(data: AgentAskAmbiguousError) {
		super(data.error);
		this.name = "AgentAskAmbiguousException";
		this.matches = data.matches;
	}
}

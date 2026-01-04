import { toAbsoluteWsUrl } from "@/lib/url";

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
};

export async function getFeatures(): Promise<Features> {
	const res = await fetch(controlPlaneApiUrl("/api/features"), {
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
// Auth API
// ============================================================================

export async function login(request: LoginRequest): Promise<LoginResponse> {
	const res = await fetch(controlPlaneApiUrl("/api/auth/login"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function register(
	request: RegisterRequest,
): Promise<RegisterResponse> {
	const res = await fetch(controlPlaneApiUrl("/api/auth/register"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function logout(): Promise<void> {
	const res = await fetch(controlPlaneApiUrl("/api/auth/logout"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

export async function getCurrentUser(): Promise<UserInfo | null> {
	const res = await fetch(controlPlaneApiUrl("/api/me"), {
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
	const res = await fetch(controlPlaneApiUrl("/api/sessions"), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
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
	const res = await fetch(controlPlaneApiUrl("/api/projects"), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function createWorkspaceSession(
	request: CreateWorkspaceSessionRequest = {},
): Promise<WorkspaceSession> {
	const res = await fetch(controlPlaneApiUrl("/api/sessions"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	const data = (await res.json()) as
		| { session?: WorkspaceSession }
		| WorkspaceSession;
	if ("id" in data) return data;
	if (data.session && "id" in data.session) return data.session;
	throw new Error("Unexpected create session response");
}

/** Get or create a session - handles auto-resume and auto-upgrade */
export async function getOrCreateWorkspaceSession(
	request: CreateWorkspaceSessionRequest = {},
): Promise<WorkspaceSession> {
	const res = await fetch(controlPlaneApiUrl("/api/sessions/get-or-create"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get or create a session for a specific workspace path.
 * This is the preferred way to resume a session from chat history.
 * It will find an existing running session for the workspace or create a new one,
 * enforcing LRU cap by stopping oldest idle session if needed.
 */
export async function getOrCreateSessionForWorkspace(
	workspacePath: string,
): Promise<WorkspaceSession> {
	const res = await fetch(
		controlPlaneApiUrl("/api/sessions/get-or-create-for-workspace"),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ workspace_path: workspacePath }),
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
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
	const res = await fetch(
		controlPlaneApiUrl(`/api/sessions/${sessionIdOrAlias}`),
		{
			credentials: "include",
		},
	);
	if (res.status === 404) return null;
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Touch session activity to prevent idle timeout */
export async function touchSessionActivity(sessionId: string): Promise<void> {
	const res = await fetch(
		controlPlaneApiUrl(`/api/sessions/${sessionId}/activity`),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

export async function stopWorkspaceSession(sessionId: string): Promise<void> {
	const res = await fetch(
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
	const res = await fetch(
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
	const res = await fetch(controlPlaneApiUrl(`/api/sessions/${sessionId}`), {
		method: "DELETE",
		credentials: "include",
	});
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
	const res = await fetch(
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
	const res = await fetch(
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
	const res = await fetch(controlPlaneApiUrl("/api/personas"), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get a specific persona by ID */
export async function getPersona(personaId: string): Promise<Persona> {
	const res = await fetch(controlPlaneApiUrl(`/api/personas/${personaId}`), {
		credentials: "include",
	});
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
	const res = await fetch(url.toString(), {
		cache: "no-store",
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

	const res = await fetch(url.toString(), {
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

	const res = await fetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get a specific chat session by ID */
export async function getChatSession(sessionId: string): Promise<ChatSession> {
	const res = await fetch(
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
	const res = await fetch(
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
	const res = await fetch(
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

export function terminalWorkspaceProxyPath(workspacePath: string) {
	return `/workspace/term?workspace_path=${encodeURIComponent(workspacePath)}`;
}

export function memoriesWorkspaceBaseUrl(workspacePath: string) {
	return controlPlaneApiUrl(
		`/api/workspace/memories?workspace_path=${encodeURIComponent(workspacePath)}`,
	);
}

export function voiceProxyWsUrl(kind: "stt" | "tts"): string {
	return toAbsoluteWsUrl(controlPlaneApiUrl(`/api/voice/${kind}`));
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
		const res = await fetch(controlPlaneApiUrl("/api/opencode/config"), {
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
		const res = await fetch(
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
	const res = await fetch(
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
	const res = await fetch(
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
	const res = await fetch(
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
	const res = await fetch(
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
	const res = await fetch(
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

/** Main Chat session */
export type MainChatSession = {
	id: number;
	session_id: string;
	title?: string;
	started_at: string;
	ended_at?: string;
	message_count: number;
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
	const res = await fetch(controlPlaneApiUrl("/api/main"), {
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
	const res = await fetch(controlPlaneApiUrl("/api/main"), {
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
	const res = await fetch(controlPlaneApiUrl("/api/main"), {
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
	const res = await fetch(controlPlaneApiUrl("/api/main"), {
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
	const res = await fetch(controlPlaneApiUrl("/api/main"), {
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
	const res = await fetch(
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
	const res = await fetch(controlPlaneApiUrl("/api/main/history"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(entry),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** List sessions for an assistant */
export async function listMainChatSessions(
	name: string,
): Promise<MainChatSession[]> {
	const res = await fetch(controlPlaneApiUrl("/api/main/sessions"), {
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
	const res = await fetch(controlPlaneApiUrl("/api/main/sessions"), {
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
	const res = await fetch(controlPlaneApiUrl("/api/main/sessions/latest"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Export history as JSONL */
export async function exportMainChatHistory(name: string): Promise<string> {
	const res = await fetch(controlPlaneApiUrl("/api/main/export"), {
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
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/status"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Start or get Pi session */
export async function startMainChatPiSession(): Promise<PiState> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/session"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get Pi session state */
export async function getMainChatPiState(): Promise<PiState> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/state"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Send a prompt to Pi */
export async function sendMainChatPiPrompt(message: string): Promise<void> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/prompt"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ message }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Abort current Pi operation */
export async function abortMainChatPi(): Promise<void> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/abort"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Get Pi messages */
export async function getMainChatPiMessages(): Promise<PiAgentMessage[]> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/messages"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Compact Pi session */
export async function compactMainChatPi(
	customInstructions?: string,
): Promise<PiCompactionResult> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/compact"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ custom_instructions: customInstructions }),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Start new Pi session (clear history) */
export async function newMainChatPiSession(): Promise<PiState> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/new"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get Pi session stats */
export async function getMainChatPiStats(): Promise<PiSessionStats> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/stats"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Close Pi session */
export async function closeMainChatPiSession(): Promise<void> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/session"), {
		method: "DELETE",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Create WebSocket connection to Pi for streaming events */
export function createMainChatPiWebSocket(): WebSocket {
	const baseUrl = getControlPlaneBaseUrl();
	if (baseUrl) {
		// Direct connection to control plane - no /api prefix needed
		const wsUrl = `${baseUrl.replace(/^http/, "ws")}/main/pi/ws`;
		return new WebSocket(wsUrl);
	}
	// Proxied via frontend dev server - use /api prefix
	const wsUrl = `${window.location.origin.replace(/^http/, "ws")}/api/main/pi/ws`;
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
export async function getMainChatPiHistory(): Promise<MainChatDbMessage[]> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/history"), {
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Clear persistent chat history */
export async function clearMainChatPiHistory(): Promise<{ deleted: number }> {
	const res = await fetch(controlPlaneApiUrl("/api/main/pi/history"), {
		method: "DELETE",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Add a session separator to history (marks new conversation start) */
export async function addMainChatPiSeparator(): Promise<MainChatDbMessage> {
	const res = await fetch(
		controlPlaneApiUrl("/api/main/pi/history/separator"),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

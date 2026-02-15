/**
 * Workspace Config API
 * Read/write agent configuration
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";
import { fileserverProxyBaseUrl } from "./files";

// ============================================================================
// Workspace Config Types
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

/** Full workspace configuration */
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

// ============================================================================
// Workspace Config API
// ============================================================================

/**
 * Read the global agent config.
 * Returns null if the file doesn't exist or can't be parsed.
 */
export async function getGlobalAgentConfig(): Promise<WorkspaceConfig | null> {
	try {
		const res = await authFetch(controlPlaneApiUrl("/api/agent/config"), {
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
 * Read workspace agent config.
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
 * Save workspace agent config.
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

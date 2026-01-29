/**
 * Search API (hstry-backed)
 * Cross-agent session search
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";

// ============================================================================
// Search Types
// ============================================================================

/** Agent filter for search */
export type HstryAgentFilter = "all" | "pi_agent" | "opencode" | string;

/** Search query parameters */
export type HstrySearchQuery = {
	/** Search query string */
	q: string;
	/** Agent filter: "all", "pi_agent", "opencode", or comma-separated */
	agents?: HstryAgentFilter;
	/** Maximum results to return */
	limit?: number;
};

/** A single search hit from hstry */
export type HstrySearchHit = {
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
	/** Full content from hstry */
	content?: string;
	/** Match type */
	match_type?: string;
};

/** Response from hstry search */
export type HstrySearchResponse = {
	hits: HstrySearchHit[];
	total?: number;
	elapsed_ms?: number;
};

// ============================================================================
// Search API
// ============================================================================

/**
 * Search across coding agent sessions using hstry.
 * Searches both Main Chat (pi_agent) and OpenCode sessions.
 */
export async function searchSessions(
	query: HstrySearchQuery,
): Promise<HstrySearchResponse> {
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

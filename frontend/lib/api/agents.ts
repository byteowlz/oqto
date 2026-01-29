/**
 * Agent Ask API
 * Cross-agent communication via @@mentions
 */

import { authFetch, controlPlaneApiUrl } from "./client";

// ============================================================================
// Agent Ask Types
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

/** Exception thrown when multiple sessions match the target */
export class AgentAskAmbiguousException extends Error {
	public matches: AgentAskAmbiguousError["matches"];

	constructor(data: AgentAskAmbiguousError) {
		super(data.error);
		this.name = "AgentAskAmbiguousException";
		this.matches = data.matches;
	}
}

// ============================================================================
// Agent Ask API
// ============================================================================

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

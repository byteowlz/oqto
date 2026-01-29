/**
 * Workspace Sessions API
 * Session CRUD, lifecycle management
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";
import type {
	CreateWorkspaceSessionRequest,
	SessionUpdateInfo,
	WorkspaceSession,
	WorkspaceSessionStatus,
} from "./types";

/** Normalize session status to lowercase (backend may return e.g. "Running" instead of "running") */
function normalizeSession(session: WorkspaceSession): WorkspaceSession {
	return {
		...session,
		status: session.status.toLowerCase() as WorkspaceSessionStatus,
	};
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
		if (current?.status === "failed") {
			const errorMsg = current.error_message || current.status;
			throw new Error(`Session failed to restart: ${errorMsg}`);
		}
		await new Promise((resolve) => setTimeout(resolve, 500));
	}

	// Return what we have even if not fully running yet
	return session;
}

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

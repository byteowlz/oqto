/**
 * File and Proxy URLs
 * URL builders for file operations, proxies, terminals, etc.
 */

import { toAbsoluteWsUrl } from "@/lib/url";
import { controlPlaneApiUrl, getAuthToken } from "./client";

// ============================================================================
// Proxy URLs
// ============================================================================

export function opencodeProxyBaseUrl(sessionId: string) {
	return controlPlaneApiUrl(`/api/session/${sessionId}/code`);
}

export function terminalProxyPath(sessionId: string) {
	return `/api/session/${sessionId}/term`;
}

export function fileserverProxyBaseUrl(sessionId: string) {
	return controlPlaneApiUrl(`/api/session/${sessionId}/files`);
}

export function fileserverWorkspaceBaseUrl() {
	return controlPlaneApiUrl("/api/workspace/files");
}

export function defaultChatFilesBaseUrl() {
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
	return `/api/workspace/term?workspace_path=${encodeURIComponent(workspacePath)}`;
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

/**
 * Base API client infrastructure
 * Token management, authFetch, URL helpers, error handling
 */

import { isTauri } from "@/lib/tauri-fetch-polyfill";
import { destroyWsManager } from "@/lib/ws-manager";

// ============================================================================
// Token Storage (for Tauri/mobile where cookies don't work)
// ============================================================================

const AUTH_TOKEN_KEY = "oqto:authToken";
const CURRENT_USER_KEY = "oqto:currentUserId";

// Keys that must NOT be cleared on user switch (identity + connection config).
const PRESERVED_KEYS = new Set([AUTH_TOKEN_KEY, CURRENT_USER_KEY]);

/**
 * Clear all user-specific localStorage data.
 * Called on login (when user identity changes) and logout to prevent
 * one user's cached sessions/state from leaking to another user.
 *
 * SECURITY: Uses a deny-by-default approach -- clears ALL localStorage keys
 * except the auth token and user identity. This prevents leaks when new
 * per-user keys are added without updating a whitelist.
 */
export function clearUserSessionData(): void {
	if (typeof window === "undefined") return;
	const keysToRemove: string[] = [];
	for (let i = 0; i < localStorage.length; i++) {
		const key = localStorage.key(i);
		if (key && !PRESERVED_KEYS.has(key)) {
			keysToRemove.push(key);
		}
	}
	for (const key of keysToRemove) {
		localStorage.removeItem(key);
	}
}

/**
 * Track which user is logged in so we can detect user switches and
 * clear stale data that belonged to the previous user.
 */
export function setCurrentUserId(userId: string | null): void {
	if (typeof window === "undefined") return;
	if (userId) {
		const previous = localStorage.getItem(CURRENT_USER_KEY);
		if (previous && previous !== userId) {
			// User switched -- nuke all cached data from previous user
			clearUserSessionData();
			// CRITICAL: Destroy the WebSocket connection so it reconnects
			// with the new user's auth token. Without this, the old WS
			// remains authenticated as the previous user and all commands
			// (session.create, prompt, etc.) go to the wrong runner.
			destroyWsManager();
		}
		localStorage.setItem(CURRENT_USER_KEY, userId);
	} else {
		localStorage.removeItem(CURRENT_USER_KEY);
	}
}

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
		// Clear user identity + cached data on logout
		clearUserSessionData();
		localStorage.removeItem(CURRENT_USER_KEY);
		// Destroy WebSocket so it doesn't keep sending with old credentials
		destroyWsManager();
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
export async function authFetch(
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
// URL Helpers
// ============================================================================

type ApiErrorResponse = {
	error?: string;
};

const trimTrailingSlash = (value: string) => value.replace(/\/$/, "");
const controlPlaneStorageKey = "oqto:controlPlaneUrl";

const env =
	(import.meta as ImportMeta & { env?: Record<string, string | undefined> })
		.env ?? (typeof process !== "undefined" ? process.env : {});

function normalizeControlPlaneUrl(value: string | null | undefined): string {
	if (!value) return "";
	const trimmed = trimTrailingSlash(value.trim());
	return trimmed.replace(/\/api$/, "");
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
	if (base) return `${base}${normalizedPath}`;
	if (normalizedPath.startsWith("/api")) return normalizedPath;
	return `/api${normalizedPath}`;
}

export async function readApiError(res: Response): Promise<string> {
	const contentType = res.headers.get("content-type") ?? "";
	if (contentType.includes("application/json")) {
		const parsed = (await res
			.json()
			.catch(() => null)) as ApiErrorResponse | null;
		if (parsed?.error) return parsed.error;
	}
	return (await res.text().catch(() => res.statusText)) || res.statusText;
}

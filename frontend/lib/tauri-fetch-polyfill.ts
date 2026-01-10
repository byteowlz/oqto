/**
 * Tauri Fetch Polyfill + Global Auth Interceptor
 *
 * This module does two things:
 * 1. When running in Tauri, replaces window.fetch with native HTTP via reqwest
 *    (bypasses iOS WebView restrictions)
 * 2. Automatically injects Authorization headers for all control plane requests
 *    (works in both Tauri and browser)
 *
 * Import this once at app startup (e.g., in main.tsx) BEFORE any fetch calls.
 *
 * Usage:
 *   import "@/lib/tauri-fetch-polyfill";
 */

import { invoke } from "@tauri-apps/api/core";

interface TauriHttpResponse {
	status: number;
	data: unknown;
	ok: boolean;
}

/**
 * Check if running in Tauri environment
 */
function isTauri(): boolean {
	return typeof window !== "undefined" && "__TAURI__" in window;
}

/**
 * Convert Headers object to plain Record
 */
function headersToRecord(
	headers?: HeadersInit,
): Record<string, string> | undefined {
	if (!headers) return undefined;

	if (headers instanceof Headers) {
		const record: Record<string, string> = {};
		headers.forEach((value, key) => {
			record[key] = value;
		});
		return record;
	}

	if (Array.isArray(headers)) {
		const record: Record<string, string> = {};
		for (const [key, value] of headers) {
			record[key] = value;
		}
		return record;
	}

	return headers as Record<string, string>;
}

/**
 * Parse body from RequestInit
 */
async function parseBody(body: BodyInit | null | undefined): Promise<unknown> {
	if (!body) return null;

	if (typeof body === "string") {
		try {
			return JSON.parse(body);
		} catch {
			return body;
		}
	}

	if (body instanceof FormData) {
		const obj: Record<string, unknown> = {};
		body.forEach((value, key) => {
			obj[key] = value;
		});
		return obj;
	}

	if (body instanceof URLSearchParams) {
		const obj: Record<string, string> = {};
		body.forEach((value, key) => {
			obj[key] = value;
		});
		return obj;
	}

	if (body instanceof Blob) {
		const text = await body.text();
		try {
			return JSON.parse(text);
		} catch {
			return text;
		}
	}

	return body;
}

/**
 * Create a Response-like object from Tauri HTTP response
 */
function createResponse(tauriResponse: TauriHttpResponse): Response {
	const body =
		typeof tauriResponse.data === "string"
			? tauriResponse.data
			: JSON.stringify(tauriResponse.data);

	return new Response(body, {
		status: tauriResponse.status,
		statusText: tauriResponse.ok ? "OK" : "Error",
		headers: {
			"Content-Type": "application/json",
		},
	});
}

// Store original fetch before we potentially override it
const originalFetch =
	typeof window !== "undefined" ? window.fetch.bind(window) : fetch;

// ============================================================================
// Auth Token Management (duplicated here to avoid circular imports)
// ============================================================================

const AUTH_TOKEN_KEY = "octo:authToken";
const CONTROL_PLANE_STORAGE_KEY = "octo:controlPlaneUrl";

function getAuthToken(): string | null {
	if (typeof window === "undefined") return null;
	return localStorage.getItem(AUTH_TOKEN_KEY);
}

function getControlPlaneBaseUrl(): string {
	if (typeof window === "undefined") return "";
	try {
		const stored = window.localStorage.getItem(CONTROL_PLANE_STORAGE_KEY);
		return stored?.trim().replace(/\/$/, "") ?? "";
	} catch {
		return "";
	}
}

/**
 * Check if a URL should receive auth headers.
 * Returns true for control plane URLs (both direct and proxied).
 */
function shouldAddAuth(url: string): boolean {
	// Skip auth endpoints - they're used to GET the token
	if (url.includes("/auth/login") || url.includes("/auth/register")) {
		return false;
	}

	const controlPlaneBase = getControlPlaneBaseUrl();

	// Direct control plane URL (e.g., http://archlinux:8080/...)
	if (controlPlaneBase && url.startsWith(controlPlaneBase)) {
		return true;
	}

	// Proxied via dev server (e.g., /api/... on same origin)
	if (url.startsWith("/api/") || url.includes("/api/")) {
		return true;
	}

	// Workspace file operations (proxied)
	if (url.includes("/workspace/")) {
		return true;
	}

	// Session-specific proxied endpoints
	if (url.includes("/session/")) {
		return true;
	}

	return false;
}

/**
 * Add auth headers to request init
 */
function addAuthHeaders(init?: RequestInit): RequestInit {
	const token = getAuthToken();
	if (!token) return init ?? {};

	const existingHeaders = init?.headers;
	let headers: Record<string, string> = {};

	if (existingHeaders instanceof Headers) {
		existingHeaders.forEach((value, key) => {
			headers[key] = value;
		});
	} else if (Array.isArray(existingHeaders)) {
		for (const [key, value] of existingHeaders) {
			headers[key] = value;
		}
	} else if (existingHeaders) {
		headers = { ...existingHeaders };
	}

	// Don't override if already set
	if (!headers.Authorization && !headers.authorization) {
		headers.Authorization = `Bearer ${token}`;
	}

	return {
		...init,
		headers,
	};
}

/**
 * Tauri-native fetch implementation
 */
async function tauriFetch(
	input: RequestInfo | URL,
	init?: RequestInit,
): Promise<Response> {
	const url =
		typeof input === "string"
			? input
			: input instanceof URL
				? input.toString()
				: input.url;

	// Add auth headers if needed
	const authInit = shouldAddAuth(url) ? addAuthHeaders(init) : init;

	const method = authInit?.method?.toUpperCase() || "GET";
	const headers = headersToRecord(authInit?.headers);

	// Add Content-Type for JSON bodies if not present
	const finalHeaders = { ...headers };
	if (
		init?.body &&
		!finalHeaders["Content-Type"] &&
		!finalHeaders["content-type"]
	) {
		finalHeaders["Content-Type"] = "application/json";
	}

	console.log("[tauri-fetch]", method, url);

	try {
		let response: TauriHttpResponse;

		switch (method) {
			case "GET":
				response = await invoke<TauriHttpResponse>("http_get", {
					url,
					headers: Object.keys(finalHeaders).length > 0 ? finalHeaders : null,
				});
				console.log("[tauri-fetch] Response:", response.status, response.ok);
				break;

			case "POST": {
				const body = await parseBody(authInit?.body);
				response = await invoke<TauriHttpResponse>("http_post", {
					url,
					body: body ?? {},
					headers: Object.keys(finalHeaders).length > 0 ? finalHeaders : null,
				});
				break;
			}

			case "PUT": {
				const body = await parseBody(authInit?.body);
				response = await invoke<TauriHttpResponse>("http_put", {
					url,
					body: body ?? {},
					headers: Object.keys(finalHeaders).length > 0 ? finalHeaders : null,
				});
				break;
			}

			case "PATCH": {
				const body = await parseBody(authInit?.body);
				response = await invoke<TauriHttpResponse>("http_patch", {
					url,
					body: body ?? {},
					headers: Object.keys(finalHeaders).length > 0 ? finalHeaders : null,
				});
				break;
			}

			case "DELETE":
				response = await invoke<TauriHttpResponse>("http_delete", {
					url,
					headers: Object.keys(finalHeaders).length > 0 ? finalHeaders : null,
				});
				break;

			default:
				// Fall back to native fetch for unsupported methods
				console.warn("[tauri-fetch] Unsupported method, falling back:", method);
				return originalFetch(input, authInit);
		}

		return createResponse(response);
	} catch (error) {
		console.error("[tauri-fetch] Request failed:", method, url, error);
		// Re-throw as TypeError to match fetch API behavior
		throw new TypeError(`Network request failed: ${String(error)}`);
	}
}

/**
 * Browser fetch with auth interceptor (for non-Tauri environments)
 */
async function browserFetchWithAuth(
	input: RequestInfo | URL,
	init?: RequestInit,
): Promise<Response> {
	const url =
		typeof input === "string"
			? input
			: input instanceof URL
				? input.toString()
				: input.url;

	// Add auth headers if needed
	const authInit = shouldAddAuth(url) ? addAuthHeaders(init) : init;

	return originalFetch(input, authInit);
}

/**
 * Install the fetch interceptor.
 * - In Tauri: uses native HTTP via reqwest + adds auth
 * - In browser: uses native fetch + adds auth
 */
export function installTauriFetchPolyfill(): void {
	if (isTauri()) {
		console.log(
			"[tauri-fetch] Installing Tauri fetch polyfill for native HTTP",
		);
		window.fetch = tauriFetch;
	} else {
		console.log("[tauri-fetch] Installing auth interceptor for browser fetch");
		window.fetch = browserFetchWithAuth;
	}
}

/**
 * Restore original fetch (useful for testing)
 */
export function restoreFetch(): void {
	window.fetch = originalFetch;
}

// Auto-install on import
installTauriFetchPolyfill();

// Also export for manual use
export { tauriFetch, originalFetch, isTauri };

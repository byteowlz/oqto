/**
 * Tauri Fetch Polyfill + Global Auth Interceptor
 *
 * Automatically injects Authorization headers for all control plane requests.
 * Works in both Tauri and browser environments.
 *
 * Import this once at app startup (e.g., in main.tsx) BEFORE any fetch calls.
 */

// Store original fetch before we potentially override it
const originalFetch =
	typeof window !== "undefined" ? window.fetch.bind(window) : fetch;

// ============================================================================
// Auth Token Management (duplicated here to avoid circular imports)
// ============================================================================

const AUTH_TOKEN_KEY = "oqto:authToken";
const CONTROL_PLANE_STORAGE_KEY = "oqto:controlPlaneUrl";

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
 */
function shouldAddAuth(url: string): boolean {
	if (url.includes("/auth/login") || url.includes("/auth/register")) {
		return false;
	}

	const controlPlaneBase = getControlPlaneBaseUrl();

	if (controlPlaneBase && url.startsWith(controlPlaneBase)) {
		return true;
	}

	if (url.startsWith("/api/") || url.includes("/api/")) {
		return true;
	}

	if (url.includes("/workspace/") || url.includes("/session/")) {
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

	if (!headers.Authorization && !headers.authorization) {
		headers.Authorization = `Bearer ${token}`;
	}

	return { ...init, headers };
}

/**
 * Fetch with auth interceptor
 */
async function fetchWithAuth(
	input: RequestInfo | URL,
	init?: RequestInit,
): Promise<Response> {
	const url =
		typeof input === "string"
			? input
			: input instanceof URL
				? input.toString()
				: input.url;

	const authInit = shouldAddAuth(url) ? addAuthHeaders(init) : init;
	return originalFetch(input, authInit);
}

/**
 * Check if running in Tauri environment
 */
export function isTauri(): boolean {
	if (typeof window === "undefined") return false;
	if ("__TAURI__" in window) return true;
	if ("__TAURI_INTERNALS__" in window) return true;
	return false;
}

/**
 * Install the fetch interceptor
 */
export function installTauriFetchPolyfill(): void {
	window.fetch = fetchWithAuth;
}

/**
 * Restore original fetch
 */
export function restoreFetch(): void {
	window.fetch = originalFetch;
}

// Auto-install on import
installTauriFetchPolyfill();

export { originalFetch };

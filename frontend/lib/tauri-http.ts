/**
 * Native HTTP client using Tauri commands with reqwest.
 * Bypasses WebView restrictions and works reliably on iOS.
 */
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";

interface HttpResponse<T = unknown> {
	status: number;
	data: T;
	ok: boolean;
}

interface DiscoveredServer {
	host: string;
	port: number;
	name: string;
	version?: string;
	response_time_ms: number;
}

type Headers = Record<string, string>;

/**
 * Check if running in Tauri environment
 */
export function isTauri(): boolean {
	return typeof window !== "undefined" && "__TAURI__" in window;
}

/**
 * Perform a GET request via native Tauri HTTP
 */
export async function httpGet<T = unknown>(
	url: string,
	headers?: Headers,
): Promise<HttpResponse<T>> {
	if (!isTauri()) {
		// Fallback to fetch for non-Tauri environments
		const response = await fetch(url, { headers });
		const data = await response.json();
		return { status: response.status, data, ok: response.ok };
	}
	return invoke<HttpResponse<T>>("http_get", { url, headers });
}

/**
 * Perform a POST request via native Tauri HTTP
 */
export async function httpPost<T = unknown>(
	url: string,
	body: unknown,
	headers?: Headers,
): Promise<HttpResponse<T>> {
	if (!isTauri()) {
		const response = await fetch(url, {
			method: "POST",
			headers: { "Content-Type": "application/json", ...headers },
			body: JSON.stringify(body),
		});
		const data = await response.json();
		return { status: response.status, data, ok: response.ok };
	}
	return invoke<HttpResponse<T>>("http_post", { url, body, headers });
}

/**
 * Perform a PUT request via native Tauri HTTP
 */
export async function httpPut<T = unknown>(
	url: string,
	body: unknown,
	headers?: Headers,
): Promise<HttpResponse<T>> {
	if (!isTauri()) {
		const response = await fetch(url, {
			method: "PUT",
			headers: { "Content-Type": "application/json", ...headers },
			body: JSON.stringify(body),
		});
		const data = await response.json();
		return { status: response.status, data, ok: response.ok };
	}
	return invoke<HttpResponse<T>>("http_put", { url, body, headers });
}

/**
 * Perform a PATCH request via native Tauri HTTP
 */
export async function httpPatch<T = unknown>(
	url: string,
	body: unknown,
	headers?: Headers,
): Promise<HttpResponse<T>> {
	if (!isTauri()) {
		const response = await fetch(url, {
			method: "PATCH",
			headers: { "Content-Type": "application/json", ...headers },
			body: JSON.stringify(body),
		});
		const data = await response.json();
		return { status: response.status, data, ok: response.ok };
	}
	return invoke<HttpResponse<T>>("http_patch", { url, body, headers });
}

/**
 * Perform a DELETE request via native Tauri HTTP
 */
export async function httpDelete<T = unknown>(
	url: string,
	headers?: Headers,
): Promise<HttpResponse<T>> {
	if (!isTauri()) {
		const response = await fetch(url, { method: "DELETE", headers });
		const text = await response.text();
		const data = text ? JSON.parse(text) : {};
		return { status: response.status, data, ok: response.ok };
	}
	return invoke<HttpResponse<T>>("http_delete", { url, headers });
}

/**
 * Discover agent servers on the local network
 */
export async function discoverServers(): Promise<DiscoveredServer[]> {
	if (!isTauri()) {
		console.warn("Server discovery only available in Tauri");
		return [];
	}
	return invoke<DiscoveredServer[]>("discover_servers");
}

/**
 * HTTP client object with all methods
 */
export const tauriHttp = {
	get: httpGet,
	post: httpPost,
	put: httpPut,
	patch: httpPatch,
	delete: httpDelete,
	discoverServers,
	isTauri,
};

export default tauriHttp;

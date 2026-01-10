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
 * Discover OpenCode servers on the local network
 */
export async function discoverServers(): Promise<DiscoveredServer[]> {
	if (!isTauri()) {
		console.warn("Server discovery only available in Tauri");
		return [];
	}
	return invoke<DiscoveredServer[]>("discover_servers");
}

/**
 * Start listening to an SSE stream via native Tauri HTTP
 * Returns an unlisten function to stop the stream
 */
export async function startSseStream(
	url: string,
	eventName: string,
	onMessage: (data: string) => void,
	onError?: (error: string) => void,
): Promise<UnlistenFn> {
	if (!isTauri()) {
		// Fallback to EventSource for non-Tauri environments
		const eventSource = new EventSource(url);
		eventSource.onmessage = (event) => onMessage(event.data);
		eventSource.onerror = () => onError?.("EventSource error");
		return () => eventSource.close();
	}

	const unlistenMessage = await listen<string>(eventName, (event) => {
		onMessage(event.payload);
	});

	const unlistenError = await listen<string>(`${eventName}-error`, (event) => {
		onError?.(event.payload);
	});

	// Start the stream
	await invoke("start_sse_stream", { url, eventName });

	return () => {
		unlistenMessage();
		unlistenError();
	};
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
	startSseStream,
	isTauri,
};

export default tauriHttp;

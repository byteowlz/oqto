/**
 * The multiplexed WebSocket endpoint, with the stored auth token attached.
 * Browser APIs live in platform adapters by design.
 */

const AUTH_TOKEN_KEY = "oqto:authToken";

function controlPlaneApiUrl(path: string): string {
	const base = import.meta.env.VITE_API_URL ?? "";
	const normalizedPath = path.startsWith("/") ? path : `/${path}`;
	if (base) return `${base}${normalizedPath}`;
	if (normalizedPath.startsWith("/api")) return normalizedPath;
	return `/api${normalizedPath}`;
}

export function muxWebSocketUrl(): string {
	const url = new URL(
		controlPlaneApiUrl("/api/ws/mux"),
		window.location.origin,
	);
	url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
	let token: string | null = null;
	try {
		token = window.localStorage.getItem(AUTH_TOKEN_KEY);
	} catch {
		token = null;
	}
	if (token) url.searchParams.set("token", token);
	return url.toString();
}

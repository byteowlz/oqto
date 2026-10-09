/**
 * The real chat transport binding: mux WebSocket, persisted outbox, and
 * platform timers. This is the platform adapter — browser APIs live here
 * and nowhere else in the engine.
 */

import { type ChatTransport, createChatTransport } from "../engine/transport";
import type {
	OutboxEntry,
	Scheduler,
	SocketLike,
} from "../engine/transport-contract";

// Minimal local copies of the control-plane URL/token helpers: importing
// lib/api/client would couple the engine adapter to a file the legacy
// refactor is actively churning. Keys mirror lib/api/client.ts exactly.
const AUTH_TOKEN_KEY = "oqto:authToken";
const CONTROL_PLANE_URL_KEY = "oqto:controlPlaneUrl";

function controlPlaneApiUrl(path: string): string {
	let base = "";
	try {
		base = (window.localStorage.getItem(CONTROL_PLANE_URL_KEY) ?? "")
			.trim()
			.replace(/\/$/, "");
	} catch {
		base = "";
	}
	const normalizedPath = path.startsWith("/") ? path : `/${path}`;
	if (base) return `${base}${normalizedPath}`;
	if (normalizedPath.startsWith("/api")) return normalizedPath;
	return `/api${normalizedPath}`;
}

function authToken(): string | null {
	try {
		return window.localStorage.getItem(AUTH_TOKEN_KEY);
	} catch {
		return null;
	}
}

function wsUrl(): string {
	const url = new URL(
		controlPlaneApiUrl("/api/ws/mux"),
		window.location.origin,
	);
	url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
	const token = authToken();
	if (token) url.searchParams.set("token", token);
	return url.toString();
}

function bindSocket(ws: WebSocket): SocketLike {
	const socket: SocketLike = {
		send: (data) => ws.send(data),
		close: (code, reason) => ws.close(code, reason),
		onOpen: null,
		onMessage: null,
		onClose: null,
	};
	ws.onopen = () => socket.onOpen?.();
	ws.onmessage = (message) => socket.onMessage?.(message.data);
	ws.onclose = () => socket.onClose?.();
	return socket;
}

const scheduler: Scheduler = {
	schedule: (callback, delayMs) => setTimeout(callback, delayMs),
	cancel: (handle) => clearTimeout(handle),
	now: () => Date.now(),
};

const outboxKey = "oqto:chatOutbox:v1";

const outbox = {
	load(): OutboxEntry[] {
		try {
			const raw = window.localStorage.getItem(outboxKey);
			return raw ? (JSON.parse(raw) as OutboxEntry[]) : [];
		} catch {
			return [];
		}
	},
	save(entries: OutboxEntry[]): void {
		try {
			window.localStorage.setItem(outboxKey, JSON.stringify(entries));
		} catch {
			// Storage unavailable: acks degrade to best-effort retries.
		}
	},
};

export const liveChatTransport: ChatTransport = createChatTransport({
	sockets: () => bindSocket(new WebSocket(wsUrl())),
	outbox,
	clock: scheduler,
});

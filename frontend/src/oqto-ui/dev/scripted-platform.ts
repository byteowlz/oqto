import { createSessionEngine } from "../engine/session-engine";
import { createChatTransport } from "../engine/transport";
import type { ChatMessage, MessagePage } from "../platform/contracts";
import type {
	OqtoUiConfigResolution,
	OqtoUiPlatform,
	OqtoUiSnapshot,
} from "../platform/contracts";
import { scriptedFixture, scriptedTimeline } from "./fixture";
import { ScriptedChatServer } from "./scripted-chat-server";

const SCRIPTED_PAGE_SIZE = 40;

/**
 * Durable store the scripted "backend" persists into. Starts as the fixture
 * timeline; prompts and completed turns append here, so page refetches after
 * turn end converge exactly like the real store would.
 */
export const scriptedStore = new Map<string, ChatMessage[]>([
	["frontend-rebuild", [...scriptedTimeline]],
]);

class FakeSocket {
	sent: string[] = [];
	serverTap: ((data: string) => void) | null = null;
	onOpen: (() => void) | null = null;
	onMessage: ((data: string) => void) | null = null;
	onClose: (() => void) | null = null;

	send(data: string): void {
		this.sent.push(data);
		this.serverTap?.(data);
	}
	close(): void {
		this.onClose?.();
	}
	drop(): void {
		this.onClose?.();
	}
	open(): void {
		this.onOpen?.();
	}
	receive(event: unknown): void {
		this.onMessage?.(JSON.stringify(event));
	}
}

const scriptedSockets: FakeSocket[] = [];
let activeSocket: FakeSocket | null = null;

const server = new ScriptedChatServer((event) => activeSocket?.receive(event), {
	onPersisted: (sessionId, message) => {
		const store = scriptedStore.get(sessionId) ?? [];
		store.push(message);
		scriptedStore.set(sessionId, store);
	},
	onTurnCompleted: (sessionId, message) => {
		const store = scriptedStore.get(sessionId) ?? [];
		store.push(message);
		scriptedStore.set(sessionId, store);
	},
});

const scriptedTransport = createChatTransport({
	sockets: () => {
		const socket = new FakeSocket();
		socket.serverTap = (data) => server.handleMessage(data);
		activeSocket = socket;
		scriptedSockets.push(socket);
		// Real sockets open asynchronously; the fake does the same on a tick.
		setTimeout(() => socket.open(), 0);
		return socket;
	},
	outbox: { load: () => [], save: () => {} },
	clock: {
		schedule: (callback, delayMs) => setTimeout(callback, delayMs),
		cancel: (handle) => clearTimeout(handle),
		now: () => Date.now(),
	},
});

export const scriptedChat = createSessionEngine(scriptedTransport);

/** Test hook: simulates the transport dying mid-turn (e.g. tab sleep). */
export function dropActiveScriptedSocket(): void {
	scriptedSockets.at(-1)?.drop();
}

const scriptedConfig: OqtoUiConfigResolution = {
	source: "user-lua",
	diagnostics: [],
	config: {
		version: 1,
		preset: "corner-v4",
		appearance: {
			scheme: "oqto-dark",
			radius: "square",
			density: "compact",
			font: "mono",
		},
		layout: { files: "right", navigator: "left" },
		bindings: [
			{ keys: "ctrl+shift+f", action: "view.openFiles" },
			{ keys: "ctrl+shift+c", action: "view.openChat" },
		],
		status_line: {
			segments: ["session", "model", "context", "connection", "runnerload"],
		},
		mobile: {
			mode: "corner",
			hold_ms: 320,
			corners: {
				top_left: { tap: "navigator.open", hold: "menu.projects" },
				top_right: { tap: "view.openFiles", hold: "menu.tools" },
				bottom_left: {
					tap: "session.openPrevious",
					hold: "menu.sessionMru",
				},
				bottom_right: { tap: "chat.send", hold: "menu.chatActions" },
			},
		},
	},
};

export const scriptedOqtoUiPlatform: OqtoUiPlatform = {
	id: "scripted",
	chat: scriptedChat,
	async loadUiConfig(): Promise<OqtoUiConfigResolution> {
		return scriptedConfig;
	},
	async load(sessionId): Promise<OqtoUiSnapshot> {
		const known = scriptedFixture.workDirectories.some((directory) =>
			directory.sessions.some((session) => session.id === sessionId),
		);
		const activeSessionId = known
			? sessionId
			: (scriptedFixture.workDirectories[0]?.sessions[0]?.id ?? null);
		return { ...scriptedFixture, activeSessionId };
	},

	async loadMessages(sessionId, before, limit): Promise<MessagePage> {
		const size = Math.min(Math.max(limit ?? SCRIPTED_PAGE_SIZE, 1), 200);
		const known = scriptedFixture.workDirectories.some((directory) =>
			directory.sessions.some((session) => session.id === sessionId),
		);
		if (!known) {
			return { sessionId, messages: [], hasMore: false, nextBefore: null };
		}
		const timeline = scriptedStore.get(sessionId) ?? [];
		// `before` is the id of the oldest message of the previous page; the
		// next page contains only messages strictly older than it.
		const endExclusive = before
			? timeline.findIndex((message) => message.id === before)
			: timeline.length;
		if (endExclusive <= 0) {
			return { sessionId, messages: [], hasMore: false, nextBefore: null };
		}
		const start = Math.max(0, endExclusive - size);
		const messages = timeline.slice(start, endExclusive);
		return {
			sessionId,
			messages,
			hasMore: start > 0,
			nextBefore: start > 0 ? (messages[0]?.id ?? null) : null,
		};
	},
};

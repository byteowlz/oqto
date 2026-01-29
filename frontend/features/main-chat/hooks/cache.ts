/**
 * Cache utilities for Pi chat hooks.
 * Handles message caching, scroll position persistence, and WebSocket state.
 */

import type {
	PiDisplayMessage,
	ScrollCache,
	SessionMessageCache,
	SessionMessageCacheEntry,
	WsConnectionState,
} from "./types";

// Constants
export const CACHE_WRITE_THROTTLE_MS = 2000; // Write to localStorage at most every 2s during streaming
export const SESSION_CACHE_VERSION = 2;
const DEFAULT_SCROLL_STORAGE_KEY = "octo:mainChat:scrollPosition";

// Global WebSocket connection cache - survives component remounts
export const wsCache: WsConnectionState = {
	ws: null,
	isConnected: false,
	sessionStarted: false,
	listeners: new Set(),
};

// Global session message cache
export const sessionMessageCache: SessionMessageCache = {
	messagesBySession: new Map<string, SessionMessageCacheEntry>(),
	initialized: false,
	lastWriteTime: new Map<string, number>(),
	pendingWrite: new Map<string, ReturnType<typeof setTimeout>>(),
};

// Global scroll position cache
export const scrollCache: ScrollCache = {
	positions: new Map<string, number | null>(),
	initialized: new Set<string>(),
};

/** Sanitize a string for use in storage keys */
export function sanitizeStorageKey(value: string): string {
	return value.replace(/[^a-zA-Z0-9._-]+/g, "_");
}

/** Check if a session ID is a pending/optimistic placeholder */
export function isPendingSessionId(id: string | null | undefined): boolean {
	return !!id && id.startsWith("pending-");
}

/** Get the cache entry key for a session */
function cacheEntryKey(sessionId: string, storageKeyPrefix: string) {
	return `${storageKeyPrefix}:${sessionId}`;
}

/** Get the localStorage key for session messages */
function cacheKeyMessages(sessionId: string, storageKeyPrefix: string) {
	return `${storageKeyPrefix}:session:${sessionId}:messages:v${SESSION_CACHE_VERSION}`;
}

/** Read cached session messages from memory or localStorage */
export function readCachedSessionMessages(
	sessionId: string,
	storageKeyPrefix: string,
): PiDisplayMessage[] {
	const cacheKey = cacheEntryKey(sessionId, storageKeyPrefix);
	const inMemory = sessionMessageCache.messagesBySession.get(cacheKey);
	if (inMemory) {
		if (inMemory.version !== SESSION_CACHE_VERSION) {
			sessionMessageCache.messagesBySession.delete(cacheKey);
		} else {
			// Strip isStreaming from cached messages - it's transient state
			return inMemory.messages.map((m) => {
				if (m.isStreaming) {
					const { isStreaming: _, ...rest } = m;
					return rest;
				}
				return m;
			});
		}
	}
	if (typeof window === "undefined") return [];
	try {
		const raw = localStorage.getItem(
			cacheKeyMessages(sessionId, storageKeyPrefix),
		);
		if (!raw) return [];
		const parsed = JSON.parse(raw) as SessionMessageCacheEntry;
		if (!parsed || !Array.isArray(parsed.messages)) return [];
		if (parsed.version !== SESSION_CACHE_VERSION) return [];
		// Strip isStreaming from cached messages - it's transient state
		const cleanedMessages = parsed.messages.map((m) => {
			if (m.isStreaming) {
				const { isStreaming: _, ...rest } = m;
				return rest;
			}
			return m;
		});
		const cleanedEntry = {
			messages: cleanedMessages,
			timestamp: parsed.timestamp,
			version: SESSION_CACHE_VERSION,
		};
		sessionMessageCache.messagesBySession.set(cacheKey, cleanedEntry);
		return cleanedMessages;
	} catch {
		return [];
	}
}

/** Write session messages to cache (throttled during streaming) */
export function writeCachedSessionMessages(
	sessionId: string,
	messages: PiDisplayMessage[],
	storageKeyPrefix: string,
	forceWrite = false,
) {
	const cacheKey = cacheEntryKey(sessionId, storageKeyPrefix);
	// Strip isStreaming flag when caching - it's transient state that shouldn't persist
	const cleanedMessages = messages.map((m) => {
		if (m.isStreaming) {
			const { isStreaming: _, ...rest } = m;
			return rest;
		}
		return m;
	});
	const entry: SessionMessageCacheEntry = {
		messages: cleanedMessages,
		timestamp: Date.now(),
		version: SESSION_CACHE_VERSION,
	};
	// Always update in-memory cache immediately
	sessionMessageCache.messagesBySession.set(cacheKey, entry);
	if (typeof window === "undefined") return;

	// Throttle localStorage writes to reduce I/O during streaming
	const now = Date.now();
	const lastWrite = sessionMessageCache.lastWriteTime.get(cacheKey) ?? 0;
	const elapsed = now - lastWrite;

	// Clear any pending write for this session
	const pending = sessionMessageCache.pendingWrite.get(cacheKey);
	if (pending) {
		clearTimeout(pending);
		sessionMessageCache.pendingWrite.delete(cacheKey);
	}

	const doWrite = () => {
		sessionMessageCache.lastWriteTime.set(cacheKey, Date.now());
		queueMicrotask(() => {
			try {
				localStorage.setItem(
					cacheKeyMessages(sessionId, storageKeyPrefix),
					JSON.stringify(entry),
				);
			} catch {
				// ignore
			}
		});
	};

	if (forceWrite || elapsed >= CACHE_WRITE_THROTTLE_MS) {
		// Write immediately
		doWrite();
	} else {
		// Schedule write after throttle interval
		const delay = CACHE_WRITE_THROTTLE_MS - elapsed;
		const timer = setTimeout(doWrite, delay);
		sessionMessageCache.pendingWrite.set(cacheKey, timer);
	}
}

/** Clear cached session messages */
export function clearCachedSessionMessages(
	sessionId: string,
	storageKeyPrefix: string,
) {
	const cacheKey = cacheEntryKey(sessionId, storageKeyPrefix);
	sessionMessageCache.messagesBySession.delete(cacheKey);
	const pending = sessionMessageCache.pendingWrite.get(cacheKey);
	if (pending) {
		clearTimeout(pending);
		sessionMessageCache.pendingWrite.delete(cacheKey);
	}
	sessionMessageCache.lastWriteTime.delete(cacheKey);
	if (typeof window === "undefined") return;
	try {
		localStorage.removeItem(cacheKeyMessages(sessionId, storageKeyPrefix));
	} catch {
		// Ignore storage errors.
	}
}

/** Initialize scroll cache for a storage key */
function initScrollCache(storageKey: string) {
	if (
		scrollCache.initialized.has(storageKey) ||
		typeof window === "undefined"
	) {
		return;
	}
	scrollCache.initialized.add(storageKey);
	try {
		const stored = localStorage.getItem(storageKey);
		if (stored !== null) {
			scrollCache.positions.set(storageKey, Number.parseInt(stored, 10));
		}
	} catch {
		// ignore
	}
}

/** Get cached scroll position (null = bottom) */
export function getCachedScrollPosition(
	storageKey: string = DEFAULT_SCROLL_STORAGE_KEY,
): number | null {
	initScrollCache(storageKey);
	return scrollCache.positions.get(storageKey) ?? null;
}

/** Save scroll position to cache */
export function setCachedScrollPosition(
	position: number | null,
	storageKey: string = DEFAULT_SCROLL_STORAGE_KEY,
) {
	scrollCache.positions.set(storageKey, position);

	// Persist asynchronously
	queueMicrotask(() => {
		try {
			if (position === null) {
				localStorage.removeItem(storageKey);
			} else {
				localStorage.setItem(storageKey, String(position));
			}
		} catch {
			// Ignore
		}
	});
}

/** Subscribe to WebSocket connection state changes */
export function subscribeToConnectionState(
	listener: (connected: boolean) => void,
) {
	wsCache.listeners.add(listener);
	return () => {
		wsCache.listeners.delete(listener);
	};
}

/** Notify all listeners of connection state change */
export function notifyConnectionStateChange(connected: boolean) {
	wsCache.isConnected = connected;
	for (const listener of wsCache.listeners) {
		listener(connected);
	}
}

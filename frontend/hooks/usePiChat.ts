"use client";

import {
	type MainChatDbMessage,
	type PiAgentMessage,
	type PiState,
	abortMainChatPi,
	addMainChatPiSeparator,
	createMainChatPiWebSocket,
	getMainChatPiHistory,
	getMainChatPiState,
	newMainChatPiSession,
	registerMainChatSession,
	resetMainChatPiSession,
	startMainChatPiSession,
} from "@/lib/control-plane-client";
import { useCallback, useEffect, useRef, useState } from "react";

/** Pi streaming event types */
export type PiEventType =
	| "connected"
	| "state"
	| "message_start"
	| "message"
	| "text"
	| "tool_use"
	| "tool_result"
	| "done"
	| "error"
	| "compaction";

/** Pi streaming event */
export type PiStreamEvent = {
	type: PiEventType;
	data?: unknown;
};

/** Message part for display */
export type PiMessagePart =
	| { type: "text"; content: string }
	| { type: "tool_use"; id: string; name: string; input: unknown }
	| {
			type: "tool_result";
			id: string;
			name?: string;
			content: unknown;
			isError?: boolean;
	  }
	| { type: "thinking"; content: string }
	| { type: "separator"; content: string; sessionId?: string }
	| { type: "compaction"; content: string };

/** Display message with parts */
export type PiDisplayMessage = {
	id: string;
	role: "user" | "assistant" | "system";
	parts: PiMessagePart[];
	timestamp: number;
	isStreaming?: boolean;
	usage?: PiAgentMessage["usage"];
};

/** Hook options */
export type UsePiChatOptions = {
	/** Auto-connect on mount */
	autoConnect?: boolean;
	/** Callback when message stream completes */
	onMessageComplete?: (message: PiDisplayMessage) => void;
	/** Callback on error */
	onError?: (error: Error) => void;
};

/** Hook return type */
export type UsePiChatReturn = {
	/** Current Pi state */
	state: PiState | null;
	/** Display messages */
	messages: PiDisplayMessage[];
	/** Whether connected to WebSocket */
	isConnected: boolean;
	/** Whether currently streaming a response */
	isStreaming: boolean;
	/** Current error if any */
	error: Error | null;
	/** Send a message */
	send: (message: string, options?: PiSendOptions) => Promise<void>;
	/** Abort current stream */
	abort: () => Promise<void>;
	/** Start new session (clear history) */
	newSession: () => Promise<void>;
	/** Reset session - restarts Pi process to reload PERSONALITY.md and USER.md */
	resetSession: () => Promise<void>;
	/** Reload messages from server */
	refresh: () => Promise<void>;
	/** Connect to WebSocket */
	connect: () => void;
	/** Disconnect from WebSocket */
	disconnect: () => void;
};

export type PiSendMode = "prompt" | "steer" | "follow_up";

export type PiSendOptions = {
	mode?: PiSendMode;
	queueIfStreaming?: boolean;
};

// localStorage keys for persistent cache
const STORAGE_KEY_MESSAGES = "octo:mainChat:messages";
const STORAGE_KEY_STATE = "octo:mainChat:state";
const STORAGE_KEY_TIMESTAMP = "octo:mainChat:timestamp";
const STORAGE_KEY_SCROLL = "octo:mainChat:scrollPosition";

// In-memory cache for instant access (populated from localStorage on load)
const memoryCache = {
	messages: null as PiDisplayMessage[] | null,
	state: null as PiState | null,
	timestamp: 0,
	scrollPosition: null as number | null, // null = scroll to bottom, number = user's scroll position
	initialized: false,
};

const PI_MESSAGE_ID_PATTERN = /^pi-msg-(\d+)$/;

function getMaxPiMessageId(messages: PiDisplayMessage[]): number {
	let maxId = 0;
	for (const message of messages) {
		const match = PI_MESSAGE_ID_PATTERN.exec(message.id);
		if (!match) continue;
		const value = Number.parseInt(match[1] ?? "0", 10);
		if (!Number.isNaN(value) && value > maxId) {
			maxId = value;
		}
	}
	return maxId;
}

// Initialize cache from localStorage synchronously on module load
function initializeCacheFromStorage() {
	if (memoryCache.initialized || typeof window === "undefined") return;
	memoryCache.initialized = true;

	try {
		const storedMessages = localStorage.getItem(STORAGE_KEY_MESSAGES);
		const storedState = localStorage.getItem(STORAGE_KEY_STATE);
		const storedTimestamp = localStorage.getItem(STORAGE_KEY_TIMESTAMP);
		const storedScroll = localStorage.getItem(STORAGE_KEY_SCROLL);

		if (storedMessages) {
			memoryCache.messages = JSON.parse(storedMessages);
		}
		if (storedState) {
			memoryCache.state = JSON.parse(storedState);
		}
		if (storedTimestamp) {
			memoryCache.timestamp = Number.parseInt(storedTimestamp, 10);
		}
		if (storedScroll) {
			memoryCache.scrollPosition = Number.parseInt(storedScroll, 10);
		}
	} catch {
		// Ignore parse errors - will fetch fresh data
	}
}

// Run initialization immediately
initializeCacheFromStorage();

// Maximum messages to cache in localStorage (keep last N for performance)
const MAX_CACHED_MESSAGES = 100;

// Update cache (both memory and localStorage)
function updateMessageCache(messages: PiDisplayMessage[]) {
	memoryCache.messages = messages;
	memoryCache.timestamp = Date.now();

	// Persist to localStorage asynchronously - limit size for performance
	queueMicrotask(() => {
		try {
			// Only cache the last N messages to keep localStorage small
			const toCache =
				messages.length > MAX_CACHED_MESSAGES
					? messages.slice(-MAX_CACHED_MESSAGES)
					: messages;
			localStorage.setItem(STORAGE_KEY_MESSAGES, JSON.stringify(toCache));
			localStorage.setItem(
				STORAGE_KEY_TIMESTAMP,
				String(memoryCache.timestamp),
			);
		} catch {
			// Ignore storage errors (quota exceeded, etc.)
		}
	});
}

// Update state cache
function updateStateCache(state: PiState) {
	memoryCache.state = state;

	// Persist to localStorage asynchronously
	queueMicrotask(() => {
		try {
			localStorage.setItem(STORAGE_KEY_STATE, JSON.stringify(state));
		} catch {
			// Ignore storage errors
		}
	});
}

// Get cached messages (instant)
function getCachedMessages(): PiDisplayMessage[] {
	return memoryCache.messages ?? [];
}

// Get cached state (instant)
function getCachedState(): PiState | null {
	return memoryCache.state;
}

// Check if cache is fresh enough to skip network fetch
function isCacheFresh(): boolean {
	// Cache is fresh for 5 minutes
	return memoryCache.timestamp > 0 && Date.now() - memoryCache.timestamp < 300000;
}

function shouldPreserveLocalMessage(message: PiDisplayMessage): boolean {
	// Local optimistic messages (not yet persisted) use pi-msg-* IDs.
	// Keep them when server refreshes history to avoid clobbering in-flight streaming.
	if (PI_MESSAGE_ID_PATTERN.test(message.id)) return true;
	if (message.id.startsWith("compaction-")) return true;
	return false;
}

function mergeServerMessages(
	previous: PiDisplayMessage[],
	serverMessages: PiDisplayMessage[],
): PiDisplayMessage[] {
	const serverIds = new Set(serverMessages.map((m) => m.id));
	const preserved = previous.filter(
		(m) => shouldPreserveLocalMessage(m) && !serverIds.has(m.id),
	);
	return preserved.length > 0 ? [...serverMessages, ...preserved] : serverMessages;
}

// Get cached scroll position (null = bottom)
export function getCachedScrollPosition(): number | null {
	return memoryCache.scrollPosition;
}

// Save scroll position to cache
export function setCachedScrollPosition(position: number | null) {
	memoryCache.scrollPosition = position;

	// Persist asynchronously
	queueMicrotask(() => {
		try {
			if (position === null) {
				localStorage.removeItem(STORAGE_KEY_SCROLL);
			} else {
				localStorage.setItem(STORAGE_KEY_SCROLL, String(position));
			}
		} catch {
			// Ignore
		}
	});
}

// Global WebSocket connection cache - survives component remounts
type WsConnectionState = {
	ws: WebSocket | null;
	isConnected: boolean;
	sessionStarted: boolean;
	listeners: Set<(connected: boolean) => void>;
};

const wsCache: WsConnectionState = {
	ws: null,
	isConnected: false,
	sessionStarted: false,
	listeners: new Set(),
};

// Subscribe to connection state changes
function subscribeToConnectionState(listener: (connected: boolean) => void) {
	wsCache.listeners.add(listener);
	return () => {
		wsCache.listeners.delete(listener);
	};
}

// Notify all listeners of connection state change
function notifyConnectionStateChange(connected: boolean) {
	wsCache.isConnected = connected;
	for (const listener of wsCache.listeners) {
		listener(connected);
	}
}

/**
 * Hook for managing Pi chat in Main Chat mode.
 * Handles WebSocket connection, message streaming, and state.
 */
export function usePiChat(options: UsePiChatOptions = {}): UsePiChatReturn {
	const { autoConnect = true, onMessageComplete, onError } = options;

	// Initialize with cached data for INSTANT display - no loading states
	const [state, setState] = useState<PiState | null>(getCachedState);
	const [messages, setMessages] =
		useState<PiDisplayMessage[]>(getCachedMessages);
	// Assume connected if we have cached data - optimistic
	const [isConnected, setIsConnected] = useState(
		wsCache.isConnected || getCachedMessages().length > 0,
	);
	const [isStreaming, setIsStreaming] = useState(false);
	const [error, setError] = useState<Error | null>(null);

	// Track if this hook instance owns the WebSocket
	const isOwnerRef = useRef(false);
	const streamingMessageRef = useRef<PiDisplayMessage | null>(null);
	const messageIdRef = useRef(getMaxPiMessageId(getCachedMessages()));
	const refreshRef = useRef<(() => Promise<void>) | null>(null);
	const initStartedRef = useRef(false);

	// Subscribe to global connection state changes
	useEffect(() => {
		return subscribeToConnectionState(setIsConnected);
	}, []);

	// Generate unique message ID
	const nextMessageId = useCallback(() => {
		messageIdRef.current += 1;
		return `pi-msg-${messageIdRef.current}`;
	}, []);

	// Convert Pi agent messages to display messages (from Pi's context)
	const convertToDisplayMessages = useCallback(
		(agentMessages: PiAgentMessage[]): PiDisplayMessage[] => {
			return agentMessages.map((msg, idx) => {
				const parts: PiMessagePart[] = [];

				if (typeof msg.content === "string") {
					parts.push({ type: "text", content: msg.content });
				} else if (Array.isArray(msg.content)) {
					for (const block of msg.content) {
						if (typeof block === "string") {
							parts.push({ type: "text", content: block });
						} else if (block && typeof block === "object") {
							const b = block as Record<string, unknown>;
							if (b.type === "text" && typeof b.text === "string") {
								parts.push({ type: "text", content: b.text });
							} else if (b.type === "thinking") {
								// Pi sends thinking blocks with "thinking" field (may be empty)
								// and optionally "thinkingSignature" for extended thinking
								const thinkingText =
									typeof b.thinking === "string" ? b.thinking : "";
								// Only add if there's actual content
								if (thinkingText.trim()) {
									parts.push({ type: "thinking", content: thinkingText });
								}
							} else if (b.type === "tool_use") {
								parts.push({
									type: "tool_use",
									id: String(b.id ?? ""),
									name: String(b.name ?? "unknown"),
									input: b.input,
								});
							} else if (b.type === "tool_result") {
								parts.push({
									type: "tool_result",
									id: String(b.tool_use_id ?? ""),
									content: b.content,
									isError: Boolean(b.is_error),
								});
							}
							// Skip unknown block types (thinkingSignature, etc.)
						}
					}
				}

				return {
					id: `pi-hist-${idx}`,
					role: msg.role as "user" | "assistant",
					parts,
					timestamp: msg.timestamp ?? Date.now(),
					usage: msg.usage,
				};
			});
		},
		[],
	);

	// Convert database messages to display messages (persistent history)
	const convertDbToDisplayMessages = useCallback(
		(dbMessages: MainChatDbMessage[]): PiDisplayMessage[] => {
			return dbMessages.map((msg) => {
				const parts: PiMessagePart[] = [];

				try {
					const content = JSON.parse(msg.content);
					if (Array.isArray(content)) {
						for (const block of content) {
							if (block && typeof block === "object") {
								if (block.type === "text" && typeof block.text === "string") {
									parts.push({ type: "text", content: block.text });
								} else if (
									block.type === "thinking" &&
									typeof block.text === "string"
								) {
									if (block.text.trim()) {
										parts.push({ type: "thinking", content: block.text });
									}
								} else if (block.type === "tool_use") {
									parts.push({
										type: "tool_use",
										id: String(block.id ?? ""),
										name: String(block.name ?? "unknown"),
										input: block.input,
									});
								} else if (block.type === "tool_result") {
									parts.push({
										type: "tool_result",
										id: String(block.id ?? ""),
										name:
											typeof block.name === "string" ? block.name : undefined,
										content: block.content,
										isError: Boolean(block.isError),
									});
								} else if (
									block.type === "separator" &&
									typeof block.text === "string"
								) {
									parts.push({ type: "separator", content: block.text });
								}
							}
						}
					}
				} catch {
					// If JSON parsing fails, treat content as plain text
					parts.push({ type: "text", content: msg.content });
				}

				return {
					id: `pi-db-${msg.id}`,
					role: msg.role as "user" | "assistant" | "system",
					parts,
					timestamp: msg.timestamp,
				};
			});
		},
		[],
	);

	// Handle incoming WebSocket messages
	const handleWsMessage = useCallback(
		(event: MessageEvent) => {
			try {
				const data = JSON.parse(event.data) as PiStreamEvent;

				switch (data.type) {
					case "connected":
						setIsConnected(true);
						break;

					case "state":
						setState(data.data as PiState);
						break;

					case "message_start": {
						if (!streamingMessageRef.current) {
							const assistantMessage: PiDisplayMessage = {
								id: nextMessageId(),
								role: "assistant",
								parts: [],
								timestamp: Date.now(),
								isStreaming: true,
							};
							streamingMessageRef.current = assistantMessage;
							setMessages((prev) => [...prev, assistantMessage]);
						}
						setIsStreaming(true);
						break;
					}

					case "text": {
						// Append text to streaming message
						const text = data.data as string;
						const currentTextMsg = streamingMessageRef.current;
						if (currentTextMsg) {
							const lastPart =
								currentTextMsg.parts[currentTextMsg.parts.length - 1];
							if (lastPart?.type === "text") {
								lastPart.content += text;
							} else {
								currentTextMsg.parts.push({
									type: "text",
									content: text,
								});
							}
							// Update messages state - create new parts array for React to detect change
							setMessages((prev) => {
								const idx = prev.findIndex((m) => m.id === currentTextMsg.id);
								if (idx >= 0) {
									const updated = [...prev];
									updated[idx] = {
										...currentTextMsg,
										parts: currentTextMsg.parts.map(p => ({ ...p })),
									};
									return updated;
								}
								return prev;
							});
						}
						break;
					}

					case "tool_use": {
						const tool = data.data as {
							id: string;
							name: string;
							input: unknown;
						};
						const currentToolMsg = streamingMessageRef.current;
						if (currentToolMsg) {
							currentToolMsg.parts.push({
								type: "tool_use",
								id: tool.id,
								name: tool.name,
								input: tool.input,
							});
							setMessages((prev) => {
								const idx = prev.findIndex((m) => m.id === currentToolMsg.id);
								if (idx >= 0) {
									const updated = [...prev];
									updated[idx] = {
										...currentToolMsg,
										parts: currentToolMsg.parts.map(p => ({ ...p })),
									};
									return updated;
								}
								return prev;
							});
						}
						break;
					}

					case "tool_result": {
						const result = data.data as {
							id: string;
							name?: string;
							content: unknown;
							isError?: boolean;
						};
						const currentResultMsg = streamingMessageRef.current;
						if (currentResultMsg) {
							currentResultMsg.parts.push({
								type: "tool_result",
								id: result.id,
								name: result.name,
								content: result.content,
								isError: result.isError,
							});
							setMessages((prev) => {
								const idx = prev.findIndex((m) => m.id === currentResultMsg.id);
								if (idx >= 0) {
									const updated = [...prev];
									updated[idx] = {
										...currentResultMsg,
										parts: currentResultMsg.parts.map(p => ({ ...p })),
									};
									return updated;
								}
								return prev;
							});
						}
						break;
					}

					case "done": {
						// Mark message as complete
						if (streamingMessageRef.current) {
							streamingMessageRef.current.isStreaming = false;
							const completedMessage = {
								...streamingMessageRef.current,
								parts: streamingMessageRef.current.parts.map(p => ({ ...p })),
							};
							setMessages((prev) => {
								const idx = prev.findIndex((m) => m.id === completedMessage.id);
								if (idx >= 0) {
									const updated = [...prev];
									updated[idx] = completedMessage;
									return updated;
								}
								return prev;
							});
							onMessageComplete?.(completedMessage);
							streamingMessageRef.current = null;
						}
						setIsStreaming(false);
						break;
					}

					case "error": {
						const errMsg =
							typeof data.data === "string" ? data.data : "Unknown error";
						const err = new Error(errMsg);
						setError(err);
						onError?.(err);
						setIsStreaming(false);
						if (streamingMessageRef.current) {
							streamingMessageRef.current.isStreaming = false;
							streamingMessageRef.current = null;
						}
						break;
					}

					case "compaction":
						// Add a compaction marker message so token counting can reset
						setMessages((prev) => [
							...prev,
							{
								id: `compaction-${Date.now()}`,
								role: "system",
								parts: [{ type: "compaction", content: "Context compacted" }],
								timestamp: Date.now(),
							},
						]);
						// Also refresh to sync with server state (use ref to avoid circular dependency)
						refreshRef.current?.();
						break;
				}
			} catch (e) {
				console.error("Failed to parse Pi WebSocket message:", e);
			}
		},
		[onMessageComplete, onError, nextMessageId],
	);

	// Connect to WebSocket - uses global cache to survive remounts
	const connect = useCallback(() => {
		// If global WebSocket is already open and healthy, reuse it
		if (wsCache.ws?.readyState === WebSocket.OPEN) {
			// Just attach our message handler
			wsCache.ws.onmessage = handleWsMessage;
			isOwnerRef.current = true;
			return;
		}

		// Close any stale connection
		if (wsCache.ws && wsCache.ws.readyState !== WebSocket.CLOSED) {
			wsCache.ws.close();
		}

		const ws = createMainChatPiWebSocket();
		wsCache.ws = ws;
		isOwnerRef.current = true;

		ws.onopen = () => {
			notifyConnectionStateChange(true);
			setError(null);
		};

		ws.onmessage = handleWsMessage;

		ws.onerror = () => {
			const err = new Error("WebSocket connection error");
			setError(err);
			onError?.(err);
		};

		ws.onclose = () => {
			notifyConnectionStateChange(false);
			// Clear the global ws reference on close
			if (wsCache.ws === ws) {
				wsCache.ws = null;
				wsCache.sessionStarted = false;
			}
		};
	}, [handleWsMessage, onError]);

	// Disconnect from WebSocket - only actually disconnects if we're the owner
	const disconnect = useCallback((force = false) => {
		if (force && wsCache.ws) {
			wsCache.ws.close();
			wsCache.ws = null;
			wsCache.sessionStarted = false;
			notifyConnectionStateChange(false);
		}
		isOwnerRef.current = false;
	}, []);

	// Refresh messages and state from server (background, non-blocking)
	const refresh = useCallback(async () => {
		// Don't refresh while streaming - can cause race conditions with local messages
		if (isStreaming) {
			return;
		}
		try {
			const [piState, dbMessages] = await Promise.all([
				getMainChatPiState(),
				getMainChatPiHistory(),
			]);
			setState(piState);
			updateStateCache(piState);
			const displayMessages = convertDbToDisplayMessages(dbMessages);
			setMessages((previous) => mergeServerMessages(previous, displayMessages));
		} catch (e) {
			// Don't show errors for background refresh - we have cached data
			console.warn("Background refresh failed:", e);
		}
	}, [convertDbToDisplayMessages, isStreaming]);

	// Keep refreshRef in sync so handleWsMessage can call it
	useEffect(() => {
		refreshRef.current = refresh;
	}, [refresh]);

	// Keep cache in sync when messages change
	useEffect(() => {
		if (messages.length > 0) {
			updateMessageCache(messages);
		}
	}, [messages]);

	// Ensure nextMessageId never collides with cached/loaded messages
	useEffect(() => {
		const maxId = getMaxPiMessageId(messages);
		if (maxId > messageIdRef.current) {
			messageIdRef.current = maxId;
		}
	}, [messages]);

	// Send a message via WebSocket (which persists user messages)
	const send = useCallback(
		async (message: string, options?: PiSendOptions) => {
			// Must be connected to send
			if (!wsCache.ws || wsCache.ws.readyState !== WebSocket.OPEN) {
				const err = new Error("Not connected to chat server");
				setError(err);
				onError?.(err);
				return;
			}

			setError(null);
			let mode: PiSendMode = options?.mode ?? "prompt";
			if (
				isStreaming &&
				mode === "prompt" &&
				(options?.queueIfStreaming ?? true)
			) {
				mode = "follow_up";
			}
			if (!isStreaming && (mode === "follow_up" || mode === "steer")) {
				mode = "prompt";
			}

			// Add user message immediately
			const userMessage: PiDisplayMessage = {
				id: nextMessageId(),
				role: "user",
				parts: [{ type: "text", content: message }],
				timestamp: Date.now(),
			};
			setMessages((prev) => [...prev, userMessage]);

			if (!isStreaming && mode === "prompt") {
				setIsStreaming(true);
				const assistantMessage: PiDisplayMessage = {
					id: nextMessageId(),
					role: "assistant",
					parts: [],
					timestamp: Date.now(),
					isStreaming: true,
				};
				streamingMessageRef.current = assistantMessage;
				setMessages((prev) => [...prev, assistantMessage]);
			}

			try {
				// Send via WebSocket - backend will persist user message
				wsCache.ws.send(JSON.stringify({ type: mode, message }));
			} catch (e) {
				const err = e instanceof Error ? e : new Error("Failed to send");
				setError(err);
				onError?.(err);
				if (streamingMessageRef.current) {
					streamingMessageRef.current.isStreaming = false;
					streamingMessageRef.current = null;
				}
				setIsStreaming(false);
			}
		},
		[isStreaming, nextMessageId, onError],
	);

	// Abort current stream
	const abort = useCallback(async () => {
		try {
			await abortMainChatPi();
			setIsStreaming(false);
			if (streamingMessageRef.current) {
				streamingMessageRef.current.isStreaming = false;
				streamingMessageRef.current = null;
			}
		} catch (e) {
			const err = e instanceof Error ? e : new Error("Failed to abort");
			setError(err);
			onError?.(err);
		}
	}, [onError]);

	// Start new session (resets Pi context, adds separator to history)
	const newSession = useCallback(async () => {
		try {
			// Add separator to mark new conversation
			const separatorMsg = await addMainChatPiSeparator();
			// Then create new Pi session
			const newState = await newMainChatPiSession();
			setState(newState);

			// Register the new session with main chat backend
			if (newState.session_id) {
				try {
					await registerMainChatSession("default", {
						session_id: newState.session_id,
						title: `Session ${new Date().toLocaleDateString()}`,
					});
				} catch (regErr) {
					console.warn("Failed to register session:", regErr);
				}
			}

			// Add separator to display with session ID for scroll targeting
			const separatorDisplay: PiDisplayMessage = {
				id: `pi-db-${separatorMsg.id}`,
				role: "system",
				parts: [
					{
						type: "separator",
						content: "New conversation started",
						sessionId: newState.session_id,
					},
				],
				timestamp: separatorMsg.timestamp,
			};
			setMessages((prev) => [...prev, separatorDisplay]);
			streamingMessageRef.current = null;
		} catch (e) {
			const err =
				e instanceof Error ? e : new Error("Failed to start new session");
			setError(err);
			onError?.(err);
		}
	}, [onError]);

	// Reset session - restarts Pi process to reload PERSONALITY.md and USER.md
	const resetSession = useCallback(async () => {
		try {
			// Force disconnect WebSocket first
			disconnect(true);

			// Add separator to mark reset
			const separatorMsg = await addMainChatPiSeparator();

			// Reset the session (this restarts the Pi process)
			const newState = await resetMainChatPiSession();
			setState(newState);
			wsCache.sessionStarted = true;

			// Register the new session with main chat backend
			if (newState.session_id) {
				try {
					await registerMainChatSession("default", {
						session_id: newState.session_id,
						title: `Session ${new Date().toLocaleDateString()}`,
					});
				} catch (regErr) {
					console.warn("Failed to register session:", regErr);
				}
			}

			// Add separator to display with session ID for scroll targeting
			const separatorDisplay: PiDisplayMessage = {
				id: `pi-db-${separatorMsg.id}`,
				role: "system",
				parts: [
					{
						type: "separator",
						content: "Session reset - reloaded personality",
						sessionId: newState.session_id,
					},
				],
				timestamp: separatorMsg.timestamp,
			};
			setMessages((prev) => [...prev, separatorDisplay]);
			streamingMessageRef.current = null;

			// Reconnect WebSocket
			connect();
		} catch (e) {
			const err = e instanceof Error ? e : new Error("Failed to reset session");
			setError(err);
			onError?.(err);
		}
	}, [connect, disconnect, onError]);

	// Keep WebSocket handler in sync when callback changes
	useEffect(() => {
		if (isOwnerRef.current && wsCache.ws?.readyState === WebSocket.OPEN) {
			wsCache.ws.onmessage = handleWsMessage;
		}
	}, [handleWsMessage]);

	// Initialize on mount - INSTANT with cached data, background refresh
	useEffect(() => {
		// Prevent double initialization in strict mode
		if (initStartedRef.current) return;
		initStartedRef.current = true;

		let mounted = true;

		// If WebSocket already connected, just reattach handler
		if (wsCache.ws?.readyState === WebSocket.OPEN) {
			wsCache.ws.onmessage = handleWsMessage;
			isOwnerRef.current = true;
			setIsConnected(true);

			// Background refresh if cache is stale
			if (!isCacheFresh()) {
				refresh();
			}
			return () => {
				mounted = false;
				isOwnerRef.current = false;
			};
		}

		// Start session and connect in background - UI already has cached data
		const initSession = async () => {
			try {
				// Start session (may already be running on backend)
				const piState = await startMainChatPiSession();
				if (!mounted) return;

				setState(piState);
				updateStateCache(piState);
				wsCache.sessionStarted = true;

				// Connect WebSocket
				if (autoConnect) {
					connect();
				}

				// Fetch fresh history in background
				getMainChatPiHistory()
					.then((dbMessages) => {
						if (!mounted) return;
						const displayMessages = convertDbToDisplayMessages(dbMessages);
						// Only update if we got data
						if (displayMessages.length > 0) {
							setMessages((previous) =>
								mergeServerMessages(previous, displayMessages),
							);
						}
					})
					.catch(() => {
						// Ignore - we have cached data
					});
			} catch (e) {
				if (!mounted) return;
				// Only show error if we have no cached data
				if (getCachedMessages().length === 0) {
					const err =
						e instanceof Error ? e : new Error("Failed to initialize");
					setError(err);
					onError?.(err);
				} else {
					console.warn("Session init failed, using cached data:", e);
				}
			}
		};

		initSession();

		return () => {
			mounted = false;
			isOwnerRef.current = false;
			initStartedRef.current = false;
		};
	}, [
		autoConnect,
		connect,
		convertDbToDisplayMessages,
		handleWsMessage,
		onError,
		refresh,
	]);

	return {
		state,
		messages,
		isConnected,
		isStreaming,
		error,
		send,
		abort,
		newSession,
		resetSession,
		refresh,
		connect,
		disconnect,
	};
}

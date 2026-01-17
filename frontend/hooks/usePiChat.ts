"use client";

import {
	type PiAgentMessage,
	type PiSessionMessage,
	type PiState,
	abortMainChatPi,
	createMainChatPiWebSocket,
	getMainChatPiSessionMessages,
	getMainChatPiState,
	newMainChatPiSessionFile,
	resetMainChatPiSession,
	resumeMainChatPiSession,
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
	/** Selected Pi session ID (disk-backed Main Chat session) */
	selectedSessionId?: string | null;
	/** Notify when a new session becomes active (e.g. /new) */
	onSelectedSessionIdChange?: (id: string | null) => void;
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

const STORAGE_KEY_SCROLL = "octo:mainChat:scrollPosition";

type SessionMessageCacheEntry = {
	messages: PiDisplayMessage[];
	timestamp: number;
};

const sessionMessageCache = {
	messagesBySession: new Map<string, SessionMessageCacheEntry>(),
	initialized: false,
};

function cacheKeyMessages(sessionId: string) {
	return `octo:mainChatPi:session:${sessionId}:messages:v1`;
}

function readCachedSessionMessages(sessionId: string): PiDisplayMessage[] {
	const inMemory = sessionMessageCache.messagesBySession.get(sessionId);
	if (inMemory) {
		// Strip isStreaming from cached messages - it's transient state
		return inMemory.messages.map((m) => {
			if (m.isStreaming) {
				const { isStreaming: _, ...rest } = m;
				return rest;
			}
			return m;
		});
	}
	if (typeof window === "undefined") return [];
	try {
		const raw = localStorage.getItem(cacheKeyMessages(sessionId));
		if (!raw) return [];
		const parsed = JSON.parse(raw) as SessionMessageCacheEntry;
		if (!parsed || !Array.isArray(parsed.messages)) return [];
		// Strip isStreaming from cached messages - it's transient state
		const cleanedMessages = parsed.messages.map((m) => {
			if (m.isStreaming) {
				const { isStreaming: _, ...rest } = m;
				return rest;
			}
			return m;
		});
		const cleanedEntry = { messages: cleanedMessages, timestamp: parsed.timestamp };
		sessionMessageCache.messagesBySession.set(sessionId, cleanedEntry);
		return cleanedMessages;
	} catch {
		return [];
	}
}

function writeCachedSessionMessages(sessionId: string, messages: PiDisplayMessage[]) {
	// Strip isStreaming flag when caching - it's transient state that shouldn't persist
	const cleanedMessages = messages.map((m) => {
		if (m.isStreaming) {
			const { isStreaming: _, ...rest } = m;
			return rest;
		}
		return m;
	});
	const entry: SessionMessageCacheEntry = { messages: cleanedMessages, timestamp: Date.now() };
	sessionMessageCache.messagesBySession.set(sessionId, entry);
	if (typeof window === "undefined") return;
	queueMicrotask(() => {
		try {
			localStorage.setItem(cacheKeyMessages(sessionId), JSON.stringify(entry));
		} catch {
			// ignore
		}
	});
}

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

// We keep only per-session message caches; state is fetched from backend.
function getCachedState(): PiState | null {
	return null;
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

const scrollCache = {
	position: null as number | null,
	initialized: false,
};

function initScrollCache() {
	if (scrollCache.initialized || typeof window === "undefined") return;
	scrollCache.initialized = true;
	try {
		const stored = localStorage.getItem(STORAGE_KEY_SCROLL);
		if (stored !== null) {
			scrollCache.position = Number.parseInt(stored, 10);
		}
	} catch {
		// ignore
	}
}

initScrollCache();

// Get cached scroll position (null = bottom)
export function getCachedScrollPosition(): number | null {
	return scrollCache.position;
}

// Save scroll position to cache
export function setCachedScrollPosition(position: number | null) {
	scrollCache.position = position;

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
	const {
		autoConnect = true,
		selectedSessionId,
		onSelectedSessionIdChange,
		onMessageComplete,
		onError,
	} = options;

	const activeSessionId = selectedSessionId ?? null;
	const resumeInFlightRef = useRef<string | null>(null);
	// Track sessions created by newSession() to skip resume for them
	const justCreatedSessionRef = useRef<string | null>(null);

	// Initialize with cached data for INSTANT display
	const [state, setState] = useState<PiState | null>(getCachedState);
	const [messages, setMessages] = useState<PiDisplayMessage[]>(
		activeSessionId ? readCachedSessionMessages(activeSessionId) : [],
	);
	// Assume connected if we have cached data - optimistic
	const [isConnected, setIsConnected] = useState(
		wsCache.isConnected || messages.length > 0,
	);
	const [isStreaming, setIsStreaming] = useState(false);
	const [error, setError] = useState<Error | null>(null);

	// Track if this hook instance owns the WebSocket
	const isOwnerRef = useRef(false);
	const streamingMessageRef = useRef<PiDisplayMessage | null>(null);
	const messageIdRef = useRef(getMaxPiMessageId(messages));
	const refreshRef = useRef<(() => Promise<void>) | null>(null);
	const initStartedRef = useRef(false);

	// We need access to connect/disconnect in the effect, but they depend on handleWsMessage.
	// Store them in refs to avoid circular dependencies.
	const connectRef = useRef<(() => void) | null>(null);
	const disconnectRef = useRef<((force?: boolean) => void) | null>(null);

	// When selection changes, swap to cached messages instantly and resume backend session.
	useEffect(() => {
		if (!activeSessionId) {
			setMessages([]);
			return;
		}

		// If this session was just created by newSession(), skip resume
		// (the session is already active and empty on the backend)
		if (justCreatedSessionRef.current === activeSessionId) {
			justCreatedSessionRef.current = null;
			// WebSocket reconnection is handled by newSession() itself
			return;
		}

		setMessages(readCachedSessionMessages(activeSessionId));
		streamingMessageRef.current = null;
		setIsStreaming(false);

		// Resume selected session in background, then reconnect WebSocket.
		if (resumeInFlightRef.current === activeSessionId) return;
		resumeInFlightRef.current = activeSessionId;
		resumeMainChatPiSession(activeSessionId)
			.then(() => {
				// Force WebSocket reconnect so it gets the newly resumed session.
				// The backend creates a new session when resuming, but the old WebSocket
				// still holds a reference to the previous session.
				disconnectRef.current?.(true);
				connectRef.current?.();
				return refreshRef.current?.();
			})
			.catch((err) => {
				console.warn("Failed to resume session:", err);
			})
			.finally(() => {
				if (resumeInFlightRef.current === activeSessionId) {
					resumeInFlightRef.current = null;
				}
			});
	}, [activeSessionId]);

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

	const convertSessionMessagesToDisplay = useCallback(
		(sessionMessages: PiSessionMessage[]): PiDisplayMessage[] => {
			return sessionMessages.map((msg, idx) => {
				const parts: PiMessagePart[] = [];
				const content = msg.content;

				if (typeof content === "string") {
					parts.push({ type: "text", content });
				} else if (Array.isArray(content)) {
					for (const block of content) {
						if (typeof block === "string") {
							parts.push({ type: "text", content: block });
							continue;
						}
						if (!block || typeof block !== "object") continue;
						const b = block as Record<string, unknown>;
						if (b.type === "text" && typeof b.text === "string") {
							parts.push({ type: "text", content: b.text });
						} else if (b.type === "thinking") {
							const thinkingText =
								typeof b.thinking === "string" ? b.thinking : "";
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
								id: String(b.tool_use_id ?? b.id ?? ""),
								name: typeof b.name === "string" ? b.name : undefined,
								content: b.content,
								isError: Boolean(b.is_error ?? b.isError),
							});
						}
					}
				}

				return {
					id: `pi-session-${idx}-${msg.id}`,
					role: msg.role,
					parts,
					timestamp: msg.timestamp || Date.now(),
					usage: msg.usage as PiAgentMessage["usage"],
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

					case "state": {
						const nextState = data.data as PiState;
						setState(nextState);
						// If backend says it's not streaming, ensure UI clears "Working".
						if (nextState && nextState.is_streaming === false) {
							setIsStreaming(false);
							if (streamingMessageRef.current) {
								streamingMessageRef.current.isStreaming = false;
								streamingMessageRef.current = null;
							}
						}
						break;
					}

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

		// If a connection is already in progress, don't restart it.
		if (wsCache.ws && wsCache.ws.readyState === WebSocket.CONNECTING) {
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

			// Finalize any in-flight message so per-message "Working" clears.
			if (streamingMessageRef.current) {
				const completedMessage = {
					...streamingMessageRef.current,
					isStreaming: false,
					parts: streamingMessageRef.current.parts.map((p) => ({ ...p })),
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
				streamingMessageRef.current = null;
			}

			setIsStreaming(false);
		};

		ws.onclose = () => {
			notifyConnectionStateChange(false);

			// Finalize any in-flight message so per-message "Working" clears.
			if (streamingMessageRef.current) {
				const completedMessage = {
					...streamingMessageRef.current,
					isStreaming: false,
					parts: streamingMessageRef.current.parts.map((p) => ({ ...p })),
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
				streamingMessageRef.current = null;
			}

			setIsStreaming(false);

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
		setIsStreaming(false);
		if (streamingMessageRef.current) {
			streamingMessageRef.current.isStreaming = false;
			streamingMessageRef.current = null;
		}
		isOwnerRef.current = false;
	}, []);

	// Refresh messages and state from server (background, non-blocking)
	const refresh = useCallback(async () => {
		// Don't refresh while streaming - can cause race conditions with local messages
		if (isStreaming) {
			return;
		}
		if (!activeSessionId) {
			return;
		}
		try {
			const [piState, sessionMessages] = await Promise.all([
				getMainChatPiState(),
				getMainChatPiSessionMessages(activeSessionId),
			]);
			setState(piState);

			// If backend says it's not streaming, ensure local state is cleared
			if (piState && piState.is_streaming === false) {
				setIsStreaming(false);
				if (streamingMessageRef.current) {
					streamingMessageRef.current.isStreaming = false;
					streamingMessageRef.current = null;
				}
			}

			const displayMessages = convertSessionMessagesToDisplay(sessionMessages);
			setMessages((previous) => mergeServerMessages(previous, displayMessages));
			writeCachedSessionMessages(activeSessionId, displayMessages);
		} catch (e) {
			// Don't show errors for background refresh - we have cached data
			console.warn("Background refresh failed:", e);
		}
	}, [activeSessionId, convertSessionMessagesToDisplay, isStreaming]);

	// Keep refs in sync so the session-change effect can use them
	useEffect(() => {
		refreshRef.current = refresh;
	}, [refresh]);

	useEffect(() => {
		connectRef.current = connect;
	}, [connect]);

	useEffect(() => {
		disconnectRef.current = disconnect;
	}, [disconnect]);

	// Keep per-session cache in sync when messages change
	useEffect(() => {
		if (!activeSessionId) return;
		if (messages.length > 0) {
			writeCachedSessionMessages(activeSessionId, messages);
		}
	}, [activeSessionId, messages]);

	// Fallback: if streaming gets stuck, poll backend state to clear.
	useEffect(() => {
		if (!isStreaming) return;
		let cancelled = false;

		const checkStreamingState = async () => {
			try {
				const piState = await getMainChatPiState();
				if (cancelled) return;
				if (piState && piState.is_streaming === false) {
					setState(piState);
					setIsStreaming(false);
					if (streamingMessageRef.current) {
						streamingMessageRef.current.isStreaming = false;
						const completedMessage = {
							...streamingMessageRef.current,
							parts: streamingMessageRef.current.parts.map((p) => ({ ...p })),
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
						streamingMessageRef.current = null;
					}
				}
			} catch {
				// Ignore polling errors.
			}
		};

		const interval = setInterval(checkStreamingState, 4000);
		const timeout = setTimeout(checkStreamingState, 4000);

		return () => {
			cancelled = true;
			clearInterval(interval);
			clearTimeout(timeout);
		};
	}, [isStreaming]);

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

	// Start a new Pi session file (discrete Main Chat sessions)
	const newSession = useCallback(async () => {
		try {
			const newState = await newMainChatPiSessionFile();
			setState(newState);
			streamingMessageRef.current = null;
			setIsStreaming(false);
			setMessages([]);

			// Mark this session as just-created so the effect doesn't try to resume it
			const newSessionId = newState.session_id ?? null;
			if (newSessionId) {
				justCreatedSessionRef.current = newSessionId;
			}

			// Tell the UI to select the new session immediately.
			onSelectedSessionIdChange?.(newSessionId);

			// Reconnect WebSocket to the new session after a brief delay
			// to allow the backend to be ready
			disconnectRef.current?.(true);
			setTimeout(() => {
				connectRef.current?.();
			}, 100);
		} catch (e) {
			const err =
				e instanceof Error ? e : new Error("Failed to start new session");
			setError(err);
			onError?.(err);
		}
	}, [onError, onSelectedSessionIdChange]);

	// Reset session - restarts Pi process to reload PERSONALITY.md and USER.md
	const resetSession = useCallback(async () => {
		try {
			// Force disconnect WebSocket first
			disconnect(true);

			// Reset the session (this restarts the Pi process)
			const newState = await resetMainChatPiSession();
			setState(newState);
			wsCache.sessionStarted = true;
			setMessages([]);
			streamingMessageRef.current = null;
			setIsStreaming(false);

			// Tell UI selection to follow the new backend session id.
			onSelectedSessionIdChange?.(newState.session_id ?? null);

			// Reconnect WebSocket
			connect();
		} catch (e) {
			const err = e instanceof Error ? e : new Error("Failed to reset session");
			setError(err);
			onError?.(err);
		}
	}, [connect, disconnect, onError, onSelectedSessionIdChange]);

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

			// Refresh selected session in background
			refresh();
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
				wsCache.sessionStarted = true;

				// Connect WebSocket
				if (autoConnect) {
					connect();
				}

				// Load selected session messages in background (UI already has cached)
				if (activeSessionId) {
					getMainChatPiSessionMessages(activeSessionId)
						.then((sessionMessages) => {
							if (!mounted) return;
							const displayMessages =
								convertSessionMessagesToDisplay(sessionMessages);
							if (displayMessages.length > 0) {
								setMessages((previous) =>
									mergeServerMessages(previous, displayMessages),
								);
								writeCachedSessionMessages(activeSessionId, displayMessages);
							}
						})
						.catch(() => {
							// Ignore - we have cached data
						});
				}
			} catch (e) {
				if (!mounted) return;
				// Only show error if we have no cached data for this session
				if (!activeSessionId || readCachedSessionMessages(activeSessionId).length === 0) {
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
		activeSessionId,
		autoConnect,
		connect,
		convertSessionMessagesToDisplay,
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

"use client";

/**
 * Pi Chat hook using the multiplexed WebSocket manager.
 *
 * This hook provides the same external API as the legacy hook but uses the
 * multiplexed WebSocket connection via WsConnectionManager instead of
 * per-session WebSocket connections.
 *
 * Key differences from the legacy hook:
 * - Uses wsManager.subscribePiSession() for event subscription
 * - Manages session subscriptions explicitly (subscribe/unsubscribe commands)
 * - Single WebSocket connection shared across all Pi sessions
 */

import {
	createPiSessionId,
	isPendingSessionId,
	normalizeWorkspacePath,
} from "@/lib/session-utils";
import { getWsManager } from "@/lib/ws-manager";
import type { PiWsEvent, WsMuxConnectionState } from "@/lib/ws-mux-types";
import { useCallback, useEffect, useRef, useState } from "react";
import {
	readCachedSessionMessages,
	sanitizeStorageKey,
	transferCachedSessionMessages,
	writeCachedSessionMessages,
} from "./cache";
import { getMaxPiMessageId, normalizePiContentToParts, normalizePiMessages } from "./message-utils";
import type {
	PiDisplayMessage,
	PiMessagePart,
	PiSendMode,
	PiSendOptions,
	PiState,
	UsePiChatOptions,
	UsePiChatReturn,
} from "./types";

const BATCH_FLUSH_INTERVAL_MS = 50;

function isPiDebugEnabled(): boolean {
	if (!import.meta.env.DEV) return false;
	try {
		if (typeof localStorage !== "undefined") {
			return localStorage.getItem("debug:pi-v2") === "1";
		}
	} catch {
		// ignore
	}
	return import.meta.env.VITE_DEBUG_PI_V2 === "1";
}

/**
 * Hook for managing Pi chat using the multiplexed WebSocket.
 * Provides the same API as the legacy hook for easy migration.
 */
export function useChat(options: UsePiChatOptions = {}): UsePiChatReturn {
	const {
		autoConnect = true,
		workspacePath = null,
		storageKeyPrefix,
		selectedSessionId,
		onSelectedSessionIdChange,
		onMessageComplete,
		onError,
	} = options;

	const normalizedWorkspacePath = normalizeWorkspacePath(workspacePath);
	const resolvedStorageKeyPrefix =
		storageKeyPrefix ??
		`octo:workspacePi:v2:${sanitizeStorageKey(
			normalizedWorkspacePath ?? "unknown",
		)}`;

	const activeSessionId = selectedSessionId ?? null;
	const activeSessionIdRef = useRef(activeSessionId);
	activeSessionIdRef.current = activeSessionId;
	const lastActiveSessionIdRef = useRef<string | null>(null);

	// State
	const [state, setState] = useState<PiState | null>(null);
	const [messages, setMessages] = useState<PiDisplayMessage[]>(
		activeSessionId
			? readCachedSessionMessages(activeSessionId, resolvedStorageKeyPrefix)
			: [],
	);
	const [isConnected, setIsConnected] = useState(false);
	const [isStreaming, setIsStreaming] = useState(false);
	const [isAwaitingResponse, setIsAwaitingResponse] = useState(false);
	const [error, setError] = useState<Error | null>(null);

	// Refs
	const messageIdRef = useRef(getMaxPiMessageId(messages));
	const streamingMessageRef = useRef<PiDisplayMessage | null>(null);
	const lastAssistantMessageIdRef = useRef<string | null>(null);
	const unsubscribeRef = useRef<(() => void) | null>(null);
	const messagesRef = useRef(messages);
	const lastSessionRecoveryRef = useRef(0);

	// Batched update state
	const batchedUpdateRef = useRef({
		rafId: null as number | null,
		lastFlushTime: 0,
		pendingUpdate: false,
	});

	// Generate unique message ID
	const nextMessageId = useCallback(() => {
		messageIdRef.current += 1;
		return `pi-msg-${messageIdRef.current}`;
	}, []);

	const appendLocalAssistantMessage = useCallback(
		(content: string) => {
			const assistantMessage: PiDisplayMessage = {
				id: nextMessageId(),
				role: "assistant",
				parts: [{ type: "text", content }],
				timestamp: Date.now(),
			};
			setMessages((prev) => [...prev, assistantMessage]);
			lastAssistantMessageIdRef.current = assistantMessage.id;
			onMessageComplete?.(assistantMessage);
		},
		[nextMessageId, onMessageComplete],
	);

	const getSessionConfig = useCallback(() => {
		if (normalizedWorkspacePath) {
			return { scope: "workspace" as const, cwd: normalizedWorkspacePath };
		}
		return undefined;
	}, [normalizedWorkspacePath]);

	const appendPartToMessage = useCallback(
		(messageId: string, part: PiMessagePart) => {
			setMessages((prev) => {
				const idx = prev.findIndex((m) => m.id === messageId);
				if (idx < 0) return prev;
				const message = prev[idx];
				const updated = [...prev];
				updated[idx] = {
					...message,
					parts: [...message.parts, part],
				};
				return updated;
			});
		},
		[],
	);

	const ensureAssistantMessage = useCallback(
		(preferStreaming: boolean) => {
			if (streamingMessageRef.current) return streamingMessageRef.current;
			const lastId = lastAssistantMessageIdRef.current;
			if (lastId) {
				const existing = messagesRef.current.find((m) => m.id === lastId);
				if (existing) {
					return existing;
				}
			}
			const assistantMessage: PiDisplayMessage = {
				id: nextMessageId(),
				role: "assistant",
				parts: [],
				timestamp: Date.now(),
				isStreaming: preferStreaming,
			};
			if (preferStreaming) {
				streamingMessageRef.current = assistantMessage;
			}
			lastAssistantMessageIdRef.current = assistantMessage.id;
			setMessages((prev) => [...prev, assistantMessage]);
			return assistantMessage;
		},
		[nextMessageId],
	);

	// Flush batched streaming update
	const flushStreamingUpdate = useCallback(() => {
		const batch = batchedUpdateRef.current;
		batch.rafId = null;
		batch.pendingUpdate = false;

		const currentMsg = streamingMessageRef.current;
		if (!currentMsg) return;

		batch.lastFlushTime = Date.now();

		setMessages((prev) => {
			const idx = prev.findIndex((m) => m.id === currentMsg.id);
			if (idx >= 0) {
				const updated = [...prev];
				updated[idx] = {
					...currentMsg,
					parts: currentMsg.parts.map((p) => ({ ...p })),
				};
				return updated;
			}
			return prev;
		});
	}, []);

	// Schedule batched update
	const scheduleStreamingUpdate = useCallback(() => {
		const batch = batchedUpdateRef.current;
		batch.pendingUpdate = true;

		if (batch.rafId !== null) return;

		const elapsed = Date.now() - batch.lastFlushTime;
		if (elapsed >= BATCH_FLUSH_INTERVAL_MS) {
			batch.rafId = requestAnimationFrame(flushStreamingUpdate);
		} else {
			const delay = BATCH_FLUSH_INTERVAL_MS - elapsed;
			setTimeout(() => {
				if (batch.pendingUpdate && batch.rafId === null) {
					batch.rafId = requestAnimationFrame(flushStreamingUpdate);
				}
			}, delay);
		}
	}, [flushStreamingUpdate]);

	// Handle Pi WebSocket events
	const handlePiEvent = useCallback(
		(event: PiWsEvent) => {
			// Validate session_id
			if ("session_id" in event) {
				const activeId = activeSessionIdRef.current;
				if (activeId && event.session_id !== activeId) {
					if (isPiDebugEnabled()) {
						console.debug(
							`[useChat] Ignoring event for session ${event.session_id}, active is ${activeId}`,
						);
					}
					return;
				}
			}

			if (isPiDebugEnabled()) {
				console.debug("[useChat] Event:", event.type, event);
			}

			switch (event.type) {
				case "session_created": {
					// Session created/resumed - request messages to populate history
					const manager = getWsManager();
					manager.send({
						channel: "pi",
						type: "get_messages",
						session_id: event.session_id,
					});
					if (isPiDebugEnabled()) {
						console.debug(
							"[useChat] Session created, requesting messages:",
							event.session_id,
						);
					}
					break;
				}

				case "state": {
					const nextState = event.state as PiState;
					setState(nextState);
					if (nextState?.is_streaming === false) {
						setIsStreaming(false);
						setIsAwaitingResponse(false);
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
						lastAssistantMessageIdRef.current = assistantMessage.id;
						setMessages((prev) => [...prev, assistantMessage]);
					}
					setIsStreaming(true);
					setIsAwaitingResponse(false);
					break;
				}

				case "text": {
					const text = event.data;
					if (!text) break;
					const currentMsg = ensureAssistantMessage(true);
					const lastPart = currentMsg.parts[currentMsg.parts.length - 1];
					if (lastPart?.type === "text") {
						if (text === lastPart.content) {
							break;
						}
						if (text.startsWith(lastPart.content)) {
							lastPart.content = text;
						} else {
							lastPart.content += text;
						}
					} else {
						currentMsg.parts.push({ type: "text", content: text });
					}
					scheduleStreamingUpdate();
					setIsAwaitingResponse(false);
					break;
				}

				case "thinking": {
					const text = event.data;
					if (!text) break;
					const currentMsg = ensureAssistantMessage(true);
					const lastPart = currentMsg.parts[currentMsg.parts.length - 1];
					if (lastPart?.type === "thinking") {
						if (text === lastPart.content) {
							break;
						}
						if (text.startsWith(lastPart.content)) {
							lastPart.content = text;
						} else {
							lastPart.content += text;
						}
					} else {
						currentMsg.parts.push({ type: "thinking", content: text });
					}
					scheduleStreamingUpdate();
					setIsAwaitingResponse(false);
					break;
				}

				case "tool_use":
				case "tool_start": {
					const tool = event.data;
					const targetMessage = ensureAssistantMessage(true);
					const alreadyPresent = targetMessage.parts.some(
						(p) => p.type === "tool_use" && p.id === tool.id,
					);
					if (!alreadyPresent) {
						const part: PiMessagePart = {
							type: "tool_use",
							id: tool.id,
							name: tool.name,
							input: tool.input,
						};
						if (streamingMessageRef.current?.id === targetMessage.id) {
							targetMessage.parts.push(part);
							scheduleStreamingUpdate();
						} else {
							appendPartToMessage(targetMessage.id, part);
						}
					}
					setIsStreaming(true);
					setIsAwaitingResponse(false);
					break;
				}

				case "tool_result": {
					const result = event.data;
					const targetMessage = ensureAssistantMessage(false);
					const matchingToolUse = targetMessage.parts.find(
						(p) => p.type === "tool_use" && p.id === result.id,
					);
					const part: PiMessagePart = {
						type: "tool_result",
						id: result.id,
						name:
							result.name ||
							(matchingToolUse?.type === "tool_use"
								? matchingToolUse.name
								: undefined),
						content: result.content,
						isError: result.is_error,
					};
					if (streamingMessageRef.current?.id === targetMessage.id) {
						targetMessage.parts.push(part);
						scheduleStreamingUpdate();
					} else {
						appendPartToMessage(targetMessage.id, part);
					}
					setIsStreaming(true);
					setIsAwaitingResponse(false);
					break;
				}

				case "done": {
					// Cancel pending batched update
					const batch = batchedUpdateRef.current;
					if (batch.rafId !== null) {
						cancelAnimationFrame(batch.rafId);
						batch.rafId = null;
					}
					batch.pendingUpdate = false;

					if (streamingMessageRef.current) {
						streamingMessageRef.current.isStreaming = false;
						const completedMessage = {
							...streamingMessageRef.current,
							parts: streamingMessageRef.current.parts.map((p) => ({ ...p })),
						};

						setMessages((prev) => {
							const idx = prev.findIndex(
								(m) => m.id === completedMessage.id,
							);
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
					setIsAwaitingResponse(false);
					break;
				}

				case "error": {
					const errMsg = event.error || "Unknown error";
					const err = new Error(errMsg);
					setError(err);
					onError?.(err);
					setIsStreaming(false);
					setIsAwaitingResponse(false);
					const sessionId = activeSessionIdRef.current;
					const now = Date.now();
					const shouldRecover =
						Boolean(sessionId) &&
						(errMsg.includes("PiSessionNotFound") ||
							errMsg.includes("SessionNotFound") ||
							errMsg.includes("Response channel closed"));
					if (shouldRecover && now - lastSessionRecoveryRef.current > 5000) {
						lastSessionRecoveryRef.current = now;
						const manager = getWsManager();
						manager.piCreateSession(sessionId as string, getSessionConfig());
						setTimeout(() => {
							manager.piGetState(sessionId as string);
							manager.send({
								channel: "pi",
								type: "get_messages",
								session_id: sessionId as string,
							});
						}, 250);
					}

					if (streamingMessageRef.current) {
						streamingMessageRef.current.isStreaming = false;
						streamingMessageRef.current.parts.push({
							type: "error",
							content: errMsg,
						});
						const completedMessage = {
							...streamingMessageRef.current,
							parts: streamingMessageRef.current.parts.map((p) => ({ ...p })),
						};
						setMessages((prev) => {
							const idx = prev.findIndex(
								(m) => m.id === completedMessage.id,
							);
							if (idx >= 0) {
								const updated = [...prev];
								updated[idx] = completedMessage;
								return updated;
							}
							return prev;
						});
						onMessageComplete?.(completedMessage);
						streamingMessageRef.current = null;
					} else {
						const errorMessage: PiDisplayMessage = {
							id: nextMessageId(),
							role: "assistant",
							parts: [{ type: "error", content: errMsg }],
							timestamp: Date.now(),
							isStreaming: false,
						};
						setMessages((prev) => [...prev, errorMessage]);
						onMessageComplete?.(errorMessage);
					}
					break;
				}

				case "persisted": {
					if (isPiDebugEnabled()) {
						console.debug(
							"[useChat] Persisted:",
							event.session_id,
							event.message_count,
						);
					}
					break;
				}

				case "model_changed": {
					// Refresh state to get updated model info
					const manager = getWsManager();
					manager.piGetState(event.session_id);
					if (isPiDebugEnabled()) {
						console.debug(
							"[useChat] Model changed:",
							event.session_id,
							"provider" in event ? event.provider : "",
							"model_id" in event ? event.model_id : "",
						);
					}
					break;
				}

				case "messages": {
					// Load messages from server response
					if ("messages" in event && Array.isArray(event.messages)) {
						// Use normalizePiMessages which properly handles toolResult role messages
						// by merging tool results with their corresponding tool_use parts
						const displayMessages = normalizePiMessages(
							event.messages,
							`server-${event.session_id}`,
						);

						if (displayMessages.length > 0) {
							setMessages(displayMessages);
							messageIdRef.current = getMaxPiMessageId(displayMessages);
							const lastAssistant = [...displayMessages]
								.reverse()
								.find((msg) => msg.role === "assistant");
							lastAssistantMessageIdRef.current = lastAssistant?.id ?? null;
						}

						if (isPiDebugEnabled()) {
							console.debug(
							"[useChat] Loaded messages:",
								event.session_id,
								displayMessages.length,
							);
						}
					}
					break;
				}
			}
		},
		[
			appendPartToMessage,
			ensureAssistantMessage,
			nextMessageId,
			scheduleStreamingUpdate,
			onMessageComplete,
			onError,
			getSessionConfig,
		],
	);

	// Connect to WebSocket manager
	const connect = useCallback(() => {
		const manager = getWsManager();
		manager.connect();
	}, []);

	// Disconnect from WebSocket manager
	const disconnect = useCallback(() => {
		// Unsubscribe from current session
		if (unsubscribeRef.current) {
			unsubscribeRef.current();
			unsubscribeRef.current = null;
		}
	}, []);

	const ensureSession = useCallback(async (): Promise<string> => {
		let sessionId = activeSessionIdRef.current;
		if (!sessionId) {
			sessionId = createPiSessionId();
			activeSessionIdRef.current = sessionId;
			onSelectedSessionIdChange?.(sessionId);
		}
		const manager = getWsManager();
		const sessionConfig = getSessionConfig();
		unsubscribeRef.current?.();
		unsubscribeRef.current = manager.subscribePiSession(
			sessionId,
			handlePiEvent,
			sessionConfig,
		);

		await manager.ensureConnected(4000);
		manager.piCreateSession(sessionId, sessionConfig);
		await manager.waitForPiSessionReady(sessionId, 4000);
		return sessionId;
	}, [getSessionConfig, handlePiEvent, onSelectedSessionIdChange]);

	// Send message
	const send = useCallback(
		async (message: string, options?: PiSendOptions) => {
			const mode: PiSendMode = options?.mode ?? "prompt";
			let sessionId = options?.sessionId ?? activeSessionIdRef.current;
			if (options?.sessionId && options.sessionId !== activeSessionIdRef.current) {
				activeSessionIdRef.current = options.sessionId;
				onSelectedSessionIdChange?.(options.sessionId);
				const manager = getWsManager();
				const sessionConfig = getSessionConfig();
				unsubscribeRef.current?.();
				unsubscribeRef.current = manager.subscribePiSession(
					options.sessionId,
					handlePiEvent,
					sessionConfig,
				);
			}
			if (!sessionId) {
				// Clear local state for the new session.
				setMessages([]);
				streamingMessageRef.current = null;
				setIsStreaming(false);
				setError(null);
				messageIdRef.current = 0;
				sessionId = await ensureSession();
			}

			// Add user message to display
			const userMessage: PiDisplayMessage = {
				id: nextMessageId(),
				role: "user",
				parts: [{ type: "text", content: message }],
				timestamp: Date.now(),
			};
			setMessages((prev) => [...prev, userMessage]);
			setError(null);
			setIsAwaitingResponse(true);

			const manager = getWsManager();
			try {
				await manager.ensureConnected(4000);
				await manager.waitForPiSessionReady(sessionId, 4000);
			} catch (err) {
				const error =
					err instanceof Error ? err : new Error("WebSocket not ready");
				setIsAwaitingResponse(false);
				setError(error);
				throw error;
			}

			switch (mode) {
				case "prompt":
					manager.piPrompt(sessionId, message);
					break;
				case "steer":
					manager.piPrompt(sessionId, message);
					break;
				case "follow_up":
					manager.piFollowUp(sessionId, message);
					break;
			}
		},
		[
			ensureSession,
			getSessionConfig,
			handlePiEvent,
			nextMessageId,
			onSelectedSessionIdChange,
		],
	);

	// Abort current stream
	const abort = useCallback(async () => {
		const sessionId = activeSessionIdRef.current;
		if (!sessionId) return;

		setIsAwaitingResponse(false);
		const manager = getWsManager();
		manager.piAbort(sessionId);
	}, []);

	// Compact session
	const compact = useCallback(async (customInstructions?: string) => {
		const sessionId = activeSessionIdRef.current;
		if (!sessionId) return;

		const manager = getWsManager();
		manager.piCompact(sessionId, customInstructions);
	}, []);

	// New session - creates a brand new session with a new UUID
	const newSession = useCallback(async () => {
		// Clear local state
		setMessages([]);
		streamingMessageRef.current = null;
		setIsStreaming(false);
		setIsAwaitingResponse(false);
		setError(null);
		messageIdRef.current = 0;
		await ensureSession();
	}, [ensureSession]);

	// Reset session - closes and recreates
	const resetSession = useCallback(async () => {
		const sessionId = activeSessionIdRef.current;
		if (!sessionId) {
			console.warn("[useChat] resetSession: no active session");
			return;
		}

		// Clear local state
		setMessages([]);
		streamingMessageRef.current = null;
		setIsStreaming(false);
		setIsAwaitingResponse(false);
		setError(null);
		messageIdRef.current = 0;

		// Close and recreate session
		const manager = getWsManager();
		manager.piCloseSession(sessionId);

		// Small delay then recreate
		setTimeout(() => {
			manager.piCreateSession(sessionId, getSessionConfig());
		}, 100);

		if (isPiDebugEnabled()) {
			console.debug("[useChat] resetSession for:", sessionId);
		}
	}, []);

	// Refresh - request current state from backend
	const refresh = useCallback(async () => {
		const sessionId = activeSessionIdRef.current;
		if (!sessionId) return;

		const manager = getWsManager();
		manager.piGetState(sessionId);

		// Also request messages
		manager.send({
			channel: "pi",
			type: "get_messages",
			session_id: sessionId,
		});

		if (isPiDebugEnabled()) {
			console.debug("[useChat] refresh requested for:", sessionId);
		}
	}, []);

	// Subscribe to connection state
	useEffect(() => {
		const manager = getWsManager();

		const unsubscribe = manager.onConnectionState(
			(connectionState: WsMuxConnectionState) => {
				setIsConnected(connectionState === "connected");
			},
		);

		return unsubscribe;
	}, []);

	// Subscribe to Pi session when active session changes
	useEffect(() => {
		// Unsubscribe from previous session
		if (unsubscribeRef.current) {
			unsubscribeRef.current();
			unsubscribeRef.current = null;
		}

		if (!activeSessionId) {
			return;
		}
		// If we just transitioned from a pending ID to a real session ID,
		// migrate cached messages so the first message doesn't disappear.
		const previousId = lastActiveSessionIdRef.current;
		if (
			previousId &&
			previousId !== activeSessionId &&
			isPendingSessionId(previousId) &&
			!isPendingSessionId(activeSessionId)
		) {
			const existing = readCachedSessionMessages(
				activeSessionId,
				resolvedStorageKeyPrefix,
			);
			if (existing.length === 0) {
				transferCachedSessionMessages(
					previousId,
					activeSessionId,
					resolvedStorageKeyPrefix,
				);
			}
		}
		// Load cached messages for this session
		const cached = readCachedSessionMessages(
			activeSessionId,
			resolvedStorageKeyPrefix,
		);
		if (cached.length > 0) {
			setMessages(cached);
			messageIdRef.current = getMaxPiMessageId(cached);
			const lastAssistant = [...cached]
				.reverse()
				.find((msg) => msg.role === "assistant");
			lastAssistantMessageIdRef.current = lastAssistant?.id ?? null;
		} else {
			setMessages([]);
			messageIdRef.current = 0;
			lastAssistantMessageIdRef.current = null;
		}
		streamingMessageRef.current = null;
		setIsStreaming(false);
		setIsAwaitingResponse(false);
		setError(null);

		// Subscribe to the new session (passes scope/cwd for session creation)
		const manager = getWsManager();
		const sessionConfig = getSessionConfig();
		unsubscribeRef.current = manager.subscribePiSession(
			activeSessionId,
			handlePiEvent,
			sessionConfig,
		);

		if (isPiDebugEnabled()) {
			console.debug("[useChat] Subscribed to session:", activeSessionId, "workspacePath:", workspacePath);
		}
		lastActiveSessionIdRef.current = activeSessionId;

		return () => {
			if (unsubscribeRef.current) {
				unsubscribeRef.current();
				unsubscribeRef.current = null;
			}
		};
	}, [activeSessionId, resolvedStorageKeyPrefix, handlePiEvent, getSessionConfig]);

	// Auto-connect on mount
	useEffect(() => {
		if (autoConnect && activeSessionId) {
			connect();
		}
	}, [autoConnect, activeSessionId, connect]);

	useEffect(() => {
		messagesRef.current = messages;
	}, [messages]);

	useEffect(() => {
		if (!activeSessionId) return;
		// Persist messages for instant session restore.
		// Use throttled writes during streaming, force write on idle.
		writeCachedSessionMessages(
			activeSessionId,
			messages,
			resolvedStorageKeyPrefix,
			!isStreaming,
		);
	}, [activeSessionId, isStreaming, messages, resolvedStorageKeyPrefix]);

	return {
		state,
		messages,
		isConnected,
		isStreaming,
		isAwaitingResponse,
		error,
		send,
		appendLocalAssistantMessage,
		abort,
		compact,
		newSession,
		resetSession,
		refresh,
		connect,
		disconnect,
	};
}

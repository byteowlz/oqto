"use client";

/**
 * Streaming hook for Pi chat.
 * Handles WebSocket connection, message streaming, and batched UI updates.
 */

import {
	createMainChatPiWebSocket,
	createWorkspacePiWebSocket,
} from "@/features/main-chat/api";
import type { PiState } from "@/lib/control-plane-client";
import {
	type MutableRefObject,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";
import {
	isPendingSessionId,
	notifyConnectionStateChange,
	subscribeToConnectionState,
	wsCache,
} from "./cache";
import type {
	BatchedUpdateState,
	PiDisplayMessage,
	PiStreamEvent,
} from "./types";

const BATCH_FLUSH_INTERVAL_MS = 50; // Flush UI updates at most every 50ms

export type UsePiChatStreamingOptions = {
	scope: "main" | "workspace";
	workspacePath: string | null;
	activeSessionIdRef: MutableRefObject<string | null>;
	sessionSelectedAtRef: MutableRefObject<number | null>;
	nextMessageId: () => string;
	onStateChange: (state: PiState) => void;
	onMessageStart: (message: PiDisplayMessage) => void;
	onMessageUpdate: (message: PiDisplayMessage) => void;
	onMessageComplete: (message: PiDisplayMessage) => void;
	onCompaction: () => void;
	onError: (error: Error) => void;
};

export type UsePiChatStreamingReturn = {
	isConnected: boolean;
	isStreaming: boolean;
	streamingMessageRef: MutableRefObject<PiDisplayMessage | null>;
	connect: () => void;
	disconnect: (force?: boolean) => void;
	setIsStreaming: (streaming: boolean) => void;
};

export function usePiChatStreaming({
	scope,
	workspacePath,
	activeSessionIdRef,
	sessionSelectedAtRef,
	nextMessageId,
	onStateChange,
	onMessageStart,
	onMessageUpdate,
	onMessageComplete,
	onCompaction,
	onError,
}: UsePiChatStreamingOptions): UsePiChatStreamingReturn {
	const [isConnected, setIsConnected] = useState(wsCache.isConnected);
	const [isStreaming, setIsStreaming] = useState(false);

	// Track if this hook instance owns the WebSocket
	const isOwnerRef = useRef(false);
	const streamingMessageRef = useRef<PiDisplayMessage | null>(null);

	// Batched update state for streaming - reduces per-token React re-renders
	const batchedUpdateRef = useRef<BatchedUpdateState>({
		rafId: null,
		lastFlushTime: 0,
		pendingUpdate: false,
	});

	// Flush batched streaming message updates to React state
	const flushStreamingUpdate = useCallback(() => {
		const batch = batchedUpdateRef.current;
		batch.rafId = null;
		batch.pendingUpdate = false;

		const currentMsg = streamingMessageRef.current;
		if (!currentMsg) return;

		batch.lastFlushTime = Date.now();
		onMessageUpdate(currentMsg);
	}, [onMessageUpdate]);

	// Schedule a batched update - coalesces rapid token updates
	const scheduleStreamingUpdate = useCallback(() => {
		const batch = batchedUpdateRef.current;
		batch.pendingUpdate = true;

		// If RAF already scheduled, let it handle the update
		if (batch.rafId !== null) return;

		const elapsed = Date.now() - batch.lastFlushTime;
		if (elapsed >= BATCH_FLUSH_INTERVAL_MS) {
			// Enough time has passed, flush immediately via RAF
			batch.rafId = requestAnimationFrame(flushStreamingUpdate);
		} else {
			// Schedule flush after remaining interval
			const delay = BATCH_FLUSH_INTERVAL_MS - elapsed;
			setTimeout(() => {
				if (batch.pendingUpdate && batch.rafId === null) {
					batch.rafId = requestAnimationFrame(flushStreamingUpdate);
				}
			}, delay);
		}
	}, [flushStreamingUpdate]);

	// Handle incoming WebSocket messages
	const handleWsMessage = useCallback(
		(event: MessageEvent) => {
			try {
				const data = JSON.parse(event.data) as PiStreamEvent;

				// Validate session_id to prevent messages from wrong session leaking through
				// Skip validation for 'connected' events which establish the session
				if (data.type !== "connected" && data.session_id !== undefined) {
					const activeId = activeSessionIdRef.current;
					if (activeId && data.session_id !== activeId) {
						// Message belongs to a different session - ignore it
						console.debug(
							`[usePiChatStreaming] Ignoring message for session ${data.session_id}, active session is ${activeId}`,
						);
						return;
					}
				}

				switch (data.type) {
					case "connected":
						setIsConnected(true);
						break;

					case "state": {
						const nextState = data.data as PiState;
						onStateChange(nextState);
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
							onMessageStart(assistantMessage);
						}
						setIsStreaming(true);
						break;
					}

					case "text": {
						// Append text to streaming message (mutate ref, batch UI updates)
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
							// Schedule batched update instead of per-token setState
							scheduleStreamingUpdate();
						}
						break;
					}

					case "thinking": {
						const text = data.data as string;
						const currentThinkingMsg = streamingMessageRef.current;
						if (currentThinkingMsg && text) {
							const lastPart =
								currentThinkingMsg.parts[currentThinkingMsg.parts.length - 1];
							if (lastPart?.type === "thinking") {
								lastPart.content += text;
							} else {
								currentThinkingMsg.parts.push({
									type: "thinking",
									content: text,
								});
							}
							scheduleStreamingUpdate();
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
							// Tool events are less frequent, flush immediately via RAF
							scheduleStreamingUpdate();
						}
						break;
					}

					case "tool_start": {
						const tool = data.data as {
							id: string;
							name: string;
							input: unknown;
						};
						const currentToolMsg = streamingMessageRef.current;
						if (currentToolMsg) {
							const alreadyPresent = currentToolMsg.parts.some(
								(p) => p.type === "tool_use" && p.id === tool.id,
							);
							if (!alreadyPresent) {
								currentToolMsg.parts.push({
									type: "tool_use",
									id: tool.id,
									name: tool.name,
									input: tool.input,
								});
								scheduleStreamingUpdate();
							}
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
							// Check if there's a matching tool_use to associate the name with
							const matchingToolUse = currentResultMsg.parts.find(
								(p) => p.type === "tool_use" && p.id === result.id,
							);
							currentResultMsg.parts.push({
								type: "tool_result",
								id: result.id,
								// Use the tool name from the matching tool_use if available
								name:
									result.name ||
									(matchingToolUse?.type === "tool_use"
										? matchingToolUse.name
										: undefined),
								content: result.content,
								isError: result.isError,
							});
							// Tool events are less frequent, flush immediately via RAF
							scheduleStreamingUpdate();
						}
						break;
					}

					case "done": {
						// Cancel any pending batched update
						const batch = batchedUpdateRef.current;
						if (batch.rafId !== null) {
							cancelAnimationFrame(batch.rafId);
							batch.rafId = null;
						}
						batch.pendingUpdate = false;

						// Mark message as complete
						if (streamingMessageRef.current) {
							streamingMessageRef.current.isStreaming = false;
							const completedMessage = {
								...streamingMessageRef.current,
								parts: streamingMessageRef.current.parts.map((p) => ({ ...p })),
							};
							onMessageComplete(completedMessage);
							streamingMessageRef.current = null;
						}
						setIsStreaming(false);
						break;
					}

					case "error": {
						const errMsg =
							typeof data.data === "string" ? data.data : "Unknown error";
						const err = new Error(errMsg);
						onError(err);
						setIsStreaming(false);
						if (streamingMessageRef.current) {
							streamingMessageRef.current.isStreaming = false;
							streamingMessageRef.current = null;
						}
						break;
					}

					case "compaction":
						onCompaction();
						break;
				}
			} catch (e) {
				console.error("Failed to parse Pi WebSocket message:", e);
			}
		},
		// Note: activeSessionIdRef is intentionally read dynamically via .current
		// to get the latest session ID at message time, not at callback creation time
		[
			activeSessionIdRef,
			nextMessageId,
			onCompaction,
			onError,
			onMessageComplete,
			onMessageStart,
			onStateChange,
			scheduleStreamingUpdate,
		],
	);

	// Finalize streaming message on error/close
	const finalizeStreamingMessage = useCallback(
		(setMessagesCallback: (completed: PiDisplayMessage) => void) => {
			if (streamingMessageRef.current) {
				const completedMessage = {
					...streamingMessageRef.current,
					isStreaming: false,
					parts: streamingMessageRef.current.parts.map((p) => ({ ...p })),
				};
				setMessagesCallback(completedMessage);
				streamingMessageRef.current = null;
			}
			setIsStreaming(false);
		},
		[],
	);

	// Connect to WebSocket - uses global cache to survive remounts
	const connect = useCallback(() => {
		if (
			scope === "workspace" &&
			(!activeSessionIdRef.current ||
				isPendingSessionId(activeSessionIdRef.current))
		) {
			return;
		}
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

		const ws =
			scope === "workspace"
				? createWorkspacePiWebSocket(
						workspacePath ?? "global",
						activeSessionIdRef.current ?? "",
					)
				: createMainChatPiWebSocket();
		wsCache.ws = ws;
		isOwnerRef.current = true;

		ws.onopen = () => {
			notifyConnectionStateChange(true);
		};

		ws.onmessage = handleWsMessage;

		ws.onerror = () => {
			// Suppress connection errors that occur shortly after session selection
			// This handles race conditions where WebSocket connects before the backend
			// is fully ready (e.g., after creating a new session)
			const now = Date.now();
			const sessionSelectedAt = sessionSelectedAtRef.current;
			if (sessionSelectedAt && now - sessionSelectedAt < 3000) {
				// Error occurred within 3 seconds of session selection - likely spurious
				console.debug(
					`[usePiChatStreaming] Suppressing WebSocket error that occurred ${now - sessionSelectedAt}ms after session selection`,
				);
				return;
			}

			const err = new Error("WebSocket connection error");
			onError(err);

			finalizeStreamingMessage(onMessageComplete);
		};

		ws.onclose = () => {
			notifyConnectionStateChange(false);

			finalizeStreamingMessage(onMessageComplete);

			// Clear the global ws reference on close
			if (wsCache.ws === ws) {
				wsCache.ws = null;
				wsCache.sessionStarted = false;
			}
		};
	}, [
		activeSessionIdRef,
		finalizeStreamingMessage,
		handleWsMessage,
		onError,
		onMessageComplete,
		scope,
		sessionSelectedAtRef,
		workspacePath,
	]);

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

	// Subscribe to global connection state changes
	useEffect(() => {
		return subscribeToConnectionState(setIsConnected);
	}, []);

	// Keep WebSocket handler in sync when callback changes
	useEffect(() => {
		if (isOwnerRef.current && wsCache.ws?.readyState === WebSocket.OPEN) {
			wsCache.ws.onmessage = handleWsMessage;
		}
	}, [handleWsMessage]);

	return {
		isConnected,
		isStreaming,
		streamingMessageRef,
		connect,
		disconnect,
		setIsStreaming,
	};
}

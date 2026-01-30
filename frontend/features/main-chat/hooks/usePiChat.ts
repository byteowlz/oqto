"use client";

/**
 * Main composition hook for Pi chat.
 * Combines streaming, history, and core hooks into a unified interface.
 */

import type { PiState } from "@/lib/control-plane-client";
import { useCallback, useRef, useState } from "react";
import {
	readCachedSessionMessages,
	sanitizeStorageKey,
	wsCache,
} from "./cache";
import { getMaxPiMessageId } from "./message-utils";
import type {
	PiDisplayMessage,
	UsePiChatOptions,
	UsePiChatReturn,
} from "./types";
import {
	usePiChatCore,
	usePiChatInit,
	usePiChatSessionEffects,
	usePiChatStreamingFallback,
} from "./usePiChatCore";
import { usePiChatHistory, usePiChatHistoryEffects } from "./usePiChatHistory";
import { usePiChatStreaming } from "./usePiChatStreaming";

// We keep only per-session message caches; state is fetched from backend.
function getCachedState(): PiState | null {
	return null;
}

/**
 * Hook for managing Pi chat in Main Chat mode.
 * Handles WebSocket connection, message streaming, and state.
 */
export function usePiChat(options: UsePiChatOptions = {}): UsePiChatReturn {
	const {
		autoConnect = true,
		scope = "main",
		workspacePath = null,
		storageKeyPrefix,
		selectedSessionId,
		onSelectedSessionIdChange,
		onMessageComplete,
		onError,
	} = options;

	const resolvedStorageKeyPrefix =
		storageKeyPrefix ??
		(scope === "main"
			? "octo:mainChatPi"
			: `octo:workspacePi:${sanitizeStorageKey(workspacePath ?? "global")}`);

	const activeSessionId = selectedSessionId ?? null;
	const activeSessionIdRef = useRef(activeSessionId);
	activeSessionIdRef.current = activeSessionId;

	// Track when a session was selected to suppress spurious connection errors
	const sessionSelectedAtRef = useRef<number | null>(null);

	// Initialize with cached data for INSTANT display
	const [state, setState] = useState<PiState | null>(getCachedState);
	const [messages, setMessages] = useState<PiDisplayMessage[]>(
		activeSessionId
			? readCachedSessionMessages(activeSessionId, resolvedStorageKeyPrefix)
			: [],
	);
	// Assume connected if we have cached data - optimistic
	const [isConnected, setIsConnected] = useState(
		wsCache.isConnected || messages.length > 0,
	);
	const [error, setError] = useState<Error | null>(null);

	// Message ID counter
	const messageIdRef = useRef(getMaxPiMessageId(messages));

	// Generate unique message ID
	const nextMessageId = useCallback(() => {
		messageIdRef.current += 1;
		return `pi-msg-${messageIdRef.current}`;
	}, []);

	// History hook (refresh, cache management)
	const historyHook = usePiChatHistory({
		scope,
		workspacePath,
		storageKeyPrefix: resolvedStorageKeyPrefix,
		activeSessionIdRef,
		isStreaming: false, // Will be updated by streaming hook
		streamingMessageRef: { current: null }, // Placeholder, will be set by streaming hook
		setMessages,
		setState,
		setIsStreaming: () => {}, // Placeholder
	});

	// Streaming hook callbacks
	const handleStateChange = useCallback((nextState: PiState) => {
		setState(nextState);
	}, []);

	const handleMessageStart = useCallback((message: PiDisplayMessage) => {
		setMessages((prev) => [...prev, message]);
	}, []);

	const handleMessageUpdate = useCallback((message: PiDisplayMessage) => {
		setMessages((prev) => {
			const idx = prev.findIndex((m) => m.id === message.id);
			if (idx >= 0) {
				const updated = [...prev];
				updated[idx] = {
					...message,
					parts: message.parts.map((p) => ({ ...p })),
				};
				return updated;
			}
			return prev;
		});
	}, []);

	const handleStreamingMessageComplete = useCallback(
		(message: PiDisplayMessage) => {
			setMessages((prev) => {
				const idx = prev.findIndex((m) => m.id === message.id);
				if (idx >= 0) {
					const updated = [...prev];
					updated[idx] = message;
					return updated;
				}
				return prev;
			});
			onMessageComplete?.(message);
		},
		[onMessageComplete],
	);

	const handleCompaction = useCallback(() => {
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
		// Also refresh to sync with server state
		historyHook.refreshRef.current?.();
	}, [historyHook.refreshRef]);

	const handleError = useCallback(
		(err: Error) => {
			setError(err);
			onError?.(err);
		},
		[onError],
	);

	// Streaming hook
	const streamingHook = usePiChatStreaming({
		scope,
		workspacePath,
		activeSessionIdRef,
		sessionSelectedAtRef,
		nextMessageId,
		onStateChange: handleStateChange,
		onMessageStart: handleMessageStart,
		onMessageUpdate: handleMessageUpdate,
		onMessageComplete: handleStreamingMessageComplete,
		onCompaction: handleCompaction,
		onError: handleError,
	});

	// Core hook (send, abort, session management)
	const coreHook = usePiChatCore({
		scope,
		workspacePath,
		storageKeyPrefix: resolvedStorageKeyPrefix,
		activeSessionIdRef,
		isStreaming: streamingHook.isStreaming,
		streamingMessageRef: streamingHook.streamingMessageRef,
		nextMessageId,
		setMessages,
		setState,
		setIsStreaming: streamingHook.setIsStreaming,
		setError,
		connect: streamingHook.connect,
		disconnect: streamingHook.disconnect,
		refreshRef: historyHook.refreshRef,
		onSelectedSessionIdChange,
		onError,
	});

	// Re-create history hook with proper streaming refs
	const { refresh } = usePiChatHistory({
		scope,
		workspacePath,
		storageKeyPrefix: resolvedStorageKeyPrefix,
		activeSessionIdRef,
		isStreaming: streamingHook.isStreaming,
		streamingMessageRef: streamingHook.streamingMessageRef,
		setMessages,
		setState,
		setIsStreaming: streamingHook.setIsStreaming,
	});

	// Update the refresh ref for core hook
	historyHook.refreshRef.current = refresh;

	// Session selection effects
	usePiChatSessionEffects({
		scope,
		workspacePath,
		storageKeyPrefix: resolvedStorageKeyPrefix,
		activeSessionId,
		activeSessionIdRef,
		sessionSelectedAtRef,
		resumeInFlightRef: coreHook.resumeInFlightRef,
		justCreatedSessionRef: coreHook.justCreatedSessionRef,
		streamingMessageRef: streamingHook.streamingMessageRef,
		connectRef: coreHook.connectRef,
		disconnectRef: coreHook.disconnectRef,
		refreshRef: historyHook.refreshRef,
		setMessages,
		setIsStreaming: streamingHook.setIsStreaming,
		setError,
	});

	// History effects (cache sync, periodic refresh)
	usePiChatHistoryEffects({
		activeSessionId,
		isConnected: streamingHook.isConnected,
		isStreaming: streamingHook.isStreaming,
		messages,
		storageKeyPrefix: resolvedStorageKeyPrefix,
		refreshRef: historyHook.refreshRef,
		messageIdRef,
	});

	// Streaming fallback (poll backend when streaming gets stuck)
	usePiChatStreamingFallback({
		scope,
		workspacePath,
		activeSessionIdRef,
		isStreaming: streamingHook.isStreaming,
		streamingMessageRef: streamingHook.streamingMessageRef,
		setState,
		setIsStreaming: streamingHook.setIsStreaming,
		setMessages,
	});

	// Initialization effect
	usePiChatInit({
		scope,
		workspacePath,
		storageKeyPrefix: resolvedStorageKeyPrefix,
		activeSessionId,
		activeSessionIdRef,
		autoConnect,
		connect: streamingHook.connect,
		handleWsMessage: () => {}, // The streaming hook manages its own message handler
		refresh,
		setMessages,
		setState,
		setIsConnected,
		setError,
		onError,
	});

	return {
		state,
		messages,
		isConnected: streamingHook.isConnected,
		isStreaming: streamingHook.isStreaming,
		error,
		send: coreHook.send,
		abort: coreHook.abort,
		compact: coreHook.compact,
		newSession: coreHook.newSession,
		resetSession: coreHook.resetSession,
		refresh,
		connect: streamingHook.connect,
		disconnect: streamingHook.disconnect,
	};
}

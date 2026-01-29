"use client";

/**
 * History hook for Pi chat.
 * Handles loading, refreshing, and caching session messages.
 */

import {
	type PiState,
	getMainChatPiSessionMessages,
	getMainChatPiState,
	getWorkspacePiSessionMessages,
	getWorkspacePiState,
} from "@/features/main-chat/api";
import type { MutableRefObject } from "react";
import { useCallback, useEffect, useRef } from "react";
import {
	isPendingSessionId,
	readCachedSessionMessages,
	writeCachedSessionMessages,
} from "./cache";
import {
	convertSessionMessagesToDisplay,
	getMaxPiMessageId,
	mergeServerMessages,
} from "./message-utils";
import type { PiDisplayMessage } from "./types";

export type UsePiChatHistoryOptions = {
	scope: "main" | "workspace";
	workspacePath: string | null;
	storageKeyPrefix: string;
	activeSessionIdRef: MutableRefObject<string | null>;
	isStreaming: boolean;
	streamingMessageRef: MutableRefObject<PiDisplayMessage | null>;
	setMessages: React.Dispatch<React.SetStateAction<PiDisplayMessage[]>>;
	setState: React.Dispatch<React.SetStateAction<PiState | null>>;
	setIsStreaming: React.Dispatch<React.SetStateAction<boolean>>;
};

export type UsePiChatHistoryReturn = {
	refresh: () => Promise<void>;
	refreshRef: MutableRefObject<(() => Promise<void>) | null>;
	messageIdRef: MutableRefObject<number>;
	loadCachedMessages: (sessionId: string) => PiDisplayMessage[];
};

export function usePiChatHistory({
	scope,
	workspacePath,
	storageKeyPrefix,
	activeSessionIdRef,
	isStreaming,
	streamingMessageRef,
	setMessages,
	setState,
	setIsStreaming,
}: UsePiChatHistoryOptions): UsePiChatHistoryReturn {
	const refreshRef = useRef<(() => Promise<void>) | null>(null);
	const messageIdRef = useRef(0);

	// Load cached messages for a session
	const loadCachedMessages = useCallback(
		(sessionId: string): PiDisplayMessage[] => {
			const cached = readCachedSessionMessages(sessionId, storageKeyPrefix);
			// Update messageIdRef to avoid collisions
			const maxId = getMaxPiMessageId(cached);
			if (maxId > messageIdRef.current) {
				messageIdRef.current = maxId;
			}
			return cached;
		},
		[storageKeyPrefix],
	);

	// Refresh messages and state from server (background, non-blocking)
	const refresh = useCallback(async () => {
		// Don't refresh while streaming - can cause race conditions with local messages
		if (isStreaming) {
			return;
		}
		const targetSessionId = activeSessionIdRef.current;
		if (!targetSessionId) {
			return;
		}
		if (scope === "workspace" && isPendingSessionId(targetSessionId)) {
			return;
		}
		try {
			const [piState, sessionMessages] = await Promise.all([
				scope === "workspace"
					? getWorkspacePiState(workspacePath ?? "global", targetSessionId)
					: getMainChatPiState(),
				scope === "workspace"
					? getWorkspacePiSessionMessages(
							workspacePath ?? "global",
							targetSessionId,
						)
					: getMainChatPiSessionMessages(targetSessionId),
			]);

			// Check if session changed during async fetch - discard stale response
			if (activeSessionIdRef.current !== targetSessionId) {
				return;
			}

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
			writeCachedSessionMessages(
				targetSessionId,
				displayMessages,
				storageKeyPrefix,
			);
		} catch (e) {
			// Don't show errors for background refresh - we have cached data
			console.warn("Background refresh failed:", e);
		}
	}, [
		activeSessionIdRef,
		isStreaming,
		scope,
		setIsStreaming,
		setMessages,
		setState,
		storageKeyPrefix,
		streamingMessageRef,
		workspacePath,
	]);

	// Keep ref in sync so other hooks can use it
	useEffect(() => {
		refreshRef.current = refresh;
	}, [refresh]);

	// Ensure messageIdRef never collides with cached/loaded messages
	useEffect(() => {
		// This effect runs when messages change to keep ID counter in sync
		// Actual implementation deferred to the main hook where messages state lives
	}, []);

	return {
		refresh,
		refreshRef,
		messageIdRef,
		loadCachedMessages,
	};
}

export type UsePiChatHistoryEffectsOptions = {
	activeSessionId: string | null;
	isConnected: boolean;
	isStreaming: boolean;
	messages: PiDisplayMessage[];
	storageKeyPrefix: string;
	refreshRef: MutableRefObject<(() => Promise<void>) | null>;
	messageIdRef: MutableRefObject<number>;
};

/**
 * History-related effects for Pi chat.
 * Handles periodic refresh, cache sync, and streaming state fallback.
 */
export function usePiChatHistoryEffects({
	activeSessionId,
	isConnected,
	isStreaming,
	messages,
	storageKeyPrefix,
	refreshRef,
	messageIdRef,
}: UsePiChatHistoryEffectsOptions) {
	// Keep per-session cache in sync when messages change (throttled during streaming)
	useEffect(() => {
		if (!activeSessionId) return;
		if (messages.length > 0) {
			// During streaming, writes are throttled; on completion they're forced
			writeCachedSessionMessages(
				activeSessionId,
				messages,
				storageKeyPrefix,
				!isStreaming,
			);
		}
	}, [activeSessionId, messages, isStreaming, storageKeyPrefix]);

	// Periodic refresh when idle - catches missed WebSocket events and handles
	// the case where messages appear empty until reload
	useEffect(() => {
		if (!activeSessionId || !isConnected || isStreaming) return;

		let cancelled = false;
		const targetSessionId = activeSessionId;

		// Initial refresh after a short delay (handles page load with existing session)
		const initialTimeout = setTimeout(() => {
			if (cancelled) return;
			refreshRef.current?.();
		}, 500);

		// Periodic refresh every 15 seconds as a safety net
		const interval = setInterval(() => {
			if (cancelled) return;
			refreshRef.current?.();
		}, 15000);

		return () => {
			cancelled = true;
			clearTimeout(initialTimeout);
			clearInterval(interval);
		};
	}, [activeSessionId, isConnected, isStreaming, refreshRef]);

	// Ensure nextMessageId never collides with cached/loaded messages
	useEffect(() => {
		const maxId = getMaxPiMessageId(messages);
		if (maxId > messageIdRef.current) {
			messageIdRef.current = maxId;
		}
	}, [messages, messageIdRef]);
}

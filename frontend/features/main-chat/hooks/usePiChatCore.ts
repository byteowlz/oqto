"use client";

/**
 * Core hook for Pi chat.
 * Handles message sending, abort, session management, and compaction.
 */

import {
	type PiState,
	abortMainChatPi,
	abortWorkspacePiSession,
	getMainChatPiSessionMessages,
	getMainChatPiState,
	getWorkspacePiSessionMessages,
	getWorkspacePiState,
	newMainChatPiSessionFile,
	newWorkspacePiSession,
	resetMainChatPiSession,
	resumeMainChatPiSession,
	resumeWorkspacePiSession,
	startMainChatPiSession,
} from "@/features/main-chat/api";
import type { MutableRefObject } from "react";
import { useCallback, useEffect, useRef } from "react";
import {
	clearCachedSessionMessages,
	isPendingSessionId,
	readCachedSessionMessages,
	writeCachedSessionMessages,
	wsCache,
} from "./cache";
import {
	convertSessionMessagesToDisplay,
	mergeServerMessages,
} from "./message-utils";
import type { PiDisplayMessage, PiSendMode, PiSendOptions } from "./types";

export type UsePiChatCoreOptions = {
	scope: "main" | "workspace";
	workspacePath: string | null;
	storageKeyPrefix: string;
	activeSessionIdRef: MutableRefObject<string | null>;
	isStreaming: boolean;
	streamingMessageRef: MutableRefObject<PiDisplayMessage | null>;
	nextMessageId: () => string;
	setMessages: React.Dispatch<React.SetStateAction<PiDisplayMessage[]>>;
	setState: React.Dispatch<React.SetStateAction<PiState | null>>;
	setIsStreaming: React.Dispatch<React.SetStateAction<boolean>>;
	setError: React.Dispatch<React.SetStateAction<Error | null>>;
	connect: () => void;
	disconnect: (force?: boolean) => void;
	refreshRef: MutableRefObject<(() => Promise<void>) | null>;
	onSelectedSessionIdChange?: (id: string | null) => void;
	onError?: (error: Error) => void;
};

export type UsePiChatCoreReturn = {
	send: (message: string, options?: PiSendOptions) => Promise<void>;
	abort: () => Promise<void>;
	compact: (customInstructions?: string) => Promise<void>;
	newSession: () => Promise<void>;
	resetSession: () => Promise<void>;
	connectRef: MutableRefObject<(() => void) | null>;
	disconnectRef: MutableRefObject<((force?: boolean) => void) | null>;
	resumeInFlightRef: MutableRefObject<string | null>;
	justCreatedSessionRef: MutableRefObject<string | null>;
};

export function usePiChatCore({
	scope,
	workspacePath,
	storageKeyPrefix,
	activeSessionIdRef,
	isStreaming,
	streamingMessageRef,
	nextMessageId,
	setMessages,
	setState,
	setIsStreaming,
	setError,
	connect,
	disconnect,
	refreshRef,
	onSelectedSessionIdChange,
	onError,
}: UsePiChatCoreOptions): UsePiChatCoreReturn {
	const connectRef = useRef<(() => void) | null>(null);
	const disconnectRef = useRef<((force?: boolean) => void) | null>(null);
	const resumeInFlightRef = useRef<string | null>(null);
	const justCreatedSessionRef = useRef<string | null>(null);

	// Keep refs in sync
	useEffect(() => {
		connectRef.current = connect;
	}, [connect]);

	useEffect(() => {
		disconnectRef.current = disconnect;
	}, [disconnect]);

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
		[
			isStreaming,
			nextMessageId,
			onError,
			setError,
			setIsStreaming,
			setMessages,
			streamingMessageRef,
		],
	);

	// Abort current stream
	const abort = useCallback(async () => {
		try {
			if (scope === "workspace") {
				const targetSessionId = activeSessionIdRef.current;
				if (!targetSessionId) return;
				await abortWorkspacePiSession(
					workspacePath ?? "global",
					targetSessionId,
				);
			} else {
				await abortMainChatPi();
			}
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
	}, [
		activeSessionIdRef,
		onError,
		scope,
		setError,
		setIsStreaming,
		streamingMessageRef,
		workspacePath,
	]);

	// Compact session context
	const compact = useCallback(
		async (customInstructions?: string) => {
			if (!wsCache.ws || wsCache.ws.readyState !== WebSocket.OPEN) {
				const err = new Error("Not connected to chat server");
				setError(err);
				onError?.(err);
				return;
			}
			try {
				wsCache.ws.send(
					JSON.stringify({
						type: "compact",
						custom_instructions: customInstructions ?? null,
					}),
				);
			} catch (e) {
				const err = e instanceof Error ? e : new Error("Failed to compact");
				setError(err);
				onError?.(err);
			}
		},
		[onError, setError],
	);

	// Start a new Pi session file (discrete Main Chat sessions)
	const newSession = useCallback(async () => {
		try {
			const newState =
				scope === "workspace"
					? await newWorkspacePiSession(workspacePath ?? "global")
					: await newMainChatPiSessionFile();
			setState(newState);
			streamingMessageRef.current = null;
			setIsStreaming(false);
			setMessages([]);

			// Mark this session as just-created so the effect doesn't try to resume it
			const newSessionId = newState.session_id ?? null;
			if (newSessionId) {
				justCreatedSessionRef.current = newSessionId;
				clearCachedSessionMessages(newSessionId, storageKeyPrefix);
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
	}, [
		onError,
		onSelectedSessionIdChange,
		scope,
		setError,
		setIsStreaming,
		setMessages,
		setState,
		storageKeyPrefix,
		streamingMessageRef,
		workspacePath,
	]);

	// Reset session - restarts Pi process to reload PERSONALITY.md and USER.md
	const resetSession = useCallback(async () => {
		try {
			// Force disconnect WebSocket first
			disconnect(true);

			// Reset the session (this restarts the Pi process)
			const newState =
				scope === "workspace"
					? await newWorkspacePiSession(workspacePath ?? "global")
					: await resetMainChatPiSession();
			setState(newState);
			wsCache.sessionStarted = true;
			setMessages([]);
			streamingMessageRef.current = null;
			setIsStreaming(false);

			// Tell UI selection to follow the new backend session id.
			const newSessionId = newState.session_id ?? null;
			if (newSessionId) {
				clearCachedSessionMessages(newSessionId, storageKeyPrefix);
			}
			onSelectedSessionIdChange?.(newSessionId);

			// Reconnect WebSocket
			connect();
		} catch (e) {
			const err = e instanceof Error ? e : new Error("Failed to reset session");
			setError(err);
			onError?.(err);
		}
	}, [
		connect,
		disconnect,
		onError,
		onSelectedSessionIdChange,
		scope,
		setError,
		setIsStreaming,
		setMessages,
		setState,
		storageKeyPrefix,
		streamingMessageRef,
		workspacePath,
	]);

	return {
		send,
		abort,
		compact,
		newSession,
		resetSession,
		connectRef,
		disconnectRef,
		resumeInFlightRef,
		justCreatedSessionRef,
	};
}

export type UsePiChatSessionEffectsOptions = {
	scope: "main" | "workspace";
	workspacePath: string | null;
	storageKeyPrefix: string;
	activeSessionId: string | null;
	activeSessionIdRef: MutableRefObject<string | null>;
	sessionSelectedAtRef: MutableRefObject<number | null>;
	resumeInFlightRef: MutableRefObject<string | null>;
	justCreatedSessionRef: MutableRefObject<string | null>;
	streamingMessageRef: MutableRefObject<PiDisplayMessage | null>;
	connectRef: MutableRefObject<(() => void) | null>;
	disconnectRef: MutableRefObject<((force?: boolean) => void) | null>;
	refreshRef: MutableRefObject<(() => Promise<void>) | null>;
	setMessages: React.Dispatch<React.SetStateAction<PiDisplayMessage[]>>;
	setIsStreaming: React.Dispatch<React.SetStateAction<boolean>>;
	setError: React.Dispatch<React.SetStateAction<Error | null>>;
};

/**
 * Session selection effects for Pi chat.
 * Handles resuming sessions when selection changes.
 */
export function usePiChatSessionEffects({
	scope,
	workspacePath,
	storageKeyPrefix,
	activeSessionId,
	activeSessionIdRef,
	sessionSelectedAtRef,
	resumeInFlightRef,
	justCreatedSessionRef,
	streamingMessageRef,
	connectRef,
	disconnectRef,
	refreshRef,
	setMessages,
	setIsStreaming,
	setError,
}: UsePiChatSessionEffectsOptions) {
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
			sessionSelectedAtRef.current = Date.now(); // Track creation time for error suppression
			// WebSocket reconnection is handled by newSession() itself
			return;
		}

		// Skip resume attempts for optimistic placeholder sessions
		if (activeSessionId.startsWith("pending-")) {
			return;
		}

		sessionSelectedAtRef.current = Date.now(); // Track selection time for error suppression
		setMessages(readCachedSessionMessages(activeSessionId, storageKeyPrefix));
		streamingMessageRef.current = null;
		setIsStreaming(false);
		setError(null); // Clear any previous errors when switching sessions

		// Resume selected session in background, then reconnect WebSocket.
		if (resumeInFlightRef.current === activeSessionId) return;
		resumeInFlightRef.current = activeSessionId;
		const resumeSession = async () => {
			if (scope === "workspace") {
				if (!workspacePath) {
					throw new Error("Workspace path is required");
				}
				return resumeWorkspacePiSession(workspacePath, activeSessionId);
			}
			return resumeMainChatPiSession(activeSessionId);
		};
		resumeSession()
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
	}, [
		activeSessionId,
		connectRef,
		disconnectRef,
		justCreatedSessionRef,
		refreshRef,
		resumeInFlightRef,
		scope,
		sessionSelectedAtRef,
		setError,
		setIsStreaming,
		setMessages,
		storageKeyPrefix,
		streamingMessageRef,
		workspacePath,
	]);
}

export type UsePiChatStreamingFallbackOptions = {
	scope: "main" | "workspace";
	workspacePath: string | null;
	activeSessionIdRef: MutableRefObject<string | null>;
	isStreaming: boolean;
	streamingMessageRef: MutableRefObject<PiDisplayMessage | null>;
	setState: React.Dispatch<React.SetStateAction<PiState | null>>;
	setIsStreaming: React.Dispatch<React.SetStateAction<boolean>>;
	setMessages: React.Dispatch<React.SetStateAction<PiDisplayMessage[]>>;
};

/**
 * Fallback polling effect for when streaming state gets stuck.
 * Polls backend state to clear streaming UI if backend has finished.
 */
export function usePiChatStreamingFallback({
	scope,
	workspacePath,
	activeSessionIdRef,
	isStreaming,
	streamingMessageRef,
	setState,
	setIsStreaming,
	setMessages,
}: UsePiChatStreamingFallbackOptions) {
	useEffect(() => {
		if (!isStreaming) return;
		let cancelled = false;

		const checkStreamingState = async () => {
			try {
				const targetSessionId = activeSessionIdRef.current;
				if (
					scope === "workspace" &&
					(!targetSessionId || isPendingSessionId(targetSessionId))
				) {
					return;
				}
				const piState =
					scope === "workspace"
						? await getWorkspacePiState(
								workspacePath ?? "global",
								targetSessionId ?? "",
							)
						: await getMainChatPiState();
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
	}, [
		activeSessionIdRef,
		isStreaming,
		scope,
		setIsStreaming,
		setMessages,
		setState,
		streamingMessageRef,
		workspacePath,
	]);
}

export type UsePiChatInitOptions = {
	scope: "main" | "workspace";
	workspacePath: string | null;
	storageKeyPrefix: string;
	activeSessionId: string | null;
	activeSessionIdRef: MutableRefObject<string | null>;
	autoConnect: boolean;
	connect: () => void;
	handleWsMessage: (event: MessageEvent) => void;
	refresh: () => Promise<void>;
	setMessages: React.Dispatch<React.SetStateAction<PiDisplayMessage[]>>;
	setState: React.Dispatch<React.SetStateAction<PiState | null>>;
	setIsConnected: React.Dispatch<React.SetStateAction<boolean>>;
	setError: React.Dispatch<React.SetStateAction<Error | null>>;
	onError?: (error: Error) => void;
};

/**
 * Initialize on mount - INSTANT with cached data, background refresh.
 */
export function usePiChatInit({
	scope,
	workspacePath,
	storageKeyPrefix,
	activeSessionId,
	activeSessionIdRef,
	autoConnect,
	connect,
	handleWsMessage,
	refresh,
	setMessages,
	setState,
	setIsConnected,
	setError,
	onError,
}: UsePiChatInitOptions) {
	const initStartedRef = useRef(false);
	const isOwnerRef = useRef(false);

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
				const currentSessionId = activeSessionIdRef.current;
				const shouldResumeWorkspace =
					scope === "workspace" &&
					!!currentSessionId &&
					!isPendingSessionId(currentSessionId);
				const piState =
					scope === "workspace"
						? shouldResumeWorkspace
							? await resumeWorkspacePiSession(
									workspacePath ?? "global",
									currentSessionId ?? "",
								)
							: null
						: await startMainChatPiSession();
				if (!mounted) return;

				if (piState) {
					setState(piState);
					wsCache.sessionStarted = true;
				}

				// Connect WebSocket
				if (
					autoConnect &&
					(scope !== "workspace" ||
						(currentSessionId && !isPendingSessionId(currentSessionId)))
				) {
					connect();
				}

				// Load selected session messages in background (UI already has cached)
				if (activeSessionId && !isPendingSessionId(activeSessionId)) {
					const sessionMessages =
						scope === "workspace"
							? await getWorkspacePiSessionMessages(
									workspacePath ?? "global",
									activeSessionId,
								)
							: await getMainChatPiSessionMessages(activeSessionId);
					if (!mounted) return;
					const displayMessages =
						convertSessionMessagesToDisplay(sessionMessages);
					if (displayMessages.length > 0) {
						setMessages((previous) =>
							mergeServerMessages(previous, displayMessages),
						);
						writeCachedSessionMessages(
							activeSessionId,
							displayMessages,
							storageKeyPrefix,
						);
					}
				}
			} catch (e) {
				if (!mounted) return;
				// Only show error if we have no cached data for this session
				if (
					!activeSessionId ||
					readCachedSessionMessages(activeSessionId, storageKeyPrefix)
						.length === 0
				) {
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
		activeSessionIdRef,
		autoConnect,
		connect,
		handleWsMessage,
		onError,
		refresh,
		scope,
		setError,
		setIsConnected,
		setMessages,
		setState,
		storageKeyPrefix,
		workspacePath,
	]);
}

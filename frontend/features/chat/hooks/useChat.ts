"use client";

/**
 * Pi Chat hook using the multiplexed WebSocket manager.
 *
 * This hook provides the same external API as the legacy hook but uses the
 * multiplexed WebSocket connection via WsConnectionManager instead of
 * per-session WebSocket connections.
 *
 * Key differences from the legacy hook:
 * - Uses wsManager.subscribeAgentSession() for event subscription
 * - Uses canonical protocol (agent channel) for all communication
 * - Single WebSocket connection shared across all agent sessions
 */

import { useBusySessions } from "@/components/contexts";
import {
	clearSharedWorkspaceSessionId,
	getRunnerHistoryAlias,
	setSharedWorkspaceSessionId,
	sharedWorkspaceSessionMap,
} from "@/components/contexts/chat-context";
import { getChatMessages, triggerChatHistoryBackfill } from "@/lib/api/chat";
import type { CommandResponse, SessionConfig } from "@/lib/canonical-types";
import {
	createPiSessionId,
	getWorkspaceModelStorageKey,
	isPendingSessionId,
	normalizeWorkspacePath,
} from "@/lib/session-utils";
import {
	type StreamingThrottle,
	createStreamingThrottle,
} from "@/lib/streaming-throttle";
import { getWsManager } from "@/lib/ws-manager";
import type { AgentWsEvent, WsMuxConnectionState } from "@/lib/ws-mux-types";
import { useCallback, useEffect, useRef, useState } from "react";
import {
	readCachedSessionMessages,
	sanitizeStorageKey,
	transferCachedSessionMessages,
	writeCachedSessionMessages,
} from "./cache";
import {
	beginMessageSync,
	bindIdentity,
	completeMessageSync,
	createInitialChatStateMachine,
	deriveUiFlags,
	resetIdentity,
	transitionTransport,
	transitionTurn,
} from "./chat-state-machine";
import {
	convertCanonicalMessageToDisplay,
	mergeServerMessages,
	nextPartId,
	normalizeContentToParts,
	normalizeMessages,
} from "./message-utils";
import type {
	AgentState,
	DisplayMessage,
	DisplayPart,
	ErrorPart,
	PromptQueueItem,
	RawMessage,
	SendMode,
	SendOptions,
	UseChatOptions,
	UseChatReturn,
} from "./types";

const BATCH_FLUSH_INTERVAL_MS = 50;

// Streaming delta coalescing interval. During fast streaming, text_delta and
// thinking_delta events arrive much faster than React can usefully re-render.
// We coalesce intermediate accumulated snapshots and emit at this cadence.
// Inspired by pi-mobile's UiUpdateThrottler. We use a slightly higher
// cadence to reduce visible layout twitch during very fast token bursts.
const TEXT_DELTA_THROTTLE_MS = 100;

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

function createTempMessageId(): string {
	if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
		return `tmp:${crypto.randomUUID()}`;
	}
	return `tmp:${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/**
 * Hook for managing Pi chat using the multiplexed WebSocket.
 * Provides the same API as the legacy hook for easy migration.
 */
export function useChat(options: UseChatOptions = {}): UseChatReturn {
	const {
		autoConnect = true,
		workspacePath = null,
		storageKeyPrefix,
		selectedSessionId,
		senderName,
		onSelectedSessionIdChange,
		onMessageComplete,
		onError,
		onTitleChanged,
	} = options;

	const normalizedWorkspacePath = normalizeWorkspacePath(workspacePath);
	const resolvedStorageKeyPrefix =
		storageKeyPrefix ??
		`oqto:workspacePi:v2:${sanitizeStorageKey(
			normalizedWorkspacePath ?? "unknown",
		)}`;

	const activeSessionId = selectedSessionId ?? null;
	const activeSessionIdRef = useRef(activeSessionId);
	activeSessionIdRef.current = activeSessionId;
	const lastActiveSessionIdRef = useRef<string | null>(null);

	// State
	const [state, setState] = useState<AgentState | null>(null);
	const [messages, setMessages] = useState<DisplayMessage[]>(
		activeSessionId
			? readCachedSessionMessages(activeSessionId, resolvedStorageKeyPrefix)
			: [],
	);
	const [isConnected, setIsConnected] = useState(false);
	const [isStreaming, setIsStreaming] = useState(false);
	const [isAwaitingResponse, setIsAwaitingResponse] = useState(false);
	const [error, setError] = useState<Error | null>(null);
	const [promptQueue, setPromptQueue] = useState<PromptQueueItem[]>([]);
	const [historyHydrated, setHistoryHydrated] = useState(
		!activeSessionId || messages.length > 0,
	);
	const [historyLoading, setHistoryLoading] = useState(
		Boolean(activeSessionId && messages.length === 0),
	);
	const { setSessionBusy } = useBusySessions();

	// Refs
	const streamingMessageRef = useRef<DisplayMessage | null>(null);
	const lastAssistantMessageIdRef = useRef<string | null>(null);
	const unsubscribeRef = useRef<(() => void) | null>(null);
	const messagesRef = useRef(messages);
	const lastSessionRecoveryRef = useRef(0);
	const isStreamingRef = useRef(false);
	const lastAgentEventAtRef = useRef<number>(Date.now());
	const sendInFlightRef = useRef(false);
	const responseWatchdogRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const autoBackfillAttemptCountsRef = useRef<Map<string, number>>(new Map());
	const autoBackfillInFlightSessionsRef = useRef<Set<string>>(new Set());
	const persistedMessageVersionRef = useRef<number | null>(null);
	// (deferredServerMessagesRef removed — messages are always merged immediately)
	// Force a full server sync after reattaching to an active runner session.
	// Stable ref for the agent event handler so the subscription effect doesn't
	// re-run when callback identity changes (which would reset streaming state).
	const handleAgentEventRef = useRef<((event: AgentWsEvent) => void) | null>(
		null,
	);
	// Stable ref for onTitleChanged callback
	const onTitleChangedRef = useRef(onTitleChanged);
	onTitleChangedRef.current = onTitleChanged;

	// Batched update state
	const batchedUpdateRef = useRef({
		rafId: null as number | null,
		lastFlushTime: 0,
		pendingUpdate: false,
	});

	// Streaming delta throttle: coalesces high-frequency text/thinking deltas.
	// The throttle stores the full accumulated DisplayMessage snapshot and only
	// emits at TEXT_DELTA_THROTTLE_MS intervals. A flush timer ensures pending
	// coalesced updates are delivered even if no new delta arrives.
	const streamingThrottleRef = useRef<StreamingThrottle<DisplayMessage>>(
		createStreamingThrottle(TEXT_DELTA_THROTTLE_MS),
	);
	const throttleFlushTimerRef = useRef<ReturnType<typeof setInterval> | null>(
		null,
	);

	const machineRef = useRef(createInitialChatStateMachine(activeSessionId));
	const transportEpochRef = useRef(0);
	const reconnectTxnRef = useRef<Set<string>>(new Set());

	const applyTurnState = useCallback(
		(next: Parameters<typeof transitionTurn>[1]) => {
			const updated = transitionTurn(machineRef.current, next);
			machineRef.current = updated;
			const flags = deriveUiFlags(updated.turn);
			setIsStreaming(flags.isStreaming);
			setIsAwaitingResponse(flags.isAwaitingResponse);
			isStreamingRef.current = flags.isStreaming;
			if (!flags.isAwaitingResponse) {
				sendInFlightRef.current = false;
			}
		},
		[],
	);

	const bindSessionIdentity = useCallback(
		(payload: { runnerId: string; hstryId?: string; piId?: string }) => {
			machineRef.current = bindIdentity(machineRef.current, payload);
		},
		[],
	);

	const resetSessionIdentity = useCallback(
		(clientId: string | null) => {
			machineRef.current = resetIdentity(
				machineRef.current,
				clientId ?? "unbound-session",
			);
			applyTurnState({ kind: "idle" });
		},
		[applyTurnState],
	);

	const setBusyForEvent = useCallback(
		(sessionId: string | null | undefined, busy: boolean) => {
			if (!sessionId) return;
			setSessionBusy(sessionId, busy);
		},
		[setSessionBusy],
	);

	const appendLocalAssistantMessage = useCallback(
		(content: string) => {
			const assistantMessage: DisplayMessage = {
				id: createTempMessageId(),
				role: "assistant",
				parts: [{ type: "text", id: nextPartId(), text: content }],
				timestamp: Date.now(),
			};
			setMessages((prev) => [...prev, assistantMessage]);
			lastAssistantMessageIdRef.current = assistantMessage.id;
			onMessageComplete?.(assistantMessage);
		},
		[onMessageComplete],
	);

	const getSessionConfig = useCallback((): SessionConfig | undefined => {
		const config: SessionConfig = { harness: "pi" };
		if (normalizedWorkspacePath) {
			config.cwd = normalizedWorkspacePath;
		}
		try {
			const workspaceStorageKey = getWorkspaceModelStorageKey(
				normalizedWorkspacePath,
			);
			const sessionStorageKey = selectedSessionId
				? `oqto:chatModel:${selectedSessionId}`
				: null;
			const aliasSessionId = selectedSessionId
				? getRunnerHistoryAlias(selectedSessionId)
				: undefined;
			const aliasSessionStorageKey = aliasSessionId
				? `oqto:chatModel:${aliasSessionId}`
				: null;
			const storedModelRef = sessionStorageKey
				? (localStorage.getItem(sessionStorageKey) ??
					(aliasSessionStorageKey
						? localStorage.getItem(aliasSessionStorageKey)
						: null))
				: localStorage.getItem(workspaceStorageKey);
			if (storedModelRef) {
				const separatorIndex = storedModelRef.indexOf("/");
				if (separatorIndex > 0 && separatorIndex < storedModelRef.length - 1) {
					config.provider = storedModelRef.slice(0, separatorIndex);
					config.model = storedModelRef.slice(separatorIndex + 1);
				}
			}
		} catch {
			// ignore localStorage errors
		}
		return config;
	}, [normalizedWorkspacePath, selectedSessionId]);

	const applyServerMessages = useCallback(
		(
			rawMessages: RawMessage[] | unknown[],
			sessionId: string,
			serverVersion?: number,
			mode: "authoritative" | "partial" = "authoritative",
		) => {
			if (typeof serverVersion === "number") {
				persistedMessageVersionRef.current = Math.max(
					persistedMessageVersionRef.current ?? 0,
					serverVersion,
				);
			}
			if (!Array.isArray(rawMessages) || rawMessages.length === 0) return;
			let incoming = rawMessages as RawMessage[];
			// Partial/live snapshots can include transient user echoes while local
			// optimistic user messages are already rendered. Exclude user-role
			// rows here to avoid random live duplicates/reordering; authoritative
			// sync on agent.idle remains the source of truth.
			if (mode === "partial") {
				incoming = incoming.filter((m) => {
					const role = (m.role || "").toLowerCase();
					return role !== "user" && role !== "human";
				});
			}
			if (incoming.length === 0) return;
			machineRef.current = beginMessageSync(machineRef.current);
			const displayMessages = normalizeMessages(incoming, `srv-${sessionId}`);
			if (displayMessages.length === 0) {
				machineRef.current = completeMessageSync(machineRef.current);
				return;
			}
			setMessages((prev) => mergeServerMessages(prev, displayMessages, mode));
			const lastAssistant = [...displayMessages]
				.reverse()
				.find((msg) => msg.role === "assistant");
			lastAssistantMessageIdRef.current = lastAssistant?.id ?? null;
			machineRef.current = completeMessageSync(machineRef.current);
		},
		[],
	);

	const appendPartToMessage = useCallback(
		(messageId: string, part: DisplayPart) => {
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

	const snapshotMessagesForSessionSwitch = useCallback((): DisplayMessage[] => {
		const snapshot = messagesRef.current.map((m) => ({
			...m,
			parts: m.parts.map((p) => ({ ...p })),
		}));

		const throttled = streamingThrottleRef.current.flush();
		if (throttled) {
			const idx = snapshot.findIndex((m) => m.id === throttled.id);
			if (idx >= 0) {
				snapshot[idx] = {
					...snapshot[idx],
					parts: throttled.parts.map((p) => ({ ...p })),
				};
			}
		}

		const currentStreaming = streamingMessageRef.current;
		if (currentStreaming) {
			const streamSnapshot: DisplayMessage = {
				...currentStreaming,
				parts: currentStreaming.parts.map((p) => ({ ...p })),
			};
			const idx = snapshot.findIndex((m) => m.id === streamSnapshot.id);
			if (idx >= 0) {
				snapshot[idx] = streamSnapshot;
			} else {
				snapshot.push(streamSnapshot);
			}
		}

		return snapshot;
	}, []);

	const ensureAssistantMessage = useCallback((preferStreaming: boolean) => {
		if (streamingMessageRef.current) return streamingMessageRef.current;
		const lastId = lastAssistantMessageIdRef.current;
		if (lastId) {
			const existing = messagesRef.current.find((m) => m.id === lastId);
			if (existing && existing.role === "assistant") {
				return existing;
			}
		}
		const assistantMessage: DisplayMessage = {
			id: createTempMessageId(),
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
	}, []);

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

	// Schedule batched update.
	// Uses setTimeout instead of requestAnimationFrame so updates are not
	// stalled when the browser tab is in the background or the main thread
	// is busy with layout/paint.  rAF callbacks can be deferred indefinitely
	// by the browser, causing the streaming output to appear "stuck" and
	// then suddenly catch up in a burst.
	const scheduleStreamingUpdate = useCallback(() => {
		const batch = batchedUpdateRef.current;
		batch.pendingUpdate = true;

		if (batch.rafId !== null) return;

		const elapsed = Date.now() - batch.lastFlushTime;
		if (elapsed >= BATCH_FLUSH_INTERVAL_MS) {
			// Use a microtask-like delay (0ms setTimeout) for immediate flush
			batch.rafId = window.setTimeout(
				flushStreamingUpdate,
				0,
			) as unknown as number;
		} else {
			const delay = BATCH_FLUSH_INTERVAL_MS - elapsed;
			batch.rafId = window.setTimeout(() => {
				batch.rafId = null;
				if (batch.pendingUpdate) {
					flushStreamingUpdate();
				}
			}, delay) as unknown as number;
		}
	}, [flushStreamingUpdate]);

	/**
	 * Apply a coalesced streaming message snapshot to React state.
	 * Used by the throttle when it decides to emit.
	 */
	const applyThrottledSnapshot = useCallback((snapshot: DisplayMessage) => {
		setMessages((prev) => {
			const idx = prev.findIndex((m) => m.id === snapshot.id);
			if (idx >= 0) {
				const updated = [...prev];
				updated[idx] = {
					...snapshot,
					parts: snapshot.parts.map((p) => ({ ...p })),
				};
				return updated;
			}
			return prev;
		});
	}, []);

	/**
	 * Offer a streaming message to the throttle. If the throttle decides
	 * to emit immediately, apply to React state. Otherwise the periodic
	 * flush timer will pick it up.
	 */
	const throttledStreamingUpdate = useCallback(
		(currentMsg: DisplayMessage) => {
			const throttle = streamingThrottleRef.current;
			// Create a shallow snapshot for the throttle
			const snapshot = {
				...currentMsg,
				parts: currentMsg.parts.map((p) => ({ ...p })),
			};
			const immediate = throttle.offer(snapshot);
			if (immediate) {
				applyThrottledSnapshot(immediate);
			}
			// Ensure flush timer is running
			if (!throttleFlushTimerRef.current) {
				throttleFlushTimerRef.current = setInterval(() => {
					const ready = streamingThrottleRef.current.drainReady();
					if (ready) {
						applyThrottledSnapshot(ready);
					}
					// Stop timer when nothing is pending
					if (
						!streamingThrottleRef.current.hasPending() &&
						throttleFlushTimerRef.current
					) {
						clearInterval(throttleFlushTimerRef.current);
						throttleFlushTimerRef.current = null;
					}
				}, TEXT_DELTA_THROTTLE_MS);
			}
		},
		[applyThrottledSnapshot],
	);

	const fetchHistoryMessages = useCallback(
		async (sessionId: string, expectedVersion?: number) => {
			try {
				const swId = sharedWorkspaceSessionMap.get(sessionId);
				let history = await getChatMessages(sessionId, swId).catch(
					async (err) => {
						const alias = getRunnerHistoryAlias(sessionId);
						if (!alias || alias === sessionId) throw err;
						if (isPiDebugEnabled()) {
							console.debug(
								"[useChat] fetchHistoryMessages: retrying with runner alias",
								sessionId,
								"->",
								alias,
							);
						}
						return getChatMessages(alias, swId);
					},
				);
				if (history.length === 0) {
					const alias = getRunnerHistoryAlias(sessionId);
					if (alias && alias !== sessionId) {
						const aliasHistory = await getChatMessages(alias, swId);
						if (aliasHistory.length > 0) {
							history = aliasHistory;
						}
					}
				}

				// Guard: after the async fetch, verify the session is still
				// active. If the user switched sessions while the request was
				// in flight, discard the stale result to avoid clobbering the
				// new session's messages.
				if (activeSessionIdRef.current !== sessionId) {
					if (isPiDebugEnabled()) {
						console.debug(
							"[useChat] Discarding stale history fetch for",
							sessionId,
							"(active is now",
							`${activeSessionIdRef.current})`,
						);
					}
					return;
				}

				if (history.length === 0) {
					const backfillKey = swId ? `${swId}:${sessionId}` : sessionId;
					const attempts =
						autoBackfillAttemptCountsRef.current.get(backfillKey) ?? 0;
					const inFlight =
						autoBackfillInFlightSessionsRef.current.has(backfillKey);
					if (attempts < 2 && !inFlight) {
						autoBackfillAttemptCountsRef.current.set(backfillKey, attempts + 1);
						autoBackfillInFlightSessionsRef.current.add(backfillKey);
						void triggerChatHistoryBackfill({
							workspace: swId
								? undefined
								: (normalizedWorkspacePath ?? undefined),
							shared_workspace_id: swId,
							limit: 20_000,
						})
							.then(() => fetchHistoryMessages(sessionId, expectedVersion))
							.catch((err) => {
								if (isPiDebugEnabled()) {
									console.debug(
										"[useChat] automatic history backfill failed:",
										err,
									);
								}
							})
							.finally(() => {
								autoBackfillInFlightSessionsRef.current.delete(backfillKey);
							});
					}
					if (!isStreamingRef.current && !sendInFlightRef.current) {
						applyTurnState({ kind: "idle" });
					}
					return;
				}
				applyServerMessages(
					history as RawMessage[],
					sessionId,
					expectedVersion,
					isStreamingRef.current || sendInFlightRef.current
						? "partial"
						: "authoritative",
				);
				if (!isStreamingRef.current && !sendInFlightRef.current) {
					applyTurnState({ kind: "idle" });
				}
				if (isPiDebugEnabled()) {
					console.debug(
						"[useChat] Loaded history messages:",
						sessionId,
						history.length,
					);
				}
			} catch (err) {
				if (isPiDebugEnabled()) {
					console.debug("[useChat] Failed to load history:", err);
				}
			} finally {
				if (activeSessionIdRef.current === sessionId) {
					setHistoryHydrated(true);
					setHistoryLoading(false);
				}
			}
		},
		[applyServerMessages, applyTurnState, normalizedWorkspacePath],
	);

	const clearResponseWatchdog = useCallback(() => {
		if (responseWatchdogRef.current) {
			clearTimeout(responseWatchdogRef.current);
			responseWatchdogRef.current = null;
		}
	}, []);

	const armResponseWatchdog = useCallback(
		(sessionId: string) => {
			clearResponseWatchdog();
			responseWatchdogRef.current = setTimeout(() => {
				const manager = getWsManager();
				manager.agentGetState(sessionId);
				void fetchHistoryMessages(sessionId);
				applyTurnState({
					kind: "error",
					recoverable: true,
					message:
						"The agent did not respond in time. We recovered latest history. Please retry.",
				});
				setError(
					new Error(
						"The agent did not respond in time. We recovered latest history. Please retry.",
					),
				);
			}, 30000);
		},
		[applyTurnState, clearResponseWatchdog, fetchHistoryMessages],
	);

	const runReconnectReconcile = useCallback(
		async (
			sessionId: string,
			preferPartial: boolean,
			expectedEpoch: number,
		) => {
			if (reconnectTxnRef.current.has(sessionId)) {
				if (isPiDebugEnabled()) {
					console.debug(
						"[useChat] reconnect reconcile skipped: transaction already in flight",
						sessionId,
					);
				}
				return;
			}
			reconnectTxnRef.current.add(sessionId);
			const manager = getWsManager();
			try {
				if (
					transportEpochRef.current !== expectedEpoch ||
					activeSessionIdRef.current !== sessionId
				) {
					return;
				}
				manager.agentGetState(sessionId);
				if (preferPartial) {
					manager.agentGetMessages(sessionId);
				} else {
					await fetchHistoryMessages(sessionId);
				}
			} finally {
				reconnectTxnRef.current.delete(sessionId);
			}
		},
		[fetchHistoryMessages],
	);

	// ========================================================================
	// Canonical agent event handler
	// ========================================================================

	/**
	 * Handle canonical protocol events from the "agent" channel.
	 *
	 * These events are produced by PiTranslator on the backend and carry
	 * incremental deltas (not cumulative content like the old Pi events).
	 */
	const handleCanonicalEvent = useCallback(
		(event: AgentWsEvent) => {
			const eventType = event.event;

			if (isPiDebugEnabled()) {
				console.debug("[useChat] Canonical event:", eventType, event);
			}

			// Extra logging for debugging streaming issues
			const isStreaming =
				streamingMessageRef.current !== null || isStreamingRef.current;
			if (
				[
					"stream.message_start",
					"stream.text_delta",
					"stream.done",
					"tool.start",
					"tool.end",
					"agent.working",
					"agent.idle",
				].includes(eventType)
			) {
				console.log(
					`[useChat] Streaming event: ${eventType}, isStreaming=${isStreaming}, ref=${streamingMessageRef.current?.id}`,
				);
			}

			switch (eventType) {
				// -- Streaming lifecycle --
				case "stream.message_start": {
					clearResponseWatchdog();
					setBusyForEvent(event.session_id ?? activeSessionIdRef.current, true);
					// Only create a display message for assistant-role messages.
					// The backend sends message_start for every Pi message
					// including user echoes (steer) and tool-result messages
					// (role "user" or "tool"). Displaying those would duplicate
					// the user prompt or show raw tool output as text.
					const msgRole = event.role as string | undefined;
					const isAssistant =
						!msgRole || msgRole === "assistant" || msgRole === "agent";
					if (isAssistant && !streamingMessageRef.current) {
						const assistantMessage: DisplayMessage = {
							id: createTempMessageId(),
							role: "assistant",
							parts: [],
							timestamp: Date.now(),
							isStreaming: true,
						};
						streamingMessageRef.current = assistantMessage;
						lastAssistantMessageIdRef.current = assistantMessage.id;
						setMessages((prev) => [...prev, assistantMessage]);
					}
					applyTurnState({ kind: "streaming" });
					break;
				}

				// -- Text delta (incremental) --
				case "stream.text_delta": {
					const delta = event.delta as string | undefined;
					if (!delta) break;
					if (isPiDebugEnabled()) {
						console.debug(
							"[useChat] text_delta:",
							JSON.stringify(delta).slice(0, 60),
							"session:",
							event.session_id,
						);
					}
					const currentMsg = ensureAssistantMessage(true);
					const lastPart = currentMsg.parts[currentMsg.parts.length - 1];
					if (lastPart?.type === "text") {
						(lastPart as { text: string }).text += delta;
					} else {
						currentMsg.parts.push({
							type: "text",
							id: nextPartId(),
							text: delta,
						});
					}
					// Coalesce through throttle instead of scheduling every delta
					throttledStreamingUpdate(currentMsg);
					applyTurnState({ kind: "streaming" });
					break;
				}

				// -- Thinking delta (incremental) --
				case "stream.thinking_delta": {
					const delta = event.delta as string | undefined;
					if (!delta) break;
					const currentMsg = ensureAssistantMessage(true);
					const lastPart = currentMsg.parts[currentMsg.parts.length - 1];
					if (lastPart?.type === "thinking") {
						(lastPart as { text: string }).text += delta;
					} else {
						currentMsg.parts.push({
							type: "thinking",
							id: nextPartId(),
							text: delta,
						});
					}
					// Coalesce through throttle instead of scheduling every delta
					throttledStreamingUpdate(currentMsg);
					applyTurnState({ kind: "streaming" });
					break;
				}

				// -- Tool call being assembled by LLM --
				case "stream.tool_call_start": {
					const toolCallId =
						typeof event.tool_call_id === "string" ? event.tool_call_id : "";
					if (!toolCallId) {
						// Canonical protocol requires tool_call_id for lifecycle events.
						// Ignore malformed events to keep lifecycle reconciliation deterministic.
						break;
					}
					const name = event.name as string;
					const targetMessage = ensureAssistantMessage(true);
					const existingById = targetMessage.parts.find(
						(p) => p.type === "tool_call" && p.toolCallId === toolCallId,
					);
					if (existingById && existingById.type === "tool_call") {
						existingById.status = "running";
						existingById.name = name || existingById.name;
						scheduleStreamingUpdate();
					} else {
						const part: DisplayPart = {
							type: "tool_call",
							id: nextPartId(),
							toolCallId,
							name,
							input: undefined,
							status: "running",
						};
						if (streamingMessageRef.current?.id === targetMessage.id) {
							targetMessage.parts.push(part);
							scheduleStreamingUpdate();
						} else {
							appendPartToMessage(targetMessage.id, part);
						}
					}
					applyTurnState({ kind: "streaming" });
					break;
				}

				// -- Tool call finalized (LLM produced final input) --
				case "stream.tool_call_end": {
					const toolCall = event.tool_call as
						| { id: string; name: string; input: unknown }
						| undefined;
					if (!toolCall?.id) break;
					const targetMessage = ensureAssistantMessage(true);
					const existingById = targetMessage.parts.find(
						(p) => p.type === "tool_call" && p.toolCallId === toolCall.id,
					);
					if (existingById && existingById.type === "tool_call") {
						existingById.name = toolCall.name || existingById.name;
						if (toolCall.input !== undefined) {
							existingById.input = toolCall.input;
						}
						scheduleStreamingUpdate();
					} else {
						const part: DisplayPart = {
							type: "tool_call",
							id: nextPartId(),
							toolCallId: toolCall.id,
							name: toolCall.name,
							input: toolCall.input,
							status: "running",
						};
						if (streamingMessageRef.current?.id === targetMessage.id) {
							targetMessage.parts.push(part);
							scheduleStreamingUpdate();
						} else {
							appendPartToMessage(targetMessage.id, part);
						}
					}
					break;
				}

				// -- Tool execution started --
				case "tool.start": {
					const toolCallId =
						typeof event.tool_call_id === "string" ? event.tool_call_id : "";
					if (!toolCallId) {
						break;
					}
					const name = event.name as string;
					const input = event.input;
					// Ensure there's a tool_call part for this tool (in case we missed
					// stream.tool_call_* events, e.g. on reconnect)
					const targetMessage = ensureAssistantMessage(true);
					const existingById = targetMessage.parts.find(
						(p) => p.type === "tool_call" && p.toolCallId === toolCallId,
					);
					if (existingById && existingById.type === "tool_call") {
						if (input !== undefined) {
							existingById.input = input;
						}
						existingById.name = name || existingById.name;
						existingById.status = "running";
						scheduleStreamingUpdate();
					} else {
						const part: DisplayPart = {
							type: "tool_call",
							id: nextPartId(),
							toolCallId,
							name,
							input,
							status: "running",
						};
						if (streamingMessageRef.current?.id === targetMessage.id) {
							targetMessage.parts.push(part);
							scheduleStreamingUpdate();
						} else {
							appendPartToMessage(targetMessage.id, part);
						}
					}
					applyTurnState({ kind: "streaming" });
					break;
				}

				// -- Tool execution completed --
				case "tool.end": {
					const toolCallId =
						typeof event.tool_call_id === "string" ? event.tool_call_id : "";
					if (!toolCallId) {
						break;
					}
					const name = event.name as string;
					const output = event.output;
					const isError = event.is_error as boolean;
					const targetMessage = ensureAssistantMessage(false);
					const matchingToolCall = targetMessage.parts.find(
						(p) => p.type === "tool_call" && p.toolCallId === toolCallId,
					);
					if (matchingToolCall && matchingToolCall.type === "tool_call") {
						matchingToolCall.status = isError ? "error" : "success";
						matchingToolCall.name = name || matchingToolCall.name;
					}
					const duplicateResult = targetMessage.parts.find(
						(p) => p.type === "tool_result" && p.toolCallId === toolCallId,
					);
					if (duplicateResult && duplicateResult.type === "tool_result") {
						duplicateResult.output = output;
						duplicateResult.isError = isError;
						duplicateResult.name = name || duplicateResult.name;
						scheduleStreamingUpdate();
						applyTurnState({ kind: "streaming" });
						break;
					}
					const part: DisplayPart = {
						type: "tool_result",
						id: nextPartId(),
						toolCallId,
						name:
							name ||
							(matchingToolCall?.type === "tool_call"
								? matchingToolCall.name
								: undefined),
						output,
						isError,
					};
					if (streamingMessageRef.current?.id === targetMessage.id) {
						targetMessage.parts.push(part);
						scheduleStreamingUpdate();
					} else {
						appendPartToMessage(targetMessage.id, part);
					}
					applyTurnState({ kind: "streaming" });
					break;
				}

				// -- Stream complete --
				case "stream.done": {
					clearResponseWatchdog();
					setBusyForEvent(
						event.session_id ?? activeSessionIdRef.current,
						false,
					);
					// Flush any coalesced deltas from the throttle
					{
						const finalSnapshot = streamingThrottleRef.current.flush();
						if (finalSnapshot) {
							applyThrottledSnapshot(finalSnapshot);
						}
						streamingThrottleRef.current.reset();
						if (throttleFlushTimerRef.current) {
							clearInterval(throttleFlushTimerRef.current);
							throttleFlushTimerRef.current = null;
						}
					}
					// Cancel pending batched update
					const batch = batchedUpdateRef.current;
					if (batch.rafId !== null) {
						clearTimeout(batch.rafId);
						batch.rafId = null;
					}
					batch.pendingUpdate = false;

					if (streamingMessageRef.current) {
						const hasParts = streamingMessageRef.current.parts.length > 0;
						streamingMessageRef.current.isStreaming = false;
						if (hasParts) {
							const completedMessage = {
								...streamingMessageRef.current,
								parts: streamingMessageRef.current.parts.map((p) => ({
									...p,
								})),
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
						} else {
							const emptyId = streamingMessageRef.current.id;
							setMessages((prev) => prev.filter((m) => m.id !== emptyId));
						}
						streamingMessageRef.current = null;
					}
					// Clear streaming state. The Messages event from agent.end
					// now only contains assistant messages, so no deduplication
					// issues with the optimistic user message.
					isStreamingRef.current = false;
					applyTurnState({ kind: "idle" });
					break;
				}

				// -- Message complete (canonical full message) --
				case "stream.message_end": {
					// Flush any coalesced deltas before finalizing the message
					{
						const finalSnapshot = streamingThrottleRef.current.flush();
						if (finalSnapshot && streamingMessageRef.current) {
							// Apply the flushed content to the streaming message
							streamingMessageRef.current.parts = finalSnapshot.parts;
						}
						streamingThrottleRef.current.reset();
						if (throttleFlushTimerRef.current) {
							clearInterval(throttleFlushTimerRef.current);
							throttleFlushTimerRef.current = null;
						}
					}
					// If no streaming message exists, this message_end is for a
					// non-assistant message (user echo or tool result) that was
					// already skipped in message_start. Nothing to finalize.
					if (!streamingMessageRef.current) break;

					const fallbackId = streamingMessageRef.current.id;
					const canonical = convertCanonicalMessageToDisplay(
						event.message,
						fallbackId,
					);
					if (!canonical) break;

					// Secondary guard: skip user messages (steer echo) that
					// slipped through message_start (e.g. role field missing).
					if (canonical.role === "user") {
						if (streamingMessageRef.current.parts.length === 0) {
							const emptyId = streamingMessageRef.current.id;
							setMessages((prev) => prev.filter((m) => m.id !== emptyId));
						}
						streamingMessageRef.current = null;
						break;
					}

					const messageId = streamingMessageRef.current.id;

					// If we have a streaming message with accumulated parts from
					// text_delta/thinking_delta/tool events, preserve those parts
					// instead of replacing them with the canonical message's parts.
					// The canonical message from stream.message_end contains the
					// same content but in raw canonical format (raw tool_call JSON,
					// etc.) which would overwrite the nicely streamed content.
					// Only use the canonical message for metadata (usage, model).
					const hasStreamedParts = streamingMessageRef.current.parts.length > 0;
					const updated: DisplayMessage = hasStreamedParts
						? {
								...streamingMessageRef.current,
								id: messageId,
								role: "assistant",
								isStreaming: false,
								// Merge metadata from canonical message
								usage: canonical.usage ?? streamingMessageRef.current.usage,
							}
						: {
								...canonical,
								role: "assistant",
								id: messageId,
								isStreaming: false,
							};

					lastAssistantMessageIdRef.current = updated.id;
					setMessages((prev) => {
						const idx = prev.findIndex((m) => m.id === updated.id);
						if (idx >= 0) {
							const next = [...prev];
							next[idx] = updated;
							return next;
						}
						return [...prev, updated];
					});

					// Flush any pending batched update so it doesn't overwrite
					// the finalized message.
					const endBatch = batchedUpdateRef.current;
					if (endBatch.rafId !== null) {
						clearTimeout(endBatch.rafId);
						endBatch.rafId = null;
					}
					endBatch.pendingUpdate = false;

					// Finalize this message: call onMessageComplete and clear the
					// streaming ref so the next stream.message_start (for a
					// subsequent assistant turn) creates a new message.
					onMessageComplete?.(updated);
					streamingMessageRef.current = null;
					applyTurnState({ kind: "syncing" });
					break;
				}

				// -- Agent idle (streaming ended) --
				case "agent.idle": {
					clearResponseWatchdog();
					sendInFlightRef.current = false;
					setBusyForEvent(
						event.session_id ?? activeSessionIdRef.current,
						false,
					);
					// Flush any remaining coalesced deltas
					{
						const finalSnapshot = streamingThrottleRef.current.flush();
						if (finalSnapshot) {
							applyThrottledSnapshot(finalSnapshot);
						}
						streamingThrottleRef.current.reset();
						if (throttleFlushTimerRef.current) {
							clearInterval(throttleFlushTimerRef.current);
							throttleFlushTimerRef.current = null;
						}
					}
					applyTurnState({ kind: "syncing" });
					// Clear transient error banner (retry indicators).
					// Permanent errors are in hstry and render as messages.
					setError(null);
					if (streamingMessageRef.current) {
						streamingMessageRef.current.isStreaming = false;
						streamingMessageRef.current = null;
					}
					// Always fetch authoritative messages from hstry on
					// agent.idle. The backend persists to hstry BEFORE
					// broadcasting agent.idle, so the fetch is guaranteed
					// to return the complete conversation. This makes the
					// frontend self-healing: even if streaming events were
					// dropped (broadcast overflow, WebSocket disconnect),
					// the authoritative fetch repairs the local state.
					//
					// The small delay (150ms) avoids a visual flash when
					// the streaming-built state is replaced by the fetch.
					{
						const sessionId = activeSessionIdRef.current;
						if (sessionId) {
							const messageVersion =
								typeof event.message_version === "object" &&
								event.message_version !== null
									? Number(
											(event.message_version as { version?: number }).version,
										)
									: Number.NaN;
							const localVersion = persistedMessageVersionRef.current;
							const needsSync =
								!Number.isFinite(messageVersion) ||
								localVersion === null ||
								localVersion < messageVersion;
							setTimeout(() => {
								const manager = getWsManager();
								manager.agentGetState(sessionId);
								if (needsSync) {
									void fetchHistoryMessages(
										sessionId,
										Number.isFinite(messageVersion)
											? messageVersion
											: undefined,
									);
								} else {
									applyTurnState({ kind: "idle" });
								}
							}, 150);
						}
					}
					break;
				}

				// -- Agent working (streaming started) --
				case "agent.working": {
					setBusyForEvent(event.session_id ?? activeSessionIdRef.current, true);
					// If this turn was not initiated locally (no prior send->sending),
					// move idle -> sending so the working indicator is shown until
					// stream.message_start transitions to streaming.
					if (machineRef.current.turn.kind === "idle") {
						applyTurnState({ kind: "sending" });
					}
					// Keep isAwaitingResponse true — it will be cleared when
					// streaming actually starts (stream.message_start / text_delta)
					// or when the agent goes idle/error. Clearing it here causes
					// the working indicator to disappear between agent.working
					// and the first streaming event.
					break;
				}

				// -- Agent error --
				// -- Resync required (runner detected dropped events) --
				case "stream.resync_required": {
					const droppedCount = (event.dropped_count as number) ?? 0;
					const reason = (event.reason as string) ?? "unknown";
					console.warn(
						`[useChat] Resync required for session ${event.session_id}: dropped=${droppedCount} reason=${reason}`,
					);

					// Flush any in-progress streaming state
					{
						const finalSnapshot = streamingThrottleRef.current.flush();
						if (finalSnapshot) {
							applyThrottledSnapshot(finalSnapshot);
						}
						streamingThrottleRef.current.reset();
						if (throttleFlushTimerRef.current) {
							clearInterval(throttleFlushTimerRef.current);
							throttleFlushTimerRef.current = null;
						}
					}

					// Trigger resync: fetch fresh state + messages from the
					// runner to rebuild the timeline from scratch.
					const resyncSessionId =
						event.session_id ?? activeSessionIdRef.current;
					if (resyncSessionId) {
						applyTurnState({ kind: "syncing" });
						const manager = getWsManager();
						manager.agentGetState(resyncSessionId);
						void fetchHistoryMessages(resyncSessionId);
					}
					break;
				}

				case "agent.error": {
					const errMsg = (event.error as string) || "Unknown error";
					const recoverable = event.recoverable as boolean;
					const wasInFlight = sendInFlightRef.current;
					const isSessionNotFound =
						errMsg.includes("PiSessionNotFound") ||
						errMsg.includes("SessionNotFound") ||
						errMsg.includes("Response channel closed");

					if (!wasInFlight && isSessionNotFound) {
						// Background session lookup failure while idle (e.g. viewing history)
						// should not surface as a user-visible error.
						break;
					}

					if (recoverable) {
						// Retry-attempt failures are turn-local and transient.
						// Keep the assistant container alive and keep "working" state
						// so we don't flicker to an empty bubble between attempts.
						setBusyForEvent(
							event.session_id ?? activeSessionIdRef.current,
							true,
						);
						isStreamingRef.current = true;
						ensureAssistantMessage(true);
						applyTurnState({ kind: "streaming" });
						break;
					}

					// Terminal error path
					clearResponseWatchdog();
					sendInFlightRef.current = false;
					isStreamingRef.current = false;
					setBusyForEvent(
						event.session_id ?? activeSessionIdRef.current,
						false,
					);
					onError?.(new Error(errMsg));
					applyTurnState({
						kind: "error",
						recoverable: false,
						message: errMsg,
					});

					// Auto-recover for session-not-found errors
					const sessionId = activeSessionIdRef.current;
					const now = Date.now();
					const shouldRecover =
						Boolean(sessionId) && wasInFlight && isSessionNotFound;
					if (shouldRecover && now - lastSessionRecoveryRef.current > 5000) {
						lastSessionRecoveryRef.current = now;
						const manager = getWsManager();
						manager.agentCreateSession(sessionId as string, getSessionConfig());
						setTimeout(() => {
							manager.agentGetState(sessionId as string);
							void fetchHistoryMessages(sessionId as string);
						}, 250);
					}

					// Replace the in-flight working bubble with a terminal error part
					// so the visual container remains stable until authoritative sync.
					if (streamingMessageRef.current) {
						const msgId = streamingMessageRef.current.id;
						const completedMessage: DisplayMessage = {
							...streamingMessageRef.current,
							isStreaming: false,
							parts: [{ type: "error", id: nextPartId(), text: errMsg }],
						};
						setMessages((prev) => {
							const idx = prev.findIndex((m) => m.id === msgId);
							if (idx >= 0) {
								const updated = [...prev];
								updated[idx] = completedMessage;
								return updated;
							}
							return [...prev, completedMessage];
						});
						onMessageComplete?.(completedMessage);
						streamingMessageRef.current = null;
					}
					break;
				}

				// -- Retry progress --
				case "retry.start": {
					// Keep container in working/streaming mode during retries.
					const currentMsg = ensureAssistantMessage(true);
					if (streamingMessageRef.current?.id !== currentMsg.id) {
						streamingMessageRef.current = {
							...currentMsg,
							isStreaming: true,
						};
					}
					setBusyForEvent(event.session_id ?? activeSessionIdRef.current, true);
					isStreamingRef.current = true;
					applyTurnState({ kind: "streaming" });
					break;
				}

				case "retry.end": {
					const retrySuccess = event.success as boolean;
					if (retrySuccess) {
						setBusyForEvent(
							event.session_id ?? activeSessionIdRef.current,
							true,
						);
						isStreamingRef.current = true;
						applyTurnState({ kind: "streaming" });
					}
					// On failure, backend emits agent.error(recoverable=false)
					// which persists the error to hstry and is fetched below.
					break;
				}

				// -- Compaction --
				case "compact.start": {
					const currentMsg = ensureAssistantMessage(false);
					const part: DisplayPart = {
						type: "compaction",
						id: nextPartId(),
						text: "Compacting context...",
					};
					if (streamingMessageRef.current?.id === currentMsg.id) {
						currentMsg.parts.push(part);
						scheduleStreamingUpdate();
					} else {
						appendPartToMessage(currentMsg.id, part);
					}
					break;
				}

				case "compact.end": {
					const success = event.success as boolean;
					const summary = event.summary as string | undefined;
					const tokensBefore = event.tokens_before as number | undefined;

					// Replace the "Compacting context..." placeholder with result
					const resultText = success
						? (() => {
								const parts: string[] = ["Context compacted"];
								if (tokensBefore) {
									const fmt = (n: number) =>
										n >= 1000 ? `${(n / 1000).toFixed(1)}K` : n.toString();
									parts[0] = `Context compacted (${fmt(tokensBefore)} tokens summarized)`;
								}
								return parts[0];
							})()
						: (event.error as string) || "Compaction failed";

					const currentMsg = ensureAssistantMessage(false);
					const part: DisplayPart = success
						? {
								type: "compaction",
								id: nextPartId(),
								text: resultText,
							}
						: {
								type: "error",
								id: nextPartId(),
								text: resultText,
							};

					if (streamingMessageRef.current?.id === currentMsg.id) {
						// Replace the "Compacting context..." part if it exists
						const compactIdx = currentMsg.parts.findIndex(
							(p) =>
								p.type === "compaction" && p.text === "Compacting context...",
						);
						if (compactIdx >= 0) {
							currentMsg.parts[compactIdx] = part;
						} else {
							currentMsg.parts.push(part);
						}
						scheduleStreamingUpdate();
					} else {
						// Replace in messages array
						setMessages((prev) => {
							const msgIdx = prev.findIndex((m) => m.id === currentMsg.id);
							if (msgIdx < 0) return prev;
							const msg = prev[msgIdx];
							const compactIdx = msg.parts.findIndex(
								(p) =>
									p.type === "compaction" && p.text === "Compacting context...",
							);
							if (compactIdx >= 0) {
								const next = [...prev];
								const updatedParts = [...msg.parts];
								updatedParts[compactIdx] = part;
								next[msgIdx] = { ...msg, parts: updatedParts };
								return next;
							}
							return prev;
						});
					}

					break;
				}

				// -- Config changes --
				case "config.model_changed": {
					const sessionId = event.session_id;
					setState((prev) => {
						if (!prev) return prev;
						// Build a proper PiModelInfo object. If the previous model
						// already matches this id+provider, keep its metadata
						// (name, context_window, max_tokens). Otherwise construct
						// a minimal object -- full metadata arrives with the next
						// get_state response.
						const prevModel = prev.model;
						const model =
							prevModel &&
							typeof prevModel === "object" &&
							prevModel.id === event.model_id &&
							prevModel.provider === event.provider
								? prevModel
								: {
										id: event.model_id,
										provider: event.provider,
										name: event.model_id,
										contextWindow: 0,
										maxTokens: 0,
									};
						return {
							...prev,
							model,
						};
					});
					if (isPiDebugEnabled()) {
						console.debug(
							"[useChat] Model changed:",
							sessionId,
							event.provider,
							event.model_id,
						);
					}
					break;
				}

				case "config.thinking_level_changed": {
					const sessionId = event.session_id;
					setState((prev) => {
						if (!prev) return prev;
						return {
							...prev,
							thinking_level: event.level,
						};
					});
					if (isPiDebugEnabled()) {
						console.debug(
							"[useChat] Thinking level changed:",
							sessionId,
							event.level,
						);
					}
					break;
				}

				case "status": {
					if (event.key !== "oqto_queue_event") break;
					const text = typeof event.text === "string" ? event.text : null;
					if (!text) break;
					try {
						const payload = JSON.parse(text) as {
							type?: string;
							clientId?: string;
							intent?: "default" | "steer" | "followUp";
							bridgeSeq?: number;
							ts?: number;
						};
						const eventType = payload.type;
						if (!eventType) break;

						if (eventType === "queue_reset") {
							setPromptQueue([]);
							break;
						}

						if (eventType === "enqueued") {
							const clientId = payload.clientId;
							if (!clientId) break;
							setPromptQueue((prev) => {
								if (
									prev.some((item) =>
										payload.bridgeSeq != null
											? item.bridgeSeq === payload.bridgeSeq
											: item.clientId === clientId,
									)
								) {
									return prev;
								}
								return [
									...prev,
									{
										bridgeSeq:
											typeof payload.bridgeSeq === "number"
												? payload.bridgeSeq
												: undefined,
										clientId,
										intent: payload.intent ?? "default",
										enqueuedAt:
											typeof payload.ts === "number" ? payload.ts : Date.now(),
									},
								];
							});
							break;
						}

						if (eventType === "turn_bound") {
							setPromptQueue((prev) =>
								prev.filter((item) => {
									if (
										typeof payload.bridgeSeq === "number" &&
										item.bridgeSeq != null
									) {
										return item.bridgeSeq !== payload.bridgeSeq;
									}
									if (payload.clientId) {
										return item.clientId !== payload.clientId;
									}
									return true;
								}),
							);
						}
					} catch {
						// ignore malformed status payloads
					}
					break;
				}

				// -- Messages sync --
				case "messages": {
					// This stream can be partial during live sessions (e.g. assistant-only
					// snapshots). Merge by id/client_id instead of replacing history.
					const msgs = event.messages;
					if (Array.isArray(msgs)) {
						applyServerMessages(
							msgs,
							event.session_id ?? activeSessionIdRef.current ?? "unknown",
							undefined,
							"partial",
						);
						if (isPiDebugEnabled()) {
							console.debug(
								"[useChat] Loaded messages:",
								event.session_id,
								msgs.length,
							);
						}
					}
					break;
				}

				// -- Persisted --
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

				// -- Command response (replaces old Pi command-response events) --
				// CommandResponse fields are flattened into the top-level event by serde:
				//   { event: "response", id, cmd, success, data?, error?, session_id, ... }
				case "response": {
					const resp: CommandResponse | undefined =
						typeof event.cmd === "string"
							? {
									id: event.id as string,
									cmd: event.cmd as string,
									success: event.success as boolean,
									data: event.data as unknown,
									error: event.error as string | undefined,
								}
							: undefined;
					if (!resp) break;

					if (isPiDebugEnabled()) {
						console.debug("[useChat] Command response:", resp.cmd, resp);
					}

					switch (resp.cmd) {
						case "prompt":
						case "steer":
						case "follow_up": {
							if (!resp.success) {
								applyTurnState({ kind: "idle" });
								setBusyForEvent(
									event.session_id ?? activeSessionIdRef.current,
									false,
								);
							}
							break;
						}
						case "session.create": {
							if (resp.success) {
								const targetData = (resp.data ?? null) as {
									target_scope?: string;
									target_workspace_id?: string | null;
								} | null;
								const targetWorkspaceId =
									targetData?.target_workspace_id ?? null;
								const targetScope = targetData?.target_scope;
								if (targetScope === "shared_workspace" && targetWorkspaceId) {
									setSharedWorkspaceSessionId(
										event.session_id,
										targetWorkspaceId,
									);
								} else if (targetScope === "personal") {
									clearSharedWorkspaceSessionId(event.session_id);
								}

								// Always fetch authoritative messages from hstry
								// unless we're actively streaming. The localStorage
								// cache is only a flash-of-content optimization;
								// hstry is the source of truth and will replace it.
								if (
									!streamingMessageRef.current &&
									!isStreamingRef.current &&
									!sendInFlightRef.current
								) {
									void fetchHistoryMessages(event.session_id);
									if (isPiDebugEnabled()) {
										console.debug(
											"[useChat] Session created, fetching history:",
											event.session_id,
										);
									}
								}
							} else {
								const errMsg = resp.error || "Failed to create session";
								const err = new Error(errMsg);
								setError(err);
								onError?.(err);
							}
							break;
						}

						case "get_state": {
							if (resp.success && resp.data) {
								const nextState = resp.data as AgentState & {
									sessionId?: string;
								};
								setState(nextState);
								bindSessionIdentity({
									runnerId:
										event.session_id ?? activeSessionIdRef.current ?? "",
									piId:
										typeof nextState.sessionId === "string"
											? nextState.sessionId
											: undefined,
								});
								if (nextState?.isStreaming === true) {
									// Runner reports the session is actively streaming.
									// Restore streaming/busy UI state so spinners show
									// after page reload. The actual stream events will
									// arrive via the event subscription.
									if (!isStreamingRef.current) {
										applyTurnState({ kind: "streaming" });
										setBusyForEvent(
											event.session_id ?? activeSessionIdRef.current,
											true,
										);
									}
								} else if (nextState?.isStreaming === false) {
									// Only clear awaiting state if we don't
									// have a send in-flight. Otherwise, the
									// get_state after session.create arrives
									// before Pi starts streaming and kills the
									// working indicator prematurely.
									if (!sendInFlightRef.current) {
										applyTurnState({ kind: "idle" });
									}
									if (streamingMessageRef.current) {
										streamingMessageRef.current.isStreaming = false;
										streamingMessageRef.current = null;
									}
								}
							}
							break;
						}

						case "get_messages": {
							// get_messages can return either a direct messages array or an
							// object wrapper ({ messages, message_version }). Treat as partial
							// because live Pi paths may only include a context window.
							if (resp.success && resp.data) {
								const payload = resp.data as
									| RawMessage[]
									| {
											messages?: RawMessage[];
											message_version?: { version?: number };
									  };
								const msgs = Array.isArray(payload)
									? payload
									: payload.messages;
								const serverVersion =
									!Array.isArray(payload) &&
									typeof payload.message_version?.version === "number"
										? payload.message_version.version
										: undefined;
								if (Array.isArray(msgs)) {
									applyServerMessages(
										msgs,
										event.session_id ?? activeSessionIdRef.current ?? "unknown",
										serverVersion,
										"partial",
									);
									if (isPiDebugEnabled()) {
										console.debug(
											"[useChat] Loaded messages:",
											event.session_id,
											msgs.length,
											serverVersion,
										);
									}
								} else if (typeof serverVersion === "number") {
									persistedMessageVersionRef.current = Math.max(
										persistedMessageVersionRef.current ?? 0,
										serverVersion,
									);
								}
							}
							break;
						}

						case "get_stats": {
							// Stats errors for sessions without active Pi
							// processes (e.g. hstry-imported) are expected.
							break;
						}

						default: {
							if (!resp.success && resp.error) {
								// Generic command error
								const errMsg = resp.error;
								const err = new Error(errMsg);
								setError(err);
								onError?.(err);

								// Auto-recover for session-not-found errors
								const sessionId = activeSessionIdRef.current;
								const now = Date.now();
								const shouldRecover =
									Boolean(sessionId) &&
									(errMsg.includes("PiSessionNotFound") ||
										errMsg.includes("SessionNotFound") ||
										errMsg.includes("Response channel closed"));
								if (
									shouldRecover &&
									now - lastSessionRecoveryRef.current > 5000
								) {
									lastSessionRecoveryRef.current = now;
									const manager = getWsManager();
									manager.agentCreateSession(
										sessionId as string,
										getSessionConfig(),
									);
									setTimeout(() => {
										manager.agentGetState(sessionId as string);
										void fetchHistoryMessages(sessionId as string);
									}, 250);
								}
							}
							break;
						}
					}
					break;
				}

				// -- Session title changed --
				case "session.title_changed": {
					const title = event.title as string | undefined;
					const readableId = event.readable_id as string | undefined;
					// Use the event's session_id, NOT activeSessionIdRef.
					// A delayed title event from a previous session must not
					// overwrite the current session's title.
					const eventSessionId = event.session_id as string | undefined;
					if (title && eventSessionId) {
						onTitleChangedRef.current?.(
							eventSessionId,
							title,
							readableId ?? null,
						);
					}
					break;
				}

				default: {
					if (isPiDebugEnabled()) {
						console.debug("[useChat] Unhandled canonical event:", eventType);
					}
				}
			}
		},
		[
			appendPartToMessage,
			applyServerMessages,
			applyThrottledSnapshot,
			applyTurnState,
			clearResponseWatchdog,
			bindSessionIdentity,
			ensureAssistantMessage,
			fetchHistoryMessages,
			scheduleStreamingUpdate,
			throttledStreamingUpdate,
			setBusyForEvent,
			onMessageComplete,
			onError,
			getSessionConfig,
		],
	);

	// ========================================================================
	// Agent event handler (canonical protocol, single handler for all events)
	// ========================================================================

	/**
	 * Handle all agent channel events.
	 *
	 * Validates session_id before dispatching to handleCanonicalEvent.
	 * This is the single entry point for all agent events from ws-manager.
	 */
	const handleAgentEvent = useCallback(
		(event: AgentWsEvent) => {
			const identity = machineRef.current.identity;
			const activeId =
				identity.kind === "bound"
					? identity.runnerId
					: activeSessionIdRef.current;
			const eventSessionId = event.session_id;
			if (activeId && eventSessionId && eventSessionId !== activeId) {
				if (isPiDebugEnabled()) {
					console.debug(
						`[useChat] Ignoring agent event for session ${event.session_id}, active is ${activeId}`,
					);
				}
				return;
			}
			lastAgentEventAtRef.current = Date.now();
			handleCanonicalEvent(event);
		},
		[handleCanonicalEvent],
	);

	// Keep the ref in sync so the subscription effect can use a stable wrapper.
	handleAgentEventRef.current = handleAgentEvent;

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

	type EnsureSessionOptions = {
		createIfMissing: boolean;
		source: "send" | "new_session" | "reconnect";
	};

	const ensureSession = useCallback(
		async ({
			createIfMissing,
			source,
		}: EnsureSessionOptions): Promise<string> => {
			let sessionId = activeSessionIdRef.current;
			if (!sessionId) {
				const identity = machineRef.current.identity;
				const hadPriorSession =
					Boolean(lastActiveSessionIdRef.current) ||
					(identity.kind === "bound" && Boolean(identity.runnerId));
				if (!createIfMissing || hadPriorSession) {
					throw new Error(
						"Session selection lost during reconnect. Please reselect the chat and retry.",
					);
				}
				sessionId = createPiSessionId();
				activeSessionIdRef.current = sessionId;
				onSelectedSessionIdChange?.(sessionId);
				if (isPiDebugEnabled()) {
					console.warn(
						"[useChat] ensureSession: created new session id",
						sessionId,
						"source=",
						source,
					);
				}
			}
			const manager = getWsManager();

			// If history selected a legacy/external ID, map it to a currently active
			// runner session ID before issuing session.create. Shared workspaces can
			// return history IDs that differ from the runner's in-memory session_id.
			try {
				const activeSessions = await manager.agentListSessions();
				const alias = activeSessions.find(
					(s) => s.session_id === sessionId || s.hstry_id === sessionId,
				);
				if (alias) {
					if (alias.session_id !== sessionId) {
						const previousId = sessionId;
						sessionId = alias.session_id;
						activeSessionIdRef.current = sessionId;
						onSelectedSessionIdChange?.(sessionId);
						const sharedWorkspaceId = sharedWorkspaceSessionMap.get(previousId);
						if (sharedWorkspaceId) {
							setSharedWorkspaceSessionId(sessionId, sharedWorkspaceId);
						}
						if (isPiDebugEnabled()) {
							console.debug(
								"[useChat] ensureSession: remapped history session alias",
								previousId,
								"->",
								sessionId,
							);
						}
					}
					bindSessionIdentity({
						runnerId: alias.session_id,
						hstryId: alias.hstry_id,
					});
				}
			} catch {
				// Best-effort alias resolution; continue with original session ID.
			}

			// If the session is already ready and we have an active subscription,
			// there is nothing to do.  Re-subscribing would kill the existing
			// event handler and create a gap where streaming events are lost.
			if (manager.isSessionReady(sessionId) && unsubscribeRef.current) {
				bindSessionIdentity({ runnerId: sessionId });
				return sessionId;
			}

			const sessionConfig = getSessionConfig();
			const stableHandler = (event: AgentWsEvent) => {
				handleAgentEventRef.current?.(event);
			};
			unsubscribeRef.current?.();

			if (manager.isSessionReady(sessionId)) {
				unsubscribeRef.current = manager.subscribeAgentSession(
					sessionId,
					stableHandler,
					sessionConfig,
					{ create: false },
				);
				bindSessionIdentity({ runnerId: sessionId });
				return sessionId;
			}

			unsubscribeRef.current = manager.subscribeAgentSession(
				sessionId,
				stableHandler,
				sessionConfig,
				{ create: true },
			);

			try {
				await manager.ensureConnected(4000);
				try {
					await manager.waitForSessionReady(sessionId, 1500);
				} catch {
					// If session wasn't created by the client (e.g. from history),
					// avoid spawning a duplicate. Verify existence with get_state.
					try {
						await manager.agentGetStateWait(sessionId);
						unsubscribeRef.current?.();
						unsubscribeRef.current = manager.subscribeAgentSession(
							sessionId,
							stableHandler,
							sessionConfig,
							{ create: false },
						);
					} catch {
						// Do not fail hard here. We keep the subscription and return
						// the session ID so outbound messages can queue until the
						// runner/session becomes ready.
						if (isPiDebugEnabled()) {
							console.warn(
								"[useChat] ensureSession: session not ready yet, continuing with queued sends",
								sessionId,
							);
						}
					}
				}
			} catch {
				// Keep going in degraded mode: ws-manager will queue outbound
				// messages and flush after reconnect.
				if (isPiDebugEnabled()) {
					console.warn(
						"[useChat] ensureSession: ws connect failed, continuing with queued sends",
						sessionId,
					);
				}
			}
			bindSessionIdentity({ runnerId: sessionId });
			return sessionId;
		},
		[bindSessionIdentity, getSessionConfig, onSelectedSessionIdChange],
	);

	// Send message
	const send = useCallback(
		async (message: string, options?: SendOptions) => {
			sendInFlightRef.current = true;
			const mode: SendMode = options?.mode ?? "steer";
			let sessionId = options?.sessionId ?? activeSessionIdRef.current;
			if (
				options?.sessionId &&
				options.sessionId !== activeSessionIdRef.current
			) {
				activeSessionIdRef.current = options.sessionId;
				onSelectedSessionIdChange?.(options.sessionId);
				const manager = getWsManager();
				const sessionConfig = getSessionConfig();
				const stableHandler = (event: AgentWsEvent) => {
					handleAgentEventRef.current?.(event);
				};
				unsubscribeRef.current?.();
				unsubscribeRef.current = manager.subscribeAgentSession(
					options.sessionId,
					stableHandler,
					sessionConfig,
					{ create: true },
				);
			}
			// Ensure the session exists and is ready before sending.
			if (!sessionId) {
				// Clear local state for the new session.
				setMessages([]);
				streamingMessageRef.current = null;
				applyTurnState({ kind: "idle" });
				setError(null);
			}
			try {
				sessionId = await ensureSession({
					createIfMissing: true,
					source: "send",
				});
			} catch (err) {
				sendInFlightRef.current = false;
				const reconnectErr =
					err instanceof Error
						? err
						: new Error(
								"Session is not ready. Please reselect the chat and retry.",
							);
				setError(reconnectErr);
				onError?.(reconnectErr);
				throw reconnectErr;
			}

			const identity = machineRef.current.identity;
			if (identity.kind === "bound") {
				sessionId = identity.runnerId;
			} else {
				bindSessionIdentity({ runnerId: sessionId });
			}

			// Mark as streaming IMMEDIATELY so that any server messages
			// (from get_messages, messages events, etc.) arriving between now
			// and stream.message_start are deferred instead of overwriting
			// the optimistic user message.
			isStreamingRef.current = true;

			// Generate a client_id for optimistic message matching.
			// This ID will be sent with the prompt and returned in the persisted message,
			// allowing us to reconcile the optimistic message with the server version.
			// Use fallback for non-secure contexts (HTTP) where crypto.randomUUID
			// is unavailable.
			const clientId =
				typeof crypto !== "undefined" && "randomUUID" in crypto
					? crypto.randomUUID()
					: `${Date.now()}-${Math.random().toString(36).slice(2)}`;

			// Add user message to display with client_id for later matching
			const userMessage: DisplayMessage = {
				id: createTempMessageId(),
				role: "user",
				parts: [{ type: "text", id: nextPartId(), text: message }],
				timestamp: Date.now(),
				clientId,
				// In shared workspaces, tag the message with the current user's name
				// so it renders with the correct sender label instead of "You".
				...(senderName
					? {
							sender: {
								type: "user" as const,
								id: senderName,
								name: senderName,
							},
						}
					: {}),
			};
			lastAssistantMessageIdRef.current = null;
			setMessages((prev) => [...prev, userMessage]);
			setError(null);

			applyTurnState({ kind: "sending" });
			lastAgentEventAtRef.current = Date.now();

			const manager = getWsManager();
			try {
				await manager.ensureConnected(4000);
				await manager.waitForSessionReady(sessionId, 4000);
			} catch {
				// One-shot self-heal for transient reconnect races.
				// If readiness still fails, continue anyway: ws-manager queues
				// outbound prompt/steer/follow_up until session.create response arrives.
				try {
					await new Promise((resolve) => setTimeout(resolve, 200));
					sessionId = await ensureSession({
						createIfMissing: false,
						source: "reconnect",
					});
					await manager.ensureConnected(6000);
					await manager.waitForSessionReady(sessionId, 6000);
				} catch {
					if (isPiDebugEnabled()) {
						console.warn(
							"[useChat] send: session not ready, sending via queue fallback",
							sessionId,
						);
					}
				}
			}

			armResponseWatchdog(sessionId);
			switch (mode) {
				case "prompt":
					// Pass the clientId for optimistic message matching
					manager.agentPrompt(sessionId, message, undefined, clientId);
					break;
				case "steer":
					manager.agentSteer(sessionId, message, undefined, clientId);
					break;
				case "follow_up":
					manager.agentFollowUp(sessionId, message, undefined, clientId);
					break;
			}
		},
		[
			applyTurnState,
			armResponseWatchdog,
			bindSessionIdentity,
			ensureSession,
			getSessionConfig,
			onError,
			onSelectedSessionIdChange,
			senderName,
		],
	);

	// Abort current stream
	const abort = useCallback(async () => {
		const sessionId = activeSessionIdRef.current;
		if (!sessionId) return;

		applyTurnState({ kind: "idle" });
		const manager = getWsManager();
		manager.agentAbort(sessionId);
	}, [applyTurnState]);

	// Compact session
	const compact = useCallback(async (customInstructions?: string) => {
		const sessionId = activeSessionIdRef.current;
		if (!sessionId) return;

		const manager = getWsManager();
		manager.agentCompact(sessionId, customInstructions);
	}, []);

	// New session - creates a brand new session with a new UUID
	const newSession = useCallback(async () => {
		const previousSessionId = activeSessionIdRef.current;
		const newSessionId = createPiSessionId();
		activeSessionIdRef.current = newSessionId;
		onSelectedSessionIdChange?.(newSessionId);

		// Clear local state
		setMessages([]);
		streamingMessageRef.current = null;
		applyTurnState({ kind: "idle" });
		setError(null);
		resetSessionIdentity(previousSessionId);
		await ensureSession({
			createIfMissing: false,
			source: "new_session",
		});
	}, [
		applyTurnState,
		ensureSession,
		onSelectedSessionIdChange,
		resetSessionIdentity,
	]);

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
		applyTurnState({ kind: "idle" });
		setError(null);
		resetSessionIdentity(sessionId);

		// Close and recreate session
		const manager = getWsManager();
		manager.agentCloseSession(sessionId);

		// Small delay then recreate
		setTimeout(() => {
			manager.agentCreateSession(sessionId, getSessionConfig());
		}, 100);

		if (isPiDebugEnabled()) {
			console.debug("[useChat] resetSession for:", sessionId);
		}
	}, [applyTurnState, getSessionConfig, resetSessionIdentity]);

	// Refresh - request current state from backend
	const refresh = useCallback(async () => {
		const sessionId = activeSessionIdRef.current;
		if (!sessionId) return;

		const manager = getWsManager();
		manager.agentGetState(sessionId);
		void fetchHistoryMessages(sessionId);

		if (isPiDebugEnabled()) {
			console.debug("[useChat] refresh requested for:", sessionId);
		}
	}, [fetchHistoryMessages]);

	// Subscribe to connection state
	useEffect(() => {
		const manager = getWsManager();

		const unsubscribe = manager.onConnectionState(
			(connectionState: WsMuxConnectionState) => {
				const connected = connectionState === "connected";
				if (connectionState === "connected") {
					transportEpochRef.current += 1;
					machineRef.current = transitionTransport(
						machineRef.current,
						"connected",
						transportEpochRef.current,
					);
				} else if (connectionState === "connecting") {
					machineRef.current = transitionTransport(
						machineRef.current,
						"connecting",
						transportEpochRef.current,
					);
				} else if (connectionState === "reconnecting") {
					transportEpochRef.current += 1;
					machineRef.current = transitionTransport(
						machineRef.current,
						"reconnecting",
						transportEpochRef.current,
					);
				} else {
					machineRef.current = transitionTransport(
						machineRef.current,
						"disconnected",
						transportEpochRef.current,
					);
				}
				setIsConnected(connected);
				if (!connected) {
					// Prevent indefinite "stuck streaming" UI when WS drops mid-turn.
					clearResponseWatchdog();
					isStreamingRef.current = false;
					applyTurnState({ kind: "idle" });
					sendInFlightRef.current = false;
					if (streamingMessageRef.current) {
						streamingMessageRef.current.isStreaming = false;
						streamingMessageRef.current = null;
					}
					setError(new Error("Connection lost. Reconnecting..."));
				} else {
					setError(null);
					// On reconnect, run a single in-flight reconcile transaction
					// for the active session. This prevents duplicate concurrent
					// get_state/get_messages fetch races from multiple reconnect
					// notifications in quick succession.
					const sid = activeSessionIdRef.current;
					if (sid) {
						const epoch = transportEpochRef.current;
						const preferPartial = Boolean(
							isStreamingRef.current || streamingMessageRef.current,
						);
						void runReconnectReconcile(sid, preferPartial, epoch);
					}
				}
			},
		);

		return unsubscribe;
	}, [applyTurnState, clearResponseWatchdog, runReconnectReconcile]);

	// Watchdog: while streaming, periodically check for stalled connections.
	// We request get_messages snapshots to repair dropped WebSocket events;
	// these are merged as partial updates to avoid clobbering optimistic turns.
	// useeffect-guardrail: allow
	useEffect(() => {
		if (!isConnected) return;
		if (!activeSessionId) return;
		if (!isStreaming && !isAwaitingResponse) return;

		const manager = getWsManager();
		const sessionId = activeSessionId;

		// Periodic sync every 5s: fetch a snapshot to repair any dropped
		// streaming events.
		const syncTimer = setInterval(() => {
			if (activeSessionIdRef.current !== sessionId) return;
			manager.agentGetMessages(sessionId);
		}, 5000);

		// Watchdog: detect stalled streaming.
		const watchdogTimer = setInterval(() => {
			const idleMs = Date.now() - lastAgentEventAtRef.current;
			if (idleMs < 8000) return;

			if (isPiDebugEnabled()) {
				console.warn(
					`[useChat] stream watchdog triggered for ${sessionId} after ${idleMs}ms without events`,
				);
			}

			manager.agentGetState(sessionId);

			// After 12s of silence, recover from hstry and clear spinner.
			if (idleMs >= 12000) {
				applyTurnState({ kind: "idle" });
				void fetchHistoryMessages(sessionId);
				setError(
					new Error(
						"Live updates stalled. Recovered latest history. You can continue chatting.",
					),
				);
			} else {
				manager.agentGetMessages(sessionId);
			}
		}, 2000);

		return () => {
			clearInterval(syncTimer);
			clearInterval(watchdogTimer);
		};
	}, [
		activeSessionId,
		applyTurnState,
		fetchHistoryMessages,
		isAwaitingResponse,
		isConnected,
		isStreaming,
	]);

	// Subscribe to Pi session when active session changes.
	// IMPORTANT: This effect must NOT depend on handleAgentEvent or other
	// frequently-changing callback refs. We use handleAgentEventRef (a stable
	// ref) to dispatch events. This prevents the effect from re-running during
	// streaming (which would reset streamingMessageRef and lose the user message).
	// biome-ignore lint/correctness/useExhaustiveDependencies: stable deps intentionally omitted
	useEffect(() => {
		// Unsubscribe from previous session
		if (unsubscribeRef.current) {
			unsubscribeRef.current();
			unsubscribeRef.current = null;
		}

		if (!activeSessionId) {
			setHistoryHydrated(true);
			setHistoryLoading(false);
			return;
		}

		const previousId = lastActiveSessionIdRef.current;
		const sessionActuallyChanged = previousId !== activeSessionId;

		// If we just transitioned from a pending ID to a real session ID,
		// migrate cached messages so the first message doesn't disappear.
		if (
			previousId &&
			sessionActuallyChanged &&
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

		// Only reset state when the session ID actually changed.
		// Skipping this when only the effect deps changed (but session is the
		// same) prevents clobbering in-flight streaming and the optimistic user
		// message.
		if (sessionActuallyChanged) {
			if (previousId) {
				const previousSnapshot = snapshotMessagesForSessionSwitch();
				if (previousSnapshot.length > 0) {
					writeCachedSessionMessages(
						previousId,
						previousSnapshot,
						resolvedStorageKeyPrefix,
						true,
					);
				}
			}
			// Load cached messages for this session.
			const cached = readCachedSessionMessages(
				activeSessionId,
				resolvedStorageKeyPrefix,
			);
			if (cached.length > 0) {
				setMessages(cached);
				const lastAssistant = [...cached]
					.reverse()
					.find((msg) => msg.role === "assistant");
				lastAssistantMessageIdRef.current = lastAssistant?.id ?? null;
				setHistoryLoading(false);
			} else {
				setMessages([]);
				lastAssistantMessageIdRef.current = null;
				setHistoryLoading(true);
			}
			setHistoryHydrated(false);

			// Reset streaming and agent state for the new session.
			// Clearing state prevents stale sessionName from the previous
			// session being applied to the new one as a title.
			streamingMessageRef.current = null;
			streamingThrottleRef.current.reset();
			if (throttleFlushTimerRef.current) {
				clearInterval(throttleFlushTimerRef.current);
				throttleFlushTimerRef.current = null;
			}
			const batch = batchedUpdateRef.current;
			if (batch.rafId !== null) {
				clearTimeout(batch.rafId);
				batch.rafId = null;
			}
			batch.pendingUpdate = false;
			setState(null);
			applyTurnState({ kind: "idle" });
			resetSessionIdentity(activeSessionId);
			setError(null);
		}

		// Fetch authoritative history immediately on session switch.
		// This avoids showing "No messages yet" while waiting for
		// WebSocket session readiness / reattach logic.
		if (sessionActuallyChanged) {
			void fetchHistoryMessages(activeSessionId);
			// Also request the runner's live window immediately so in-flight
			// assistant output reappears when returning to a still-streaming session.
			// oqto-log only contains persisted turns (agent_end), so relying on
			// history alone can look like "lost" messages mid-turn.
			const manager = getWsManager();
			manager.agentGetState(activeSessionId);
			manager.agentGetMessages(activeSessionId);
		}

		// Use a stable wrapper that delegates to the latest handleAgentEvent
		// via ref. This avoids putting handleAgentEvent in the deps array.
		const stableHandler = (event: AgentWsEvent) => {
			handleAgentEventRef.current?.(event);
		};

		// Subscribe to the new session (passes harness/cwd for session creation)
		const manager = getWsManager();
		const sessionConfig = getSessionConfig();
		unsubscribeRef.current = manager.subscribeAgentSession(
			activeSessionId,
			stableHandler,
			sessionConfig,
			{ create: false },
		);

		// Register resync handler: after a reconnect, ws-manager will fetch
		// fresh state+messages and call this handler to rebuild the timeline
		// from scratch rather than trying to merge stale local state.
		const unsubscribeResync = manager.onResync(
			activeSessionId,
			(_sessionId, stateData, serverMessages) => {
				// Guard: discard resync if the session is no longer active
				if (activeSessionIdRef.current !== _sessionId) {
					console.log(
						`[useChat] Discarding stale resync for ${_sessionId} (active: ${activeSessionIdRef.current})`,
					);
					return;
				}

				console.log(
					`[useChat] Resync received for ${_sessionId}: ` +
						`state=${stateData ? "ok" : "null"}, messages=${serverMessages.length}`,
				);

				// Apply state
				if (stateData) {
					const nextState = stateData as AgentState;
					setState(nextState);
					if (nextState?.isStreaming === true) {
						applyTurnState({ kind: "streaming" });
					} else {
						applyTurnState({ kind: "idle" });
						if (streamingMessageRef.current) {
							streamingMessageRef.current.isStreaming = false;
							streamingMessageRef.current = null;
						}
					}
				}

				// For messages, merge Pi's live window with local state rather than
				// replacing. Pi's get_messages only returns the current context window
				// (not the full history), so replacing would discard earlier turns.
				// Also fetch from hstry for the complete history.
				if (serverMessages.length > 0) {
					applyServerMessages(
						serverMessages as RawMessage[],
						_sessionId,
						undefined,
						"partial",
					);
				}

				// Reset throttle state since we just rebuilt everything
				streamingThrottleRef.current.reset();
				if (throttleFlushTimerRef.current) {
					clearInterval(throttleFlushTimerRef.current);
					throttleFlushTimerRef.current = null;
				}

				// Also fetch full history from hstry to fill in messages
				// that Pi's context window may have compacted away.
				void fetchHistoryMessages(_sessionId);
			},
		);

		if (isPiDebugEnabled()) {
			console.debug("[useChat] Subscribed to session:", activeSessionId);
		}
		lastActiveSessionIdRef.current = activeSessionId;

		// For existing sessions (create: false), we need to determine if the
		// session is currently active on the runner. If it is, we send
		// session.create (which is idempotent) to set up event forwarding
		// so streaming events reach this WebSocket connection. Without this,
		// a page reload during streaming would lose all events.
		//
		// If the session is NOT on the runner (just a history entry), we
		// fetch state and messages directly.
		// Track whether this effect cycle has been cleaned up.
		// All async callbacks below check this flag and bail out if
		// the effect was torn down, preventing leaked subscriptions
		// that cause duplicate event delivery.
		let aborted = false;

		if (!manager.isSessionReady(activeSessionId) && !isStreamingRef.current) {
			const sid = activeSessionId;
			manager
				.ensureConnected(4000)
				.then(async () => {
					if (aborted) return;
					// Re-check streaming state after async wait
					if (isStreamingRef.current) return;

					// Guard: if the session changed while we waited for
					// the connection, abort to avoid clobbering new state.
					if (activeSessionIdRef.current !== sid) return;

					// Check if this session is active on the runner
					try {
						const activeSessions = await manager.agentListSessions();
						if (aborted) return;
						// Re-check after async wait
						if (activeSessionIdRef.current !== sid) return;
						const activeSession = activeSessions.find(
							(s) => s.session_id === sid,
						);
						if (activeSession) {
							// Session is alive on the runner -- send session.create
							// to set up event forwarding (idempotent, won't spawn
							// a duplicate Pi process).
							console.log(
								"[useChat] Reattaching to active session:",
								sid,
								"state:",
								activeSession.state,
							);

							// Always fetch history messages first so the chat
							// is never empty. If the session is truly streaming,
							// live events will merge on top of these. Without
							// this, a stale "streaming" state from the runner
							// blocks the session.create handler from fetching
							// (it checks !isStreamingRef) and agent.idle never
							// fires (dead Pi), leaving the chat permanently empty.
							void fetchHistoryMessages(sid);

							// Re-subscribe with create: true to trigger
							// session.create on the backend, which sets up
							// event forwarding from the runner.
							unsubscribeRef.current?.();
							unsubscribeRef.current = manager.subscribeAgentSession(
								sid,
								stableHandler,
								sessionConfig,
								{ create: true },
							);

							// If the session is actively working, mark it as
							// streaming so the UI shows spinners immediately.
							const busyStates = new Set([
								"streaming",
								"compacting",
								"starting",
							]);
							if (busyStates.has(activeSession.state)) {
								// Reattaching to a streaming session — fetch
								// current messages from Pi (live window).
								console.log(
									"[useChat] Fetching live messages from Pi for streaming session:",
									sid,
								);
								manager.agentGetMessages(sid);
								applyTurnState({ kind: "streaming" });
								setBusyForEvent(sid, true);
							}
						} else {
							if (aborted) return;
							// Session ID didn't appear in list_sessions. It may still
							// be active under a different alias (e.g. Pi native ID), so
							// probe with get_state before falling back to history.
							try {
								await manager.agentGetStateWait(sid);
								if (aborted) return;
								// Re-check after async wait
								if (activeSessionIdRef.current !== sid) return;

								// Session exists -- reattach to enable event forwarding.
								// Fetch history immediately (same reasoning as above).
								void fetchHistoryMessages(sid);
								unsubscribeRef.current?.();
								unsubscribeRef.current = manager.subscribeAgentSession(
									sid,
									stableHandler,
									sessionConfig,
									{ create: true },
								);
							} catch {
								if (aborted) return;
								// Session is not on the runner -- just fetch
								// historical state and messages.
								manager.agentGetState(sid);
								void fetchHistoryMessages(sid);
							}
						}
					} catch {
						if (aborted) return;
						// list_sessions failed -- fall back to direct fetch
						manager.agentGetState(sid);
						void fetchHistoryMessages(sid);
					}
				})
				.catch(() => {
					// Connection failed, messages will load on reconnect
				});
		}

		return () => {
			aborted = true;
			if (unsubscribeRef.current) {
				unsubscribeRef.current();
				unsubscribeRef.current = null;
			}
			unsubscribeResync();
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [
		activeSessionId,
		resolvedStorageKeyPrefix,
		getSessionConfig,
		snapshotMessagesForSessionSwitch,
	]);

	// Auto-connect on mount
	useEffect(() => {
		if (autoConnect && activeSessionId) {
			connect();
		}
	}, [autoConnect, activeSessionId, connect]);

	useEffect(() => {
		messagesRef.current = messages;
	}, [messages]);

	// NOTE: We intentionally do NOT sync isStreaming state back to
	// isStreamingRef. The ref is set manually in send() BEFORE the optimistic
	// user message is added, and cleared on agent.idle/stream.done. The ref
	// is the source of truth for streaming detection; the React state is
	// for rendering.

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

	// Cleanup throttle flush timer on unmount
	useEffect(() => {
		return () => {
			if (throttleFlushTimerRef.current) {
				clearInterval(throttleFlushTimerRef.current);
				throttleFlushTimerRef.current = null;
			}
			streamingThrottleRef.current.reset();
		};
	}, []);

	return {
		state,
		messages,
		isConnected,
		isStreaming,
		isAwaitingResponse,
		error,
		promptQueue,
		historyHydrated,
		historyLoading,
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

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
	type ChatMessage,
	type ChatMessagePart,
	getChatMessages,
} from "@/lib/api";
import type {
	CommandResponse,
	Part,
	SessionConfig,
	ToolStatus,
} from "@/lib/canonical-types";
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
	convertCanonicalMessageToDisplay,
	getMaxMessageId,
	mergeServerMessages,
	nextPartId,
	normalizeContentToParts,
	normalizeMessages,
} from "./message-utils";
import type {
	AgentState,
	DisplayMessage,
	DisplayPart,
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
// Inspired by pi-mobile's UiUpdateThrottler (80ms text, 100ms tools).
const TEXT_DELTA_THROTTLE_MS = 80;

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

function normalizeToolStatus(value?: string | null): ToolStatus {
	switch (value?.toLowerCase()) {
		case "pending":
			return "pending";
		case "running":
			return "running";
		case "error":
			return "error";
		case "success":
		case "completed":
			return "success";
		default:
			return "success";
	}
}

function chatMessagePartToCanonicalParts(part: ChatMessagePart): Part[] {
	const parts: Part[] = [];
	if (part.part_type === "thinking" && part.text) {
		parts.push({
			type: "thinking",
			id: part.id,
			text: part.text,
		});
		return parts;
	}
	if (part.part_type === "text" && part.text) {
		parts.push({
			type: "text",
			id: part.id,
			text: part.text,
		});
		return parts;
	}

	const hasToolData = Boolean(
		part.tool_name || part.tool_input || part.tool_output,
	);
	if (part.part_type === "tool_call") {
		const toolCallId = part.tool_call_id ?? part.id;
		parts.push({
			type: "tool_call",
			id: `${part.id}-call`,
			toolCallId,
			name: part.tool_name ?? "tool",
			input: part.tool_input ?? undefined,
			status: normalizeToolStatus(part.tool_status),
		});
		return parts;
	}
	if (part.part_type === "tool_result") {
		const toolCallId = part.tool_call_id ?? part.id;
		parts.push({
			type: "tool_result",
			id: `${part.id}-result`,
			toolCallId,
			name: part.tool_name ?? "tool",
			output: part.tool_output ?? part.tool_title ?? "",
			isError: normalizeToolStatus(part.tool_status) === "error",
		});
		return parts;
	}
	if (part.part_type === "tool" || hasToolData) {
		const toolCallId = part.tool_call_id ?? part.id;
		const name = part.tool_name ?? "tool";
		if (part.tool_input) {
			parts.push({
				type: "tool_call",
				id: `${part.id}-call`,
				toolCallId,
				name,
				input: part.tool_input,
				status: normalizeToolStatus(part.tool_status),
			});
		}
		if (part.tool_output || part.tool_title || part.tool_status) {
			parts.push({
				type: "tool_result",
				id: `${part.id}-result`,
				toolCallId,
				name,
				output: part.tool_output ?? part.tool_title ?? "",
				isError: normalizeToolStatus(part.tool_status) === "error",
			});
		}
		return parts;
	}

	if (part.text) {
		parts.push({
			type: "text",
			id: part.id,
			text: part.text,
		});
	}
	return parts;
}

function chatMessageToRaw(message: ChatMessage): RawMessage {
	const parts = message.parts.flatMap(chatMessagePartToCanonicalParts);
	return {
		id: message.id,
		role: message.role,
		created_at: message.created_at,
		parts,
		model_id: message.model_id ?? null,
		provider_id: message.provider_id ?? null,
		client_id: message.client_id ?? undefined,
	};
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
	const { setSessionBusy } = useBusySessions();

	// Refs
	const messageIdRef = useRef(getMaxMessageId(messages));
	const streamingMessageRef = useRef<DisplayMessage | null>(null);
	const lastAssistantMessageIdRef = useRef<string | null>(null);
	const unsubscribeRef = useRef<(() => void) | null>(null);
	const messagesRef = useRef(messages);
	const lastSessionRecoveryRef = useRef(0);
	const isStreamingRef = useRef(false);
	const sendInFlightRef = useRef(false);
	// Deferred server messages received while streaming (applied on agent.idle)
	const deferredServerMessagesRef = useRef<unknown[] | null>(null);
	// Force a full server sync after reattaching to an active runner session.
	const forceMessageSyncRef = useRef<Set<string>>(new Set());
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

	// Generate unique message ID
	const nextMessageId = useCallback(() => {
		messageIdRef.current += 1;
		return `pi-msg-${messageIdRef.current}`;
	}, []);

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
				id: nextMessageId(),
				role: "assistant",
				parts: [{ type: "text", id: nextPartId(), text: content }],
				timestamp: Date.now(),
			};
			setMessages((prev) => [...prev, assistantMessage]);
			lastAssistantMessageIdRef.current = assistantMessage.id;
			onMessageComplete?.(assistantMessage);
		},
		[nextMessageId, onMessageComplete],
	);

	const getSessionConfig = useCallback((): SessionConfig | undefined => {
		const config: SessionConfig = { harness: "pi" };
		if (normalizedWorkspacePath) {
			config.cwd = normalizedWorkspacePath;
		}
		try {
			const storageKey = getWorkspaceModelStorageKey(normalizedWorkspacePath);
			const storedModelRef = localStorage.getItem(storageKey);
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
	}, [normalizedWorkspacePath]);

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

	const ensureAssistantMessage = useCallback(
		(preferStreaming: boolean) => {
			if (streamingMessageRef.current) return streamingMessageRef.current;
			const lastId = lastAssistantMessageIdRef.current;
			if (lastId) {
				const existing = messagesRef.current.find((m) => m.id === lastId);
				if (existing && existing.role === "assistant") {
					return existing;
				}
			}
			const assistantMessage: DisplayMessage = {
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

	// biome-ignore lint/correctness/useExhaustiveDependencies: mergeServerMessages/normalizeMessages are stable refs
	const fetchHistoryMessages = useCallback(
		async (sessionId: string) => {
			try {
				const history = await getChatMessages(sessionId);
				if (history.length === 0) return;
				const displayMessages = normalizeMessages(
					history.map(chatMessageToRaw),
					`history-${sessionId}`,
				);
				if (displayMessages.length === 0) return;
				setMessages((prev) => mergeServerMessages(prev, displayMessages));
				messageIdRef.current = getMaxMessageId(displayMessages);
				const lastAssistant = [...displayMessages]
					.reverse()
					.find((msg) => msg.role === "assistant");
				lastAssistantMessageIdRef.current = lastAssistant?.id ?? null;
				if (isPiDebugEnabled()) {
					console.debug(
						"[useChat] Loaded history messages:",
						sessionId,
						displayMessages.length,
					);
				}
			} catch (err) {
				if (isPiDebugEnabled()) {
					console.debug("[useChat] Failed to load history:", err);
				}
			}
		},
		[mergeServerMessages, normalizeMessages],
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

				// -- Text delta (incremental) --
				case "stream.text_delta": {
					const delta = event.delta as string | undefined;
					if (!delta) break;
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
					setIsAwaitingResponse(false);
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
					setIsAwaitingResponse(false);
					break;
				}

				// -- Tool call being assembled by LLM --
				case "stream.tool_call_start": {
					const toolCallId = event.tool_call_id as string;
					const name = event.name as string;
					const targetMessage = ensureAssistantMessage(true);
					const alreadyPresent = targetMessage.parts.some(
						(p) => p.type === "tool_call" && p.toolCallId === toolCallId,
					);
					if (!alreadyPresent) {
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
					setIsStreaming(true);
					setIsAwaitingResponse(false);
					break;
				}

				// -- Tool call finalized (LLM produced final input) --
				case "stream.tool_call_end": {
					const toolCall = event.tool_call as
						| { id: string; name: string; input: unknown }
						| undefined;
					if (!toolCall) break;
					const targetMessage = ensureAssistantMessage(true);
					const existingPart = targetMessage.parts.find(
						(p) => p.type === "tool_call" && p.toolCallId === toolCall.id,
					);
					if (existingPart && existingPart.type === "tool_call") {
						existingPart.input = toolCall.input;
						scheduleStreamingUpdate();
					}
					break;
				}

				// -- Tool execution started --
				case "tool.start": {
					const toolCallId = event.tool_call_id as string;
					const name = event.name as string;
					const input = event.input;
					// Ensure there's a tool_call part for this tool (in case we missed
					// stream.tool_call_start, e.g. on reconnect)
					const targetMessage = ensureAssistantMessage(true);
					const existing = targetMessage.parts.find(
						(p) => p.type === "tool_call" && p.toolCallId === toolCallId,
					);
					if (!existing) {
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
					setIsStreaming(true);
					setIsAwaitingResponse(false);
					break;
				}

				// -- Tool execution completed --
				case "tool.end": {
					const toolCallId = event.tool_call_id as string;
					const name = event.name as string;
					const output = event.output;
					const isError = event.is_error as boolean;
					const targetMessage = ensureAssistantMessage(false);
					const matchingToolCall = targetMessage.parts.find(
						(p) => p.type === "tool_call" && p.toolCallId === toolCallId,
					);
					if (matchingToolCall && matchingToolCall.type === "tool_call") {
						matchingToolCall.status = isError ? "error" : "success";
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
					setIsStreaming(true);
					setIsAwaitingResponse(false);
					break;
				}

				// -- Stream complete --
				case "stream.done": {
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
						cancelAnimationFrame(batch.rafId);
						batch.rafId = null;
					}
					batch.pendingUpdate = false;

					if (streamingMessageRef.current) {
						streamingMessageRef.current.isStreaming = false;
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
						streamingMessageRef.current = null;
					}
					// Clear streaming state. The Messages event from agent.end
					// now only contains assistant messages, so no deduplication
					// issues with the optimistic user message.
					isStreamingRef.current = false;
					setIsStreaming(false);
					setIsAwaitingResponse(false);
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
						cancelAnimationFrame(endBatch.rafId);
						endBatch.rafId = null;
					}
					endBatch.pendingUpdate = false;

					// Finalize this message: call onMessageComplete and clear the
					// streaming ref so the next stream.message_start (for a
					// subsequent assistant turn) creates a new message.
					onMessageComplete?.(updated);
					streamingMessageRef.current = null;
					setIsAwaitingResponse(false);
					break;
				}

				// -- Agent idle (streaming ended) --
				case "agent.idle": {
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
					setIsStreaming(false);
					isStreamingRef.current = false;
					setIsAwaitingResponse(false);
					if (streamingMessageRef.current) {
						streamingMessageRef.current.isStreaming = false;
						streamingMessageRef.current = null;
					}
					// Discard any stale deferred messages -- they may be
					// incomplete (fetched mid-stream before all messages
					// were persisted).
					deferredServerMessagesRef.current = null;
					// Refresh session state (stats, model info, etc.) but
					// do NOT re-fetch messages unless we explicitly need
					// to recover after a reattach. Pi's get_messages RPC
					// only returns the current context window (not the full
					// history), so the response would clobber earlier turns
					// that are already in local state from streaming.
					{
						const sessionId = activeSessionIdRef.current;
						if (sessionId) {
							setTimeout(() => {
								const manager = getWsManager();
								manager.agentGetState(sessionId);
								if (forceMessageSyncRef.current.has(sessionId)) {
									forceMessageSyncRef.current.delete(sessionId);
									void fetchHistoryMessages(sessionId);
								}
							}, 100);
						}
					}
					break;
				}

				// -- Agent working (streaming started) --
				case "agent.working": {
					setBusyForEvent(event.session_id ?? activeSessionIdRef.current, true);
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
						const manager = getWsManager();
						manager.agentGetState(resyncSessionId);
						void fetchHistoryMessages(resyncSessionId);
					}
					break;
				}

				case "agent.error": {
					const wasInFlight = sendInFlightRef.current;
					sendInFlightRef.current = false;
					isStreamingRef.current = false;
					setBusyForEvent(
						event.session_id ?? activeSessionIdRef.current,
						false,
					);
					const errMsg = (event.error as string) || "Unknown error";
					const recoverable = event.recoverable as boolean;
					const isSessionNotFound =
						errMsg.includes("PiSessionNotFound") ||
						errMsg.includes("SessionNotFound") ||
						errMsg.includes("Response channel closed");
					if (!wasInFlight && isSessionNotFound) {
						// Background session lookup failure while idle (e.g. viewing history)
						// should not surface as a user-visible error.
						break;
					}
					const err = new Error(errMsg);
					setError(err);
					onError?.(err);
					setIsStreaming(false);
					setIsAwaitingResponse(false);

					// Auto-recover for session-not-found errors
					const sessionId = activeSessionIdRef.current;
					const now = Date.now();
					const shouldRecover =
						Boolean(sessionId) &&
						wasInFlight &&
						!recoverable &&
						isSessionNotFound;
					if (shouldRecover && now - lastSessionRecoveryRef.current > 5000) {
						lastSessionRecoveryRef.current = now;
						const manager = getWsManager();
						manager.agentCreateSession(sessionId as string, getSessionConfig());
						setTimeout(() => {
							manager.agentGetState(sessionId as string);
							void fetchHistoryMessages(sessionId as string);
						}, 250);
					}

					if (streamingMessageRef.current) {
						streamingMessageRef.current.isStreaming = false;
						streamingMessageRef.current.parts.push({
							type: "error",
							id: nextPartId(),
							text: errMsg,
						});
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
						streamingMessageRef.current = null;
					} else {
						const errorMessage: DisplayMessage = {
							id: nextMessageId(),
							role: "assistant",
							parts: [{ type: "error", id: nextPartId(), text: errMsg }],
							timestamp: Date.now(),
							isStreaming: false,
						};
						setMessages((prev) => [...prev, errorMessage]);
						onMessageComplete?.(errorMessage);
					}
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
					if (!success) {
						const errText = (event.error as string) || "Compaction failed";
						const currentMsg = ensureAssistantMessage(false);
						const part: DisplayPart = {
							type: "error",
							id: nextPartId(),
							text: errText,
						};
						if (streamingMessageRef.current?.id === currentMsg.id) {
							currentMsg.parts.push(part);
							scheduleStreamingUpdate();
						} else {
							appendPartToMessage(currentMsg.id, part);
						}
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

				// -- Messages sync --
				case "messages": {
					// Defer if we're currently streaming — applying persisted
					// messages would overwrite the live in-progress content.
					// They will be applied when agent.idle fires.
					if (
						streamingMessageRef.current ||
						isStreamingRef.current ||
						sendInFlightRef.current
					) {
						const msgs = event.messages;
						if (Array.isArray(msgs) && msgs.length > 0) {
							deferredServerMessagesRef.current = msgs;
						}
						if (isPiDebugEnabled()) {
							console.debug(
								"[useChat] Deferring messages sync during streaming:",
								event.session_id,
							);
						}
						break;
					}
					const msgs = event.messages;
					if (Array.isArray(msgs)) {
						const displayMessages = normalizeMessages(
							msgs,
							`server-${event.session_id}`,
						);

						if (displayMessages.length > 0) {
							setMessages((prev) => mergeServerMessages(prev, displayMessages));
							messageIdRef.current = getMaxMessageId(displayMessages);
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
						case "session.create": {
							if (resp.success) {
								// Session created — load persisted messages only
								// if we're not already streaming AND we don't
								// have local optimistic messages. If the user
								// just sent a message (via send()), messagesRef
								// already contains the optimistic user message
								// and isStreamingRef is true. Requesting
								// get_messages now would return stale data from
								// hstry (which doesn't have the user message
								// yet) and overwrite the optimistic content.
								const hasLocalMessages = messagesRef.current.length > 0;
								const forceSync = forceMessageSyncRef.current.has(
									event.session_id,
								);
								if (
									!streamingMessageRef.current &&
									!isStreamingRef.current &&
									!sendInFlightRef.current &&
									(!hasLocalMessages || forceSync)
								) {
									forceMessageSyncRef.current.delete(event.session_id);
									void fetchHistoryMessages(event.session_id);
									if (isPiDebugEnabled()) {
										console.debug(
											"[useChat] Session created, requesting messages:",
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
								const nextState = resp.data as AgentState;
								setState(nextState);
								if (nextState?.isStreaming === true) {
									// Runner reports the session is actively streaming.
									// Restore streaming/busy UI state so spinners show
									// after page reload. The actual stream events will
									// arrive via the event subscription.
									if (!isStreamingRef.current) {
										setIsStreaming(true);
										isStreamingRef.current = true;
										setBusyForEvent(
											event.session_id ?? activeSessionIdRef.current,
											true,
										);
									}
								} else if (nextState?.isStreaming === false) {
									setIsStreaming(false);
									// Only clear isAwaitingResponse if we don't
									// have a send in-flight. Otherwise, the
									// get_state after session.create arrives
									// before Pi starts streaming and kills the
									// working indicator prematurely.
									if (!sendInFlightRef.current) {
										setIsAwaitingResponse(false);
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
							// Defer if we're currently streaming — applying
							// persisted messages would overwrite live content.
							// They will be applied when agent.idle fires.
							// However, if forceMessageSyncRef is set (e.g., reconnect
							// during streaming), apply immediately to restore state.
							const forceSync = forceMessageSyncRef.current.has(
								event.session_id ?? "",
							);
							if (
								!forceSync &&
								(streamingMessageRef.current ||
									isStreamingRef.current ||
									sendInFlightRef.current)
							) {
								if (resp.success && resp.data) {
									const data = resp.data as { messages?: unknown[] };
									const msgs = data.messages;
									if (Array.isArray(msgs) && msgs.length > 0) {
										deferredServerMessagesRef.current = msgs;
									}
								}
								if (isPiDebugEnabled()) {
									console.debug(
										"[useChat] Deferring get_messages response during streaming:",
										event.session_id,
									);
								}
								break;
							}
							// Clear the force sync flag - we've applied it
							if (forceSync) {
								forceMessageSyncRef.current.delete(event.session_id ?? "");
							}
							if (resp.success && resp.data) {
								const data = resp.data as { messages?: RawMessage[] };
								const msgs = data.messages;
								if (Array.isArray(msgs)) {
									const displayMessages = normalizeMessages(
										msgs as RawMessage[],
										`server-${event.session_id}`,
									);
									if (displayMessages.length > 0) {
										setMessages((prev) =>
											mergeServerMessages(prev, displayMessages),
										);
										messageIdRef.current = getMaxMessageId(displayMessages);
										const lastAssistant = [...displayMessages]
											.reverse()
											.find((msg) => msg.role === "assistant");
										lastAssistantMessageIdRef.current =
											lastAssistant?.id ?? null;
									}
									if (isPiDebugEnabled()) {
										console.debug(
											"[useChat] Loaded messages:",
											event.session_id,
											displayMessages.length,
										);
									}
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
			applyThrottledSnapshot,
			ensureAssistantMessage,
			fetchHistoryMessages,
			nextMessageId,
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
			const activeId = activeSessionIdRef.current;
			if (activeId && event.session_id !== activeId) {
				if (isPiDebugEnabled()) {
					console.debug(
						`[useChat] Ignoring agent event for session ${event.session_id}, active is ${activeId}`,
					);
				}
				return;
			}
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

	const ensureSession = useCallback(async (): Promise<string> => {
		let sessionId = activeSessionIdRef.current;
		if (!sessionId) {
			sessionId = createPiSessionId();
			activeSessionIdRef.current = sessionId;
			onSelectedSessionIdChange?.(sessionId);
		}
		const manager = getWsManager();

		// If the session is already ready and we have an active subscription,
		// there is nothing to do.  Re-subscribing would kill the existing
		// event handler and create a gap where streaming events are lost.
		if (manager.isSessionReady(sessionId) && unsubscribeRef.current) {
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
			return sessionId;
		}

		unsubscribeRef.current = manager.subscribeAgentSession(
			sessionId,
			stableHandler,
			sessionConfig,
			{ create: true },
		);

		await manager.ensureConnected(4000);
		try {
			await manager.waitForSessionReady(sessionId, 1500);
		} catch (err) {
			// If session wasn't created by the client (e.g. from history),
			// avoid spawning a duplicate. Verify existence with get_state.
			try {
				await manager.agentGetStateWait(sessionId);
				manager.subscribeAgentSession(sessionId, stableHandler, sessionConfig, {
					create: false,
				});
			} catch {
				throw err;
			}
		}
		return sessionId;
	}, [getSessionConfig, onSelectedSessionIdChange]);

	// Send message
	const send = useCallback(
		async (message: string, options?: SendOptions) => {
			sendInFlightRef.current = true;
			const mode: SendMode = options?.mode ?? "prompt";
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
				setIsStreaming(false);
				isStreamingRef.current = false;
				setError(null);
				messageIdRef.current = 0;
			}
			try {
				sessionId = await ensureSession();
			} catch (err) {
				sendInFlightRef.current = false;
				throw err;
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
				id: nextMessageId(),
				role: "user",
				parts: [{ type: "text", id: nextPartId(), text: message }],
				timestamp: Date.now(),
				clientId,
			};
			lastAssistantMessageIdRef.current = null;
			setMessages((prev) => [...prev, userMessage]);
			setError(null);

			setIsAwaitingResponse(true);

			const manager = getWsManager();
			try {
				await manager.ensureConnected(4000);
				await manager.waitForSessionReady(sessionId, 4000);
			} catch (err) {
				const error =
					err instanceof Error ? err : new Error("WebSocket not ready");
				isStreamingRef.current = false;
				sendInFlightRef.current = false;
				setIsAwaitingResponse(false);
				setError(error);
				throw error;
			}

			switch (mode) {
				case "prompt":
					// Pass the clientId for optimistic message matching
					manager.agentPrompt(sessionId, message, undefined, clientId);
					break;
				case "steer":
					manager.agentSteer(sessionId, message);
					break;
				case "follow_up":
					manager.agentFollowUp(sessionId, message);
					break;
			}
		},
		[ensureSession, getSessionConfig, nextMessageId, onSelectedSessionIdChange],
	);

	// Abort current stream
	const abort = useCallback(async () => {
		const sessionId = activeSessionIdRef.current;
		if (!sessionId) return;

		setIsAwaitingResponse(false);
		const manager = getWsManager();
		manager.agentAbort(sessionId);
	}, []);

	// Compact session
	const compact = useCallback(async (customInstructions?: string) => {
		const sessionId = activeSessionIdRef.current;
		if (!sessionId) return;

		const manager = getWsManager();
		manager.agentCompact(sessionId, customInstructions);
	}, []);

	// New session - creates a brand new session with a new UUID
	const newSession = useCallback(async () => {
		// Clear local state
		setMessages([]);
		streamingMessageRef.current = null;
		isStreamingRef.current = false;
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
		isStreamingRef.current = false;
		setIsStreaming(false);
		setIsAwaitingResponse(false);
		setError(null);
		messageIdRef.current = 0;

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
	}, [getSessionConfig]);

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
				setIsConnected(connectionState === "connected");
			},
		);

		return unsubscribe;
	}, []);

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
			// Load cached messages for this session.
			const cached = readCachedSessionMessages(
				activeSessionId,
				resolvedStorageKeyPrefix,
			);
			if (cached.length > 0) {
				setMessages(cached);
				messageIdRef.current = getMaxMessageId(cached);
				const lastAssistant = [...cached]
					.reverse()
					.find((msg) => msg.role === "assistant");
				lastAssistantMessageIdRef.current = lastAssistant?.id ?? null;
			} else {
				setMessages([]);
				messageIdRef.current = 0;
				lastAssistantMessageIdRef.current = null;
			}

			// Reset streaming and agent state for the new session.
			// Clearing state prevents stale sessionName from the previous
			// session being applied to the new one as a title.
			streamingMessageRef.current = null;
			setState(null);
			setIsStreaming(false);
			isStreamingRef.current = false;
			setIsAwaitingResponse(false);
			setError(null);
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
				console.log(
					`[useChat] Resync received for ${_sessionId}: ` +
						`state=${stateData ? "ok" : "null"}, messages=${serverMessages.length}`,
				);

				// Apply state
				if (stateData) {
					const nextState = stateData as AgentState;
					setState(nextState);
					if (nextState?.isStreaming === true) {
						setIsStreaming(true);
						isStreamingRef.current = true;
					} else {
						setIsStreaming(false);
						isStreamingRef.current = false;
						setIsAwaitingResponse(false);
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
					const displayMessages = normalizeMessages(
						serverMessages as RawMessage[],
						`resync-${_sessionId}`,
					);
					if (displayMessages.length > 0) {
						setMessages((prev) => mergeServerMessages(prev, displayMessages));
						messageIdRef.current = getMaxMessageId(displayMessages);
						const lastAssistant = [...displayMessages]
							.reverse()
							.find((msg) => msg.role === "assistant");
						lastAssistantMessageIdRef.current = lastAssistant?.id ?? null;
					}
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
		if (!manager.isSessionReady(activeSessionId) && !isStreamingRef.current) {
			const sid = activeSessionId;
			manager
				.ensureConnected(4000)
				.then(async () => {
					// Re-check streaming state after async wait
					if (isStreamingRef.current) return;

					// Check if this session is active on the runner
					try {
						const activeSessions = await manager.agentListSessions();
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
								// CRITICAL: When reattaching to a streaming session, fetch
								// the current messages from Pi. hstry only has completed
								// turns; Pi's get_messages returns the live window.
								// Use forceMessageSyncRef to ensure the response is NOT
								// deferred (which would happen since we're streaming).
								console.log(
									"[useChat] Fetching live messages from Pi for streaming session:",
									sid,
								);
								forceMessageSyncRef.current.add(sid);
								manager.agentGetMessages(sid);

								// Note: isStreamingRef set after get_messages to reduce
								// race condition window. The forceMessageSyncRef ensures
								// the response is applied even if we're streaming.
								setIsStreaming(true);
								isStreamingRef.current = true;
								setBusyForEvent(sid, true);
							}
						} else {
							// Session ID didn't appear in list_sessions. It may still
							// be active under a different alias (e.g. Pi native ID), so
							// probe with get_state before falling back to history.
							try {
								await manager.agentGetStateWait(sid);

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
								// Session is not on the runner -- just fetch
								// historical state and messages.
								manager.agentGetState(sid);
								void fetchHistoryMessages(sid);
							}
						}
					} catch {
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
			if (unsubscribeRef.current) {
				unsubscribeRef.current();
				unsubscribeRef.current = null;
			}
			unsubscribeResync();
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [activeSessionId, resolvedStorageKeyPrefix, getSessionConfig]);

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
	// user message is added, and cleared on agent.idle/stream.done. Syncing
	// from the React state would overwrite the ref with `false` on the next
	// render (before stream events arrive), creating a window where incoming
	// get_messages responses are not deferred and overwrite the optimistic
	// user message. The ref is the source of truth for deferral logic; the
	// React state is for rendering.

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

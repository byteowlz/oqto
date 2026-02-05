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

import {
	createPiSessionId,
	isPendingSessionId,
	normalizeWorkspacePath,
} from "@/lib/session-utils";
import { getWsManager } from "@/lib/ws-manager";
import type { AgentWsEvent, WsMuxConnectionState } from "@/lib/ws-mux-types";
import type { CommandResponse, SessionConfig } from "@/lib/canonical-types";
import { useCallback, useEffect, useRef, useState } from "react";
import {
	readCachedSessionMessages,
	sanitizeStorageKey,
	transferCachedSessionMessages,
	writeCachedSessionMessages,
} from "./cache";
import { getMaxPiMessageId, mergeServerMessages, normalizePiContentToParts, normalizePiMessages } from "./message-utils";
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
	const isStreamingRef = useRef(false);
	// Deferred server messages received while streaming (applied on agent.idle)
	const deferredServerMessagesRef = useRef<unknown[] | null>(null);
	// Stable ref for the agent event handler so the subscription effect doesn't
	// re-run when callback identity changes (which would reset streaming state).
	const handleAgentEventRef = useRef<((event: AgentWsEvent) => void) | null>(null);

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

	const getSessionConfig = useCallback((): SessionConfig | undefined => {
		if (normalizedWorkspacePath) {
			return { harness: "pi", cwd: normalizedWorkspacePath };
		}
		return { harness: "pi" };
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
			const isStreaming = streamingMessageRef.current !== null || isStreamingRef.current;
			if (
				["stream.message_start", "stream.text_delta", "stream.done",
				"tool.start", "tool.end", "agent.working", "agent.idle",
				].includes(eventType)
			) {
				console.log(
					`[useChat] Streaming event: ${eventType}, isStreaming=${isStreaming}, ref=${streamingMessageRef.current?.id}`,
				);
			}

			switch (eventType) {
				// -- Streaming lifecycle --
				case "stream.message_start": {
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

				// -- Text delta (incremental) --
				case "stream.text_delta": {
					const delta = event.delta as string | undefined;
					if (!delta) break;
					const currentMsg = ensureAssistantMessage(true);
					const lastPart = currentMsg.parts[currentMsg.parts.length - 1];
					if (lastPart?.type === "text") {
						lastPart.content += delta;
					} else {
						currentMsg.parts.push({ type: "text", content: delta });
					}
					scheduleStreamingUpdate();
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
						lastPart.content += delta;
					} else {
						currentMsg.parts.push({ type: "thinking", content: delta });
					}
					scheduleStreamingUpdate();
					setIsAwaitingResponse(false);
					break;
				}

				// -- Tool call being assembled by LLM --
				case "stream.tool_call_start": {
					const toolCallId = event.tool_call_id as string;
					const name = event.name as string;
					const targetMessage = ensureAssistantMessage(true);
					const alreadyPresent = targetMessage.parts.some(
						(p) => p.type === "tool_use" && p.id === toolCallId,
					);
					if (!alreadyPresent) {
						const part: PiMessagePart = {
							type: "tool_use",
							id: toolCallId,
							name,
							input: undefined,
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
						(p) => p.type === "tool_use" && p.id === toolCall.id,
					);
					if (existingPart && existingPart.type === "tool_use") {
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
					// Ensure there's a tool_use part for this tool (in case we missed
					// stream.tool_call_start, e.g. on reconnect)
					const targetMessage = ensureAssistantMessage(true);
					const existing = targetMessage.parts.find(
						(p) => p.type === "tool_use" && p.id === toolCallId,
					);
					if (!existing) {
						const part: PiMessagePart = {
							type: "tool_use",
							id: toolCallId,
							name,
							input,
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
					const matchingToolUse = targetMessage.parts.find(
						(p) => p.type === "tool_use" && p.id === toolCallId,
					);
					const part: PiMessagePart = {
						type: "tool_result",
						id: toolCallId,
						name:
							name ||
							(matchingToolUse?.type === "tool_use"
								? matchingToolUse.name
								: undefined),
						content: output,
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

				// -- Agent idle (streaming ended) --
				case "agent.idle": {
					setIsStreaming(false);
					isStreamingRef.current = false;
					setIsAwaitingResponse(false);
					if (streamingMessageRef.current) {
						streamingMessageRef.current.isStreaming = false;
						streamingMessageRef.current = null;
					}
					// Apply deferred server messages that arrived during streaming
					const deferred = deferredServerMessagesRef.current;
					if (deferred && Array.isArray(deferred)) {
						deferredServerMessagesRef.current = null;
						const displayMessages = normalizePiMessages(
							deferred,
							`server-${event.session_id}`,
						);
						if (displayMessages.length > 0) {
							setMessages((prev) => mergeServerMessages(prev, displayMessages));
							messageIdRef.current =
								getMaxPiMessageId(displayMessages);
							const lastAssistant = [...displayMessages]
								.reverse()
								.find((msg) => msg.role === "assistant");
							lastAssistantMessageIdRef.current =
								lastAssistant?.id ?? null;
						}
						if (isPiDebugEnabled()) {
							console.debug(
								"[useChat] Applied deferred messages on idle:",
								event.session_id,
								displayMessages.length,
							);
						}
					}
					break;
				}

				// -- Agent working (streaming started) --
				case "agent.working": {
					setIsAwaitingResponse(false);
					break;
				}

				// -- Agent error --
				case "agent.error": {
					const errMsg = (event.error as string) || "Unknown error";
					const recoverable = event.recoverable as boolean;
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
						!recoverable &&
						(errMsg.includes("PiSessionNotFound") ||
							errMsg.includes("SessionNotFound") ||
							errMsg.includes("Response channel closed"));
					if (shouldRecover && now - lastSessionRecoveryRef.current > 5000) {
						lastSessionRecoveryRef.current = now;
						const manager = getWsManager();
						manager.agentCreateSession(
							sessionId as string,
							getSessionConfig(),
						);
						setTimeout(() => {
							manager.agentGetState(sessionId as string);
							manager.agentGetMessages(sessionId as string);
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
							parts: streamingMessageRef.current.parts.map((p) => ({
								...p,
							})),
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

				// -- Compaction --
				case "compact.start": {
					const currentMsg = ensureAssistantMessage(false);
					const part: PiMessagePart = {
						type: "compaction",
						content: "Compacting context...",
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
						const errText =
							(event.error as string) || "Compaction failed";
						const currentMsg = ensureAssistantMessage(false);
						const part: PiMessagePart = {
							type: "error",
							content: errText,
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
										context_window: 0,
										max_tokens: 0,
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
				if (streamingMessageRef.current || isStreamingRef.current) {
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
					const displayMessages = normalizePiMessages(
						msgs,
						`server-${event.session_id}`,
					);

					if (displayMessages.length > 0) {
						setMessages((prev) => mergeServerMessages(prev, displayMessages));
						messageIdRef.current =
							getMaxPiMessageId(displayMessages);
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
							// if we're not already streaming.
							if (!streamingMessageRef.current && !isStreamingRef.current) {
								const manager = getWsManager();
								manager.agentGetMessages(event.session_id);
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
								const nextState = resp.data as PiState;
								setState(nextState);
								if (nextState?.is_streaming === false) {
									setIsStreaming(false);
									setIsAwaitingResponse(false);
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
						if (streamingMessageRef.current || isStreamingRef.current) {
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
						if (resp.success && resp.data) {
							const data = resp.data as { messages?: unknown[] };
							const msgs = data.messages;
							if (Array.isArray(msgs)) {
								const displayMessages = normalizePiMessages(
									msgs,
									`server-${event.session_id}`,
								);
							if (displayMessages.length > 0) {
								setMessages((prev) => mergeServerMessages(prev, displayMessages));
								messageIdRef.current =
									getMaxPiMessageId(displayMessages);
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
								if (shouldRecover && now - lastSessionRecoveryRef.current > 5000) {
									lastSessionRecoveryRef.current = now;
									const manager = getWsManager();
									manager.agentCreateSession(
										sessionId as string,
										getSessionConfig(),
									);
									setTimeout(() => {
										manager.agentGetState(sessionId as string);
										manager.agentGetMessages(sessionId as string);
									}, 250);
								}
							}
							break;
						}
					}
					break;
				}

				default: {
					if (isPiDebugEnabled()) {
						console.debug(
							"[useChat] Unhandled canonical event:",
							eventType,
						);
					}
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
		const sessionConfig = getSessionConfig();
		// Use the stable ref wrapper so this callback doesn't depend on
		// handleAgentEvent identity (which changes frequently).
		const stableHandler = (event: AgentWsEvent) => {
			handleAgentEventRef.current?.(event);
		};
		unsubscribeRef.current?.();
		unsubscribeRef.current = manager.subscribeAgentSession(
			sessionId,
			stableHandler,
			sessionConfig,
		);

		await manager.ensureConnected(4000);
		manager.agentCreateSession(sessionId, sessionConfig);
		await manager.waitForSessionReady(sessionId, 4000);
		return sessionId;
	}, [getSessionConfig, onSelectedSessionIdChange]);

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
				const stableHandler = (event: AgentWsEvent) => {
					handleAgentEventRef.current?.(event);
				};
				unsubscribeRef.current?.();
				unsubscribeRef.current = manager.subscribeAgentSession(
					options.sessionId,
					stableHandler,
					sessionConfig,
				);
			}
			if (!sessionId) {
				// Clear local state for the new session.
				setMessages([]);
				streamingMessageRef.current = null;
				setIsStreaming(false);
				isStreamingRef.current = false;
				setError(null);
				messageIdRef.current = 0;
				sessionId = await ensureSession();
			}

			// Mark as streaming IMMEDIATELY so that any server messages
			// (from get_messages, messages events, etc.) arriving between now
			// and stream.message_start are deferred instead of overwriting
			// the optimistic user message.
			isStreamingRef.current = true;

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
				await manager.waitForSessionReady(sessionId, 4000);
			} catch (err) {
				const error =
					err instanceof Error ? err : new Error("WebSocket not ready");
				isStreamingRef.current = false;
				setIsAwaitingResponse(false);
				setError(error);
				throw error;
			}

			switch (mode) {
				case "prompt":
					manager.agentPrompt(sessionId, message);
					break;
				case "steer":
					manager.agentSteer(sessionId, message);
					break;
				case "follow_up":
					manager.agentFollowUp(sessionId, message);
					break;
			}
		},
		[
			ensureSession,
			getSessionConfig,
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
		manager.agentGetMessages(sessionId);

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

	// Subscribe to Pi session when active session changes.
	// IMPORTANT: This effect must NOT depend on handleAgentEvent or other
	// frequently-changing callback refs. We use handleAgentEventRef (a stable
	// ref) to dispatch events. This prevents the effect from re-running during
	// streaming (which would reset streamingMessageRef and lose the user message).
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
			// Load cached messages for this session -- skip if we're actively
			// streaming to avoid overwriting in-progress content.
			if (!streamingMessageRef.current && !isStreamingRef.current) {
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
			}
			streamingMessageRef.current = null;
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

	useEffect(() => {
		isStreamingRef.current = isStreaming;
	}, [isStreaming]);

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

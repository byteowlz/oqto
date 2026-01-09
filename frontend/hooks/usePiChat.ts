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
	| { type: "separator"; content: string };

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

/**
 * Hook for managing Pi chat in Main Chat mode.
 * Handles WebSocket connection, message streaming, and state.
 */
export function usePiChat(options: UsePiChatOptions = {}): UsePiChatReturn {
	const { autoConnect = true, onMessageComplete, onError } = options;

	const [state, setState] = useState<PiState | null>(null);
	const [messages, setMessages] = useState<PiDisplayMessage[]>([]);
	const [isConnected, setIsConnected] = useState(false);
	const [isStreaming, setIsStreaming] = useState(false);
	const [error, setError] = useState<Error | null>(null);

	const wsRef = useRef<WebSocket | null>(null);
	const streamingMessageRef = useRef<PiDisplayMessage | null>(null);
	const messageIdRef = useRef(0);

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
							// Update messages state
							setMessages((prev) => {
								const idx = prev.findIndex((m) => m.id === currentTextMsg.id);
								if (idx >= 0) {
									const updated = [...prev];
									updated[idx] = { ...currentTextMsg };
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
									updated[idx] = { ...currentToolMsg };
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
									updated[idx] = { ...currentResultMsg };
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
							const completedMessage = { ...streamingMessageRef.current };
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
						// Reload messages after compaction
						refresh();
						break;
				}
			} catch (e) {
				console.error("Failed to parse Pi WebSocket message:", e);
			}
		},
		[onMessageComplete, onError],
	);

	// Connect to WebSocket
	const connect = useCallback(() => {
		if (wsRef.current?.readyState === WebSocket.OPEN) return;

		const ws = createMainChatPiWebSocket();

		ws.onopen = () => {
			setIsConnected(true);
			setError(null);
		};

		ws.onmessage = handleWsMessage;

		ws.onerror = () => {
			const err = new Error("WebSocket connection error");
			setError(err);
			onError?.(err);
		};

		ws.onclose = () => {
			setIsConnected(false);
		};

		wsRef.current = ws;
	}, [handleWsMessage, onError]);

	// Disconnect from WebSocket
	const disconnect = useCallback(() => {
		if (wsRef.current) {
			wsRef.current.close();
			wsRef.current = null;
		}
		setIsConnected(false);
	}, []);

	// Refresh messages and state from server
	const refresh = useCallback(async () => {
		try {
			const [piState, dbMessages] = await Promise.all([
				getMainChatPiState(),
				getMainChatPiHistory(),
			]);
			setState(piState);
			setMessages(convertDbToDisplayMessages(dbMessages));
		} catch (e) {
			const err = e instanceof Error ? e : new Error("Failed to refresh");
			setError(err);
			onError?.(err);
		}
	}, [convertDbToDisplayMessages, onError]);

	// Send a message via WebSocket (which persists user messages)
	const send = useCallback(
		async (message: string, options?: PiSendOptions) => {
			// Must be connected to send
			if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
				const err = new Error("Not connected to chat server");
				setError(err);
				onError?.(err);
				return;
			}

			setError(null);
			let mode: PiSendMode = options?.mode ?? "prompt";
			if (isStreaming && mode === "prompt" && (options?.queueIfStreaming ?? true)) {
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
				wsRef.current.send(JSON.stringify({ type: mode, message }));
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
			// Add separator to display
			const separatorDisplay: PiDisplayMessage = {
				id: `pi-db-${separatorMsg.id}`,
				role: "system",
				parts: [{ type: "separator", content: "New conversation started" }],
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

	// Initialize on mount
	useEffect(() => {
		let mounted = true;

		const init = async () => {
			try {
				// Start or get session
				const piState = await startMainChatPiSession();
				if (!mounted) return;
				setState(piState);

				// Load persistent history from database (survives Pi session restarts)
				const dbMessages = await getMainChatPiHistory();
				if (!mounted) return;
				setMessages(convertDbToDisplayMessages(dbMessages));

				// Connect WebSocket
				if (autoConnect) {
					connect();
				}
			} catch (e) {
				if (!mounted) return;
				const err = e instanceof Error ? e : new Error("Failed to initialize");
				setError(err);
				onError?.(err);
			}
		};

		init();

		return () => {
			mounted = false;
			disconnect();
		};
	}, [autoConnect, connect, disconnect, convertDbToDisplayMessages, onError]);

	return {
		state,
		messages,
		isConnected,
		isStreaming,
		error,
		send,
		abort,
		newSession,
		refresh,
		connect,
		disconnect,
	};
}

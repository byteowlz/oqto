/**
 * React hooks for WebSocket-based real-time communication with Octo backend.
 *
 * These hooks wrap the OctoWsClient singleton to provide React-friendly APIs
 * for subscribing to session events and managing connection state.
 */

import {
	type ConnectionState,
	type WsEvent,
	getWsClient,
} from "@/lib/ws-client";
import {
	useCallback,
	useEffect,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";

// ============================================================================
// Connection State Hook
// ============================================================================

/**
 * Hook to track WebSocket connection state.
 * Automatically connects when first subscriber mounts.
 */
export function useWsConnection() {
	const client = getWsClient();

	const subscribe = useCallback(
		(callback: () => void) => {
			return client.onConnectionState(() => callback());
		},
		[client],
	);

	const getSnapshot = useCallback(() => client.state, [client]);

	const connectionState = useSyncExternalStore(
		subscribe,
		getSnapshot,
		getSnapshot,
	);

	// Auto-connect on mount if disconnected
	useEffect(() => {
		if (connectionState === "disconnected") {
			client.connect();
		}
	}, [client, connectionState]);

	return {
		connectionState,
		isConnected: connectionState === "connected",
		isReconnecting: connectionState === "reconnecting",
		isFailed: connectionState === "failed",
		connect: () => client.connect(),
		disconnect: () => client.disconnect(),
	};
}

// ============================================================================
// Session Event Types (mapped from backend events)
// ============================================================================

export type SessionEvent =
	| { type: "session.idle"; sessionId: string }
	| { type: "session.busy"; sessionId: string }
	| { type: "session.unavailable"; sessionId: string }
	| { type: "message.updated"; sessionId: string }
	| { type: "permission.updated"; permission: Permission }
	| { type: "permission.replied"; permissionId: string; sessionId: string }
	| { type: "transport.mode"; mode: "ws"; reason: string }
	| { type: "server.connected" }
	| {
			type: "agent.reconnecting";
			sessionId: string;
			attempt: number;
			delayMs: number;
	  }
	| { type: "agent.disconnected"; sessionId: string; reason: string }
	| {
			type: "session.error";
			sessionId: string;
			errorType: string;
			message: string;
			details?: unknown;
	  }
	| {
			type: "a2ui.surface";
			sessionId: string;
			surfaceId: string;
			messages: unknown[];
			blocking: boolean;
			requestId?: string;
	  }
	| {
			type: "a2ui.action_resolved";
			sessionId: string;
			requestId: string;
	  }
	| { type: "raw"; event: WsEvent };

// Internal Permission type for WS events - matches backend WsEvent::PermissionRequest
export type Permission = {
	id: string;
	sessionID: string;
	type: string; // Permission type (e.g., "bash", "edit")
	title: string;
	pattern?: string | string[];
	metadata?: Record<string, unknown>;
};

export type SessionEventCallback = (event: SessionEvent) => void;

// ============================================================================
// Session Subscription Hook
// ============================================================================

/**
 * Hook to subscribe to events for a specific session via WebSocket.
 *
 * Replaces the SSE-based subscribeToEvents pattern with a more reliable
 * WebSocket connection managed by the backend.
 *
 * @param sessionId - The workspace session ID to subscribe to
 * @param onEvent - Callback for session events
 * @param options - Configuration options
 */
export function useWsSession(
	sessionId: string | undefined,
	onEvent: SessionEventCallback,
	options?: {
		/** Whether subscription is enabled (default: true) */
		enabled?: boolean;
		/** Base URL for OpenCode API (used for query cache invalidation) */
		opencodeBaseUrl?: string;
		/** OpenCode session ID within the workspace session */
		activeSessionId?: string;
	},
) {
	const client = getWsClient();
	const { enabled = true } = options ?? {};

	// Keep callback ref stable to avoid re-subscriptions
	const onEventRef = useRef(onEvent);
	onEventRef.current = onEvent;

	// Track subscription state
	const [isSubscribed, setIsSubscribed] = useState(false);

	// Subscribe to session events
	useEffect(() => {
		if (!sessionId || !enabled) {
			setIsSubscribed(false);
			return;
		}

		// Subscribe to the session
		client.subscribeSession(sessionId);
		setIsSubscribed(true);

		let deltaTimer: ReturnType<typeof setTimeout> | null = null;
		let deltaLastEmitAt = 0;
		let deltaPending = false;

		const emitMessageUpdated = () => {
			onEventRef.current({ type: "message.updated", sessionId });
		};

		const scheduleMessageUpdated = (minIntervalMs: number) => {
			deltaPending = true;
			if (deltaTimer) return;

			const elapsed = Date.now() - deltaLastEmitAt;
			const wait = Math.max(0, minIntervalMs - elapsed);
			deltaTimer = setTimeout(() => {
				deltaTimer = null;
				if (!deltaPending) return;
				deltaPending = false;
				deltaLastEmitAt = Date.now();
				emitMessageUpdated();
			}, wait);
		};

		// Handle events for this session
		const unsubscribe = client.onSessionEvent(sessionId, (event: WsEvent) => {
			// Deltas can come in extremely frequently; throttle them to keep typing responsive.
			if (event.type === "text_delta" || event.type === "thinking_delta") {
				scheduleMessageUpdated(250);
				return;
			}

			const mapped = mapWsEventToSessionEvent(event, sessionId);
			if (mapped) {
				onEventRef.current(mapped);
			}

			// Note: React Query cache invalidation removed - SessionsApp uses manual message loading
			// via loadMessages() and requestMessageRefresh(), not useOpenCodeMessages hook.
			// The event callback above handles notifying components of changes.
		});

		// Emit initial transport mode event
		onEventRef.current({
			type: "transport.mode",
			mode: "ws",
			reason: "websocket",
		});

		return () => {
			unsubscribe();
			client.unsubscribeSession(sessionId);
			setIsSubscribed(false);
			if (deltaTimer) {
				clearTimeout(deltaTimer);
				deltaTimer = null;
			}
		};
	}, [sessionId, enabled, client]);

	// Action methods
	const sendMessage = useCallback(
		(
			message: string,
			attachments?: {
				type: string;
				url?: string;
				data?: string;
				media_type?: string;
				filename?: string;
			}[],
		) => {
			if (sessionId) {
				client.sendMessage(sessionId, message, attachments);
			}
		},
		[client, sessionId],
	);

	const abort = useCallback(() => {
		if (sessionId) {
			client.abort(sessionId);
		}
	}, [client, sessionId]);

	const replyPermission = useCallback(
		(permissionId: string, granted: boolean) => {
			if (sessionId) {
				client.replyPermission(sessionId, permissionId, granted);
			}
		},
		[client, sessionId],
	);

	const refreshSession = useCallback(() => {
		if (sessionId) {
			client.refreshSession(sessionId);
		}
	}, [client, sessionId]);

	return {
		isSubscribed,
		sendMessage,
		abort,
		replyPermission,
		refreshSession,
	};
}

// ============================================================================
// Event Mapping
// ============================================================================

/**
 * Maps WebSocket events from the backend to the session event format
 * used by the frontend components.
 */
function mapWsEventToSessionEvent(
	event: WsEvent,
	sessionId: string,
): SessionEvent | null {
	switch (event.type) {
		case "session_idle":
			return { type: "session.idle", sessionId };

		case "session_busy":
			return { type: "session.busy", sessionId };

		case "agent_disconnected":
			if ("reason" in event) {
				return {
					type: "agent.disconnected",
					sessionId,
					reason: event.reason,
				};
			}
			return null;

		case "agent_reconnecting":
			if ("attempt" in event && "delay_ms" in event) {
				return {
					type: "agent.reconnecting",
					sessionId,
					attempt: event.attempt,
					delayMs: event.delay_ms,
				};
			}
			return null;

		case "agent_connected":
			return { type: "server.connected" };

		case "error":
			if ("message" in event) {
				return {
					type: "session.error",
					sessionId,
					errorType: "BackendError",
					message: event.message,
					details: event,
				};
			}
			return null;

		case "tool_end":
			if ("is_error" in event && event.is_error) {
				const result = "result" in event ? event.result : undefined;
				const message = (() => {
					if (typeof result === "string") return result;
					if (result && typeof result === "object") {
						const record = result as Record<string, unknown>;
						if (typeof record.message === "string") return record.message;
					}
					return "Tool execution failed";
				})();
				const toolName = "tool_name" in event ? event.tool_name : "tool";
				return {
					type: "session.error",
					sessionId,
					errorType: `ToolError:${toolName}`,
					message,
					details: {
						type: "tool_end",
						toolName,
						toolCallId: "tool_call_id" in event ? event.tool_call_id : undefined,
						result,
					},
				};
			}
			return null;

		case "message_updated":
		case "message_end":
		case "text_delta":
		case "thinking_delta":
			return { type: "message.updated", sessionId };

		case "permission_request":
			if ("permission_id" in event && "permission_type" in event) {
				return {
					type: "permission.updated",
					permission: {
						id: event.permission_id,
						sessionID: sessionId,
						type: event.permission_type,
						title: event.title ?? "",
						pattern: event.pattern,
						metadata: event.metadata,
					},
				};
			}
			return null;

		case "permission_resolved":
			if ("permission_id" in event) {
				return {
					type: "permission.replied",
					permissionId: event.permission_id,
					sessionId,
				};
			}
			return null;

		case "session_error":
			if ("error_type" in event && "message" in event) {
				return {
					type: "session.error",
					sessionId,
					errorType: event.error_type,
					message: event.message,
					details: event.details,
				};
			}
			return null;

		case "opencode_event":
			// Pass through raw OpenCode events for components that need them
			return { type: "raw", event };

		case "a2ui_surface":
			if ("surface_id" in event && "messages" in event) {
				return {
					type: "a2ui.surface",
					sessionId,
					surfaceId: event.surface_id,
					messages: event.messages as unknown[],
					blocking: event.blocking ?? false,
					requestId: event.request_id,
				};
			}
			return null;

		case "a2ui_action_resolved":
			if ("request_id" in event) {
				return {
					type: "a2ui.action_resolved",
					sessionId,
					requestId: event.request_id,
				};
			}
			return null;

		default:
			// Return raw event for unhandled types
			return { type: "raw", event };
	}
}

// ============================================================================
// Combined Hook
// ============================================================================

/**
 * Convenience hook that combines connection state and session subscription.
 * Use this when you need both connection awareness and session events.
 */
export function useWsSessionWithConnection(
	sessionId: string | undefined,
	onEvent: SessionEventCallback,
	options?: {
		enabled?: boolean;
		opencodeBaseUrl?: string;
		activeSessionId?: string;
	},
) {
	const connection = useWsConnection();
	const session = useWsSession(sessionId, onEvent, {
		...options,
		enabled: (options?.enabled ?? true) && connection.isConnected,
	});

	return {
		...connection,
		...session,
	};
}

// ============================================================================
// SSE-Compatible Event Type (matches opencode-client's EventCallback)
// ============================================================================

/**
 * Event type that matches the opencode-client SSE event format.
 * This allows useWsSessionEvents to be a drop-in replacement for subscribeToEvents.
 */
export type LegacyEvent = {
	type: string;
	properties?: Record<string, unknown> | null;
};

export type LegacyEventCallback = (event: LegacyEvent) => void;

// ============================================================================
// SSE-Compatible Hook
// ============================================================================

/**
 * Hook that provides the same event interface as subscribeToEvents() from opencode-client.
 * Use this as a drop-in replacement when features.websocket_events is true.
 *
 * @param workspaceSessionId - The workspace session ID (container/process)
 * @param onEvent - Callback matching the SSE subscribeToEvents format
 * @param options - Configuration options
 */
export function useWsSessionEvents(
	workspaceSessionId: string | undefined,
	onEvent: LegacyEventCallback,
	options?: {
		/** Whether subscription is enabled (default: true) */
		enabled?: boolean;
		/** Base URL for OpenCode API */
		opencodeBaseUrl?: string;
		/** OpenCode session ID within the workspace session */
		activeSessionId?: string;
	},
) {
	const client = getWsClient();
	const { enabled = true, opencodeBaseUrl, activeSessionId } = options ?? {};

	// Keep callback ref stable to avoid re-subscriptions
	const onEventRef = useRef(onEvent);
	onEventRef.current = onEvent;

	// Track subscription state
	const [isSubscribed, setIsSubscribed] = useState(false);
	const [transportMode, setTransportMode] = useState<"ws" | "reconnecting">(
		"ws",
	);

	// Connection state tracking
	const connection = useWsConnection();

	// Subscribe to workspace session events
	useEffect(() => {
		if (!workspaceSessionId || !enabled) {
			setIsSubscribed(false);
			return;
		}

		// Subscribe to the workspace session
		client.subscribeSession(workspaceSessionId);
		setIsSubscribed(true);

		// Emit initial transport mode event
		onEventRef.current({
			type: "transport.mode",
			properties: { mode: "ws", reason: "websocket" },
		});

		let deltaTimer: ReturnType<typeof setTimeout> | null = null;
		let deltaLastEmitAt = 0;
		let deltaPending = false;

		const scheduleLegacyMessageUpdated = (sessionId: string, minIntervalMs: number) => {
			deltaPending = true;
			if (deltaTimer) return;

			const elapsed = Date.now() - deltaLastEmitAt;
			const wait = Math.max(0, minIntervalMs - elapsed);
			deltaTimer = setTimeout(() => {
				deltaTimer = null;
				if (!deltaPending) return;
				deltaPending = false;
				deltaLastEmitAt = Date.now();
				onEventRef.current({
					type: "message.updated",
					properties: { sessionId },
				});
			}, wait);
		};

		// Handle events for this session
		const unsubscribe = client.onSessionEvent(
			workspaceSessionId,
			(event: WsEvent) => {
				if (event.type === "text_delta" || event.type === "thinking_delta") {
					const sessionId =
						"session_id" in event
							? (event.session_id as string)
							: workspaceSessionId;
					scheduleLegacyMessageUpdated(sessionId, 250);
					return;
				}

				const legacy = mapWsEventToLegacyEvent(event);
				if (legacy) {
					onEventRef.current(legacy);
				}
			},
		);

		return () => {
			unsubscribe();
			client.unsubscribeSession(workspaceSessionId);
			setIsSubscribed(false);
			if (deltaTimer) {
				clearTimeout(deltaTimer);
				deltaTimer = null;
			}
		};
	}, [workspaceSessionId, enabled, client]);

	// Handle connection state changes
	useEffect(() => {
		if (connection.isReconnecting) {
			setTransportMode("reconnecting");
			onEventRef.current({
				type: "transport.mode",
				properties: { mode: "reconnecting", reason: "websocket reconnecting" },
			});
		} else if (connection.isConnected) {
			setTransportMode("ws");
			// Emit server connected when we reconnect
			onEventRef.current({ type: "server.connected", properties: {} });
		}
	}, [connection.isConnected, connection.isReconnecting]);

	return {
		isSubscribed,
		transportMode,
		connectionState: connection.connectionState,
		isConnected: connection.isConnected,
	};
}

/**
 * Maps WebSocket events to the legacy SSE event format used by opencode-client.
 */
function mapWsEventToLegacyEvent(event: WsEvent): LegacyEvent | null {
	switch (event.type) {
		case "session_idle":
			return {
				type: "session.idle",
				properties: {
					sessionId: "session_id" in event ? event.session_id : undefined,
				},
			};

		case "session_busy":
			return {
				type: "session.busy",
				properties: {
					sessionId: "session_id" in event ? event.session_id : undefined,
				},
			};

		case "agent_disconnected":
			return {
				type: "session.unavailable",
				properties: {
					sessionId: "session_id" in event ? event.session_id : undefined,
				},
			};

		case "agent_connected":
			return { type: "server.connected", properties: {} };

		case "error":
			return {
				type: "session.error",
				properties: {
					sessionID: "session_id" in event ? event.session_id : undefined,
					error: {
						name: "BackendError",
						data: { message: "message" in event ? event.message : "" },
					},
				},
			};

		case "tool_end":
			if ("is_error" in event && event.is_error) {
				const result = "result" in event ? event.result : undefined;
				const message = (() => {
					if (typeof result === "string") return result;
					if (result && typeof result === "object") {
						const record = result as Record<string, unknown>;
						if (typeof record.message === "string") return record.message;
					}
					return "Tool execution failed";
				})();
				return {
					type: "session.error",
					properties: {
						sessionID: "session_id" in event ? event.session_id : undefined,
						error: {
							name: "ToolError",
							data: {
								message,
								tool: "tool_name" in event ? event.tool_name : undefined,
								toolCallId: "tool_call_id" in event ? event.tool_call_id : undefined,
								result,
							},
						},
					},
				};
			}
			return null;

		case "message_updated":
		case "message_end":
		case "text_delta":
		case "thinking_delta":
		case "message_start":
			return {
				type: "message.updated",
				properties: {
					sessionId: "session_id" in event ? event.session_id : undefined,
				},
			};

		case "permission_request":
			if ("permission_id" in event && "permission_type" in event) {
				return {
					type: "permission.updated",
					properties: {
						id: event.permission_id,
						sessionID: "session_id" in event ? event.session_id : undefined,
						type: event.permission_type,
						title: event.title ?? "",
						pattern: event.pattern,
						metadata: event.metadata ?? {},
						// Include time for SDK compatibility
						time: { created: Date.now() },
					},
				};
			}
			return null;

		case "permission_resolved":
			if ("permission_id" in event) {
				return {
					type: "permission.replied",
					properties: {
						permissionID: event.permission_id,
						sessionID: "session_id" in event ? event.session_id : undefined,
						response: "granted" in event && event.granted ? "allow" : "deny",
					},
				};
			}
			return null;

		case "question_request":
			if ("request_id" in event && "questions" in event) {
				return {
					type: "question.asked",
					properties: {
						id: event.request_id,
						sessionID: "session_id" in event ? event.session_id : undefined,
						questions: event.questions,
						tool: event.tool,
					},
				};
			}
			return null;

		case "question_resolved":
			if ("request_id" in event) {
				return {
					type: "question.replied",
					properties: {
						requestID: event.request_id,
						sessionID: "session_id" in event ? event.session_id : undefined,
					},
				};
			}
			return null;

		case "session_error":
			if ("error_type" in event && "message" in event) {
				return {
					type: "session.error",
					properties: {
						sessionID: "session_id" in event ? event.session_id : undefined,
						error: {
							name: event.error_type,
							data: { message: event.message },
						},
					},
				};
			}
			return null;

		case "compaction_start":
			return {
				type: "compaction.start",
				properties: {
					sessionID: "session_id" in event ? event.session_id : undefined,
					reason: "reason" in event ? event.reason : undefined,
				},
			};

		case "compaction_end":
			return {
				type: "compaction.end",
				properties: {
					sessionID: "session_id" in event ? event.session_id : undefined,
					success: "success" in event ? event.success : undefined,
				},
			};

		case "opencode_event":
			// Pass through the inner event type if available
			if ("event_type" in event && "data" in event) {
				return {
					type: event.event_type,
					properties: event.data as Record<string, unknown>,
				};
			}
			return null;

		case "a2ui_surface":
			if ("surface_id" in event && "messages" in event) {
				return {
					type: "a2ui.surface",
					properties: {
						sessionId: "session_id" in event ? event.session_id : undefined,
						surfaceId: event.surface_id,
						messages: event.messages,
						blocking: event.blocking ?? false,
						requestId: event.request_id,
					},
				};
			}
			return null;

		case "a2ui_action_resolved":
			if ("request_id" in event) {
				return {
					type: "a2ui.action_resolved",
					properties: {
						sessionId: "session_id" in event ? event.session_id : undefined,
						requestId: event.request_id,
					},
				};
			}
			return null;

		default:
			// Pass through unknown events
			return null;
	}
}

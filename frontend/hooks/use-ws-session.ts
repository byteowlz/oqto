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
import { useQueryClient } from "@tanstack/react-query";
import {
	useCallback,
	useEffect,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";
import { openCodeKeys } from "./use-opencode";

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
	const queryClient = useQueryClient();
	const { enabled = true, opencodeBaseUrl, activeSessionId } = options ?? {};

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

		// Handle events for this session
		const unsubscribe = client.onSessionEvent(sessionId, (event: WsEvent) => {
			const mapped = mapWsEventToSessionEvent(event, sessionId);
			if (mapped) {
				onEventRef.current(mapped);
			}

			// Invalidate query cache on message updates
			if (opencodeBaseUrl && activeSessionId) {
				if (
					event.type === "message_updated" ||
					event.type === "message_end" ||
					event.type === "session_idle"
				) {
					queryClient.invalidateQueries({
						queryKey: openCodeKeys.messages(opencodeBaseUrl, activeSessionId),
					});
				}
			}
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
		};
	}, [
		sessionId,
		enabled,
		client,
		queryClient,
		opencodeBaseUrl,
		activeSessionId,
	]);

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

		// Handle events for this session
		const unsubscribe = client.onSessionEvent(
			workspaceSessionId,
			(event: WsEvent) => {
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

		case "opencode_event":
			// Pass through the inner event type if available
			if ("event_type" in event && "data" in event) {
				return {
					type: event.event_type,
					properties: event.data as Record<string, unknown>,
				};
			}
			return null;

		default:
			// Pass through unknown events
			return null;
	}
}

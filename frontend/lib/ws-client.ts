/**
 * WebSocket client for unified real-time communication with Octo backend.
 *
 * This client provides a single WebSocket connection per user that multiplexes
 * events from multiple sessions. It handles:
 * - Automatic reconnection with exponential backoff
 * - Session subscriptions (subscribe/unsubscribe to session events)
 * - Event filtering by session ID
 * - Connection state management
 */

import { getAuthToken, getControlPlaneBaseUrl } from "./control-plane-client";

function isWsDebugEnabled(): boolean {
	if (!import.meta.env.DEV) return false;
	try {
		// Enable with: localStorage.setItem("debug:ws", "1")
		if (typeof localStorage !== "undefined") {
			return localStorage.getItem("debug:ws") === "1";
		}
	} catch {
		// ignore
	}
	return false;
}

// ============================================================================
// Event Types (from backend)
// ============================================================================

/** Base event with session_id for multiplexing */
export type WsEventBase = {
	session_id?: string;
};

/** Events sent from backend to frontend */
export type WsEvent =
	| { type: "connected" }
	| { type: "ping" }
	| { type: "error"; message: string; session_id?: string }
	| {
			type: "session_updated";
			session_id: string;
			status: string;
			workspace_path: string;
	  }
	| { type: "session_deleted"; session_id: string }
	| {
			type: "session_error";
			session_id: string;
			error_type: string;
			message: string;
			details?: unknown;
	  }
	| { type: "agent_connected"; session_id: string }
	| { type: "agent_disconnected"; session_id: string; reason: string }
	| {
			type: "agent_reconnecting";
			session_id: string;
			attempt: number;
			delay_ms: number;
	  }
	| { type: "agent_start"; session_id: string }
	| { type: "agent_end"; session_id: string; error?: string }
	| { type: "session_busy"; session_id: string }
	| { type: "session_idle"; session_id: string }
	| {
			type: "message_start";
			session_id: string;
			message_id: string;
			role: string;
	  }
	| {
			type: "text_delta";
			session_id: string;
			message_id: string;
			delta: string;
	  }
	| {
			type: "thinking_delta";
			session_id: string;
			message_id: string;
			delta: string;
	  }
	| { type: "message_end"; session_id: string; message_id: string }
	| { type: "message_updated"; session_id: string; message: unknown }
	| {
			type: "tool_start";
			session_id: string;
			tool_call_id: string;
			tool_name: string;
			input?: unknown;
	  }
	| {
			type: "tool_progress";
			session_id: string;
			tool_call_id: string;
			partial_result?: unknown;
	  }
	| {
			type: "tool_end";
			session_id: string;
			tool_call_id: string;
			tool_name: string;
			result?: unknown;
			is_error: boolean;
	  }
	| {
			type: "permission_request";
			session_id: string;
			permission_id: string;
			permission_type: string;
			title: string;
			pattern?: unknown;
			metadata?: unknown;
	  }
	| {
			type: "permission_resolved";
			session_id: string;
			permission_id: string;
			granted: boolean;
	  }
	| {
			type: "question_request";
			session_id: string;
			request_id: string;
			questions: unknown;
			tool?: unknown;
	  }
	| {
			type: "question_resolved";
			session_id: string;
			request_id: string;
	  }
	| { type: "compaction_start"; session_id: string; reason?: string }
	| { type: "compaction_end"; session_id: string; success: boolean }
	| {
			type: "opencode_event";
			session_id: string;
			event_type: string;
			data: unknown;
	  }
	| {
			type: "a2ui_surface";
			session_id: string;
			surface_id: string;
			messages: unknown[];
			blocking?: boolean;
			request_id?: string;
	  }
	| {
			type: "a2ui_action_resolved";
			session_id: string;
			request_id: string;
	  };

// ============================================================================
// Command Types (to backend)
// ============================================================================

/** Commands sent from frontend to backend */
export type WsCommand =
	| { type: "pong" }
	| { type: "subscribe"; session_id: string }
	| { type: "unsubscribe"; session_id: string }
	| {
			type: "send_message";
			session_id: string;
			message: string;
			attachments?: Attachment[];
	  }
	| { type: "send_parts"; session_id: string; parts: MessagePart[] }
	| { type: "abort"; session_id: string }
	| {
			type: "permission_reply";
			session_id: string;
			permission_id: string;
			granted: boolean;
	  }
	| {
			type: "question_reply";
			session_id: string;
			request_id: string;
			answers: unknown;
	  }
	| {
			type: "question_reject";
			session_id: string;
			request_id: string;
	  }
	| { type: "refresh_session"; session_id: string }
	| { type: "get_messages"; session_id: string; after_id?: string }
	| {
			type: "a2ui_action";
			session_id: string;
			surface_id: string;
			request_id?: string;
			action_name: string;
			source_component_id: string;
			context: Record<string, unknown>;
	  };

export type Attachment = {
	type: string;
	url?: string;
	data?: string;
	media_type?: string;
	filename?: string;
};

export type MessagePart =
	| { type: "text"; text: string }
	| { type: "image"; url: string }
	| { type: "file"; path: string };

// ============================================================================
// Connection State
// ============================================================================

export type ConnectionState =
	| "disconnected"
	| "connecting"
	| "connected"
	| "reconnecting"
	| "failed";

// ============================================================================
// Event Handler Types
// ============================================================================

export type WsEventHandler = (event: WsEvent) => void;
export type ConnectionStateHandler = (state: ConnectionState) => void;

// ============================================================================
// WebSocket Client
// ============================================================================

const MAX_RECONNECT_ATTEMPTS = 20;
const BASE_RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_DELAY_MS = 30000;
const PING_TIMEOUT_MS = 45000; // If no ping for this long, reconnect

/** Singleton WebSocket client for Octo */
class OctoWsClient {
	private ws: WebSocket | null = null;
	private connectionState: ConnectionState = "disconnected";
	private reconnectAttempt = 0;
	private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
	private pingTimeout: ReturnType<typeof setTimeout> | null = null;

	// Event handlers
	private eventHandlers: Map<string, Set<WsEventHandler>> = new Map();
	private globalEventHandlers: Set<WsEventHandler> = new Set();
	private connectionStateHandlers: Set<ConnectionStateHandler> = new Set();

	// Session subscriptions
	private subscribedSessions: Set<string> = new Set();
	private pendingSubscriptions: Set<string> = new Set();

	/** Get the current connection state */
	get state(): ConnectionState {
		return this.connectionState;
	}

	/** Check if connected */
	get isConnected(): boolean {
		return this.connectionState === "connected";
	}

	/** Connect to the WebSocket server */
	connect(): void {
		if (this.ws?.readyState === WebSocket.OPEN) {
			if (isWsDebugEnabled()) {
				console.debug("[ws] Already connected");
			}
			return;
		}

		if (this.ws?.readyState === WebSocket.CONNECTING) {
			if (isWsDebugEnabled()) {
				console.debug("[ws] Connection already in progress");
			}
			return;
		}

		this.setConnectionState("connecting");
		this.createWebSocket();
	}

	/** Disconnect from the WebSocket server */
	disconnect(): void {
		this.clearReconnectTimeout();
		this.clearPingTimeout();
		this.setConnectionState("disconnected");

		if (this.ws) {
			this.ws.onclose = null; // Prevent reconnection
			this.ws.close(1000, "Client disconnect");
			this.ws = null;
		}
	}

	/** Subscribe to events for a session */
	subscribeSession(sessionId: string): void {
		if (this.subscribedSessions.has(sessionId)) {
			return;
		}

		this.subscribedSessions.add(sessionId);

		if (this.isConnected) {
			this.send({ type: "subscribe", session_id: sessionId });
		} else {
			this.pendingSubscriptions.add(sessionId);
			// Auto-connect if not connected
			if (this.connectionState === "disconnected") {
				this.connect();
			}
		}
	}

	/** Unsubscribe from a session's events */
	unsubscribeSession(sessionId: string): void {
		this.subscribedSessions.delete(sessionId);
		this.pendingSubscriptions.delete(sessionId);

		if (this.isConnected) {
			this.send({ type: "unsubscribe", session_id: sessionId });
		}
	}

	/** Check if subscribed to a session */
	isSubscribed(sessionId: string): boolean {
		return this.subscribedSessions.has(sessionId);
	}

	/** Send a message to a session */
	sendMessage(
		sessionId: string,
		message: string,
		attachments?: Attachment[],
	): void {
		this.send({
			type: "send_message",
			session_id: sessionId,
			message,
			attachments,
		});
	}

	/** Send message parts to a session */
	sendParts(sessionId: string, parts: MessagePart[]): void {
		this.send({ type: "send_parts", session_id: sessionId, parts });
	}

	/** Abort current operation in a session */
	abort(sessionId: string): void {
		this.send({ type: "abort", session_id: sessionId });
	}

	/** Reply to a permission request */
	replyPermission(
		sessionId: string,
		permissionId: string,
		granted: boolean,
	): void {
		this.send({
			type: "permission_reply",
			session_id: sessionId,
			permission_id: permissionId,
			granted,
		});
	}

	/** Reply to a question request */
	replyQuestion(sessionId: string, requestId: string, answers: unknown): void {
		this.send({
			type: "question_reply",
			session_id: sessionId,
			request_id: requestId,
			answers,
		});
	}

	/** Reject a question request */
	rejectQuestion(sessionId: string, requestId: string): void {
		this.send({
			type: "question_reject",
			session_id: sessionId,
			request_id: requestId,
		});
	}

	/** Request session state refresh */
	refreshSession(sessionId: string): void {
		this.send({ type: "refresh_session", session_id: sessionId });
	}

	/** Request messages for a session */
	getMessages(sessionId: string, afterId?: string): void {
		this.send({
			type: "get_messages",
			session_id: sessionId,
			after_id: afterId,
		});
	}

	/** Send A2UI user action */
	sendA2UIAction(
		sessionId: string,
		surfaceId: string,
		actionName: string,
		sourceComponentId: string,
		context: Record<string, unknown>,
		requestId?: string,
	): void {
		this.send({
			type: "a2ui_action",
			session_id: sessionId,
			surface_id: surfaceId,
			request_id: requestId,
			action_name: actionName,
			source_component_id: sourceComponentId,
			context,
		});
	}

	/** Add an event handler for a specific session */
	onSessionEvent(sessionId: string, handler: WsEventHandler): () => void {
		let handlers = this.eventHandlers.get(sessionId);
		if (!handlers) {
			handlers = new Set();
			this.eventHandlers.set(sessionId, handlers);
		}
		handlers.add(handler);

		return () => {
			handlers?.delete(handler);
			if (handlers?.size === 0) {
				this.eventHandlers.delete(sessionId);
			}
		};
	}

	/** Add a global event handler (receives all events) */
	onEvent(handler: WsEventHandler): () => void {
		this.globalEventHandlers.add(handler);
		return () => this.globalEventHandlers.delete(handler);
	}

	/** Add a connection state handler */
	onConnectionState(handler: ConnectionStateHandler): () => void {
		this.connectionStateHandlers.add(handler);
		// Immediately call with current state
		handler(this.connectionState);
		return () => this.connectionStateHandlers.delete(handler);
	}

	// ========================================================================
	// Private methods
	// ========================================================================

	private createWebSocket(): void {
		const baseUrl = getControlPlaneBaseUrl();
		let wsUrl: string;

		if (baseUrl) {
			// Direct connection to control plane
			wsUrl = `${baseUrl.replace(/^http/, "ws")}/ws`;
		} else {
			// Proxied via frontend dev server
			wsUrl = `${window.location.origin.replace(/^http/, "ws")}/api/ws`;
		}

		// Add auth token as query parameter for WebSocket auth
		const token = getAuthToken();
		if (token) {
			const separator = wsUrl.includes("?") ? "&" : "?";
			wsUrl = `${wsUrl}${separator}token=${encodeURIComponent(token)}`;
		}

		if (isWsDebugEnabled()) {
			console.debug("[ws] Connecting to", wsUrl);
		}
		this.ws = new WebSocket(wsUrl);

		this.ws.onopen = () => {
			if (isWsDebugEnabled()) {
				console.debug("[ws] Connected");
			}
			this.reconnectAttempt = 0;
			this.setConnectionState("connected");
			this.resetPingTimeout();

			// Send pending subscriptions
			for (const sessionId of this.pendingSubscriptions) {
				this.send({ type: "subscribe", session_id: sessionId });
			}
			this.pendingSubscriptions.clear();

			// Re-subscribe to all sessions
			for (const sessionId of this.subscribedSessions) {
				if (!this.pendingSubscriptions.has(sessionId)) {
					this.send({ type: "subscribe", session_id: sessionId });
				}
			}
		};

		this.ws.onmessage = (event) => {
			try {
				const data = JSON.parse(event.data) as WsEvent;
				// Avoid logging full payloads (can be huge + slow).
				if (isWsDebugEnabled() && data.type !== "ping") {
					console.debug("[ws] Received event:", data.type, {
						session_id: "session_id" in data ? data.session_id : undefined,
					});
				}
				this.handleEvent(data);
			} catch (err) {
				console.warn("[ws] Failed to parse message:", err, event.data);
			}
		};

		this.ws.onerror = (event) => {
			console.warn("[ws] WebSocket error:", event);
		};

		this.ws.onclose = (event) => {
			if (isWsDebugEnabled()) {
				console.debug("[ws] Connection closed:", event.code, event.reason);
			}
			this.ws = null;
			this.clearPingTimeout();

			if (event.code !== 1000) {
				// Abnormal close, attempt reconnection
				this.scheduleReconnect();
			} else {
				this.setConnectionState("disconnected");
			}
		};
	}

	private send(command: WsCommand): void {
		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
			console.warn("[ws] Cannot send, not connected:", command.type);
			return;
		}

		try {
			this.ws.send(JSON.stringify(command));
		} catch (err) {
			console.error("[ws] Failed to send command:", err);
		}
	}

	private handleEvent(event: WsEvent): void {
		// Handle ping
		if (event.type === "ping") {
			this.send({ type: "pong" });
			this.resetPingTimeout();
			return;
		}

		// Reset ping timeout on any message
		this.resetPingTimeout();

		// Debug: log A2UI events (opt-in)
		if (isWsDebugEnabled() && event.type === "a2ui_surface") {
			console.debug("[ws] A2UI surface received");
		}

		// Get session ID from event
		const sessionId =
			"session_id" in event
				? (event.session_id as string | undefined)
				: undefined;

		// Dispatch to global handlers
		for (const handler of this.globalEventHandlers) {
			try {
				handler(event);
			} catch (err) {
				console.error("[ws] Error in global event handler:", err);
			}
		}

		// Dispatch to session-specific handlers
		if (sessionId) {
			const handlers = this.eventHandlers.get(sessionId);
			if (handlers) {
				for (const handler of handlers) {
					try {
						handler(event);
					} catch (err) {
						console.error("[ws] Error in session event handler:", err);
					}
				}
			}
		}
	}

	private setConnectionState(state: ConnectionState): void {
		if (this.connectionState === state) return;

		if (isWsDebugEnabled()) {
			console.debug("[ws] Connection state:", state);
		}
		this.connectionState = state;

		for (const handler of this.connectionStateHandlers) {
			try {
				handler(state);
			} catch (err) {
				console.error("[ws] Error in connection state handler:", err);
			}
		}
	}

	private scheduleReconnect(): void {
		if (this.reconnectAttempt >= MAX_RECONNECT_ATTEMPTS) {
			console.error("[ws] Max reconnect attempts reached");
			this.setConnectionState("failed");
			return;
		}

		this.setConnectionState("reconnecting");
		this.reconnectAttempt++;

		const delay = Math.min(
			BASE_RECONNECT_DELAY_MS * 2 ** (this.reconnectAttempt - 1),
			MAX_RECONNECT_DELAY_MS,
		);

		// Add jitter
		const jitter = Math.random() * 0.2 * delay;
		const totalDelay = delay + jitter;

		if (isWsDebugEnabled()) {
			console.debug(
				`[ws] Reconnecting in ${Math.round(totalDelay)}ms (attempt ${this.reconnectAttempt})`,
			);
		}

		this.reconnectTimeout = setTimeout(() => {
			this.reconnectTimeout = null;
			this.createWebSocket();
		}, totalDelay);
	}

	private clearReconnectTimeout(): void {
		if (this.reconnectTimeout) {
			clearTimeout(this.reconnectTimeout);
			this.reconnectTimeout = null;
		}
	}

	private resetPingTimeout(): void {
		this.clearPingTimeout();
		this.pingTimeout = setTimeout(() => {
			console.warn("[ws] Ping timeout, reconnecting");
			this.ws?.close(4000, "Ping timeout");
		}, PING_TIMEOUT_MS);
	}

	private clearPingTimeout(): void {
		if (this.pingTimeout) {
			clearTimeout(this.pingTimeout);
			this.pingTimeout = null;
		}
	}
}

// ============================================================================
// Singleton instance
// ============================================================================

let instance: OctoWsClient | null = null;

/** Get the singleton WebSocket client instance */
export function getWsClient(): OctoWsClient {
	if (!instance) {
		instance = new OctoWsClient();
	}
	return instance;
}

/** Destroy the singleton instance (for cleanup in tests) */
export function destroyWsClient(): void {
	if (instance) {
		instance.disconnect();
		instance = null;
	}
}

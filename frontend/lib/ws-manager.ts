/**
 * Multiplexed WebSocket Connection Manager.
 *
 * Provides a single WebSocket connection per user that handles multiple channels:
 * - pi: Pi session commands and events
 * - files: File operations (future)
 * - terminal: Terminal I/O (future)
 * - hstry: History queries (future)
 * - system: System events (connection status, errors)
 *
 * Features:
 * - Automatic reconnection with exponential backoff
 * - Channel-based event routing
 * - Session subscription management
 * - Request/response correlation via optional IDs
 */

import { controlPlaneApiUrl, getAuthToken } from "./control-plane-client";
import { toAbsoluteWsUrl } from "./url";
import type {
	Channel,
	ConnectionStateHandler,
	PiCommandInfo,
	PiSessionConfig,
	PiWsEvent,
	WsCommand,
	WsEvent,
	WsEventHandler,
	WsMuxConnectionState,
} from "./ws-mux-types";

function isWsMuxDebugEnabled(): boolean {
	if (!import.meta.env.DEV) return false;
	try {
		if (typeof localStorage !== "undefined") {
			return localStorage.getItem("debug:ws-mux") === "1";
		}
	} catch {
		// ignore
	}
	return import.meta.env.VITE_DEBUG_WS_MUX === "1";
}

// ============================================================================
// Configuration
// ============================================================================

const MAX_RECONNECT_ATTEMPTS = 20;
const BASE_RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_DELAY_MS = 30000;
const PING_INTERVAL_MS = 30000;
const CONNECT_TIMEOUT_MS = 10000;

// ============================================================================
// WebSocket Connection Manager
// ============================================================================

/**
 * Singleton WebSocket connection manager for multiplexed communication.
 */
class WsConnectionManager {
	private ws: WebSocket | null = null;
	private connectionState: WsMuxConnectionState = "disconnected";
	private reconnectAttempt = 0;
	private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
	private pingInterval: ReturnType<typeof setInterval> | null = null;

	// Event handlers by channel
	private channelHandlers: Map<Channel, Set<WsEventHandler>> = new Map();
	// Global handlers (receive all events)
	private globalHandlers: Set<WsEventHandler> = new Set();
	// Connection state handlers
	private connectionStateHandlers: Set<ConnectionStateHandler> = new Set();

	// Pi session subscriptions (session_id -> handlers)
	private piSessionHandlers: Map<string, Set<WsEventHandler<PiWsEvent>>> =
		new Map();
	// Track sessions that have completed create_session
	private piSessionReady: Set<string> = new Set();
	private piSessionReadyWaiters: Map<string, Set<() => void>> = new Map();
	// Pending Pi messages to send once session is ready
	private pendingPiMessages: Map<
		string,
		Array<{ type: "prompt" | "steer" | "follow_up"; message: string; id?: string }>
	> = new Map();
	// Track which sessions we're subscribed to (with their configs for reconnection)
	private subscribedSessions: Map<
		string,
		{ scope?: "main" | "workspace"; cwd?: string; provider?: string; model?: string } | undefined
	> = new Map();
	// Pending subscriptions (to send after connect, with configs)
	private pendingSubscriptions: Map<
		string,
		{ scope?: "main" | "workspace"; cwd?: string; provider?: string; model?: string } | undefined
	> = new Map();

	// Request ID counter for correlation
	private requestIdCounter = 0;
	// Pending request callbacks (id -> resolve)
	private pendingRequests: Map<string, (event: WsEvent) => void> = new Map();

	// ========================================================================
	// Public API
	// ========================================================================

	/** Get the current connection state */
	get state(): WsMuxConnectionState {
		return this.connectionState;
	}

	async piGetCommands(sessionId: string): Promise<PiCommandInfo[]> {
		const event = await this.sendAndWait({
			channel: "pi",
			type: "get_commands",
			session_id: sessionId,
		});
		if (event.channel === "pi" && event.type === "commands") {
			return event.commands ?? [];
		}
		throw new Error("Unexpected response to get_commands");
	}

	async piGetSessionStats(sessionId: string): Promise<unknown> {
		const event = await this.sendAndWait({
			channel: "pi",
			type: "get_session_stats",
			session_id: sessionId,
		});
		if (event.channel === "pi" && event.type === "stats") {
			return event.stats;
		}
		throw new Error("Unexpected response to get_session_stats");
	}

	/** Check if connected */
	get isConnected(): boolean {
		return this.connectionState === "connected";
	}

	/** Connect to the WebSocket server */
	connect(): void {
		if (this.ws?.readyState === WebSocket.OPEN) {
			console.debug("[ws-mux] Already connected");
			return;
		}

		if (this.ws?.readyState === WebSocket.CONNECTING) {
			console.debug("[ws-mux] Connection already in progress");
			return;
		}

		console.log("[ws-mux] Connecting to WebSocket...");
		this.setConnectionState("connecting");
		this.createWebSocket();
	}

	/** Disconnect from the WebSocket server */
	disconnect(): void {
		this.clearReconnectTimeout();
		this.clearPingInterval();
		this.setConnectionState("disconnected");

		if (this.ws) {
			this.ws.onclose = null; // Prevent reconnection
			this.ws.close(1000, "Client disconnect");
			this.ws = null;
		}
	}

	/**
	 * Send a command to the server.
	 * @param command The command to send
	 */
	send(command: WsCommand): void {
		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
			console.warn("[ws-mux] Cannot send, not connected:", command);
			return;
		}

		try {
			if (!("id" in command) || command.id === undefined) {
				command.id = this.nextRequestId();
			}
			const json = JSON.stringify(command);
			console.log("[ws-mux] Sending:", json);
			this.ws.send(json);
		} catch (err) {
			console.error("[ws-mux] Failed to send command:", err);
		}
	}

	/**
	 * Send a command and wait for a correlated response.
	 * @param command The command to send (id will be set automatically)
	 * @param timeoutMs Timeout in milliseconds (default: 30000)
	 * @returns Promise that resolves with the response event
	 */
	async sendAndWait(
		command: Omit<WsCommand, "id">,
		timeoutMs = 30000,
	): Promise<WsEvent> {
		await this.waitForConnected(Math.min(CONNECT_TIMEOUT_MS, timeoutMs));
		const id = this.nextRequestId();
		const commandWithId = { ...command, id } as WsCommand;

		return new Promise<WsEvent>((resolve, reject) => {
			const timeout = setTimeout(() => {
				this.pendingRequests.delete(id);
				reject(new Error(`Request timeout: ${command.type}`));
			}, timeoutMs);

			this.pendingRequests.set(id, (event) => {
				clearTimeout(timeout);
				this.pendingRequests.delete(id);
				resolve(event);
			});

			this.send(commandWithId);
		});
	}

	/**
	 * Subscribe to events on a specific channel.
	 * @param channel The channel to subscribe to
	 * @param handler The event handler
	 * @returns Unsubscribe function
	 */
	subscribe(channel: Channel, handler: WsEventHandler): () => void {
		let handlers = this.channelHandlers.get(channel);
		if (!handlers) {
			handlers = new Set();
			this.channelHandlers.set(channel, handlers);
		}
		handlers.add(handler);

		return () => {
			handlers?.delete(handler);
			if (handlers?.size === 0) {
				this.channelHandlers.delete(channel);
			}
		};
	}

	/**
	 * Subscribe to all events (global handler).
	 * @param handler The event handler
	 * @returns Unsubscribe function
	 */
	subscribeAll(handler: WsEventHandler): () => void {
		this.globalHandlers.add(handler);
		return () => this.globalHandlers.delete(handler);
	}

	/**
	 * Subscribe to connection state changes.
	 * @param handler The state handler
	 * @returns Unsubscribe function
	 */
	onConnectionState(handler: ConnectionStateHandler): () => void {
		this.connectionStateHandlers.add(handler);
		// Immediately call with current state
		handler(this.connectionState);
		return () => this.connectionStateHandlers.delete(handler);
	}

	// ========================================================================
	// Pi Channel Helpers
	// ========================================================================

	/**
	 * Subscribe to a Pi session's events.
	 * Creates the session in the runner if needed, then subscribes to events.
	 * @param sessionId The session ID to subscribe to
	 * @param handler Handler for Pi events for this session
	 * @param config Optional session config (cwd, provider, model)
	 * @returns Unsubscribe function
	 */
	subscribePiSession(
		sessionId: string,
		handler: WsEventHandler<PiWsEvent>,
		config?: PiSessionConfig,
	): () => void {
		console.log("[ws-mux] subscribePiSession:", sessionId, "config:", config, "isConnected:", this.isConnected);
		
		// Add handler locally
		let handlers = this.piSessionHandlers.get(sessionId);
		if (!handlers) {
			handlers = new Set();
			this.piSessionHandlers.set(sessionId, handlers);
		}
		handlers.add(handler);

		// Track subscription (store config for reconnection)
		if (!this.subscribedSessions.has(sessionId)) {
			this.subscribedSessions.set(sessionId, config);

			if (this.isConnected) {
				console.log("[ws-mux] Sending create_session + subscribe for:", sessionId);
				// Create session first (idempotent - will resume if exists)
				this.piSessionReady.delete(sessionId);
				this.send({
					channel: "pi",
					type: "create_session",
					session_id: sessionId,
					config,
				});
				// Then subscribe to events
				this.send({
					channel: "pi",
					type: "subscribe",
					session_id: sessionId,
				});
			} else {
				console.log("[ws-mux] Not connected, queueing subscription for:", sessionId);
				// Queue for after connect (with config)
				this.pendingSubscriptions.set(sessionId, config);
				// Auto-connect if not connected
				if (this.connectionState === "disconnected") {
					this.connect();
				}
			}
		} else {
			console.log("[ws-mux] Already subscribed to session:", sessionId);
		}

		// Return unsubscribe function
		return () => {
			handlers?.delete(handler);
			if (handlers?.size === 0) {
				this.piSessionHandlers.delete(sessionId);
				this.subscribedSessions.delete(sessionId);
				this.pendingSubscriptions.delete(sessionId);
				this.piSessionReady.delete(sessionId);
				this.pendingPiMessages.delete(sessionId);

				if (this.isConnected) {
					this.send({
						channel: "pi",
						type: "unsubscribe",
						session_id: sessionId,
					});
				}
			}
		};
	}

	/**
	 * Send a prompt to a Pi session.
	 */
	piPrompt(sessionId: string, message: string, id?: string): void {
		this.enqueueOrSendPiMessage(sessionId, "prompt", message, id);
	}

	/**
	 * Send a steering message to a Pi session.
	 */
	piSteer(sessionId: string, message: string, id?: string): void {
		this.enqueueOrSendPiMessage(sessionId, "steer", message, id);
	}

	/**
	 * Send a follow-up message to a Pi session.
	 */
	piFollowUp(sessionId: string, message: string, id?: string): void {
		this.enqueueOrSendPiMessage(sessionId, "follow_up", message, id);
	}

	/**
	 * Abort a Pi session's current operation.
	 */
	piAbort(sessionId: string, id?: string): void {
		this.send({
			channel: "pi",
			type: "abort",
			session_id: sessionId,
			id,
		});
	}

	/**
	 * Compact a Pi session's context.
	 */
	piCompact(sessionId: string, instructions?: string, id?: string): void {
		this.send({
			channel: "pi",
			type: "compact",
			session_id: sessionId,
			instructions,
			id,
		});
	}

	/**
	 * Create or resume a Pi session.
	 */
	piCreateSession(
		sessionId: string,
		config?: PiSessionConfig,
		id?: string,
	): void {
		const resolvedConfig = config ?? this.subscribedSessions.get(sessionId);
		this.send({
			channel: "pi",
			type: "create_session",
			session_id: sessionId,
			config: resolvedConfig,
			id,
		});
	}

	/**
	 * Close a Pi session.
	 */
	piCloseSession(sessionId: string, id?: string): void {
		this.send({
			channel: "pi",
			type: "close_session",
			session_id: sessionId,
			id,
		});
	}

	/**
	 * Get Pi session state.
	 */
	piGetState(sessionId: string, id?: string): void {
		this.send({
			channel: "pi",
			type: "get_state",
			session_id: sessionId,
			id,
		});
	}

	/**
	 * List Pi sessions.
	 */
	piListSessions(id?: string): void {
		this.send({
			channel: "pi",
			type: "list_sessions",
			id,
		});
	}

	/**
	 * Set model for a Pi session.
	 */
	async piSetModel(
		sessionId: string,
		provider: string,
		modelId: string,
	): Promise<void> {
		await this.sendAndWait({
			channel: "pi",
			type: "set_model",
			session_id: sessionId,
			provider,
			model_id: modelId,
		});
	}

	/**
	 * Get available models for a Pi session.
	 */
	async piGetAvailableModels(sessionId: string): Promise<unknown> {
		const response = await this.sendAndWait({
			channel: "pi",
			type: "get_available_models",
			session_id: sessionId,
		});
		if (response.type === "available_models" && "models" in response) {
			return response.models;
		}
		throw new Error("Unexpected response type");
	}

	// ========================================================================
	// Private Methods
	// ========================================================================

	private nextRequestId(): string {
		this.requestIdCounter += 1;
		return `req-${this.requestIdCounter}-${Date.now()}`;
	}

	private async waitForConnected(timeoutMs: number): Promise<void> {
		if (this.isConnected) return;
		this.connect();
		return new Promise<void>((resolve, reject) => {
			let unsubscribe = () => {};
			const timeout = setTimeout(() => {
				unsubscribe();
				reject(new Error("WebSocket connection timeout"));
			}, timeoutMs);

			unsubscribe = this.onConnectionState((state) => {
				if (state === "connected") {
					clearTimeout(timeout);
					unsubscribe();
					resolve();
				} else if (state === "failed") {
					clearTimeout(timeout);
					unsubscribe();
					reject(new Error("WebSocket connection failed"));
				}
			});
		});
	}

	async ensureConnected(timeoutMs = 4000): Promise<void> {
		return this.waitForConnected(timeoutMs);
	}

	async waitForPiSessionReady(sessionId: string, timeoutMs = 4000): Promise<void> {
		if (this.piSessionReady.has(sessionId)) return;
		return new Promise<void>((resolve, reject) => {
			let done = false;
			const waiters = this.piSessionReadyWaiters.get(sessionId) ?? new Set();
			const onReady = () => {
				if (done) return;
				done = true;
				clearTimeout(timeout);
				const current = this.piSessionReadyWaiters.get(sessionId);
				if (current) {
					current.delete(onReady);
					if (current.size === 0) this.piSessionReadyWaiters.delete(sessionId);
				}
				resolve();
			};
			waiters.add(onReady);
			this.piSessionReadyWaiters.set(sessionId, waiters);
			const timeout = setTimeout(() => {
				if (done) return;
				done = true;
				const current = this.piSessionReadyWaiters.get(sessionId);
				if (current) {
					current.delete(onReady);
					if (current.size === 0) this.piSessionReadyWaiters.delete(sessionId);
				}
				reject(new Error("Pi session did not become ready in time"));
			}, timeoutMs);
		});
	}

	private createWebSocket(): void {
		let wsUrl = toAbsoluteWsUrl(controlPlaneApiUrl("/api/ws/mux"));

		// Add auth token as query parameter
		const token = getAuthToken();
		if (token) {
			const separator = wsUrl.includes("?") ? "&" : "?";
			wsUrl = `${wsUrl}${separator}token=${encodeURIComponent(token)}`;
		}

		console.log("[ws-mux] Connecting to", wsUrl);

		this.ws = new WebSocket(wsUrl);

		this.ws.onopen = () => {
			console.log("[ws-mux] Connected!");
			this.reconnectAttempt = 0;
			this.setConnectionState("connected");
			this.startPingInterval();
			
			console.log("[ws-mux] Processing pending subscriptions:", this.pendingSubscriptions.size);
			console.log("[ws-mux] Processing subscribed sessions:", this.subscribedSessions.size);

			const pendingSessionIds = new Set(this.pendingSubscriptions.keys());
			// Send pending subscriptions (create session first, then subscribe)
			for (const [sessionId, config] of this.pendingSubscriptions) {
				this.piSessionReady.delete(sessionId);
				this.send({
					channel: "pi",
					type: "create_session",
					session_id: sessionId,
					config,
				});
				this.send({
					channel: "pi",
					type: "subscribe",
					session_id: sessionId,
				});
			}
			this.pendingSubscriptions.clear();

			// Re-subscribe to all tracked sessions (create session first for reconnect)
			for (const [sessionId, config] of this.subscribedSessions) {
				if (pendingSessionIds.has(sessionId)) continue;
				this.piSessionReady.delete(sessionId);
				this.send({
					channel: "pi",
					type: "create_session",
					session_id: sessionId,
					config,
				});
				this.send({
					channel: "pi",
					type: "subscribe",
					session_id: sessionId,
				});
			}

			if (this.pendingPiMessages.size > 0 && isWsMuxDebugEnabled()) {
				console.debug(
					"[ws-mux] Pending Pi messages queued until session_ready:",
					this.pendingPiMessages.size,
				);
			}
		};

		this.ws.onmessage = (event) => {
			try {
				const data = JSON.parse(event.data) as WsEvent;
				if (isWsMuxDebugEnabled() && data.type !== "ping") {
					console.debug("[ws-mux] Received:", data);
				}
				this.handleEvent(data);
			} catch (err) {
				console.warn("[ws-mux] Failed to parse message:", err, event.data);
			}
		};

		this.ws.onerror = (event) => {
			console.error("[ws-mux] WebSocket error:", event);
		};

		this.ws.onclose = (event) => {
			console.log("[ws-mux] Connection closed:", event.code, event.reason);
			this.ws = null;
			this.clearPingInterval();

			if (event.code !== 1000) {
				// Abnormal close, attempt reconnection
				this.scheduleReconnect();
			} else {
				this.setConnectionState("disconnected");
			}
		};
	}

	private handleEvent(event: WsEvent): void {
		// Track Pi session readiness and flush queued messages
		if (event.channel === "pi" && event.type === "session_created") {
			const sessionId = (event as PiWsEvent).session_id;
			console.log("[ws-mux] Received session_created for:", sessionId);
			this.piSessionReady.add(sessionId);
			const waiters = this.piSessionReadyWaiters.get(sessionId);
			if (waiters) {
				for (const waiter of waiters) {
					waiter();
				}
				this.piSessionReadyWaiters.delete(sessionId);
			}

			// Request initial state (includes model info)
			this.send({
				channel: "pi",
				type: "get_state",
				session_id: sessionId,
			});

			const pending = this.pendingPiMessages.get(sessionId);
			console.log("[ws-mux] Pending messages for session:", sessionId, pending?.length ?? 0);
			if (pending?.length) {
				for (const entry of pending) {
					console.log("[ws-mux] Flushing queued message:", entry.type, "for session:", sessionId);
					this.send({
						channel: "pi",
						type: entry.type,
						session_id: sessionId,
						message: entry.message,
						id: entry.id,
					});
				}
				this.pendingPiMessages.delete(sessionId);
			}
		}

		// Handle system pings
		if (event.channel === "system" && event.type === "ping") {
			// Respond with any message to keep connection alive
			// (Backend uses the message to detect liveness)
			return;
		}

		// Check for correlated response
		const id = "id" in event ? event.id : undefined;
		if (id && this.pendingRequests.has(id)) {
			const callback = this.pendingRequests.get(id);
			if (callback) {
				callback(event);
			}
		}

		// Dispatch to global handlers
		for (const handler of this.globalHandlers) {
			try {
				handler(event);
			} catch (err) {
				console.error("[ws-mux] Error in global event handler:", err);
			}
		}

		// Dispatch to channel handlers
		const channelHandlers = this.channelHandlers.get(event.channel);
		if (channelHandlers) {
			for (const handler of channelHandlers) {
				try {
					handler(event);
				} catch (err) {
					console.error("[ws-mux] Error in channel event handler:", err);
				}
			}
		}

		// Dispatch Pi events to session-specific handlers
		if (event.channel === "pi") {
			const piEvent = event as PiWsEvent;
			const sessionId =
				"session_id" in piEvent ? piEvent.session_id : undefined;
			if (sessionId) {
				const sessionHandlers = this.piSessionHandlers.get(sessionId);
				if (sessionHandlers) {
					for (const handler of sessionHandlers) {
						try {
							handler(piEvent);
						} catch (err) {
							console.error(
								"[ws-mux] Error in Pi session event handler:",
								err,
							);
						}
					}
				}
			}
		}
	}

	private enqueueOrSendPiMessage(
		sessionId: string,
		type: "prompt" | "steer" | "follow_up",
		message: string,
		id?: string,
	): void {
		if (!this.isConnected) {
			const pending = this.pendingPiMessages.get(sessionId) ?? [];
			pending.push({ type, message, id });
			this.pendingPiMessages.set(sessionId, pending);
			return;
		}

		if (!this.piSessionReady.has(sessionId)) {
			const pending = this.pendingPiMessages.get(sessionId) ?? [];
			pending.push({ type, message, id });
			this.pendingPiMessages.set(sessionId, pending);
			console.log(
				"[ws-mux] Queued Pi message until session is ready:",
				sessionId,
				type,
				"queue size:",
				pending.length,
			);
			return;
		}

		this.send({
			channel: "pi",
			type,
			session_id: sessionId,
			message,
			id,
		});
	}

	private setConnectionState(state: WsMuxConnectionState): void {
		if (this.connectionState === state) return;

		if (isWsMuxDebugEnabled()) {
			console.debug("[ws-mux] Connection state:", state);
		}
		this.connectionState = state;

		for (const handler of this.connectionStateHandlers) {
			try {
				handler(state);
			} catch (err) {
				console.error("[ws-mux] Error in connection state handler:", err);
			}
		}
	}

	private scheduleReconnect(): void {
		if (this.reconnectAttempt >= MAX_RECONNECT_ATTEMPTS) {
			console.error("[ws-mux] Max reconnect attempts reached");
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

		if (isWsMuxDebugEnabled()) {
			console.debug(
				`[ws-mux] Reconnecting in ${Math.round(totalDelay)}ms (attempt ${this.reconnectAttempt})`,
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

	private startPingInterval(): void {
		this.clearPingInterval();
		this.pingInterval = setInterval(() => {
			if (this.ws?.readyState === WebSocket.OPEN) {
				// Keepalive placeholder: server doesn't emit pings yet.
				// Avoid closing healthy connections due to missing pong frames.
			}
		}, PING_INTERVAL_MS);
	}

	private clearPingInterval(): void {
		if (this.pingInterval) {
			clearInterval(this.pingInterval);
			this.pingInterval = null;
		}
	}
}

// ============================================================================
// Singleton Instance
// ============================================================================

let instance: WsConnectionManager | null = null;

/** Get the singleton WebSocket manager instance */
export function getWsManager(): WsConnectionManager {
	if (!instance) {
		instance = new WsConnectionManager();
	}
	return instance;
}

/** Destroy the singleton instance (for cleanup in tests) */
export function destroyWsManager(): void {
	if (instance) {
		instance.disconnect();
		instance = null;
	}
}

// Export the manager instance directly for convenience
export const wsManager = {
	get instance() {
		return getWsManager();
	},
	connect: () => getWsManager().connect(),
	disconnect: () => getWsManager().disconnect(),
	get isConnected() {
		return getWsManager().isConnected;
	},
	get state() {
		return getWsManager().state;
	},
};

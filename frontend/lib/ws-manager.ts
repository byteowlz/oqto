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

import type { CommandResponse, SessionConfig } from "./canonical-types";
import { controlPlaneApiUrl, getAuthToken } from "./control-plane-client";
import { toAbsoluteWsUrl } from "./url";
import type {
	AgentWsEvent,
	Channel,
	ConnectionStateHandler,
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

function isWsTraceEnabled(): boolean {
	if (!import.meta.env.DEV) return false;
	try {
		if (typeof localStorage !== "undefined") {
			return localStorage.getItem("debug:ws-trace") === "1";
		}
	} catch {
		// ignore
	}
	return false;
}

type WsTraceEntry = {
	ts: number;
	dir: "send" | "recv" | "client";
	channel?: string;
	session_id?: string;
	cmd?: string;
	event?: string;
	id?: string;
	detail?: string;
};

const MAX_TRACE_ENTRIES = 2000;

export function recordWsTrace(entry: WsTraceEntry): void {
	if (!isWsTraceEnabled()) return;
	const root = globalThis as unknown as {
		__octoWsTrace?: WsTraceEntry[];
		__octoWsTraceDump?: () => string;
		__octoWsTraceClear?: () => void;
	};
	const trace = Array.isArray(root.__octoWsTrace) ? root.__octoWsTrace : [];
	trace.push(entry);
	if (trace.length > MAX_TRACE_ENTRIES) {
		trace.splice(0, trace.length - MAX_TRACE_ENTRIES);
	}
	root.__octoWsTrace = trace;
	if (!root.__octoWsTraceDump) {
		root.__octoWsTraceDump = () =>
			JSON.stringify(root.__octoWsTrace ?? [], null, 2);
	}
	if (!root.__octoWsTraceClear) {
		root.__octoWsTraceClear = () => {
			root.__octoWsTrace = [];
		};
	}
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

	// Agent session subscriptions (session_id -> handlers)
	private agentSessionHandlers: Map<string, Set<WsEventHandler<AgentWsEvent>>> =
		new Map();
	// Track sessions that have completed session.create
	private sessionReady: Set<string> = new Set();
	private sessionReadyWaiters: Map<string, Set<() => void>> = new Map();
	// Pending messages to send once session is ready
	private pendingMessages: Map<
		string,
		Array<{
			cmd: "prompt" | "steer" | "follow_up";
			message: string;
			id?: string;
			client_id?: string;
		}>
	> = new Map();
	// Track which sessions we're subscribed to (with configs and create intent)
	private subscribedSessions: Map<
		string,
		{ config?: SessionConfig; create: boolean }
	> = new Map();
	// Pending subscriptions (to send after connect, with configs and create intent)
	private pendingSubscriptions: Map<
		string,
		{ config?: SessionConfig; create: boolean }
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

	/** Check if a session is marked ready */
	isSessionReady(sessionId: string): boolean {
		return this.sessionReady.has(sessionId);
	}

	async agentGetCommands(sessionId: string): Promise<unknown[]> {
		const event = await this.sendAndWait({
			channel: "agent",
			session_id: sessionId,
			cmd: "get_commands",
		});
		const resp = this.extractCommandResponse(event);
		if (resp?.success && resp.data) {
			const data = resp.data as { commands?: unknown[] };
			return data.commands ?? [];
		}
		const errMsg = resp?.error ?? "Unexpected response to get_commands";
		if (
			errMsg.includes("SessionNotFound") ||
			errMsg.includes("PiSessionNotFound")
		) {
			return [];
		}
		throw new Error(errMsg);
	}

	async agentGetStateWait(sessionId: string): Promise<unknown> {
		const event = await this.sendAndWait({
			channel: "agent",
			session_id: sessionId,
			cmd: "get_state",
		});
		const resp = this.extractCommandResponse(event);
		if (resp?.success) {
			return resp.data ?? null;
		}
		throw new Error(resp?.error ?? "Unexpected response to get_state");
	}

	/**
	 * List all active Pi sessions on the runner.
	 * Returns session IDs, states, and working directories.
	 * Used on reconnect to discover running sessions and restore busy state.
	 */
	async agentListSessions(): Promise<
		Array<{
			session_id: string;
			state: string;
			cwd: string;
			provider?: string;
			model?: string;
			last_activity: number;
			subscriber_count: number;
		}>
	> {
		const event = await this.sendAndWait({
			channel: "agent",
			session_id: "_system",
			cmd: "list_sessions",
		});
		const resp = this.extractCommandResponse(event);
		if (resp?.success && resp.data) {
			const data = resp.data as { sessions?: unknown[] };
			return (data.sessions ?? []) as Array<{
				session_id: string;
				state: string;
				cwd: string;
				provider?: string;
				model?: string;
				last_activity: number;
				subscriber_count: number;
			}>;
		}
		throw new Error(resp?.error ?? "Unexpected response to list_sessions");
	}

	async agentGetSessionStats(sessionId: string): Promise<unknown> {
		const event = await this.sendAndWait({
			channel: "agent",
			session_id: sessionId,
			cmd: "get_stats",
		});
		const resp = this.extractCommandResponse(event);
		if (resp?.success && resp.data) {
			return resp.data;
		}
		const errMsg = resp?.error ?? "Unexpected response to get_stats";
		if (
			errMsg.includes("SessionNotFound") ||
			errMsg.includes("PiSessionNotFound")
		) {
			return null;
		}
		throw new Error(errMsg);
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
			recordWsTrace({
				ts: Date.now(),
				dir: "send",
				channel: command.channel,
				session_id: "session_id" in command ? command.session_id : undefined,
				cmd: "cmd" in command ? command.cmd : undefined,
				id: command.id as string | undefined,
			});
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

		const label =
			"cmd" in command
				? command.cmd
				: "type" in command
					? (command as { type?: string }).type
					: command.channel;

		return new Promise<WsEvent>((resolve, reject) => {
			const timeout = setTimeout(() => {
				this.pendingRequests.delete(id);
				reject(new Error(`Request timeout: ${label}`));
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
	// Agent Channel Helpers
	// ========================================================================

	/**
	 * Subscribe to an agent session's events.
	 * Creates the session in the runner if needed (auto-subscribes to events).
	 * @param sessionId The session ID to subscribe to
	 * @param handler Handler for agent events for this session
	 * @param config Optional session config (harness, cwd, provider, model)
	 * @param options Optional controls for session creation
	 * @returns Unsubscribe function
	 */
	subscribeAgentSession(
		sessionId: string,
		handler: WsEventHandler<AgentWsEvent>,
		config?: SessionConfig,
		options?: { create?: boolean },
	): () => void {
		console.log(
			"[ws-mux] subscribeAgentSession:",
			sessionId,
			"config:",
			config,
			"isConnected:",
			this.isConnected,
		);
		const shouldCreate = options?.create !== false;

		// Add handler locally
		let handlers = this.agentSessionHandlers.get(sessionId);
		if (!handlers) {
			handlers = new Set();
			this.agentSessionHandlers.set(sessionId, handlers);
		}
		handlers.add(handler);

		if (!shouldCreate) {
			// Track the subscription for reconnection, but do not send session.create.
			this.subscribedSessions.set(sessionId, { config, create: false });
			return () => {
				handlers?.delete(handler);
				if (handlers?.size === 0) {
					this.agentSessionHandlers.delete(sessionId);
				}
			};
		}

		// Track subscription (store config and create intent for reconnection).
		// If this session is already tracked AND ready, we skip sending another
		// session.create — this handles React StrictMode double-invoke and HMR
		// re-renders that unsubscribe + re-subscribe in quick succession.
		const existingEntry = this.subscribedSessions.get(sessionId);
		const alreadyTracked = Boolean(existingEntry);
		const alreadyReady = this.sessionReady.has(sessionId);
		const wasCreate = existingEntry?.create ?? false;

		this.subscribedSessions.set(sessionId, { config, create: shouldCreate });

		if (alreadyTracked && alreadyReady && wasCreate && shouldCreate) {
			console.log(
				"[ws-mux] Session already tracked and ready, skipping session.create:",
				sessionId,
			);
		} else if (!alreadyTracked || shouldCreate) {
			if (this.isConnected) {
				console.log("[ws-mux] Sending session.create for:", sessionId);
				// Create session (backend auto-subscribes to events)
				this.sessionReady.delete(sessionId);
				this.send({
					channel: "agent",
					session_id: sessionId,
					cmd: "session.create",
					config: config ?? {},
				});
			} else {
				console.log(
					"[ws-mux] Not connected, queueing subscription for:",
					sessionId,
				);
				// Queue for after connect (with config)
				this.pendingSubscriptions.set(sessionId, {
					config,
					create: shouldCreate,
				});
				// Auto-connect if not connected
				if (this.connectionState === "disconnected") {
					this.connect();
				}
			}
		}

		// Return unsubscribe function.
		// Note: this only removes the local event handler. It does NOT close the
		// session on the backend -- the runner session stays alive for reconnection.
		// Use agentCloseSession() explicitly to destroy a session.
		//
		// We intentionally keep subscribedSessions and sessionReady intact even
		// when the last handler is removed. This prevents React StrictMode
		// double-invoke and HMR from triggering redundant session.create calls
		// (old effect cleanup removes handler, new effect re-subscribes and would
		// see the session as new if we cleared these maps).
		return () => {
			handlers?.delete(handler);
			if (handlers?.size === 0) {
				this.agentSessionHandlers.delete(sessionId);
				// Don't clear subscribedSessions/sessionReady — the session
				// stays alive on the backend and we want to reuse it on
				// re-subscription. Only agentCloseSession clears these.
			}
		};
	}

	/**
	 * Send a prompt to an agent session.
	 * @param sessionId - Target session ID
	 * @param message - User message content
	 * @param id - Command correlation ID (optional)
	 * @param clientId - Client-generated ID for optimistic message matching (optional)
	 */
	agentPrompt(
		sessionId: string,
		message: string,
		id?: string,
		clientId?: string,
	): void {
		this.enqueueOrSendAgentMessage(sessionId, "prompt", message, id, clientId);
	}

	/**
	 * Send a steering message to an agent session.
	 */
	agentSteer(sessionId: string, message: string, id?: string): void {
		this.enqueueOrSendAgentMessage(sessionId, "steer", message, id);
	}

	/**
	 * Send a follow-up message to an agent session.
	 */
	agentFollowUp(sessionId: string, message: string, id?: string): void {
		this.enqueueOrSendAgentMessage(sessionId, "follow_up", message, id);
	}

	/**
	 * Abort an agent session's current operation.
	 */
	agentAbort(sessionId: string, id?: string): void {
		this.send({
			channel: "agent",
			session_id: sessionId,
			cmd: "abort",
			id,
		});
	}

	/**
	 * Compact an agent session's context.
	 */
	agentCompact(sessionId: string, instructions?: string, id?: string): void {
		this.send({
			channel: "agent",
			session_id: sessionId,
			cmd: "compact",
			instructions,
			id,
		});
	}

	/**
	 * Create or resume an agent session.
	 */
	agentCreateSession(
		sessionId: string,
		config?: SessionConfig,
		id?: string,
	): void {
		const resolvedConfig =
			config ?? this.subscribedSessions.get(sessionId)?.config ?? {};
		this.send({
			channel: "agent",
			session_id: sessionId,
			cmd: "session.create",
			config: resolvedConfig,
			id,
		});
	}

	/**
	 * Close an agent session.
	 * This is the only way to fully clean up a session's tracking state.
	 */
	agentCloseSession(sessionId: string, id?: string): void {
		this.subscribedSessions.delete(sessionId);
		this.sessionReady.delete(sessionId);
		this.pendingSubscriptions.delete(sessionId);
		this.pendingMessages.delete(sessionId);
		this.agentSessionHandlers.delete(sessionId);

		this.send({
			channel: "agent",
			session_id: sessionId,
			cmd: "session.close",
			id,
		});
	}

	/**
	 * Re-key a session from a provisional ID to Pi's real ID.
	 * Migrates all internal maps (subscriptions, handlers, pending
	 * messages, readiness state) so subsequent commands use the real ID.
	 */
	reKeySession(provisionalId: string, realId: string): void {
		console.log(`[ws-mux] Re-keying session: ${provisionalId} -> ${realId}`);

		const entry = this.subscribedSessions.get(provisionalId);
		if (entry !== undefined) {
			this.subscribedSessions.delete(provisionalId);
			this.subscribedSessions.set(realId, entry);
		}

		const handlers = this.agentSessionHandlers.get(provisionalId);
		if (handlers) {
			this.agentSessionHandlers.delete(provisionalId);
			this.agentSessionHandlers.set(realId, handlers);
		}

		if (this.sessionReady.has(provisionalId)) {
			this.sessionReady.delete(provisionalId);
			this.sessionReady.add(realId);
		}

		const pendingMsgs = this.pendingMessages.get(provisionalId);
		if (pendingMsgs) {
			this.pendingMessages.delete(provisionalId);
			this.pendingMessages.set(realId, pendingMsgs);
		}

		const pendingSub = this.pendingSubscriptions.get(provisionalId);
		if (pendingSub !== undefined) {
			this.pendingSubscriptions.delete(provisionalId);
			this.pendingSubscriptions.set(realId, pendingSub);
		}
	}

	/**
	 * Get agent session state.
	 */
	agentGetState(sessionId: string, id?: string): void {
		this.send({
			channel: "agent",
			session_id: sessionId,
			cmd: "get_state",
			id,
		});
	}

	/**
	 * Get messages for a session.
	 */
	agentGetMessages(sessionId: string, id?: string): void {
		this.send({
			channel: "agent",
			session_id: sessionId,
			cmd: "get_messages",
			id,
		});
	}

	/**
	 * Set model for an agent session.
	 */
	async agentSetModel(
		sessionId: string,
		provider: string,
		modelId: string,
	): Promise<void> {
		const event = await this.sendAndWait({
			channel: "agent",
			session_id: sessionId,
			cmd: "set_model",
			provider,
			model_id: modelId,
		});
		const resp = this.extractCommandResponse(event);
		if (resp && !resp.success) {
			throw new Error(resp.error ?? "Failed to set model");
		}
	}

	/**
	 * Get available models for an agent session or cached workdir.
	 */
	async agentGetAvailableModels(
		sessionId: string,
		workdir?: string,
	): Promise<unknown> {
		const event = await this.sendAndWait({
			channel: "agent",
			session_id: sessionId,
			cmd: "get_models",
			...(workdir ? { workdir } : {}),
		});
		const resp = this.extractCommandResponse(event);
		if (resp?.success && resp.data) {
			// Backend may send models as a direct array or wrapped in { models: [...] }
			if (Array.isArray(resp.data)) return resp.data;
			const data = resp.data as { models?: unknown[] };
			if (Array.isArray(data.models)) return data.models;
			return [];
		}
		throw new Error(resp?.error ?? "Unexpected response type");
	}

	/**
	 * Cycle to next model in scoped models list.
	 */
	async agentCycleModel(sessionId: string): Promise<unknown> {
		const event = await this.sendAndWait({
			channel: "agent",
			session_id: sessionId,
			cmd: "cycle_model",
		});
		const resp = this.extractCommandResponse(event);
		if (resp?.success && resp.data) {
			return resp.data;
		}
		throw new Error(resp?.error ?? "Failed to cycle model");
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

	async waitForSessionReady(
		sessionId: string,
		timeoutMs = 4000,
	): Promise<void> {
		if (this.sessionReady.has(sessionId)) return;
		return new Promise<void>((resolve, reject) => {
			let done = false;
			const waiters = this.sessionReadyWaiters.get(sessionId) ?? new Set();
			const onReady = () => {
				if (done) return;
				done = true;
				clearTimeout(timeout);
				const current = this.sessionReadyWaiters.get(sessionId);
				if (current) {
					current.delete(onReady);
					if (current.size === 0) this.sessionReadyWaiters.delete(sessionId);
				}
				resolve();
			};
			waiters.add(onReady);
			this.sessionReadyWaiters.set(sessionId, waiters);
			const timeout = setTimeout(() => {
				if (done) return;
				done = true;
				const current = this.sessionReadyWaiters.get(sessionId);
				if (current) {
					current.delete(onReady);
					if (current.size === 0) this.sessionReadyWaiters.delete(sessionId);
				}
				reject(new Error("Agent session did not become ready in time"));
			}, timeoutMs);
		});
	}

	/** Extract CommandResponse from an agent event (for sendAndWait results).
	 *  CommandResponse fields (id, cmd, success, data, error) are flattened
	 *  into the top-level event object by serde, not nested under "response".
	 */
	private extractCommandResponse(event: WsEvent): CommandResponse | null {
		if (event.channel !== "agent") return null;
		const agentEvent = event as AgentWsEvent;
		if (agentEvent.event === "response") {
			return {
				id: agentEvent.id as string,
				cmd: agentEvent.cmd as string,
				success: agentEvent.success as boolean,
				data: agentEvent.data as unknown,
				error: agentEvent.error as string | undefined,
			};
		}
		return null;
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

			console.log(
				"[ws-mux] Processing pending subscriptions:",
				this.pendingSubscriptions.size,
			);
			console.log(
				"[ws-mux] Processing subscribed sessions:",
				this.subscribedSessions.size,
			);
			console.log(
				"[ws-mux] Active session handlers:",
				this.agentSessionHandlers.size,
			);

			const pendingSessionIds = new Set(this.pendingSubscriptions.keys());
			// Send pending subscriptions (session.create auto-subscribes)
			for (const [sessionId, entry] of this.pendingSubscriptions) {
				if (!entry.create) continue;
				this.sessionReady.delete(sessionId);
				this.send({
					channel: "agent",
					session_id: sessionId,
					cmd: "session.create",
					config: entry.config ?? {},
				});
			}
			this.pendingSubscriptions.clear();

			// Re-subscribe to tracked sessions that still have active
			// handlers (i.e., a React component is currently subscribed).
			// Sessions without handlers are stale leftovers from a previous
			// page load (the singleton on globalThis survives but the React
			// tree unmounted) and must NOT be re-created -- doing so spawns
			// orphaned Pi processes on the runner.
			for (const [sessionId, entry] of this.subscribedSessions) {
				if (pendingSessionIds.has(sessionId)) continue;
				if (!this.agentSessionHandlers.has(sessionId)) {
					console.log(
						"[ws-mux] Skipping stale session (no handlers):",
						sessionId,
					);
					continue;
				}
				if (!entry.create) continue;
				this.sessionReady.delete(sessionId);
				this.send({
					channel: "agent",
					session_id: sessionId,
					cmd: "session.create",
					config: entry.config ?? {},
				});
			}

			// Prune stale sessions that have no handlers.
			for (const sessionId of Array.from(this.subscribedSessions.keys())) {
				if (!this.agentSessionHandlers.has(sessionId)) {
					this.subscribedSessions.delete(sessionId);
					this.sessionReady.delete(sessionId);
					this.pendingMessages.delete(sessionId);
				}
			}

			if (this.pendingMessages.size > 0 && isWsMuxDebugEnabled()) {
				console.debug(
					"[ws-mux] Pending messages queued until session_ready:",
					this.pendingMessages.size,
				);
			}
		};

		this.ws.onmessage = (event) => {
			try {
				const data = JSON.parse(event.data) as WsEvent;
				const recordEvent = (evt: WsEvent) => {
					const base: WsTraceEntry = {
						ts: Date.now(),
						dir: "recv",
						channel: "channel" in evt ? evt.channel : undefined,
						session_id: "session_id" in evt ? evt.session_id : undefined,
						id: "id" in evt ? (evt as { id?: string }).id : undefined,
					};
					if ("event" in evt && typeof evt.event === "string") {
						base.event = evt.event;
					}
					if ("cmd" in evt && typeof evt.cmd === "string") {
						base.cmd = evt.cmd;
					}
					recordWsTrace(base);
				};
				recordEvent(data);
				if (isWsMuxDebugEnabled()) {
					const eventType =
						"type" in data
							? (data as { type?: string }).type
							: "event" in data
								? (data as { event?: string }).event
								: "";
					if (eventType !== "ping") {
						console.debug("[ws-mux] Received:", data);
					}
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
		// Track agent session readiness via response to session.create command
		if (event.channel === "agent") {
			const agentEvent = event as AgentWsEvent;

			// Check if this is a successful session.create response.
			// CommandResponse fields are flattened into the top-level event:
			//   { event: "response", id, cmd, success, data?, error?, session_id, ... }
			if (agentEvent.event === "response") {
				const cmd = agentEvent.cmd as string | undefined;
				const success = agentEvent.success as boolean | undefined;
				if (cmd === "session.create" && success) {
					const sessionId = agentEvent.session_id;
					console.log("[ws-mux] Session created (response) for:", sessionId);
					this.sessionReady.add(sessionId);
					const waiters = this.sessionReadyWaiters.get(sessionId);
					if (waiters) {
						for (const waiter of waiters) {
							waiter();
						}
						this.sessionReadyWaiters.delete(sessionId);
					}

					// Request initial state (includes model info and Pi's
					// real session ID -- useChat detects ID mismatches and
					// triggers re-keying from there).
					this.send({
						channel: "agent",
						session_id: sessionId,
						cmd: "get_state",
					});

					const pending = this.pendingMessages.get(sessionId);
					console.log(
						"[ws-mux] Pending messages for session:",
						sessionId,
						pending?.length ?? 0,
					);
					if (pending?.length) {
						for (const entry of pending) {
							console.log(
								"[ws-mux] Flushing queued message:",
								entry.cmd,
								"for session:",
								sessionId,
							);
							this.send({
								channel: "agent",
								session_id: sessionId,
								cmd: entry.cmd,
								message: entry.message,
								id: entry.id,
								client_id: entry.client_id,
							});
						}
						this.pendingMessages.delete(sessionId);
					}
				}
			}

			// Also mark session ready on session.created event (from runner)
			if (agentEvent.event === "session.created") {
				const sessionId = agentEvent.session_id;
				if (!this.sessionReady.has(sessionId)) {
					console.log("[ws-mux] Session created (event) for:", sessionId);
					this.sessionReady.add(sessionId);
					const waiters = this.sessionReadyWaiters.get(sessionId);
					if (waiters) {
						for (const waiter of waiters) {
							waiter();
						}
						this.sessionReadyWaiters.delete(sessionId);
					}
				}
			}
		}

		// Handle system pings
		if (event.channel === "system" && event.type === "ping") {
			return;
		}

		// Check for correlated response.
		// Agent response events have `id` flattened at top level (from CommandResponse).
		// Other channels may have `id` directly on the event.
		let id: string | undefined;
		if ("id" in event) {
			id = (event as { id?: string }).id;
		}
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

		// Dispatch agent channel events to session-specific handlers
		if (event.channel === "agent") {
			const agentEvent = event as AgentWsEvent;
			const sessionId = agentEvent.session_id;
			if (sessionId) {
				const sessionHandlers = this.agentSessionHandlers.get(sessionId);
				if (sessionHandlers) {
					for (const handler of sessionHandlers) {
						try {
							handler(agentEvent);
						} catch (err) {
							console.error(
								"[ws-mux] Error in agent session event handler:",
								err,
							);
						}
					}
				}
			}
		}
	}

	private enqueueOrSendAgentMessage(
		sessionId: string,
		cmd: "prompt" | "steer" | "follow_up",
		message: string,
		id?: string,
		clientId?: string,
	): void {
		if (!this.isConnected) {
			const pending = this.pendingMessages.get(sessionId) ?? [];
			pending.push({ cmd, message, id, client_id: clientId });
			this.pendingMessages.set(sessionId, pending);
			return;
		}

		if (!this.sessionReady.has(sessionId)) {
			const pending = this.pendingMessages.get(sessionId) ?? [];
			pending.push({ cmd, message, id, client_id: clientId });
			this.pendingMessages.set(sessionId, pending);
			console.log(
				"[ws-mux] Queued agent message until session is ready:",
				sessionId,
				cmd,
				"queue size:",
				pending.length,
			);
			return;
		}

		this.send({
			channel: "agent",
			session_id: sessionId,
			cmd,
			message,
			id,
			client_id: clientId,
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

// Store the singleton on globalThis so it survives Vite HMR module reloads.
// Without this, every HMR update creates a new WsConnectionManager, losing all
// tracked sessions, subscriptions, and readiness state — which triggers redundant
// session.create calls and the associated get_messages clobbering.
const WS_MANAGER_KEY = "__octo_ws_manager__" as const;

/** Get the singleton WebSocket manager instance */
export function getWsManager(): WsConnectionManager {
	const g = globalThis as unknown as Record<
		string,
		WsConnectionManager | undefined
	>;
	if (!g[WS_MANAGER_KEY]) {
		g[WS_MANAGER_KEY] = new WsConnectionManager();
	}
	return g[WS_MANAGER_KEY] as WsConnectionManager;
}

/** Destroy the singleton instance (for cleanup in tests) */
export function destroyWsManager(): void {
	const g = globalThis as unknown as Record<
		string,
		WsConnectionManager | undefined
	>;
	const instance = g[WS_MANAGER_KEY];
	if (instance) {
		instance.disconnect();
		g[WS_MANAGER_KEY] = undefined;
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

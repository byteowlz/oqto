/**
 * Agent-channel transport: reconnect, session readiness, command acks,
 * and a persisted outbox — as pure logic over injected primitives.
 *
 * Ported from the proven legacy mux manager, trimmed to the agent channel
 * (files/terminal/trx stay legacy until their views move). No browser
 * globals: sockets, storage, and timers come from `TransportDeps`, so
 * tests drive everything deterministically.
 */

import type { JsonValue } from "./projection";
import type {
	CommandFields,
	ConnectionState,
	OutboxEntry,
	SendKind,
	SessionOptions,
	SocketLike,
	TimerHandle,
	TransportDeps,
	TransportStatus,
	WireEvent,
} from "./transport-contract";

export type ChatTransport = {
	disconnect(): void;
	attach(
		sessionId: string,
		handler: (event: WireEvent) => void,
		options?: SessionOptions,
	): () => void;
	sendMessage(sessionId: string, kind: SendKind, message: string): string;
	request(
		sessionId: string,
		cmd: string,
		fields?: CommandFields,
		timeoutMs?: number,
	): Promise<WireEvent>;
	observe(
		onConnection: ((state: ConnectionState) => void) | null,
		onResync?: (sessionId: string) => void,
	): () => void;
	status(sessionId?: string): TransportStatus;
};

const MAX_RECONNECT_ATTEMPTS = 20;
const BASE_RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_DELAY_MS = 30000;
const OUTBOX_MAX_AGE_MS = 10 * 60 * 1000;
const RESYNC_DELAY_MS = 300;
const ACK_TIMEOUT_MS = 5000;
const ACK_MAX_ATTEMPTS = 3;
const REQUEST_TIMEOUT_MS = 30000;

type PendingMessage = { id: string; cmd: SendKind; message: string };

export function createChatTransport(deps: TransportDeps): ChatTransport {
	const { clock } = deps;
	let socket: SocketLike | null = null;
	let state: ConnectionState = "disconnected";
	let epoch = 0;
	let reconnectAttempt = 0;
	let reconnectTimer: TimerHandle | null = null;
	let resyncTimer: TimerHandle | null = null;
	let everConnected = false;
	let requestCounter = 0;

	const sessionHandlers = new Map<string, (event: WireEvent) => void>();
	const subscribed = new Map<string, SessionOptions>();
	const ready = new Set<string>();
	const pendingBySession = new Map<string, PendingMessage[]>();
	const pendingRequests = new Map<string, (event: WireEvent) => void>();
	const acks = new Map<
		string,
		{ entry: OutboxEntry; attempts: number; timer: TimerHandle | null }
	>();
	const connectionHandlers = new Set<(state: ConnectionState) => void>();
	const resyncHandlers = new Set<(sessionId: string) => void>();

	const nextId = () => `req-${++requestCounter}-${clock.now()}`;

	function setState(next: ConnectionState): void {
		if (state === next) return;
		state = next;
		for (const handler of connectionHandlers) handler(next);
	}

	function persistOutbox(): void {
		deps.outbox.save([...acks.values()].map(({ entry }) => entry));
	}

	function write(command: Record<string, JsonValue | undefined>): boolean {
		if (!socket || state !== "connected") return false;
		socket.send(JSON.stringify(command));
		return true;
	}

	function clearAck(id: string): void {
		const ack = acks.get(id);
		if (!ack) return;
		if (ack.timer) clock.cancel(ack.timer);
		acks.delete(id);
		persistOutbox();
	}

	function scheduleAckRetry(id: string): void {
		const ack = acks.get(id);
		if (!ack) return;
		if (ack.timer) clock.cancel(ack.timer);
		ack.timer = clock.schedule(() => {
			const current = acks.get(id);
			if (!current) return;
			if (current.attempts >= ACK_MAX_ATTEMPTS) {
				clearAck(id);
				return;
			}
			current.attempts += 1;
			if (ready.has(current.entry.sessionId)) {
				write({
					channel: "agent",
					session_id: current.entry.sessionId,
					cmd: current.entry.cmd,
					message: current.entry.message,
					id,
				});
			}
			scheduleAckRetry(id);
		}, ACK_TIMEOUT_MS);
	}

	function trackAck(entry: OutboxEntry): void {
		const existing = acks.get(entry.id);
		if (existing?.timer) clock.cancel(existing.timer);
		acks.set(entry.id, {
			entry,
			attempts: existing?.attempts ?? 0,
			timer: null,
		});
		persistOutbox();
		scheduleAckRetry(entry.id);
	}

	function queueMessage(sessionId: string, message: PendingMessage): void {
		const queue = pendingBySession.get(sessionId) ?? [];
		queue.push(message);
		pendingBySession.set(sessionId, queue);
	}

	function flushSession(sessionId: string): void {
		const queue = pendingBySession.get(sessionId);
		if (!queue?.length) return;
		pendingBySession.delete(sessionId);
		for (const item of queue) {
			write({
				channel: "agent",
				session_id: sessionId,
				cmd: item.cmd,
				message: item.message,
				id: item.id,
			});
		}
	}

	function sendSessionCreate(sessionId: string): void {
		const options = subscribed.get(sessionId);
		ready.delete(sessionId);
		write({
			channel: "agent",
			session_id: sessionId,
			cmd: "session.create",
			config: options?.config ?? {},
		});
	}

	function markReady(sessionId: string): void {
		if (ready.has(sessionId)) return;
		ready.add(sessionId);
		flushSession(sessionId);
	}

	function scheduleResync(): void {
		if (resyncTimer) clock.cancel(resyncTimer);
		resyncTimer = clock.schedule(() => {
			resyncTimer = null;
			for (const sessionId of subscribed.keys()) {
				for (const handler of resyncHandlers) handler(sessionId);
			}
		}, RESYNC_DELAY_MS);
	}

	function handleEvent(event: WireEvent): void {
		if (event.channel !== "agent") return;

		if (event.event === "response") {
			if (event.id && acks.has(event.id)) clearAck(event.id);
			if (event.cmd === "session.create" && event.success && event.session_id) {
				markReady(event.session_id);
			}
			if (event.id) {
				const resolver = pendingRequests.get(event.id);
				if (resolver) {
					pendingRequests.delete(event.id);
					resolver(event);
				}
			}
		}
		if (event.event === "session.created" && event.session_id) {
			markReady(event.session_id);
		}

		if (event.session_id) {
			const handler = sessionHandlers.get(event.session_id);
			if (handler) handler(event);
		}
	}

	function handleClose(closedEpoch: number): void {
		if (closedEpoch !== epoch) return;
		socket = null;
		ready.clear();
		if (state === "disconnected") return;
		if (reconnectAttempt >= MAX_RECONNECT_ATTEMPTS) {
			setState("disconnected");
			return;
		}
		setState("reconnecting");
		const delay = Math.min(
			BASE_RECONNECT_DELAY_MS * 2 ** reconnectAttempt,
			MAX_RECONNECT_DELAY_MS,
		);
		reconnectAttempt += 1;
		if (reconnectTimer) clock.cancel(reconnectTimer);
		reconnectTimer = clock.schedule(() => {
			reconnectTimer = null;
			open();
		}, delay);
	}

	function open(): void {
		epoch += 1;
		const openedEpoch = epoch;
		setState(reconnectAttempt > 0 ? "reconnecting" : "connecting");
		const nextSocket = deps.sockets();
		socket = nextSocket;
		nextSocket.onOpen = () => {
			if (openedEpoch !== epoch) return;
			reconnectAttempt = 0;
			setState("connected");
			for (const [sessionId, options] of subscribed) {
				if (options.create !== false) sendSessionCreate(sessionId);
			}
			// Only a RE-connection needs resync; the initial load renders
			// fresh pages anyway.
			if (everConnected) scheduleResync();
			everConnected = true;
		};
		nextSocket.onMessage = (data) => {
			if (openedEpoch !== epoch) return;
			try {
				handleEvent(JSON.parse(data) as WireEvent);
			} catch {
				// Malformed frames are dropped; durable truth lives in the store.
			}
		};
		nextSocket.onClose = () => handleClose(openedEpoch);
	}

	function connect(): void {
		if (state === "connected" || state === "connecting") return;
		reconnectAttempt = 0;
		open();
	}

	// Restore the persisted outbox: queued sends survive a reload.
	for (const entry of deps.outbox.load()) {
		if (clock.now() - entry.updatedAt > OUTBOX_MAX_AGE_MS) continue;
		queueMessage(entry.sessionId, {
			id: entry.id,
			cmd: entry.cmd,
			message: entry.message,
		});
		acks.set(entry.id, { entry, attempts: 0, timer: null });
	}

	return {
		disconnect(): void {
			epoch += 1;
			if (reconnectTimer) clock.cancel(reconnectTimer);
			if (resyncTimer) clock.cancel(resyncTimer);
			for (const ack of acks.values()) {
				if (ack.timer) clock.cancel(ack.timer);
			}
			setState("disconnected");
			socket?.close(1000, "client disconnect");
			socket = null;
			ready.clear();
		},
		attach(
			sessionId: string,
			handler: (event: WireEvent) => void,
			options?: SessionOptions,
		): () => void {
			// Single timeline owner per session: replacing prevents double
			// application of additive deltas; a stale detach cannot remove a
			// newer owner.
			sessionHandlers.set(sessionId, handler);
			const wasTracked = subscribed.has(sessionId);
			subscribed.set(sessionId, options ?? {});
			const shouldCreate = options?.create !== false;
			if (shouldCreate && !(wasTracked && ready.has(sessionId))) {
				if (state === "connected") {
					sendSessionCreate(sessionId);
				} else if (state === "disconnected") {
					connect();
				}
			}
			return () => {
				if (sessionHandlers.get(sessionId) === handler) {
					sessionHandlers.delete(sessionId);
				}
				// subscribed/ready intentionally survive detach (StrictMode/HMR
				// re-subscribe must not re-create the session).
			};
		},
		sendMessage(sessionId, kind, message): string {
			const id = nextId();
			trackAck({
				id,
				sessionId,
				cmd: kind,
				message,
				updatedAt: clock.now(),
			});
			if (state !== "connected" || !ready.has(sessionId)) {
				queueMessage(sessionId, { id, cmd: kind, message });
				if (state === "disconnected") connect();
				return id;
			}
			const sent = write({
				channel: "agent",
				session_id: sessionId,
				cmd: kind,
				message,
				id,
			});
			if (!sent) queueMessage(sessionId, { id, cmd: kind, message });
			return id;
		},
		request(sessionId, cmd, fields, timeoutMs = REQUEST_TIMEOUT_MS) {
			const id = nextId();
			return new Promise<WireEvent>((resolve, reject) => {
				const timer = clock.schedule(() => {
					pendingRequests.delete(id);
					reject(new Error(`Request timeout: ${cmd}`));
				}, timeoutMs);
				pendingRequests.set(id, (event) => {
					clock.cancel(timer);
					resolve(event);
				});
				const sent = write({
					channel: "agent",
					session_id: sessionId,
					cmd,
					id,
					...fields,
				});
				if (!sent) {
					clock.cancel(timer);
					pendingRequests.delete(id);
					reject(new Error(`Not connected: ${cmd}`));
				}
			});
		},
		observe(onConnection, onResync): () => void {
			if (onConnection) connectionHandlers.add(onConnection);
			if (onResync) resyncHandlers.add(onResync);
			return () => {
				if (onConnection) connectionHandlers.delete(onConnection);
				if (onResync) resyncHandlers.delete(onResync);
			};
		},
		status(sessionId): TransportStatus {
			return {
				connection: state,
				sessionReady: sessionId ? ready.has(sessionId) : false,
			};
		},
	};
}

/**
 * Oqto Event Bus client.
 *
 * Provides publish/subscribe over the "bus" WebSocket channel.
 * All authorization is server-enforced; the client just sends commands.
 */

import { getWsManager } from "./ws-manager";
import type {
	BusEvent,
	BusScope,
	BusWsEvent,
	WsEventHandler,
} from "./ws-mux-types";

// ============================================================================
// Types
// ============================================================================

export type BusEventHandler = (event: BusEvent) => void;

export interface BusSubscription {
	/** Unsubscribe from this subscription (local + server). */
	unsubscribe: () => void;
}

export interface PublishOptions {
	scope: BusScope;
	scopeId: string;
	topic: string;
	payload: unknown;
	v?: number;
	priority?: string;
	ttlMs?: number;
	idempotencyKey?: string;
	correlationId?: string;
	ack?: { replyTo: string; timeoutMs: number };
}

export interface SubscribeOptions {
	scope: BusScope;
	scopeId: string;
	topics: string[];
	filter?: Record<string, unknown>;
}

export interface PullOptions {
	scope: BusScope;
	scopeId: string;
	topics: string[];
	sinceTs?: number;
	limit?: number;
}

// ============================================================================
// Bus Client
// ============================================================================

let nextCmdId = 1;

/**
 * Publish an event to the bus.
 */
export function busPublish(opts: PublishOptions): void {
	const ws = getWsManager();
	ws.send({
		channel: "bus",
		type: "publish",
		id: `bus-pub-${nextCmdId++}`,
		scope: opts.scope,
		scope_id: opts.scopeId,
		topic: opts.topic,
		payload: opts.payload,
		v: opts.v ?? 1,
		priority: opts.priority,
		ttl_ms: opts.ttlMs,
		idempotency_key: opts.idempotencyKey,
		correlation_id: opts.correlationId,
		ack: opts.ack
			? { reply_to: opts.ack.replyTo, timeout_ms: opts.ack.timeoutMs }
			: undefined,
	});
}

/**
 * Subscribe to bus events matching topics in a scope.
 * Returns an unsubscribe handle.
 */
export function busPull(opts: PullOptions): void {
	const ws = getWsManager();
	ws.send({
		channel: "bus",
		type: "pull",
		id: `bus-pull-${nextCmdId++}`,
		topics: opts.topics,
		scope: opts.scope,
		scope_id: opts.scopeId,
		since_ts: opts.sinceTs,
		limit: opts.limit,
	});
}

export function busSubscribe(
	opts: SubscribeOptions,
	handler: BusEventHandler,
): BusSubscription {
	const ws = getWsManager();

	// Tell the backend to register the subscription
	const cmdId = `bus-sub-${nextCmdId++}`;
	ws.send({
		channel: "bus",
		type: "subscribe",
		id: cmdId,
		topics: opts.topics,
		scope: opts.scope,
		scope_id: opts.scopeId,
		filter: opts.filter,
	});

	// Listen for bus events on the WS channel and filter locally
	const wsHandler: WsEventHandler = (event) => {
		const busEvent = event as BusWsEvent;
		if (busEvent.channel !== "bus") return;
		if (busEvent.type === "event") {
			// The backend already filtered by subscription, but we double-check scope.
			// scopeId "*" means wildcard across all scope IDs in that scope.
			const evt = busEvent as unknown as {
				channel: "bus";
				type: "event";
			} & BusEvent;
			if (
				evt.scope === opts.scope &&
				(opts.scopeId === "*" || evt.scope_id === opts.scopeId)
			) {
				handler(evt);
			}
		}
	};

	const unsubWs = ws.subscribe("bus", wsHandler);

	return {
		unsubscribe: () => {
			unsubWs();
			// Tell backend to remove subscription
			ws.send({
				channel: "bus",
				type: "unsubscribe",
				id: `bus-unsub-${nextCmdId++}`,
				topics: opts.topics,
				scope: opts.scope,
				scope_id: opts.scopeId,
			});
		},
	};
}

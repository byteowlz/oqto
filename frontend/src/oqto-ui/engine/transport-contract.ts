/**
 * Host-neutral primitives the transport runs on. Platform adapters supply
 * real sockets, storage, and timers; tests supply deterministic fakes.
 */

import type { JsonValue } from "./projection";

export type WireEvent = {
	channel: string;
	event?: string;
	type?: string;
	session_id?: string;
	cmd?: string;
	success?: boolean;
	id?: string;
	[key: string]: JsonValue | undefined;
};

export type SocketLike = {
	send(data: string): void;
	close(code?: number, reason?: string): void;
	onOpen: (() => void) | null;
	onMessage: ((data: string) => void) | null;
	onClose: (() => void) | null;
};

export type SocketFactory = () => SocketLike;

export type SendKind = "prompt" | "steer" | "follow_up";

export type OutboxEntry = {
	id: string;
	sessionId: string;
	cmd: SendKind;
	message: string;
	updatedAt: number;
};

export type OutboxStore = {
	load(): OutboxEntry[];
	save(entries: OutboxEntry[]): void;
};

/** Token minted by the platform's scheduler (web: setTimeout id). */
export type TimerHandle = number;

export type Scheduler = {
	schedule(callback: () => void, delayMs: number): TimerHandle;
	cancel(handle: TimerHandle): void;
	now(): number;
};

export type TransportDeps = {
	sockets: SocketFactory;
	outbox: OutboxStore;
	clock: Scheduler;
};

export type ConnectionState =
	| "disconnected"
	| "connecting"
	| "connected"
	| "reconnecting";

export type SessionOptions = {
	create?: boolean;
	config?: Record<string, JsonValue>;
};

export type CommandFields = Record<string, JsonValue>;

export type TransportStatus = {
	connection: ConnectionState;
	sessionReady: boolean;
};

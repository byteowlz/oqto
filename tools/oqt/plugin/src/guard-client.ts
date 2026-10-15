// Client-process adapter for the oqt guard operator socket (coding mode only).
// The operator socket is never visible to a brain; this is the local
// operator's decision path, not a model-reachable approve function.
import { connect, type Socket } from "node:net";
import type { Context } from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import type { PendingPermission, PermissionDecision } from "./contract.ts";

export interface InterruptedCall {
	readonly session: string;
	readonly tool: string;
	readonly argumentsText: string;
	readonly startedAt: number;
}
export interface RememberedRules {
	readonly read: boolean;
	readonly edit: boolean;
	readonly shell: boolean;
	readonly shell_prefixes: readonly string[];
}
export interface ActiveResource {
	readonly session: string;
	readonly capability: string;
	readonly resource: string;
	readonly expiresAt: number;
}
export interface OperatorState {
	readonly mode: "coding";
	readonly pending: readonly PendingPermission[];
	readonly interrupted: readonly InterruptedCall[];
	readonly resources: readonly ActiveResource[];
	readonly rules?: RememberedRules;
}
type Listener = (state: OperatorState, context: Context) => void;

export interface PermissionsPort {
	readonly state: {
		readonly value: OperatorState | undefined;
		subscribe(listener: Listener): () => void;
	};
	decide(
		decision: PermissionDecision & { session: string },
		context: Context,
	): Promise<void>;
	revokeAll(context: Context): Promise<void>;
	acknowledge(context: Context): Promise<void>;
	forgetRules(context: Context): Promise<void>;
	close(): void;
}

export function guardPermissions(path: string): PermissionsPort {
	let socket: Socket | undefined;
	let value: OperatorState | undefined;
	let buffer = "";
	let next = 0;
	let closed = false;
	const listeners = new Set<Listener>();
	const calls = new Map<
		number,
		{ resolve(value: unknown): void; reject(error: Error): void }
	>();
	const known = new Set<string>();
	const sessions = () => [
		...new Set([
			...(value?.pending ?? []).map((row) => row.session),
			...(value?.interrupted ?? []).map((row) => row.session),
			...known,
		]),
	];
	const open = () => {
		if (closed) return;
		const current = connect(path);
		socket = current;
		current.setEncoding("utf8");
		current.on("error", () => current.destroy());
		current.once("connect", () => {
			current.write(`${JSON.stringify({ id: 0, method: "subscribe" })}\n`);
		});
		current.on("data", (chunk: string) => {
			buffer += chunk;
			for (;;) {
				const newline = buffer.indexOf("\n");
				if (newline < 0) break;
				const line = buffer.slice(0, newline);
				buffer = buffer.slice(newline + 1);
				let message: Record<string, unknown>;
				try {
					message = JSON.parse(line);
				} catch {
					current.destroy();
					return;
				}
				if (
					message.event === "state" &&
					message.data &&
					typeof message.data === "object"
				) {
					const data = message.data as {
						sessions: {
							id: string;
							pending: PendingPermission[];
							interrupted: Omit<InterruptedCall, "session">[];
							resources?: Omit<ActiveResource, "session">[];
						}[];
						rules?: RememberedRules;
					};
					for (const session of data.sessions) known.add(session.id);
					value = {
						mode: "coding",
						pending: data.sessions.flatMap((session) => session.pending),
						interrupted: data.sessions.flatMap((session) =>
							session.interrupted.map((row) => ({
								...row,
								session: session.id,
							})),
						),
						resources: data.sessions.flatMap((session) =>
							(session.resources ?? []).map((row) => ({
								...row,
								session: session.id,
							})),
						),
						...(data.rules ? { rules: data.rules } : {}),
					};
					for (const listener of listeners) listener(value, BACKGROUND_CONTEXT);
					continue;
				}
				if (typeof message.id === "number" && message.id > 0) {
					const call = calls.get(message.id);
					calls.delete(message.id);
					if (!call) continue;
					if (message.ok === true) call.resolve(message.result);
					else
						call.reject(
							new Error(String(message.error ?? "Guard operator call failed")),
						);
				}
			}
		});
		current.once("close", () => {
			for (const call of calls.values())
				call.reject(new Error("Guard operator disconnected"));
			calls.clear();
			if (!closed) setTimeout(open, 250);
		});
	};
	const call = (method: string, params: Record<string, unknown>) =>
		new Promise<unknown>((resolve, reject) => {
			const current = socket;
			if (!current || current.destroyed) {
				reject(new Error("Guard operator unavailable"));
				return;
			}
			const id = ++next;
			calls.set(id, { resolve, reject });
			current.write(`${JSON.stringify({ id, method, params })}\n`);
		});
	open();
	return {
		state: {
			get value() {
				return value;
			},
			subscribe(listener) {
				listeners.add(listener);
				return () => listeners.delete(listener);
			},
		},
		async decide(decision, context) {
			context.abortSignal?.throwIfAborted();
			await call("decide", { ...decision });
		},
		async revokeAll(context) {
			context.abortSignal?.throwIfAborted();
			for (const session of sessions()) await call("revoke_all", { session });
		},
		async acknowledge(context) {
			context.abortSignal?.throwIfAborted();
			for (const session of new Set(
				(value?.interrupted ?? []).map((row) => row.session),
			))
				await call("acknowledge", { session });
		},
		async forgetRules(context) {
			context.abortSignal?.throwIfAborted();
			await call("forget_rules", {});
		},
		close() {
			closed = true;
			socket?.destroy();
		},
	};
}

import { setTimeout as sleep } from "node:timers/promises";
import type { Context } from "@earendil-works/chord";
import { Carrier } from "./carrier.ts";
import type { CodingGate, CodingOperation } from "./gated-tool.ts";

export interface CodingRequest {
	id: string;
	digest: string;
	revision: number;
	operation: CodingOperation;
}
function object(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === "object";
}

/** Local owner authority, not authenticated Oqto Account routing. */
export class CodingAuthority implements CodingGate {
	readonly tools: Carrier;
	readonly operator: Carrier;
	readonly pending = new Map<string, CodingRequest>();
	readonly deadlines = new Map<string, number>();
	readonly session: string;
	readonly publish: (requests: CodingRequest[]) => void;
	readonly replaceHands: (() => Promise<void>) | undefined;
	#frozen = false;
	#transition: Promise<void> | undefined;
	constructor(
		session: string,
		directory: string,
		binding: object,
		publish: (requests: CodingRequest[]) => void,
		policy: "ask" | "deny" = "ask",
		replaceHands?: () => Promise<void>,
	) {
		this.session = session;
		this.publish = publish;
		this.replaceHands = replaceHands;
		this.tools = new Carrier("tools", directory, binding, policy);
		this.operator = new Carrier("operator", directory, binding, policy);
	}
	async ready(context: Context) {
		context.abortSignal?.throwIfAborted();
		await this.#transition;
		context.abortSignal?.throwIfAborted();
		if (this.#frozen) throw new Error("Authority remains frozen");
	}
	async prepare(operation: CodingOperation, context: Context) {
		if (!operation.request_id.startsWith(`${this.session}:store:`))
			throw new Error("Operation is not bound to the public Session");
		try {
			for (;;) {
				await this.ready(context);
				context.abortSignal?.throwIfAborted();
				const result = await this.tools.call({ command: "prepare", operation });
				if (!object(result)) throw new Error("Invalid admission response");
				if (result.status === "allowed") {
					if (
						typeof result.expires_at !== "number" ||
						!Number.isFinite(result.expires_at)
					)
						throw new Error("Missing protected invocation deadline");
					this.deadlines.set(operation.request_id, result.expires_at);
					return;
				}
				const admission = result.admission;
				if (!object(admission) || admission.status !== "pending")
					throw new Error("Permission denied");
				const request = admission.value;
				if (
					!object(request) ||
					typeof request.digest !== "string" ||
					typeof request.revision !== "number"
				)
					throw new Error("Invalid pending request");
				this.pending.set(operation.request_id, {
					id: operation.request_id,
					digest: request.digest,
					revision: request.revision,
					operation,
				});
				this.publish([...this.pending.values()]);
				await sleep(100, undefined, { signal: context.abortSignal });
			}
		} finally {
			this.pending.delete(operation.request_id);
			this.publish([...this.pending.values()]);
		}
	}
	async check(operation: CodingOperation, context: Context) {
		if (this.#frozen) throw new Error("Authority delivery is frozen");
		context.abortSignal?.throwIfAborted();
		const result = await this.tools.call({ command: "check", operation });
		if (!object(result) || result.checked !== true)
			throw new Error("Delivery not authorized");
	}
	budget(operation: CodingOperation) {
		return Math.max(
			0,
			(this.deadlines.get(operation.request_id) ?? 0) - Date.now() - 100,
		);
	}
	async drain() {
		await this.replaceHands?.();
	}
	async finish(operation: CodingOperation) {
		this.deadlines.delete(operation.request_id);
		await this.tools.call({ command: "finish", id: operation.request_id });
	}
	async decide(
		request: CodingRequest,
		allow: boolean,
		lifetime: "once" | "session",
	) {
		await this.operator.call({
			command: "decide",
			id: request.id,
			digest: request.digest,
			revision: request.revision,
			lifetime,
			allow,
		});
	}
	async revokeAll() {
		if (this.#frozen) throw new Error("Authority is already frozen");
		this.#frozen = true;
		// If either ledger revocation or confirmed replacement fails, leave
		// admissions/delivery frozen. Never reactivate after an uncertain stop.
		this.#transition = (async () => {
			await this.operator.call({ command: "revoke_all" });
			await this.replaceHands?.();
			this.#frozen = false;
		})();
		await this.#transition;
	}
	async close() {
		await Promise.all([this.tools.close(), this.operator.close()]);
	}
}

import { setTimeout as sleep } from "node:timers/promises";
import { Carrier } from "./carrier.ts";
import type { CodingOperation } from "./gated-tool.ts";

export interface CodingRequest {
	id: string;
	digest: string;
	revision: number;
	operation: CodingOperation;
}
export interface InterruptedInvocation {
	operation: CodingOperation;
	started_at: number;
}
function object(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === "object";
}

/** Guard-side local owner authority, not authenticated Oqto Account routing.
 * Lives outside the brain: the brain reaches it only through the per-session
 * tool channel, the operator only through the guard's operator socket.
 */
export class CodingAuthority {
	tools: Carrier;
	readonly operator: Carrier;
	readonly pending = new Map<string, CodingRequest>();
	readonly session: string;
	readonly publish: (requests: CodingRequest[]) => void;
	readonly replaceHands: (() => Promise<void>) | undefined;
	readonly #directory: string;
	readonly #binding: object;
	readonly #policy: "ask" | "deny";
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
		this.#directory = directory;
		this.#binding = binding;
		this.#policy = policy;
		this.tools = new Carrier("tools", directory, binding, policy);
		this.operator = new Carrier("operator", directory, binding, policy);
	}
	async ready(signal?: AbortSignal) {
		signal?.throwIfAborted();
		await this.#transition;
		signal?.throwIfAborted();
		if (this.#frozen) throw new Error("Authority remains frozen");
	}
	/** Owner-only: live journal rows from a dead owner become interrupted and
	 * every overlay is revoked. Call before admitting work. */
	async recover(): Promise<InterruptedInvocation[]> {
		const result = await this.operator.call({ command: "recover" });
		if (!object(result) || !Array.isArray(result.interrupted))
			throw new Error("Invalid recovery response");
		return result.interrupted as InterruptedInvocation[];
	}
	async acknowledge() {
		await this.operator.call({ command: "acknowledge_interrupted" });
	}
	/** Discard the tool carrier's in-memory permits after its brain was lost. */
	async resetTools() {
		const previous = this.tools;
		this.tools = new Carrier(
			"tools",
			this.#directory,
			this.#binding,
			this.#policy,
		);
		await previous.close();
	}
	async prepare(operation: CodingOperation, signal: AbortSignal) {
		if (!operation.request_id.startsWith(`${this.session}:store:`))
			throw new Error("Operation is not bound to the public Session");
		try {
			for (;;) {
				await this.ready(signal);
				const result = await this.tools.call({ command: "prepare", operation });
				if (!object(result)) throw new Error("Invalid admission response");
				if (result.status === "allowed") {
					if (
						typeof result.expires_at !== "number" ||
						!Number.isFinite(result.expires_at)
					)
						throw new Error("Missing protected invocation deadline");
					return { expiresAt: result.expires_at };
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
				await sleep(100, undefined, { signal });
			}
		} finally {
			if (this.pending.delete(operation.request_id))
				this.publish([...this.pending.values()]);
		}
	}
	async check(operation: CodingOperation) {
		if (this.#frozen) throw new Error("Authority delivery is frozen");
		const result = await this.tools.call({ command: "check", operation });
		if (!object(result) || result.checked !== true)
			throw new Error("Delivery not authorized");
	}
	async finish(id: string) {
		await this.tools.call({ command: "finish", id });
	}
	async decide(
		request: { id: string; digest: string; revision: number },
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
	/** Freeze; run `step` (ledger mutation + confirmed hands replacement);
	 * unfreeze only if every step succeeded. Uncertain stops stay frozen. */
	async transition(step: () => Promise<void>) {
		if (this.#frozen) throw new Error("Authority is already frozen");
		this.#frozen = true;
		this.#transition = (async () => {
			await step();
			this.#frozen = false;
		})();
		await this.#transition;
	}
	async revokeAll() {
		await this.transition(async () => {
			await this.operator.call({ command: "revoke_all" });
			await this.replaceHands?.();
		});
	}
	async close() {
		await Promise.all([this.tools.close(), this.operator.close()]);
	}
}

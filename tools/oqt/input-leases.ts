// Input authority is separate from tool/resource permission. This object lives
// in the trusted host, never the brain. Attachment IDs come from the native
// coordinator, not a model or a caller-supplied service argument.
import { randomUUID } from "node:crypto";
import type { Peer } from "./guard-wire.ts";

export interface NativeInputCall {
	readonly serviceId: string;
	readonly member: string;
	readonly args: readonly unknown[];
}
export interface InputRequest {
	readonly id: string;
	readonly attachment: string;
	readonly label: string;
	readonly expiresAt: number;
}
export interface InputView {
	readonly available: boolean;
	readonly pending: InputRequest | null;
	readonly holder: string | null;
}
interface NativeAttachment {
	remote: boolean;
	readonly queued: Set<string>;
	cancel(id: string): Promise<void>;
	stop(): Promise<void>;
}
interface Pending {
	deciding: boolean;
	readonly view: InputRequest;
	readonly peer: Peer;
	finish(value: { granted: boolean; reason: string }): void;
}
export interface InputAdmission {
	readonly attachment: string;
	readonly generation: number;
	readonly remote: boolean;
}
const AGENT = "pi.agent-controller";
const MUTATIONS = new Set([
	"prompt",
	"steer",
	"followUp",
	"abort",
	"compact",
	"cancelQueued",
]);

export class SessionInputLeases {
	readonly #attachments = new Map<string, NativeAttachment>();
	#pending: Pending | undefined;
	#holder: { attachment: string; peer: Peer } | undefined;
	#generation = 0;
	#timer: ReturnType<typeof setTimeout> | undefined;
	#cleanup: Promise<void> = Promise.resolve();
	#closed = false;
	#failed = false;
	readonly changed: () => void;
	readonly durationMs: number;
	constructor(changed: () => void, durationMs = 60_000) {
		if (!Number.isInteger(durationMs) || durationMs < 1 || durationMs > 60_000)
			throw new Error("Invalid input lease duration");
		this.changed = changed;
		this.durationMs = durationMs;
	}
	view(): InputView {
		return {
			available: !this.#closed && !this.#failed,
			pending: this.#pending?.view ?? null,
			holder: this.#holder?.attachment ?? null,
		};
	}
	register(
		attachment: string,
		cancel: (id: string) => Promise<void>,
		stop: () => Promise<void>,
	) {
		if (
			this.#closed ||
			!attachment ||
			this.#attachments.has(attachment) ||
			this.#attachments.size >= 256
		)
			throw new Error("Invalid native attachment");
		this.#attachments.set(attachment, {
			remote: false,
			queued: new Set(),
			cancel,
			stop,
		});
	}
	/** Native manager has retired this scope; never carry its grant to recovery. */
	forget(attachment: string) {
		this.#attachments.get(attachment)?.queued.clear();
		if (
			this.#holder?.attachment === attachment ||
			this.#pending?.view.attachment === attachment
		)
			this.reclaim("native_worker_lost");
		this.#attachments.delete(attachment);
	}
	async unregister(attachment: string) {
		if (
			this.#holder?.attachment === attachment ||
			this.#pending?.view.attachment === attachment
		)
			this.reclaim("native_detach");
		const slot = this.#attachments.get(attachment);
		if (slot) {
			await this.#drain(slot);
			this.#attachments.delete(attachment);
		}
	}
	request(
		peer: Peer,
		attachment: string,
		label: string,
		signal: AbortSignal,
	): Promise<{ granted: boolean; reason: string }> {
		const slot = this.#attachments.get(attachment);
		if (
			this.#closed ||
			this.#failed ||
			!slot ||
			peer.closed.aborted ||
			signal.aborted
		)
			return Promise.resolve({
				granted: false,
				reason: "attachment_unavailable",
			});
		// Keep it classified remote after release. A stale native connection must
		// not regain local-terminal privileges by asking without a lease.
		slot.remote = true;
		if (this.#holder || this.#pending)
			return Promise.resolve({ granted: false, reason: "input_busy" });
		const view = {
			id: randomUUID(),
			attachment,
			label: Array.from(label)
				.filter((character) => character >= " " && character <= "~")
				.slice(0, 160)
				.join(""),
			expiresAt: Date.now() + 30_000,
		};
		return new Promise((resolve) => {
			const abort = () => this.reclaim("request_cancelled");
			const timer = setTimeout(abort, 30_000);
			timer.unref();
			peer.closed.addEventListener("abort", abort, { once: true });
			signal.addEventListener("abort", abort, { once: true });
			this.#pending = {
				deciding: false,
				view,
				peer,
				finish: (result) => {
					clearTimeout(timer);
					peer.closed.removeEventListener("abort", abort);
					signal.removeEventListener("abort", abort);
					resolve(result);
				},
			};
			this.changed();
		});
	}
	async decide(decider: Peer, id: string, allow: boolean) {
		const pending = this.#pending;
		if (
			!pending ||
			pending.deciding ||
			pending.view.id !== id ||
			pending.peer === decider ||
			decider.closed.aborted ||
			pending.peer.closed.aborted ||
			Date.now() >= pending.view.expiresAt
		)
			throw new Error("Input decision is stale or self-approved");
		pending.deciding = true;
		if (!allow) {
			this.#pending = undefined;
			pending.finish({ granted: false, reason: "terminal_denied" });
			this.changed();
			return;
		}
		const generation = this.#generation;
		await this.#cleanup;
		if (
			this.#closed ||
			this.#failed ||
			this.#pending !== pending ||
			generation !== this.#generation ||
			pending.peer.closed.aborted ||
			!this.#attachments.has(pending.view.attachment)
		) {
			if (this.#pending === pending) this.#pending = undefined;
			pending.finish({ granted: false, reason: "input_changed" });
			this.changed();
			return;
		}
		this.#pending = undefined;
		// Acquisition changes authority just as revocation does. Invalidate any
		// terminal admission that yielded while this confirmation was pending.
		this.#generation++;
		this.#holder = { attachment: pending.view.attachment, peer: pending.peer };
		this.#timer = setTimeout(
			() => this.reclaim("lease_expired"),
			this.durationMs,
		);
		this.#timer.unref();
		pending.finish({ granted: true, reason: "terminal_confirmed" });
		this.changed();
	}
	disconnect(peer: Peer) {
		if (this.#pending?.peer === peer || this.#holder?.peer === peer)
			this.reclaim("operator_disconnect");
	}
	release(peer: Peer) {
		if (this.#holder?.peer !== peer)
			throw new Error("Input lease belongs to another connection");
		this.reclaim("operator_release");
	}
	reclaim(reason: string) {
		this.#generation++;
		clearTimeout(this.#timer);
		this.#timer = undefined;
		const pending = this.#pending;
		this.#pending = undefined;
		pending?.finish({ granted: false, reason });
		const holder = this.#holder;
		this.#holder = undefined;
		if (holder) {
			const slot = this.#attachments.get(holder.attachment);
			if (slot) void this.#drain(slot);
		}
		this.changed();
	}
	async before(
		attachment: string,
		call: NativeInputCall,
	): Promise<InputAdmission | undefined> {
		const agentMutation =
			call.serviceId === AGENT && MUTATIONS.has(call.member);
		const modelMutation =
			call.serviceId === "pi.models" &&
			["cycleThinking", "refresh", "select", "selectThinking"].includes(
				call.member,
			);
		const navigationMutation =
			call.serviceId === "oqt.session-forks" &&
			["select", "fork"].includes(call.member);
		if (!agentMutation && !modelMutation && !navigationMutation) return;
		const slot = this.#attachments.get(attachment);
		if (this.#closed || this.#failed || !slot)
			throw new Error("Native input attachment unavailable");
		if (slot.remote) {
			if (
				modelMutation ||
				navigationMutation ||
				!["prompt", "steer", "abort", "cancelQueued"].includes(call.member)
			)
				throw new Error("Remote input action unsupported");
			const cleanup =
				call.member === "cancelQueued" &&
				typeof call.args[0] === "string" &&
				slot.queued.has(call.args[0]);
			if (call.member === "cancelQueued" && !cleanup)
				throw new Error("Queued input belongs to another source");
			if (!cleanup && this.#holder?.attachment !== attachment)
				throw new Error("Remote input lease required");
		} else {
			this.reclaim("terminal_input");
			await this.#cleanup;
		}
		if (this.#closed || this.#failed || !this.#attachments.has(attachment))
			throw new Error("Native input authority changed");
		return { attachment, generation: this.#generation, remote: slot.remote };
	}
	/** Synchronous dispatch fence: there must be no await between this and native send. */
	assertCurrent(admission: InputAdmission | undefined) {
		if (!admission) return;
		if (
			this.#closed ||
			this.#failed ||
			admission.generation !== this.#generation ||
			!this.#attachments.has(admission.attachment) ||
			(admission.remote && this.#holder?.attachment !== admission.attachment)
		)
			throw new Error("Input authority changed before dispatch");
	}
	async after(
		admission: InputAdmission | undefined,
		call: NativeInputCall,
		result: unknown,
	) {
		if (!admission?.remote) return;
		const slot = this.#attachments.get(admission.attachment);
		if (!slot) return;
		if (call.member === "cancelQueued") {
			slot.queued.delete(String(call.args[0]));
			return;
		}
		if (!["prompt", "steer", "followUp"].includes(call.member)) return;
		if (!result || typeof result !== "object")
			return this.#uncertain(slot, "invalid_native_receipt");
		const receipt = result as {
			accepted?: boolean;
			operationId?: unknown;
			entryId?: unknown;
		};
		const id = receipt.operationId ?? receipt.entryId;
		if (receipt.accepted === false) return;
		if (
			receipt.accepted !== true ||
			typeof id !== "string" ||
			!/^[1-9][0-9]{0,15}$/.test(id) ||
			!Number.isSafeInteger(Number(id))
		) {
			return this.#uncertain(slot, "invalid_native_receipt");
		}
		if (slot.queued.size >= 128)
			return this.#uncertain(slot, "input_queue_bound");
		slot.queued.add(id);
		if (
			admission.generation !== this.#generation ||
			this.#holder?.attachment !== admission.attachment
		)
			await this.#drain(slot);
	}
	#drain(slot: NativeAttachment): Promise<void> {
		this.#cleanup = this.#cleanup.then(() => this.#cancel(slot));
		return this.#cleanup;
	}
	async #uncertain(slot: NativeAttachment, reason: string): Promise<never> {
		this.reclaim(reason);
		try {
			await slot.stop();
		} catch {
			this.#failed = true;
		}
		throw new Error("Native input outcome uncertain");
	}
	async #cancel(slot: NativeAttachment) {
		try {
			for (const id of slot.queued) {
				await slot.cancel(id);
				slot.queued.delete(id);
			}
		} catch {
			try {
				await slot.stop();
				slot.queued.clear();
			} catch {
				this.#failed = true;
				this.changed();
			}
		}
	}
	async close() {
		this.#closed = true;
		this.reclaim("host_closed");
		await this.#cleanup;
		for (const id of [...this.#attachments.keys()]) await this.unregister(id);
	}
}

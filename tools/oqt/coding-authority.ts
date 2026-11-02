import { setTimeout as sleep } from "node:timers/promises";
import { Carrier } from "./carrier.ts";
import { GuardOperationalError, operationalFeedback } from "./guard-errors.ts";

export type PermissionCode =
	| "protected_safety"
	| "configured_ceiling"
	| "operator_denied"
	| "expired"
	| "stale"
	| "cancelled"
	| "unavailable"
	| "unknown";
export interface PermissionFeedback {
	code: PermissionCode;
}
const permissionMessages: Record<PermissionCode, string> = {
	protected_safety:
		"Built-in safety protection blocks this scope. Request a narrower directory that does not contain protected resources.",
	configured_ceiling:
		"The configured permission ceiling blocks this operation. Ask the owner to review policy or request a narrower scope.",
	operator_denied:
		"The operator declined this permission request. Discuss the scope with the operator before making a new request.",
	expired:
		"Permission expired. Review the current state before requesting permission again; do not replay effects.",
	stale:
		"Permission is no longer current. Review the current state before making a new request.",
	cancelled:
		"Permission waiting was cancelled. No operator denial was recorded by this result.",
	unavailable:
		"The permission authority is unavailable. No operator denial is established; restore the authority before retrying.",
	unknown:
		"Permission was not authorized; the cause is unknown. Ask the owner to inspect the request and policy.",
};
export function permissionFeedback(
	value: unknown,
): PermissionFeedback | undefined {
	if (
		!object(value) ||
		typeof value.code !== "string" ||
		!Object.hasOwn(permissionMessages, value.code)
	)
		return undefined;
	return { code: value.code as PermissionCode };
}
export class PermissionDeniedError extends Error {
	readonly feedback: PermissionFeedback;
	constructor(feedback: PermissionFeedback) {
		super(permissionMessages[feedback.code]);
		this.name = "PermissionDeniedError";
		this.feedback = { code: feedback.code };
	}
}

/** Any guarded operation: workspace tools, external paths or network. */
export interface Operation {
	request_id: string;
	target: { capability: string; resource_id: string };
	arguments: Record<string, unknown>;
}
export interface CodingRequest {
	id: string;
	digest: string;
	revision: number;
	operation: Operation;
}
export interface InterruptedInvocation {
	operation: Operation;
	started_at: number;
}
export interface ResourceGrant {
	target: { capability: string; resource_id: string };
	expires_at: number;
}
/** Trusted ceiling inputs; the carrier compiles them deterministically. */
/** Where the carrier loads the ceiling from: the one sandbox config
 * (ADR-0060). The carrier reads both files itself through oqto-sandbox. */
export interface CeilingInput {
	/** The owner's sandbox.toml (an absent file means defaults). */
	user_config: string;
	remembered: {
		read: boolean;
		edit: boolean;
		shell: boolean;
		shell_prefixes: string[];
	};
	home: string;
	workspace: string;
	hard_deny: string[];
}
function object(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === "object";
}

/** Guard-side local owner authority, not authenticated Oqto Account routing.
 * Lives outside the brain: the brain reaches it only through the per-session
 * tool channel, the operator only through the guard's operator socket.
 */
export class CodingAuthority {
	tools!: Carrier;
	readonly operator: Carrier;
	readonly pending = new Map<string, CodingRequest>();
	readonly session: string;
	readonly publish: (requests: CodingRequest[]) => void;
	readonly replaceHands: (() => Promise<void>) | undefined;
	readonly #directory: string;
	readonly #binding: object;
	readonly #ceiling: CeilingInput;
	#frozen = false;
	#transition: Promise<void> | undefined;
	private constructor(
		session: string,
		directory: string,
		binding: object,
		publish: (requests: CodingRequest[]) => void,
		ceiling: CeilingInput,
		replaceHands?: () => Promise<void>,
	) {
		this.session = session;
		this.publish = publish;
		this.replaceHands = replaceHands;
		this.#directory = directory;
		this.#binding = binding;
		this.#ceiling = ceiling;
		this.operator = new Carrier("operator", directory, binding, ceiling);
	}
	/** Operator first: only it may adopt a changed ceiling (revoking every
	 * grant) and recover a dead owner's journal, before the tool channel opens
	 * strictly against the same ledger. */
	static async open(
		session: string,
		directory: string,
		binding: object,
		publish: (requests: CodingRequest[]) => void,
		ceiling: CeilingInput,
		replaceHands?: () => Promise<void>,
	) {
		const authority = new CodingAuthority(
			session,
			directory,
			binding,
			publish,
			ceiling,
			replaceHands,
		);
		try {
			const interrupted = await authority.recover();
			authority.tools = new Carrier("tools", directory, binding, ceiling);
			return { authority, interrupted };
		} catch (error) {
			await authority.operator.close();
			throw error;
		}
	}
	async ready(signal?: AbortSignal) {
		signal?.throwIfAborted();
		await this.#transition;
		signal?.throwIfAborted();
		if (this.#frozen) throw new GuardOperationalError("unavailable");
	}
	/** Owner-only: live journal rows from a dead owner become interrupted and
	 * every overlay is revoked. Call before admitting work. */
	async recover(): Promise<InterruptedInvocation[]> {
		// The first call also covers carrier start, config load and ledger
		// migration, which can be slow on a loaded single CPU.
		const result = await this.operator.call({ command: "recover" }, 30_000);
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
			this.#ceiling,
		);
		await previous.close();
	}
	/** Active Session overlays plus baseline pre-approvals (both need mounts). */
	async resources(): Promise<{
		grants: ResourceGrant[];
		baseline: {
			readPaths: string[];
			writePaths: string[];
			network: boolean;
			deniedPaths: string[];
		};
	}> {
		const result = await this.operator.call({ command: "resources" });
		if (
			!object(result) ||
			!Array.isArray(result.resources) ||
			!object(result.baseline)
		)
			throw new Error("Invalid resource response");
		const baseline = result.baseline;
		if (
			!Array.isArray(baseline.read_paths) ||
			!Array.isArray(baseline.write_paths) ||
			!Array.isArray(baseline.denied_paths) ||
			typeof baseline.network !== "boolean"
		)
			throw new Error("Invalid baseline resource response");
		return {
			grants: result.resources as ResourceGrant[],
			baseline: {
				readPaths: baseline.read_paths as string[],
				writePaths: baseline.write_paths as string[],
				network: baseline.network,
				deniedPaths: baseline.denied_paths as string[],
			},
		};
	}
	async prepare(operation: Operation, signal: AbortSignal) {
		if (!operation.request_id.startsWith(`${this.session}:store:`))
			throw new Error("Operation is not bound to the public Session");
		try {
			for (;;) {
				await this.ready(signal);
				const result = await this.tools.call({ command: "prepare", operation });
				if (!object(result))
					throw new PermissionDeniedError({ code: "unknown" });
				if (result.status === "allowed") {
					if (
						typeof result.expires_at !== "number" ||
						!Number.isFinite(result.expires_at)
					)
						throw new PermissionDeniedError({ code: "unknown" });
					return { expiresAt: result.expires_at };
				}
				const admission = result.admission;
				if (!object(admission) || admission.status !== "pending")
					throw new PermissionDeniedError(
						object(admission) && admission.status === "denied"
							? (permissionFeedback(admission.value) ?? { code: "unknown" })
							: { code: "unknown" },
					);
				const request = admission.value;
				if (
					!object(request) ||
					typeof request.digest !== "string" ||
					typeof request.revision !== "number"
				)
					throw new PermissionDeniedError({ code: "unknown" });
				this.pending.set(operation.request_id, {
					id: operation.request_id,
					digest: request.digest,
					revision: request.revision,
					operation,
				});
				this.publish([...this.pending.values()]);
				await sleep(100, undefined, { signal });
			}
		} catch (error) {
			if (error instanceof PermissionDeniedError) throw error;
			throw new PermissionDeniedError({
				code: signal.aborted ? "cancelled" : "unavailable",
			});
		} finally {
			if (this.pending.delete(operation.request_id))
				this.publish([...this.pending.values()]);
		}
	}
	async check(operation: Operation) {
		if (this.#frozen) throw new PermissionDeniedError({ code: "unavailable" });
		let result: unknown;
		try {
			result = await this.tools.call({ command: "check", operation });
		} catch {
			throw new PermissionDeniedError({ code: "unavailable" });
		}
		if (!object(result) || result.checked !== true)
			throw new PermissionDeniedError(
				object(result)
					? (permissionFeedback(result.feedback) ?? { code: "unknown" })
					: { code: "unknown" },
			);
	}
	async operatorBegin(id: string, command: string) {
		const result = await this.operator.call({
			command: "operator_begin",
			operation: {
				request_id: id,
				target: { capability: "execute", resource_id: "operator-shell" },
				arguments: { tool: "bash", input: { command } },
			},
		});
		if (object(result)) {
			const operational = operationalFeedback(result.operational_error);
			if (operational) throw new GuardOperationalError(operational.code);
			if (result.journaled === true) return;
		}
		throw new GuardOperationalError("unknown");
	}
	async operatorSeal(id: string, digest: string) {
		await this.operator.call({ command: "operator_seal", id, digest });
	}
	async operatorFinish(id: string, digest: string) {
		await this.operator.call({ command: "operator_finish", id, digest });
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
		await Promise.all([this.tools?.close(), this.operator.close()]);
	}
}

// Brain-side adapters over the per-session guard tool channel. Nothing here can
// approve, revoke or read owner files: the guard decides; the brain asks.
import type { Context } from "@earendil-works/chord";
import type {
	AuthOperationOptions,
	Credential,
	CredentialStore,
} from "@earendil-works/pi-ai";
import { Connection, RemoteExecutionEnv } from "@earendil-works/pi-env";
import type { CodingGate, CodingOperation } from "./gated-tool.ts";
import type { WireClient } from "./guard-wire.ts";
import type { HandsClient } from "./hands-client.ts";

function object(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === "object";
}

export class GuardGate implements CodingGate {
	readonly guard: WireClient;
	readonly hands: HandsClient;
	readonly #deadlines = new Map<string, number>();
	readonly #pinned = new Map<string, unknown>();
	/** Per-call remote hands (ADR-0086): a fresh Connection for exactly one
	 * guard-admitted call, closed at finish; never reused or reconnected. */
	readonly #remote = new Map<
		string,
		{ machine: string; cwd: string; connection: Connection }
	>();
	#remoteCount = 0;
	/** Set once the guard offered named machines: results then name theirs. */
	namedMachines = false;
	constructor(guard: WireClient, hands: HandsClient) {
		this.guard = guard;
		this.hands = hands;
	}
	#adopt(result: unknown) {
		if (!object(result) || typeof result.epoch !== "string")
			throw new Error("Guard returned no hands generation");
		this.hands.use(result.epoch);
		return result;
	}
	async prepare(operation: CodingOperation, context: Context) {
		const raw = await this.guard.call(
			"prepare",
			{ operation },
			context.abortSignal,
		);
		if (!object(raw) || typeof raw.expires_at !== "number")
			throw new Error("Guard returned no invocation deadline");
		const remote = raw.remote;
		if (object(remote)) {
			if (
				typeof remote.machine !== "string" ||
				typeof remote.cwd !== "string" ||
				typeof remote.epoch !== "string" ||
				!/^[a-f0-9]{32}$/.test(remote.epoch)
			)
				throw new Error("Guard returned an invalid remote admission");
			// The guard routes this one-use token to the paired machine; the
			// brain holds no key, address or link of its own.
			const connection = new Connection({
				command: [
					process.execPath,
					this.hands.proxy,
					this.hands.socketPath,
					remote.epoch,
				],
			});
			const env = new RemoteExecutionEnv({
				connection,
				id: `oqt-remote-${remote.machine}-${++this.#remoteCount}`,
				cwd: remote.cwd,
			});
			this.#remote.set(operation.request_id, {
				machine: remote.machine,
				cwd: remote.cwd,
				connection,
			});
			this.#deadlines.set(operation.request_id, raw.expires_at);
			this.#pinned.set(operation.request_id, env);
			return;
		}
		const result = this.#adopt(raw);
		this.#deadlines.set(operation.request_id, result.expires_at as number);
		this.#pinned.set(operation.request_id, this.hands.current);
	}
	/** Where an admitted call ran: named machine and its cwd. */
	target(operation: CodingOperation) {
		const remote = this.#remote.get(operation.request_id);
		return remote
			? { machine: remote.machine, cwd: remote.cwd }
			: { machine: "local", cwd: this.hands.workspace };
	}
	env(operation: CodingOperation) {
		const pinned = this.#pinned.get(operation.request_id);
		if (!pinned) throw new Error("Operation has no admitted hands generation");
		return pinned;
	}
	/** Harness-internal (non-tool) access follows the guard's current generation. */
	async refresh() {
		this.#adopt(await this.guard.call("epoch"));
	}
	async check(operation: CodingOperation, context: Context) {
		await this.guard.call("check", { operation }, context.abortSignal);
	}
	budget(operation: CodingOperation) {
		return Math.max(
			0,
			(this.#deadlines.get(operation.request_id) ?? 0) - Date.now() - 100,
		);
	}
	/** Ask the guard for an external resource; resolves once the operator decided. */
	async requestAccess(
		params: {
			request_id: string;
			path?: string;
			access?: "read" | "write";
			network?: boolean;
			reason: string;
		},
		signal?: AbortSignal,
	) {
		const result = this.#adopt(
			await this.guard.call("request_resource", params, signal),
		);
		return result as {
			granted: { capability: string; resource: string };
			expires_at: number;
		};
	}
	async drain(operation: CodingOperation) {
		// The remote stream closes first, so the destination generation drains.
		this.#remote.get(operation.request_id)?.connection.close();
		this.#adopt(await this.guard.call("drain", { id: operation.request_id }));
	}
	async finish(operation: CodingOperation) {
		this.#deadlines.delete(operation.request_id);
		this.#pinned.delete(operation.request_id);
		this.#remote.get(operation.request_id)?.connection.close();
		this.#remote.delete(operation.request_id);
		await this.guard.call("finish", { id: operation.request_id });
	}
	/** Named machines the guard offers (names and descriptions only). */
	async machines() {
		const value = await this.guard.call("machines");
		return Array.isArray(value)
			? value.filter(
					(
						row,
					): row is {
						machine: string;
						platform: string | null;
						cwd: string | null;
					} => object(row) && typeof row.machine === "string",
				)
			: [];
	}
}

/** Credentials arrive from the guard's owner broker with refresh secrets
 * removed. The brain can never write, rotate or log out owner credentials. */
export function guardCredentials(guard: WireClient): CredentialStore {
	return {
		async read(providerId: string, options?: AuthOperationOptions) {
			const value = await guard.call(
				"credential",
				{ provider: providerId },
				options?.signal,
			);
			return (value ?? undefined) as Credential | undefined;
		},
		async list(options?: AuthOperationOptions) {
			const value = await guard.call("credentials", {}, options?.signal);
			if (!Array.isArray(value)) throw new Error("Invalid credential list");
			return value as { providerId: string; type: Credential["type"] }[];
		},
		async modify() {
			throw new Error(
				"Credentials are owned by the guard broker; the brain cannot modify them",
			);
		},
		async delete() {
			throw new Error(
				"Credentials are owned by the guard broker; the brain cannot delete them",
			);
		},
	};
}

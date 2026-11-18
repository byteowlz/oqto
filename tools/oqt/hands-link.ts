// Guard-side client for the remote hands link (ADR-0086). The guard spawns
// `oqt_link serve` as a foreground child: JSON lines over inherited stdio and
// a private data socket for per-call pi-env bytes. The brain never sees the
// link, its keys or its sockets; every remote call is admitted here first.
import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { connect, type Socket } from "node:net";
import { homedir } from "node:os";
import { join } from "node:path";

export interface MachineStatus {
	machine: string;
	binding: string;
	lease_ms: number;
	renew_ms: number;
	max_concurrent_calls: number;
	connected: boolean;
	cwd: string | null;
	platform: string | null;
	remote_attention: boolean | null;
	attention: string | null;
	active_calls: number;
}

export type RemoteTool = "read" | "write" | "edit" | "bash";

export interface RemoteClaim {
	call_id: string;
	generation: number;
	binding: string;
	cwd: string;
	platform: string;
	attach: string;
}

export interface RemoteReceipt {
	call_id: string;
	binding_id: string;
	generation: number;
	state: string;
	stop: "not_started" | "pending" | "confirmed" | "unconfirmed";
	cwd: string;
	bytes_in: number;
	bytes_out: number;
}

/** Machine names are the model-visible `machine` values (never addresses). */
export const MACHINE_NAME = /^[a-z][a-z0-9-]{0,31}$/;

export function linkConfigPath(env: NodeJS.ProcessEnv = process.env) {
	if (env.OQT_HANDS_LINK_CONFIG) return env.OQT_HANDS_LINK_CONFIG;
	return join(
		env.XDG_CONFIG_HOME || join(homedir(), ".config"),
		"oqt",
		"hands-link.toml",
	);
}

/** Stable controller namespace per guard profile (persisted takeover CAS). */
export function linkNamespace(profile: string) {
	return createHash("sha256")
		.update(`oqt-hands-link:${profile}`)
		.digest("hex")
		.slice(0, 32);
}

/** call id: the guard's controller namespace plus the stable tool-call
 * identity (Session, conversation, task), never text or visible order. */
export function remoteCallId(namespace: string, requestId: string) {
	return createHash("sha256")
		.update(`${namespace}\n${requestId}`)
		.digest("hex")
		.slice(0, 32);
}

/** Digest of the immutable operation descriptor bound to the claim. */
export function remoteDigest(
	machine: string,
	binding: string,
	tool: string,
	input: unknown,
) {
	return createHash("sha256")
		.update(JSON.stringify({ machine, binding, tool, input }))
		.digest("hex");
}

type Pending = { resolve(value: Record<string, unknown>): void };

export class HandsLink {
	readonly child: ChildProcessWithoutNullStreams;
	readonly dataSocket: string;
	readonly namespace: string;
	readonly #pending = new Map<number, Pending>();
	readonly #listeners = new Set<(event: Record<string, unknown>) => void>();
	#next = 1;
	#buffer = "";
	#closed = false;
	#machines: MachineStatus[] = [];
	private constructor(
		child: ChildProcessWithoutNullStreams,
		dataSocket: string,
		namespace: string,
	) {
		this.child = child;
		this.dataSocket = dataSocket;
		this.namespace = namespace;
		child.stdout.setEncoding("utf8");
		child.stdout.on("data", (chunk: string) => this.#data(chunk));
		child.stdin.on("error", () => {});
		child.stderr.on("data", (chunk: Buffer) => {
			if (process.env.OQT_DEBUG)
				console.error(`oqt link: ${String(chunk).trimEnd()}`);
		});
		child.once("exit", () => {
			this.#closed = true;
			for (const pending of this.#pending.values())
				pending.resolve({ ok: false, status: "unknown", code: "link_exited" });
			this.#pending.clear();
		});
	}

	/** Spawn the adapter when an owner config exists; otherwise no machines. */
	static async open(options: {
		binary: string | undefined;
		dataSocket: string;
		profile: string;
		env?: NodeJS.ProcessEnv;
	}): Promise<HandsLink | undefined> {
		const env = options.env ?? process.env;
		if (!options.binary || !existsSync(linkConfigPath(env))) return undefined;
		const child = spawn(
			options.binary,
			[
				"serve",
				"--data-socket",
				options.dataSocket,
				"--namespace",
				linkNamespace(options.profile),
			],
			{
				stdio: ["pipe", "pipe", "pipe"],
				env: {
					PATH: "/usr/bin:/bin",
					HOME: homedir(),
					OQT_HANDS_LINK_CONFIG: linkConfigPath(env),
					...(env.OQT_HANDS_LINK_STATE
						? { OQT_HANDS_LINK_STATE: env.OQT_HANDS_LINK_STATE }
						: env.XDG_STATE_HOME
							? { XDG_STATE_HOME: env.XDG_STATE_HOME }
							: {}),
					...(env.OQT_DEBUG ? { RUST_LOG: "info" } : {}),
				},
			},
		);
		const link = new HandsLink(
			child,
			options.dataSocket,
			linkNamespace(options.profile),
		);
		await new Promise<void>((resolve, reject) => {
			const timer = setTimeout(
				() => reject(new Error("oqt_link did not start")),
				10_000,
			);
			const ready = (event: Record<string, unknown>) => {
				if (event.event !== "ready") return;
				clearTimeout(timer);
				link.#listeners.delete(ready);
				resolve();
			};
			link.#listeners.add(ready);
			child.once("exit", () => {
				clearTimeout(timer);
				reject(new Error("oqt_link exited during startup"));
			});
		});
		await link.refresh();
		return link;
	}

	#data(chunk: string) {
		this.#buffer += chunk;
		for (;;) {
			const newline = this.#buffer.indexOf("\n");
			if (newline < 0) break;
			const line = this.#buffer.slice(0, newline);
			this.#buffer = this.#buffer.slice(newline + 1);
			let message: Record<string, unknown>;
			try {
				message = JSON.parse(line);
			} catch {
				continue;
			}
			if (typeof message.id === "number") {
				const pending = this.#pending.get(message.id);
				this.#pending.delete(message.id);
				pending?.resolve(message);
			} else for (const listener of this.#listeners) listener(message);
		}
		if (this.#buffer.length > 1024 * 1024) this.child.kill("SIGKILL");
	}

	onEvent(listener: (event: Record<string, unknown>) => void) {
		this.#listeners.add(listener);
		return () => this.#listeners.delete(listener);
	}

	request(
		op: Record<string, unknown>,
		timeoutMs = 40_000,
	): Promise<Record<string, unknown>> {
		if (this.#closed)
			return Promise.resolve({
				ok: false,
				status: "not_started",
				code: "link_exited",
			});
		const id = this.#next++;
		return new Promise((resolve) => {
			const timer = setTimeout(() => {
				this.#pending.delete(id);
				// A lost reply never becomes "not started": the request may have run.
				resolve({ ok: false, status: "unknown", code: "link_timeout" });
			}, timeoutMs);
			this.#pending.set(id, {
				resolve: (value) => {
					clearTimeout(timer);
					resolve(value);
				},
			});
			this.child.stdin.write(`${JSON.stringify({ id, ...op })}\n`);
		});
	}

	get machines(): readonly MachineStatus[] {
		return this.#machines;
	}

	async refresh() {
		const reply = await this.request({ op: "machines" }, 5_000);
		if (reply.ok === true && Array.isArray(reply.machines))
			this.#machines = reply.machines as MachineStatus[];
		return this.#machines;
	}

	/** Connect the control link (no effect) so offline/unbound fail first. */
	async probe(machine: string) {
		const reply = await this.request({ op: "probe", machine }, 15_000);
		await this.refresh().catch(() => {});
		return reply;
	}

	claim(machine: string, callId: string, digest: string, tool: RemoteTool) {
		return this.request(
			{ op: "claim", machine, call_id: callId, digest, tool },
			20_000,
		);
	}
	release(callId: string) {
		return this.request({ op: "release", call_id: callId }, 30_000);
	}
	cancel(callId: string) {
		return this.request({ op: "cancel", call_id: callId }, 30_000);
	}
	inspect(machine: string, callId: string) {
		return this.request({ op: "inspect", machine, call_id: callId }, 15_000);
	}
	async freeze(machine?: string) {
		const reply = await this.request(
			{ op: "freeze", ...(machine ? { machine } : {}) },
			40_000,
		);
		await this.refresh().catch(() => {});
		return reply;
	}

	/** Open the call's data path: one-use local attach, then raw pi-env bytes. */
	attachData(
		callId: string,
		attach: string,
		syncToken: string,
	): Promise<Socket> {
		return new Promise((resolve, reject) => {
			const socket = connect(this.dataSocket);
			socket.once("error", reject);
			socket.once("connect", () => {
				socket.off("error", reject);
				socket.write(
					`${JSON.stringify({ call_id: callId, attach, sync_token: syncToken })}\n`,
				);
				resolve(socket);
			});
		});
	}

	async close() {
		this.#closed = true;
		this.child.stdin.end();
		await new Promise<void>((resolve) => {
			if (this.child.exitCode !== null || this.child.signalCode !== null)
				resolve();
			else {
				const timer = setTimeout(() => {
					this.child.kill("SIGKILL");
					resolve();
				}, 3_000);
				this.child.once("exit", () => {
					clearTimeout(timer);
					resolve();
				});
			}
		});
	}
}

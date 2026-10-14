// The oqt guard: protected authority outside the brain. It owns the permission
// ledger carriers, confined hands, owner credentials and the operator surface.
// A brain reaches only its own per-session tool socket; operators reach the
// operator socket, which no brain can see. Local OS ownership is the operator
// boundary here; this is not production Account authentication.
import { type ChildProcess, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import {
	chmodSync,
	closeSync,
	lstatSync,
	mkdirSync,
	mkdtempSync,
	openSync,
	writeFileSync,
} from "node:fs";
import { rm } from "node:fs/promises";
import type { Server } from "node:net";
import { userInfo } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import {
	CodingAuthority,
	type CodingRequest,
	type InterruptedInvocation,
} from "./coding-authority.ts";
import type { CodingOperation } from "./gated-tool.ts";
import { type Peer, serve } from "./guard-wire.ts";
import { HandsSupervisor } from "./hands-supervisor.ts";
import type { OwnerBroker } from "./owner-broker.ts";

function object(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === "object" && !Array.isArray(value);
}

function codingOperation(value: unknown): CodingOperation {
	if (
		!object(value) ||
		typeof value.request_id !== "string" ||
		!object(value.target) ||
		value.target.capability !== "execute" ||
		value.target.resource_id !== "workspace-tools" ||
		!object(value.arguments) ||
		typeof value.arguments.tool !== "string"
	)
		throw new Error("Invalid coding operation");
	return value as unknown as CodingOperation;
}

function privateDirectory(path: string) {
	mkdirSync(path, { recursive: true, mode: 0o700 });
	const meta = lstatSync(path);
	if (
		!meta.isDirectory() ||
		meta.uid !== userInfo().uid ||
		(meta.mode & 0o077) !== 0
	)
		throw new Error(`Guard directory must be private and owned: ${path}`);
}

function summarize(input: unknown) {
	const text = JSON.stringify(input) ?? "";
	return text.length > 2000 ? `${text.slice(0, 2000)}…` : text;
}

export interface BrainConfig {
	/** oqt_brain launcher. */
	launcher: string;
	node: string;
	bpf: string;
	shim: string;
	/** Read-only binds every brain needs (private source snapshot, pinned checkout). */
	ro: string[];
	/** Read-write binds (session store, private agent profile). */
	rw: string[];
	/** Never bindable into a brain, inside or as an ancestor. */
	forbidden: string[];
	/** Static environment: PI_, OQT_ and NODE_OPTIONS keys only. */
	env: Record<string, string>;
	/** Workspaces inside this tree are refused. */
	trustedSource: string;
	/** Directory of the private upstream experimental sources. */
	experimental: string;
	log?: string;
}

export interface GuardOptions {
	authorityRoot: string;
	protectedRoots: string[];
	hands: { launcher: string; daemon: string; bpf: string };
	broker: OwnerBroker;
	brain?: BrainConfig;
	policy?: "ask" | "deny";
	/** Parent for the private guard directory; defaults to /var/tmp. */
	parent?: string;
}

export interface SessionState {
	id: string;
	workspace: string;
	pending: {
		id: string;
		digest: string;
		revision: number;
		capability: string;
		resource: string;
		argumentsText: string;
		session: string;
	}[];
	interrupted: { tool: string; argumentsText: string; startedAt: number }[];
}

export class GuardSession {
	readonly id: string;
	readonly workspace: string;
	readonly guard: Guard;
	/** Guard-private: brain spec, hands spec. Never bound into a brain. */
	readonly privateDir: string;
	/** Brain-visible (read-only bind): tool and hands transport sockets only. */
	readonly socketsDir: string;
	/** Brain bwrap work directory; empty and session-local. */
	readonly brainCwd: string;
	readonly toolPath: string;
	readonly handsPath: string;
	readonly ready: Promise<void>;
	authority!: CodingAuthority;
	supervisor!: HandsSupervisor;
	pending: CodingRequest[] = [];
	interrupted: InterruptedInvocation[] = [];
	/** Host pid of the current brain's bwrap, as spawned by this guard. */
	brainPid: number | undefined;
	readonly #live = new Map<Peer, Set<string>>();
	#toolServer: Server | undefined;
	#serial: Promise<unknown> = Promise.resolve();
	constructor(guard: Guard, id: string, workspace: string, index: number) {
		this.guard = guard;
		this.id = id;
		this.workspace = workspace;
		this.privateDir = join(guard.directory, `s${index}`);
		privateDirectory(this.privateDir);
		// Short brain-visible root keeps socket paths far below 108 bytes.
		const brainRoot = mkdtempSync("/var/tmp/oqs-");
		chmodSync(brainRoot, 0o700);
		this.socketsDir = join(brainRoot, "sockets");
		this.brainCwd = join(brainRoot, "cwd");
		privateDirectory(this.socketsDir);
		privateDirectory(this.brainCwd);
		this.toolPath = join(this.socketsDir, "tool.sock");
		this.handsPath = join(this.socketsDir, "hands.sock");
		this.ready = this.#init();
		this.ready.catch(() => {});
	}
	/** One protected state transition at a time. */
	serial<T>(step: () => Promise<T>): Promise<T> {
		const next = this.#serial.then(step, step);
		this.#serial = next.catch(() => {});
		return next;
	}
	async #init() {
		const options = this.guard.options;
		const authorityDir = join(
			options.authorityRoot,
			createHash("sha256").update(this.id).digest("hex").slice(0, 32),
		);
		privateDirectory(authorityDir);
		this.supervisor = await HandsSupervisor.open({
			launcher: options.hands.launcher,
			workspace: this.workspace,
			daemon: options.hands.daemon,
			bpf: options.hands.bpf,
			protectedRoots: [
				...options.protectedRoots,
				this.guard.directory,
				dirname(this.socketsDir),
			],
			parent: this.privateDir,
			socketPath: this.handsPath,
		});
		const hash = createHash("sha256").update(this.workspace).digest("hex");
		this.authority = new CodingAuthority(
			this.id,
			authorityDir,
			{
				account_id: "local-operator",
				principal_id: `uid:${userInfo().uid}`,
				workspace_id: `local:${hash}`,
				work_directory_id: `workdir:${hash}`,
				session_id: this.id,
				audience: "local-operator-only",
			},
			(requests) => {
				this.pending = requests;
				this.guard.emit();
			},
			options.policy ?? "ask",
			() => this.supervisor.replace(),
		);
		// A previous guard/host may have died mid-invocation.
		this.interrupted = await this.authority.recover();
		this.#toolServer = await serve(
			this.toolPath,
			(method, params, signal, peer) =>
				this.#tool(method, params, signal, peer),
			{
				onConnect: (peer) => this.#live.set(peer, new Set()),
				onClose: (peer) => {
					const lost = this.#live.get(peer) ?? new Set();
					this.#live.delete(peer);
					void this.brainLost(lost).catch(() => {});
				},
			},
		);
		this.guard.emit();
	}
	async #tool(
		method: string,
		params: Record<string, unknown>,
		signal: AbortSignal,
		peer: Peer,
	): Promise<unknown> {
		await this.ready;
		const live = this.#live.get(peer);
		if (!live) throw new Error("Unknown brain connection");
		switch (method) {
			case "prepare": {
				const operation = codingOperation(params.operation);
				const { expiresAt } = await this.authority.prepare(operation, signal);
				live.add(operation.request_id);
				await this.authority.ready(signal);
				return { expires_at: expiresAt, epoch: this.supervisor.epoch };
			}
			case "check": {
				const operation = codingOperation(params.operation);
				if (!live.has(operation.request_id))
					throw new Error("No live invocation on this brain connection");
				await this.authority.check(operation);
				return { checked: true };
			}
			case "finish": {
				if (typeof params.id !== "string" || !live.has(params.id))
					throw new Error("No live invocation on this brain connection");
				await this.authority.finish(params.id);
				live.delete(params.id);
				return { finished: true };
			}
			case "drain": {
				// Retire the executor (and any detached descendants) before delivery.
				await this.supervisor.replace();
				await this.authority.ready(signal);
				return { epoch: this.supervisor.epoch };
			}
			case "epoch":
				await this.authority.ready(signal);
				return { epoch: this.supervisor.epoch };
			case "identity":
				// The worker must report the host pid the manager spawned and will signal.
				if (this.brainPid === undefined) throw new Error("No spawned brain");
				return { pid: this.brainPid };
			case "credential":
				if (typeof params.provider !== "string")
					throw new Error("Provider required");
				return (await this.guard.options.broker.read(params.provider)) ?? null;
			case "credentials":
				return await this.guard.options.broker.list();
			default:
				throw new Error("Method unavailable on the tool channel");
		}
	}
	/** The brain is gone. Hands stop, in-memory permits are discarded and any
	 * invocation it held becomes an interrupted, never-replayed record that also
	 * fences every overlay. Failure leaves the authority frozen. */
	brainLost(lost: Set<string>) {
		return this.serial(() =>
			this.authority.transition(async () => {
				await this.supervisor.stop();
				await this.authority.resetTools();
				if (lost.size > 0) this.interrupted = await this.authority.recover();
				this.supervisor.install();
				this.guard.emit();
			}),
		);
	}
	revokeAll() {
		return this.serial(() => this.authority.revokeAll());
	}
	async acknowledge() {
		await this.authority.acknowledge();
		this.interrupted = [];
		this.guard.emit();
	}
	state(): SessionState {
		return {
			id: this.id,
			workspace: this.workspace,
			pending: this.pending.map((request) => ({
				id: request.id,
				digest: request.digest,
				revision: request.revision,
				capability: request.operation.arguments.tool,
				resource: "confined workspace",
				argumentsText: summarize(request.operation.arguments.input),
				session: this.id,
			})),
			interrupted: this.interrupted.map((entry) => ({
				tool: entry.operation.arguments.tool,
				argumentsText: summarize(entry.operation.arguments.input),
				startedAt: entry.started_at,
			})),
		};
	}
	async close() {
		// Init may still be creating the tool server; close it only afterwards.
		await this.ready.catch(() => {});
		await new Promise<void>((resolve) =>
			this.#toolServer ? this.#toolServer.close(() => resolve()) : resolve(),
		);
		try {
			await this.supervisor?.close();
		} finally {
			await this.authority?.close();
			await rm(dirname(this.socketsDir), { recursive: true, force: true });
		}
	}
}

export class Guard {
	readonly options: GuardOptions;
	readonly directory: string;
	readonly operatorPath: string;
	readonly sessions = new Map<string, GuardSession>();
	readonly #subscribers = new Set<Peer>();
	#operator: Server | undefined;
	#index = 0;
	private constructor(options: GuardOptions) {
		this.options = options;
		this.directory = mkdtempSync(join(options.parent ?? "/var/tmp", "oqg-"));
		chmodSync(this.directory, 0o700);
		this.operatorPath = join(this.directory, "operator.sock");
	}
	static async open(options: GuardOptions) {
		privateDirectory(options.authorityRoot);
		const guard = new Guard(options);
		guard.#operator = await serve(
			guard.operatorPath,
			(method, params, _signal, peer) => guard.#operate(method, params, peer),
			{ onClose: (peer) => guard.#subscribers.delete(peer) },
		);
		chmodSync(guard.operatorPath, 0o600);
		return guard;
	}
	session(id: string, workspace: string) {
		const existing = this.sessions.get(id);
		if (existing) {
			if (existing.workspace !== workspace)
				throw new Error("Session work directory changed; refusing rebinding");
			return existing;
		}
		const session = new GuardSession(this, id, workspace, ++this.#index);
		this.sessions.set(id, session);
		return session;
	}
	state() {
		return { sessions: [...this.sessions.values()].map((s) => s.state()) };
	}
	emit() {
		const state = this.state();
		for (const peer of this.#subscribers) peer.push("state", state);
	}
	#find(params: Record<string, unknown>) {
		if (typeof params.session !== "string") throw new Error("Session required");
		const session = this.sessions.get(params.session);
		if (!session) throw new Error("Unknown session");
		return session;
	}
	async #operate(
		method: string,
		params: Record<string, unknown>,
		peer: Peer,
	): Promise<unknown> {
		switch (method) {
			case "subscribe":
				this.#subscribers.add(peer);
				peer.push("state", this.state());
				return { subscribed: true };
			case "decide": {
				const session = this.#find(params);
				await session.ready;
				if (
					typeof params.id !== "string" ||
					typeof params.digest !== "string" ||
					typeof params.revision !== "number" ||
					typeof params.allow !== "boolean" ||
					(params.lifetime !== "once" && params.lifetime !== "session")
				)
					throw new Error("Invalid decision");
				await session.authority.decide(
					{ id: params.id, digest: params.digest, revision: params.revision },
					params.allow,
					params.lifetime,
				);
				return { decided: true };
			}
			case "revoke_all": {
				const session = this.#find(params);
				await session.ready;
				await session.revokeAll();
				return { revoked: true };
			}
			case "acknowledge": {
				const session = this.#find(params);
				await session.ready;
				await session.acknowledge();
				return { acknowledged: true };
			}
			default:
				throw new Error("Unknown operator method");
		}
	}
	/** Synchronous replacement for upstream's unconfined worker spawn. */
	spawnBrain(
		options: {
			metadata: { id: string; cwd: string; path: string };
			pluginManifestPaths: readonly string[];
		},
		env: Record<string, string>,
	): ChildProcess {
		const brain = this.options.brain;
		if (!brain) throw new Error("Guard has no brain confinement configured");
		const workspace = resolve(options.metadata.cwd);
		const trusted = resolve(brain.trustedSource);
		if (workspace === trusted || workspace.startsWith(trusted + sep))
			throw new Error(
				"Choose a coding workspace outside oqt's own trusted source tree (use --workdir)",
			);
		const session = this.session(options.metadata.id, workspace);
		const control = env.PI_SESSION_WORKER_CONTROL_ADDRESS;
		const spec = {
			node: brain.node,
			cwd: session.brainCwd,
			ro: [
				...brain.ro,
				session.socketsDir,
				...new Set(options.pluginManifestPaths.map((path) => dirname(path))),
				...(control ? [control] : []),
			],
			rw: brain.rw,
			forbidden: [
				...brain.forbidden,
				this.directory,
				this.options.authorityRoot,
			],
			bpf: brain.bpf,
			env: {
				...brain.env,
				...env,
				__PI_INTERNAL_SPAWN: "session-worker",
				OQT_GUARD_SOCKET: session.toolPath,
				OQT_HANDS_SOCKET: session.handsPath,
			},
			args: [
				"--import",
				pathToFileURL(join(brain.experimental, "source-resolver.ts")).href,
				join(brain.experimental, "session-worker.ts"),
				JSON.stringify(options),
			],
		};
		const specPath = join(session.privateDir, "brain.json");
		writeFileSync(specPath, JSON.stringify(spec), { mode: 0o600 });
		chmodSync(specPath, 0o600);
		const log = brain.log ? openSync(brain.log, "a", 0o600) : "ignore";
		try {
			const child = spawn(brain.launcher, [specPath], {
				detached: true,
				stdio: ["ignore", log, log],
				env: {
					PATH: "/usr/local/bin:/usr/bin:/bin",
					OQTO_SANDBOX_SHIM_BIN: brain.shim,
				},
			});
			child.unref();
			session.brainPid = child.pid;
			child.once("exit", (code, signal) => {
				if (code !== 0)
					console.error(
						`oqt: brain for session ${session.id} exited (${signal ?? code}); see brain.log`,
					);
			});
			return child;
		} finally {
			if (typeof log === "number") closeSync(log);
		}
	}
	async close() {
		await new Promise<void>((resolve) =>
			this.#operator ? this.#operator.close(() => resolve()) : resolve(),
		);
		for (const session of this.sessions.values())
			await session.close().catch(() => {});
		await rm(this.directory, { recursive: true, force: true });
	}
}

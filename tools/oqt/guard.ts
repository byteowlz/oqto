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
	constants,
	lstatSync,
	mkdirSync,
	mkdtempSync,
	openSync,
	writeFileSync,
} from "node:fs";
import { open, realpath, rm } from "node:fs/promises";
import type { Server } from "node:net";
import { userInfo } from "node:os";
import { dirname, isAbsolute, join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import {
	CodingAuthority,
	type CodingRequest,
	type InterruptedInvocation,
	type Operation,
	type ResourceGrant,
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

/** Hands mounts compiled from active resource grants. */
function handsSpec(
	grants: ResourceGrant[],
	baseline?: {
		readPaths: string[];
		writePaths: string[];
		network: boolean;
		deniedPaths: string[];
	},
) {
	const read = new Set<string>();
	const write = new Set<string>();
	let network = false;
	for (const { target } of grants) {
		const path = target.resource_id.startsWith("path:")
			? target.resource_id.slice(5)
			: undefined;
		if (target.capability === "file_read" && path) read.add(path);
		else if (target.capability === "file_write" && path) write.add(path);
		else if (target.capability === "network") network = true;
	}
	for (const path of write) read.delete(path);
	// Baseline pre-approvals are mounts as well (read-write: "always allow").
	if (baseline) {
		for (const path of baseline.readPaths) read.add(path);
		for (const path of baseline.writePaths) write.add(path);
		network = network || baseline.network;
	}
	return {
		readPaths: [...read].sort(),
		writePaths: [...write].sort(),
		network,
		mask: (baseline?.deniedPaths ?? []).sort(),
	};
}

function describe(operation: Operation) {
	const { capability, resource_id } = operation.target;
	const args = operation.arguments;
	if (capability === "execute")
		return {
			kind: "tool" as const,
			capability: String(args.tool),
			resource: "confined workspace",
			argumentsText: summarize(args.input),
		};
	return {
		kind: "resource" as const,
		capability:
			capability === "file_write"
				? "write"
				: capability === "file_read"
					? "read"
					: capability,
		resource: resource_id.startsWith("path:")
			? resource_id.slice(5)
			: "host network (all destinations, including localhost)",
		argumentsText: typeof args.reason === "string" ? args.reason : "",
	};
}

/** The workspace ceiling is agent-writable data: bounded, regular, never followed. */
async function readWorkspaceCeiling(workspace: string) {
	let handle: Awaited<ReturnType<typeof open>> | undefined;
	try {
		handle = await open(
			join(workspace, ".oqt", "policy.toml"),
			constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
		);
	} catch (error) {
		if (error instanceof Error && "code" in error && error.code === "ENOENT")
			return null;
		throw new Error(
			"Workspace ceiling .oqt/policy.toml is unreadable or a symlink",
		);
	}
	try {
		const info = await handle.stat();
		if (!info.isFile() || info.size > 64 * 1024)
			throw new Error("Workspace ceiling must be a regular file under 64 KiB");
		return await handle.readFile("utf8");
	} finally {
		await handle.close();
	}
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
	/** Owner ceiling inputs. The workspace's own .oqt/policy.toml is read per
	 * session and may only narrow. Built-in hard denies always apply. */
	ceiling?: { global: string | null; home: string; hardDeny: string[] };
	/** Parent for the private guard directory; defaults to /var/tmp. */
	parent?: string;
	/** Project context files (AGENTS.md chain) read by the trusted host for
	 * the brain's system prompt. Content is untrusted prompt data. */
	contextFiles?: (workspace: string) => { path: string; content: string }[];
}

const CONTEXT_FILE_LIMIT = 256 * 1024;
const CONTEXT_TOTAL_LIMIT = 1024 * 1024;
function boundedContext(files: { path: string; content: string }[]) {
	let total = 0;
	const out: { path: string; content: string }[] = [];
	for (const file of files) {
		let content = file.content;
		if (content.length > CONTEXT_FILE_LIMIT)
			content = `${content.slice(0, CONTEXT_FILE_LIMIT)}\n[truncated by oqt: file exceeds ${CONTEXT_FILE_LIMIT} bytes]`;
		if (total + content.length > CONTEXT_TOTAL_LIMIT) break;
		total += content.length;
		out.push({ path: file.path, content });
	}
	return out;
}

export interface SessionState {
	id: string;
	workspace: string;
	pending: {
		id: string;
		digest: string;
		revision: number;
		kind: "tool" | "resource";
		capability: string;
		resource: string;
		argumentsText: string;
		session: string;
	}[];
	interrupted: { tool: string; argumentsText: string; startedAt: number }[];
	resources: { capability: string; resource: string; expiresAt: number }[];
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
	resources: ResourceGrant[] = [];
	#resourceKey = JSON.stringify(
		handsSpec([], {
			readPaths: [],
			writePaths: [],
			network: false,
			deniedPaths: [],
		}),
	);
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
		guard.recordRoot(brainRoot);
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
		const ceiling = options.ceiling ?? {
			global: null,
			home: userInfo().homedir,
			hardDeny: [],
		};
		const opened = await CodingAuthority.open(
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
			{
				global: ceiling.global,
				workspace_file: await readWorkspaceCeiling(this.workspace),
				home: ceiling.home,
				workspace: this.workspace,
				hard_deny: [
					...ceiling.hardDeny,
					...options.protectedRoots,
					this.guard.directory,
					dirname(this.socketsDir),
					options.authorityRoot,
				],
			},
			() => this.#applyResources(),
		);
		this.authority = opened.authority;
		// A previous guard/host may have died mid-invocation.
		this.interrupted = opened.interrupted;
		await this.#applyResources();
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
				const { expiresAt } = await this.authority.prepare(
					operation as unknown as Operation,
					signal,
				);
				live.add(operation.request_id);
				// Expired/revoked resource grants leave the mounts before this call runs.
				await this.syncResources();
				await this.authority.ready(signal);
				return { expires_at: expiresAt, epoch: this.supervisor.epoch };
			}
			case "request_resource": {
				const operation = await this.#resourceOperation(params);
				const { expiresAt } = await this.authority.prepare(operation, signal);
				live.add(operation.request_id);
				try {
					// The grant itself has no executor effect; it becomes a mount.
					await this.syncResources();
				} finally {
					await this.authority.finish(operation.request_id);
					live.delete(operation.request_id);
				}
				return {
					granted: describe(operation),
					expires_at: expiresAt,
					epoch: this.supervisor.epoch,
				};
			}
			case "check": {
				const operation = codingOperation(params.operation);
				if (!live.has(operation.request_id))
					throw new Error("No live invocation on this brain connection");
				await this.authority.check(operation as unknown as Operation);
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
			case "context":
				return boundedContext(
					this.guard.options.contextFiles?.(this.workspace) ?? [],
				);
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
	async #resourceOperation(
		params: Record<string, unknown>,
	): Promise<Operation> {
		const id = params.request_id;
		if (typeof id !== "string" || !id.startsWith(`${this.id}:store:`))
			throw new Error("Request is not bound to the public Session");
		const reason =
			typeof params.reason === "string" ? params.reason.slice(0, 500) : "";
		if (!reason) throw new Error("A reason is required for access requests");
		if (params.network === true)
			return {
				request_id: id,
				target: { capability: "network", resource_id: "egress" },
				arguments: { network: true, reason },
			};
		const access = params.access;
		if (access !== "read" && access !== "write")
			throw new Error('access must be "read" or "write"');
		if (typeof params.path !== "string" || !params.path)
			throw new Error("path required");
		const requested = isAbsolute(params.path)
			? params.path
			: resolve(this.workspace, params.path);
		let real: string;
		try {
			// Mount what the path resolves to now; symlinks are not followed later.
			real = await realpath(requested);
		} catch {
			throw new Error("Requested path does not exist");
		}
		return {
			request_id: id,
			target: {
				capability: access === "write" ? "file_write" : "file_read",
				resource_id: `path:${real}`,
			},
			arguments: { path: real, access, reason },
		};
	}
	/** Replace hands with mounts compiled from currently active grants. Runs
	 * inside an authority transition (frozen admission/delivery). */
	async #applyResources() {
		const { grants, baseline } = await this.authority.resources();
		if (process.env.OQT_DEBUG)
			console.error(
				`oqt resources: grants=${grants.length} allow=${baseline.readPaths.length} deny=${baseline.deniedPaths.length}`,
			);
		this.resources = grants;
		const spec = handsSpec(grants, baseline);
		await this.supervisor.replace(spec);
		this.#resourceKey = JSON.stringify(spec);
		this.guard.emit();
	}
	/** Apply grant changes (new, expired or revoked) before admitting work. */
	async syncResources() {
		const { grants, baseline } = await this.authority.resources();
		if (JSON.stringify(handsSpec(grants, baseline)) === this.#resourceKey) {
			this.resources = grants;
			return;
		}
		await this.serial(() =>
			this.authority.transition(() => this.#applyResources()),
		);
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
				// Recovery may have revoked resource grants; mount only what is active.
				await this.#applyResources();
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
				...describe(request.operation),
				session: this.id,
			})),
			interrupted: this.interrupted.map((entry) => {
				const described = describe(entry.operation);
				return {
					tool: `${described.capability}`,
					argumentsText: described.argumentsText,
					startedAt: entry.started_at,
				};
			}),
			resources: this.resources.map((grant) => {
				const described = describe({
					request_id: "",
					target: grant.target,
					arguments: {},
				});
				return {
					capability: described.capability,
					resource: described.resource,
					expiresAt: grant.expires_at,
				};
			}),
		};
	}
	async close() {
		// Init may still be creating the tool server; close it only afterwards.
		await this.ready.catch(() => {});
		// Let in-flight transitions (brain loss, revocation) finish before the
		// carriers underneath them disappear.
		await this.serial(() => Promise.resolve()).catch(() => {});
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
	readonly #roots = new Set<string>();
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
	/** Brain-visible roots live outside the guard directory (they must not
	 * overlap its forbidden root); list them so a crashed host can be swept. */
	recordRoot(root: string) {
		this.#roots.add(root);
		writeFileSync(
			join(this.directory, "roots.json"),
			JSON.stringify([...this.#roots]),
			{
				mode: 0o600,
			},
		);
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

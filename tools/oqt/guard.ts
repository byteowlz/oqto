// The oqt guard: protected authority outside the brain. It owns the permission
// ledger carriers, confined hands, owner credentials and the operator surface.
// A brain reaches only its own per-session tool socket; operators reach the
// operator socket, which no brain can see. Local OS ownership is the operator
// boundary here; this is not production Account authentication.

import { type ChildProcess, spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import {
	chmodSync,
	closeSync,
	existsSync,
	lstatSync,
	mkdirSync,
	mkdtempSync,
	openSync,
	readFileSync,
	renameSync,
	writeFileSync,
} from "node:fs";
import { realpath, rm } from "node:fs/promises";
import type { Server } from "node:net";
import { userInfo } from "node:os";
import { dirname, isAbsolute, join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import {
	BACKGROUND_CONTEXT,
	withAbortSignal,
} from "@earendil-works/chord/context";
import {
	canAdmitSession,
	classicImportSeedPath,
	isClassicImportStaged,
	operateClassicImport,
	retireClassicImportPeer,
} from "./classic-import-host.ts";
import { classify } from "./classify.ts";
import {
	CodingAuthority,
	type CodingRequest,
	type InterruptedInvocation,
	type Operation,
	type ResourceGrant,
} from "./coding-authority.ts";
import { GuardOperationalError } from "./guard-errors.ts";
import { type Peer, serve } from "./guard-wire.ts";
import { HandsClient } from "./hands-client.ts";
import { HandsSupervisor } from "./hands-supervisor.ts";
import { SessionInputLeases } from "./input-leases.ts";
import {
	digestOperatorResult,
	type OperatorResult,
	validateOperatorResult,
	validReceiptConversation,
} from "./operator-receipts.ts";
import type { OwnerBroker } from "./owner-broker.ts";
import { mark } from "./trace.ts";

function object(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === "object" && !Array.isArray(value);
}

/** Validate the brain's call and replace its target with the guard's own
 * classification: what a call may do is decided here, not by the brain. */
function codingOperation(value: unknown): Operation {
	if (
		!object(value) ||
		typeof value.request_id !== "string" ||
		!object(value.arguments) ||
		typeof value.arguments.tool !== "string"
	)
		throw new GuardOperationalError("invalid_request");
	const { resource } = classify(value.arguments.tool, value.arguments.input);
	return {
		request_id: value.request_id,
		target: { capability: "execute", resource_id: resource },
		arguments: { tool: value.arguments.tool, input: value.arguments.input },
	};
}

function privateDirectory(path: string) {
	mkdirSync(path, { recursive: true, mode: 0o700 });
	const meta = lstatSync(path);
	if (
		!meta.isDirectory() ||
		meta.uid !== userInfo().uid ||
		(meta.mode & 0o077) !== 0
	)
		throw new GuardOperationalError("invalid_binding");
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
	if (capability === "execute") {
		const classified = classify(String(args.tool), args.input);
		return {
			kind: "tool" as const,
			category: classified.category,
			...(classified.prefix ? { prefix: classified.prefix } : {}),
			capability: String(args.tool),
			resource: "confined workspace",
			argumentsText: summarize(args.input),
			preview: classified.preview,
		};
	}
	return {
		kind: "resource" as const,
		category: "resource" as const,
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
		preview: typeof args.reason === "string" ? args.reason : "",
	};
}

export interface Remembered {
	read: boolean;
	edit: boolean;
	shell: boolean;
	shell_prefixes: string[];
}

/** Workspace rules the operator chose to keep ("always here"). */
export class RulesStore {
	readonly path: string | undefined;
	constructor(path: string | undefined) {
		this.path = path;
	}
	load(): Remembered {
		const empty = {
			read: false,
			edit: false,
			shell: false,
			shell_prefixes: [],
		};
		if (!this.path || !existsSync(this.path)) return empty;
		const value: unknown = JSON.parse(readFileSync(this.path, "utf8"));
		if (!object(value)) return empty;
		return {
			read: value.read === true,
			edit: value.edit === true,
			shell: value.shell === true,
			shell_prefixes: Array.isArray(value.shell_prefixes)
				? value.shell_prefixes.filter((p): p is string => typeof p === "string")
				: [],
		};
	}
	save(rules: Remembered) {
		if (!this.path) throw new GuardOperationalError("unavailable");
		const pending = `${this.path}.tmp`;
		writeFileSync(pending, JSON.stringify(rules, null, 2), { mode: 0o600 });
		chmodSync(pending, 0o600);
		renameSync(pending, this.path);
	}
	remember(category: string, prefix: string | undefined) {
		const rules = this.load();
		if (category === "read") rules.read = true;
		else if (category === "edit") rules.edit = true;
		else if (category === "shell" && prefix) {
			if (!rules.shell_prefixes.includes(prefix))
				rules.shell_prefixes.push(prefix);
		} else if (category === "shell") rules.shell = true;
		else throw new GuardOperationalError("invalid_request");
		this.save(rules);
		return rules;
	}
	forget() {
		if (this.path && existsSync(this.path))
			this.save({ read: false, edit: false, shell: false, shell_prefixes: [] });
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
	/** Ceiling source (ADR-0060): the owner's sandbox.toml; the carrier also
	 * reads the workspace's .oqto/sandbox.toml, which may only narrow.
	 * Built-in hard denies always apply. */
	ceiling?: { userConfig: string; home: string; hardDeny: string[] };
	/** Parent for the private guard directory; defaults to /var/tmp. */
	parent?: string;
	/** Guard-owned "always in this workspace" decisions (private, outside
	 * every brain bind). Absent: nothing is remembered across sessions. */
	rulesPath?: string;
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
	interrupted: {
		id: string;
		tool: string;
		argumentsText: string;
		startedAt: number;
	}[];
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
	readonly #admitted = new Map<string, Operation>();
	#toolServer: Server | undefined;
	#serial: Promise<unknown> = Promise.resolve();
	readonly #releases = new Map<string, () => void>();
	readonly #drained = new Set<string>();
	#receiptPeer: Peer | undefined;
	#receiptReady = false;
	#receiptConversation: number | undefined;
	#selection: { peer: Peer; token: string; previous: number } | undefined;
	#operatorReserved = false;
	#operatorInvocation: string | undefined;
	readonly #publications = new Map<
		string,
		{
			peer: Peer;
			digest: string;
			conversationId: number;
			resolve: (value: unknown) => void;
			reject: (error: Error) => void;
		}
	>();

	async #publishOperatorResult(
		receipt: OperatorResult,
		signal: AbortSignal,
	): Promise<void> {
		const peer = this.#receiptPeer;
		if (!peer || peer.closed.aborted)
			throw new GuardOperationalError("receipt_unavailable");
		const digest = digestOperatorResult(receipt);
		await this.authority.operatorSeal(receipt.id, digest);
		signal.throwIfAborted();
		let timer: ReturnType<typeof setTimeout> | undefined;
		let cancel: (() => void) | undefined;
		try {
			await new Promise((resolve, reject) => {
				this.#publications.set(receipt.id, {
					peer,
					digest,
					conversationId: receipt.conversationId,
					resolve,
					reject,
				});
				cancel = () =>
					reject(new GuardOperationalError("receipt_unacknowledged"));
				signal.addEventListener("abort", cancel, { once: true });
				timer = setTimeout(
					() => reject(new GuardOperationalError("receipt_unacknowledged")),
					10_000,
				);
				if (signal.aborted) {
					cancel();
					return;
				}
				peer.push("operator_result", receipt);
			});
		} finally {
			clearTimeout(timer);
			if (cancel) signal.removeEventListener("abort", cancel);
			this.#publications.delete(receipt.id);
		}
	}
	assertAdmission(): void {
		if (!canAdmitSession(this.id))
			throw new GuardOperationalError("unavailable");
	}
	async acquire(id: string, signal: AbortSignal) {
		let acquired!: () => void;
		const ready = new Promise<void>((resolve) => {
			acquired = resolve;
		});
		const hold = this.serial(async () => {
			await new Promise<void>((release) => {
				this.#releases.set(id, release);
				acquired();
			});
		});
		await ready;
		if (signal.aborted) {
			this.release(id);
			signal.throwIfAborted();
		}
		void hold;
	}
	release(id: string) {
		this.#drained.delete(id);
		this.#releases.get(id)?.();
		this.#releases.delete(id);
	}
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
		// Never swallow: a failed session leaves its brain without a tool socket.
		this.ready.catch((error) =>
			console.error(`oqt guard: session ${id} failed to start:`, error),
		);
	}
	/** The operator's own `!command`: typed by the human at the TUI, so it
	 * needs no approval, but it still runs only inside the confined hands
	 * (built-in denies and masks apply). Serialized with the brain's calls;
	 * the executor is retired afterwards so nothing it started lingers. */
	operatorShell(
		command: string,
		signal: AbortSignal = new AbortController().signal,
		excluded = false,
		conversationId?: number,
	): Promise<{
		id: string;
		exitCode: number;
		output: string;
		truncated: boolean;
		durable: true;
	}> {
		this.assertAdmission();
		if (
			!validReceiptConversation(conversationId) ||
			conversationId !== this.#receiptConversation ||
			!this.#receiptReady ||
			this.#selection ||
			this.#operatorReserved
		)
			throw new GuardOperationalError("destination_busy");
		// Capture before the first await/effect, and fence even queued invocations.
		const destination = conversationId;
		this.#operatorReserved = true;
		return this.serial(async () => {
			this.assertAdmission();
			signal.throwIfAborted();
			await this.ready;
			await this.authority.ready(signal);
			await this.syncResources(true);
			signal.throwIfAborted();
			if (!this.#receiptPeer || this.#receiptPeer.closed.aborted)
				throw new GuardOperationalError("receipt_unavailable");
			const invocation = `${this.id}:operator:${randomUUID()}`;
			// Validate bounded command/identity before any effect.
			validateOperatorResult({
				version: 2,
				session: this.id,
				conversationId: destination,
				id: invocation,
				command,
				exitCode: 0,
				output: "",
				truncated: false,
				excluded: false, // size-check private command before its opaque projection
				completedAt: Date.now(),
			});
			await this.authority.operatorBegin(invocation, command);
			this.#operatorInvocation = invocation;
			const resourceKey = this.#resourceKey;
			const epoch = this.supervisor.epoch;
			const hands = new HandsClient(this.handsPath, this.workspace);
			// Worst-case JSON escaping uses six bytes per UTF-16 code unit.
			const limit = 9 * 1024;
			let output = "";
			let truncated = false;
			let resultData:
				| { exitCode: number; output: string; truncated: boolean }
				| undefined;
			const abort = () => hands.close();
			signal.addEventListener("abort", abort, { once: true });
			try {
				signal.throwIfAborted();
				const result = await hands.use(epoch).exec(
					command,
					{
						// Never outlive a mounted resource's remaining lease.
						timeout: Math.max(
							0.001,
							Math.min(
								30,
								...this.resources.map(
									(grant) => (grant.expires_at - Date.now()) / 1000,
								),
							),
						),
						onOutput: (text) => {
							output += text;
							if (output.length > limit) {
								output = output.slice(-limit);
								truncated = true;
							}
						},
					},
					withAbortSignal(signal, BACKGROUND_CONTEXT),
				);
				signal.throwIfAborted();
				await this.authority.ready(signal);
				await this.syncResources(true);
				if (epoch !== this.supervisor.epoch)
					throw new GuardOperationalError("operation_revoked");
				resultData = !result.ok
					? {
							exitCode: -1,
							output: `${output}${result.error.message.slice(0, 1024)}\n`,
							truncated,
						}
					: { exitCode: result.value.exitCode, output, truncated };
			} finally {
				signal.removeEventListener("abort", abort);
				hands.close();
				await this.authority.ready();
				await this.authority.transition(async () => {
					await this.supervisor.replace();
					if (!resultData) {
						this.interrupted = await this.authority.recover();
						await this.#applyResources();
						this.#operatorInvocation = undefined;
						this.guard.emit();
					}
				});
			}
			try {
				signal.throwIfAborted();
				await this.authority.ready(signal);
				await this.syncResources(true);
				if (resourceKey !== this.#resourceKey)
					throw new GuardOperationalError("receipt_unacknowledged");
				if (!resultData) throw new GuardOperationalError("result_unavailable");
				let receipt: OperatorResult = {
					version: 2,
					session: this.id,
					conversationId: destination,
					id: invocation,
					command,
					...resultData,
					excluded,
					completedAt: Date.now(),
				};
				// Account for JSON escaping, including worst-case Unicode/control output.
				for (;;) {
					try {
						validateOperatorResult({ ...receipt, excluded: false });
						break;
					} catch {
						if (!receipt.output.length)
							throw new GuardOperationalError("receipt_bound");
						receipt = {
							...receipt,
							output: receipt.output.slice(
								Math.max(1, Math.floor(receipt.output.length / 4)),
							),
							truncated: true,
						};
					}
				}
				// !! is private to the operator: even the confined brain receives
				// only an opaque completion envelope, never command/output bytes.
				await this.#publishOperatorResult(
					excluded ? { ...receipt, command: "", output: "" } : receipt,
					signal,
				);
				return {
					id: invocation,
					exitCode: receipt.exitCode,
					output: receipt.output,
					truncated: receipt.truncated,
					durable: true as const,
				};
			} catch {
				this.interrupted = await this.authority.recover();
				this.guard.emit();
				throw new GuardOperationalError("receipt_unacknowledged");
			} finally {
				this.#operatorInvocation = undefined;
			}
		}).finally(() => {
			this.#operatorReserved = false;
		});
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
		mark("guard: session init");
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
		mark("guard: hands ready");
		const hash = createHash("sha256").update(this.workspace).digest("hex");
		const ceiling = options.ceiling ?? {
			userConfig: join(userInfo().homedir, ".config", "oqto", "sandbox.toml"),
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
				user_config: ceiling.userConfig,
				remembered: this.guard.rules.load(),
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
		mark("guard: authority open");

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
					if (this.#receiptPeer === peer) {
						this.#receiptPeer = undefined;
						this.#receiptReady = false;
						this.#receiptConversation = undefined;
					}
					if (this.#selection?.peer === peer) this.#selection = undefined;
					for (const publication of this.#publications.values())
						if (publication.peer === peer)
							publication.reject(new GuardOperationalError("unavailable"));
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
		if (!live) throw new GuardOperationalError("invalid_binding");
		switch (method) {
			case "operator_results_ready": {
				if (
					this.#receiptPeer &&
					this.#receiptPeer !== peer &&
					!this.#receiptPeer.closed.aborted
				)
					throw new GuardOperationalError("invalid_binding");
				if (
					!validReceiptConversation(params.conversationId) ||
					(params.recovering !== undefined &&
						typeof params.recovering !== "boolean")
				)
					throw new GuardOperationalError("invalid_binding");
				if (this.#selection || this.#operatorReserved)
					throw new GuardOperationalError("destination_busy");
				this.#receiptPeer = peer;
				this.#receiptConversation = params.conversationId;
				this.#receiptReady = params.recovering !== true;
				return {
					ready: this.#receiptReady,
					conversationId: this.#receiptConversation,
				};
			}
			case "operator_results_select_begin": {
				if (
					peer !== this.#receiptPeer ||
					!this.#receiptReady ||
					!validReceiptConversation(params.conversationId) ||
					params.conversationId !== this.#receiptConversation
				)
					throw new GuardOperationalError("invalid_binding");
				if (
					this.#operatorReserved ||
					this.#operatorInvocation ||
					this.#selection
				)
					throw new GuardOperationalError("navigation_live");
				const token = randomUUID();
				this.#selection = { peer, token, previous: params.conversationId };
				return { token };
			}
			case "operator_results_select_end": {
				const selection = this.#selection;
				if (
					!selection ||
					selection.peer !== peer ||
					selection.token !== params.token ||
					!validReceiptConversation(params.conversationId)
				)
					throw new GuardOperationalError("invalid_request");
				this.#receiptConversation = params.conversationId;
				this.#selection = undefined;
				return { conversationId: this.#receiptConversation };
			}
			case "operator_result_committed": {
				if (
					peer !== this.#receiptPeer ||
					typeof params.id !== "string" ||
					typeof params.digest !== "string" ||
					!validReceiptConversation(params.conversationId)
				)
					throw new GuardOperationalError("invalid_request");
				const publication = this.#publications.get(params.id);
				if (
					publication &&
					(publication.peer !== peer ||
						publication.digest !== params.digest ||
						publication.conversationId !== params.conversationId)
				)
					throw new GuardOperationalError("invalid_binding");
				await this.authority.ready(signal);
				await this.authority.operatorFinish(params.id, params.digest);
				this.interrupted = this.interrupted.filter(
					(row) => row.operation.request_id !== params.id,
				);
				publication?.resolve({ durable: true });
				this.guard.emit();
				return { acknowledged: true };
			}
			case "operator_result_failed": {
				const publication =
					typeof params.id === "string"
						? this.#publications.get(params.id)
						: undefined;
				if (publication?.peer === peer)
					publication.reject(
						new GuardOperationalError("receipt_unacknowledged"),
					);
				return { recorded: true };
			}
			case "prepare": {
				this.assertAdmission();
				const operation = codingOperation(params.operation);
				const { expiresAt } = await this.authority.prepare(operation, signal);
				live.add(operation.request_id);
				this.#admitted.set(operation.request_id, operation);
				try {
					await this.acquire(operation.request_id, signal);
					// Hold the lane until finish, including actual brain execution.
					await this.syncResources(true);
					await this.authority.ready(signal);
					return { expires_at: expiresAt, epoch: this.supervisor.epoch };
				} catch (error) {
					this.release(operation.request_id);
					throw error;
				}
			}
			case "request_resource": {
				this.assertAdmission();
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
				this.assertAdmission();
				const operation = codingOperation(params.operation);
				const admitted = this.#admitted.get(operation.request_id);
				if (!live.has(operation.request_id) || !admitted)
					throw new GuardOperationalError("no_live_invocation");
				if (JSON.stringify(admitted) !== JSON.stringify(operation))
					throw new GuardOperationalError("invalid_binding");
				await this.authority.check(admitted);
				return { checked: true };
			}
			case "finish": {
				if (typeof params.id !== "string" || !live.has(params.id))
					throw new GuardOperationalError("no_live_invocation");
				if (!this.#drained.has(params.id))
					throw new GuardOperationalError("teardown_unconfirmed");
				await this.authority.finish(params.id);
				this.#drained.delete(params.id);
				live.delete(params.id);
				this.#admitted.delete(params.id);
				this.release(params.id);
				return { finished: true };
			}
			case "drain": {
				if (![...live].some((id) => this.#releases.has(id)))
					throw new GuardOperationalError("no_live_invocation");
				// Retire the executor (and any detached descendants) before delivery.
				await this.authority.transition(() => this.supervisor.replace());
				await this.authority.ready(signal);
				for (const id of live)
					if (this.#releases.has(id)) this.#drained.add(id);
				return { epoch: this.supervisor.epoch };
			}
			case "epoch":
				await this.authority.ready(signal);
				return { epoch: this.supervisor.epoch };
			case "identity":
				// The worker must report the host pid the manager spawned and will signal.
				if (this.brainPid === undefined)
					throw new GuardOperationalError("unavailable");
				return { pid: this.brainPid };
			case "context":
				return boundedContext(
					this.guard.options.contextFiles?.(this.workspace) ?? [],
				);
			case "credential":
				if (typeof params.provider !== "string")
					throw new GuardOperationalError("invalid_request");
				return (await this.guard.options.broker.read(params.provider)) ?? null;
			case "credentials":
				return await this.guard.options.broker.list();
			default:
				throw new GuardOperationalError("tool_method_unavailable");
		}
	}
	async #resourceOperation(
		params: Record<string, unknown>,
	): Promise<Operation> {
		const id = params.request_id;
		if (typeof id !== "string" || !id.startsWith(`${this.id}:store:`))
			throw new GuardOperationalError("invalid_binding");
		const reason =
			typeof params.reason === "string" ? params.reason.slice(0, 500) : "";
		if (!reason) throw new GuardOperationalError("invalid_request");
		if (params.network === true)
			return {
				request_id: id,
				target: { capability: "network", resource_id: "egress" },
				arguments: { network: true, reason },
			};
		const access = params.access;
		if (access !== "read" && access !== "write")
			throw new GuardOperationalError("invalid_request");
		if (typeof params.path !== "string" || !params.path)
			throw new GuardOperationalError("invalid_request");
		const requested = isAbsolute(params.path)
			? params.path
			: resolve(this.workspace, params.path);
		let real: string;
		try {
			// Mount what the path resolves to now; symlinks are not followed later.
			real = await realpath(requested);
		} catch {
			throw new GuardOperationalError("invalid_request");
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
	async syncResources(locked = false) {
		const { grants, baseline } = await this.authority.resources();
		if (JSON.stringify(handsSpec(grants, baseline)) === this.#resourceKey) {
			this.resources = grants;
			return;
		}
		const apply = () => this.authority.transition(() => this.#applyResources());
		if (locked) await apply();
		else await this.serial(apply);
	}
	/** The brain is gone. Hands stop, in-memory permits are discarded and any
	 * invocation it held becomes an interrupted, never-replayed record that also
	 * fences every overlay. Failure leaves the authority frozen. */
	brainLost(lost: Set<string>) {
		return this.authority
			.transition(async () => {
				await this.supervisor.stop();
				await this.authority.resetTools();
				const interrupted =
					lost.size > 0 || this.#operatorInvocation
						? await this.authority.recover()
						: this.interrupted;
				// Publish recovery only after replacement is confirmed.
				await this.#applyResources();
				this.interrupted = interrupted;
				this.guard.emit();
			})
			.finally(() => {
				for (const id of lost) this.release(id);
			});
	}
	revokeAll() {
		return this.authority.revokeAll();
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
					id: entry.operation.request_id,
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
	readonly #peers = new Set<Peer>();
	readonly #inputs = new Map<string, SessionInputLeases>();
	#emitting = false;
	#emitDirty = false;
	readonly #roots = new Set<string>();
	readonly rules: RulesStore;
	#operator: Server | undefined;
	#index = 0;
	private constructor(options: GuardOptions) {
		this.options = options;
		this.directory = mkdtempSync(join(options.parent ?? "/var/tmp", "oqg-"));
		chmodSync(this.directory, 0o700);
		this.operatorPath = join(this.directory, "operator.sock");
		this.rules = new RulesStore(options.rulesPath);
	}
	static async open(options: GuardOptions) {
		privateDirectory(options.authorityRoot);
		const guard = new Guard(options);
		guard.#operator = await serve(
			guard.operatorPath,
			(method, params, signal, peer) =>
				guard.#operate(method, params, peer, signal),
			{
				onConnect: (peer) => {
					if (guard.#peers.size >= 64) peer.socket.destroy();
					else guard.#peers.add(peer);
				},
				onClose: (peer) => {
					guard.#retirePeer(peer);
				},
			},
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
				throw new GuardOperationalError("invalid_binding");
			return existing;
		}
		const session = new GuardSession(this, id, workspace, ++this.#index);
		this.sessions.set(id, session);
		return session;
	}
	input(session: string): SessionInputLeases {
		if (!this.sessions.has(session))
			throw new GuardOperationalError("invalid_binding");
		let input = this.#inputs.get(session);
		if (!input) {
			input = new SessionInputLeases(() => this.emit());
			this.#inputs.set(session, input);
		}
		return input;
	}
	state() {
		let rules: Remembered | undefined;
		try {
			rules = this.rules.load();
		} catch {}
		return {
			sessions: [...this.sessions.entries()].map(([id, session]) => ({
				...session.state(),
				input: this.#inputs.get(id)?.view() ?? { pending: null, holder: null },
			})),
			...(rules ? { rules } : {}),
		};
	}
	#retirePeer(peer: Peer) {
		retireClassicImportPeer(peer);
		this.#peers.delete(peer);
		this.#subscribers.delete(peer);
		for (const input of this.#inputs.values()) input.disconnect(peer);
	}
	emit() {
		if (this.#emitting) {
			this.#emitDirty = true;
			return;
		}
		this.#emitting = true;
		try {
			do {
				this.#emitDirty = false;
				const state = this.state();
				for (const peer of this.#subscribers) {
					// Retiring a lease owner changes state. Never deliver an older
					// snapshot after that reentrant revocation publication.
					if (this.#emitDirty) break;
					try {
						peer.push("state", state);
					} catch {
						this.#retirePeer(peer);
						peer.socket.destroy();
					}
				}
			} while (this.#emitDirty);
		} finally {
			this.#emitting = false;
		}
	}
	#find(params: Record<string, unknown>) {
		if (typeof params.session !== "string")
			throw new GuardOperationalError("invalid_request");
		const session = this.sessions.get(params.session);
		if (!session) throw new GuardOperationalError("invalid_binding");
		return session;
	}
	async #operate(
		method: string,
		params: Record<string, unknown>,
		peer: Peer,
		signal: AbortSignal,
	): Promise<unknown> {
		if (peer.closed.aborted || signal.aborted)
			throw new GuardOperationalError("unavailable");
		switch (method) {
			case "import_classic_begin":
			case "import_classic_chunk":
			case "import_classic_commit":
				return operateClassicImport(method, params, peer);
			case "input_describe": {
				const session = this.#find(params);
				return {
					protocol: "oqt-guard-input-v1",
					session: session.id,
					workspace: session.workspace,
					input: this.input(session.id).view(),
				};
			}
			case "input_request": {
				const session = this.#find(params);
				session.assertAdmission();
				if (
					typeof params.attachment !== "string" ||
					typeof params.label !== "string"
				)
					throw new GuardOperationalError("invalid_request");
				return this.input(session.id).request(
					peer,
					params.attachment,
					params.label,
					signal,
				);
			}
			case "input_decide": {
				const session = this.#find(params);
				session.assertAdmission();
				if (
					!this.#subscribers.has(peer) ||
					typeof params.id !== "string" ||
					typeof params.allow !== "boolean"
				)
					throw new GuardOperationalError("invalid_input_decision");
				try {
					await this.input(session.id).decide(peer, params.id, params.allow);
				} catch {
					throw new GuardOperationalError("invalid_input_decision");
				}
				return { decided: true };
			}
			case "input_release": {
				const session = this.#find(params);
				try {
					this.input(session.id).release(peer);
				} catch {
					throw new GuardOperationalError("input_lease_owner");
				}
				return { released: true };
			}
			case "subscribe":
				this.#subscribers.add(peer);
				peer.push("state", this.state());
				return { subscribed: true };
			case "decide": {
				const session = this.#find(params);
				session.assertAdmission();
				await session.ready;
				if (
					typeof params.id !== "string" ||
					typeof params.digest !== "string" ||
					typeof params.revision !== "number" ||
					typeof params.allow !== "boolean" ||
					(params.lifetime !== "once" && params.lifetime !== "session")
				)
					throw new GuardOperationalError("invalid_request");
				// "Always here": remember the rule (applies from the next session
				// start via the ceiling) and grant it for this session now.
				let remember: ReturnType<typeof describe> | undefined;
				if (params.allow && params.remember === true) {
					const request = session.pending.find((row) => row.id === params.id);
					if (!request) throw new GuardOperationalError("request_stale");
					const described = describe(request.operation);
					if (described.kind !== "tool")
						throw new GuardOperationalError("resource_session_only");
					remember = described;
				}
				await session.authority.decide(
					{ id: params.id, digest: params.digest, revision: params.revision },
					params.allow,
					params.remember === true ? "session" : params.lifetime,
				);
				// Only a successfully CAS-validated exact decision may persist a rule.
				if (remember)
					this.rules.remember(
						remember.category,
						"prefix" in remember ? remember.prefix : undefined,
					);
				this.emit();
				return { decided: true };
			}
			case "revoke_all": {
				const session = this.#find(params);
				await session.ready;
				await session.revokeAll();
				return { revoked: true };
			}
			case "forget_rules": {
				this.rules.forget();
				// Grants issued from remembered rules end now, not at next start.
				for (const session of this.sessions.values()) {
					await session.ready;
					await session.revokeAll();
				}
				this.emit();
				return { forgotten: true };
			}
			case "shell": {
				const session = this.#find(params);
				session.assertAdmission();
				if (
					typeof params.command !== "string" ||
					params.command.trim() === "" ||
					params.command.length > 8192
				)
					throw new GuardOperationalError("invalid_request");
				if (
					params.excluded !== undefined &&
					typeof params.excluded !== "boolean"
				)
					throw new GuardOperationalError("invalid_request");
				if (!validReceiptConversation(params.conversationId))
					throw new GuardOperationalError("invalid_request");
				return session.operatorShell(
					params.command,
					signal,
					params.excluded === true,
					params.conversationId,
				);
			}
			case "acknowledge": {
				const session = this.#find(params);
				await session.ready;
				await session.acknowledge();
				return { acknowledged: true };
			}
			default:
				throw new GuardOperationalError("unknown_operator_method");
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
		if (!brain) throw new GuardOperationalError("unavailable");
		const workspace = resolve(options.metadata.cwd);
		const trusted = resolve(brain.trustedSource);
		if (workspace === trusted || workspace.startsWith(trusted + sep))
			throw new Error(
				"Choose a coding workspace outside oqt's own trusted source tree (use --workdir)",
			);
		const session = this.session(options.metadata.id, workspace);
		const control = env.PI_SESSION_WORKER_CONTROL_ADDRESS;
		const importSeed = isClassicImportStaged(options.metadata.path)
			? classicImportSeedPath(options.metadata.path)
			: undefined;
		const spec = {
			node: brain.node,
			cwd: session.brainCwd,
			ro: [
				...brain.ro,
				...(importSeed ? [importSeed] : []),
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
				...(importSeed ? { OQT_CLASSIC_IMPORT_SEED: importSeed } : {}),
			},
			args: [
				"--import",
				pathToFileURL(join(brain.experimental, "source-resolver.ts")).href,
				join(brain.experimental, "session-worker.ts"),
				JSON.stringify(options),
			],
		};
		mark("guard: spawn brain");
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
		for (const input of this.#inputs.values()) await input.close();
		for (const peer of this.#peers) peer.socket.destroy();
		await new Promise<void>((resolve) =>
			this.#operator ? this.#operator.close(() => resolve()) : resolve(),
		);
		for (const session of this.sessions.values())
			await session.close().catch(() => {});
		await rm(this.directory, { recursive: true, force: true });
	}
}

/** oqt server host: accepts local clients on a private Unix socket and routes
 * `session:<id>` to exactly one worker process per Session. Workers connect
 * back on a second private socket with a single-use, per-generation token; a
 * new generation is started only after the previous one has exited. Clients
 * never choose where objects live, never spawn workers, and never supply their
 * own principal.
 */
import { spawn } from "node:child_process";
import { randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import { chmod, lstat, mkdir } from "node:fs/promises";
import { createServer, type IncomingMessage, type Server } from "node:http";
import { join } from "node:path";
import {
	acceptWebSockets,
	createHost,
	type Host,
	type Link,
	webSocketLink,
} from "@earendil-works/chord";
import { WebSocketServer } from "ws";
import { type Principal, principalKey, SESSION_ID } from "./contract.ts";
import { type InputGate, serveFacade } from "./facade.ts";
import { INIT_ENV, type WorkerInit } from "./worker.ts";

/** A started worker process. `exited` settles once it is gone. */
export interface WorkerProcess {
	readonly pid?: number;
	readonly exited: Promise<unknown>;
	/** Kills the worker and everything it spawned. */
	kill(): void;
}

/** Starts a Session worker; the server supplies all data in `init`. */
export type WorkerLauncher = (init: WorkerInit) => WorkerProcess;

/** Test/dev launcher: an unconfined Node child. Coding uses the guard's
 * confined-brain launcher instead (host2/guard-launcher.ts). */
export function forkLauncher(
	entry: string,
	execArgv: readonly string[] = [],
): WorkerLauncher {
	return (init) => {
		const child = spawn(process.execPath, [...execArgv, entry], {
			detached: true,
			stdio: ["ignore", "inherit", "inherit"],
			env: { ...process.env, [INIT_ENV]: JSON.stringify(init) },
		});
		const exited = new Promise<unknown>((resolve) =>
			child.once("exit", resolve),
		);
		return {
			pid: child.pid,
			exited,
			kill() {
				try {
					if (child.pid) process.kill(-child.pid, "SIGKILL");
				} catch {
					child.kill("SIGKILL");
				}
			},
		};
	};
}

export interface ServerOptions {
	/** Private directory (0700): sockets and per-Session databases. */
	readonly directory: string;
	readonly launch: WorkerLauncher;
	/** Milliseconds a new worker may take to connect back (default 30000). */
	readonly connectTimeout?: number;
	/** The guard's remote input leases; absent: every local owner is admitted. */
	readonly input?: InputGate;
}

export interface WorkerStatus {
	readonly generation: number;
	readonly pid?: number;
	readonly connected: boolean;
}

export interface OqtServer {
	readonly host: Host<Principal>;
	readonly socketPath: string;
	readonly workerSocketPath: string;
	/** Bearer secret a local client presents; readable only by the owning user. */
	readonly token: string;
	worker(session: string): WorkerStatus | undefined;
	/** Stops a Session's worker (it restarts on next use). */
	stopWorker(session: string): Promise<void>;
	close(): Promise<void>;
}

interface Generation {
	readonly session: string;
	readonly generation: number;
	readonly token: Buffer;
	readonly process: WorkerProcess;
	link?: Link;
	/** Resolves with the worker's link once it connected with this token. */
	readonly connected: Promise<Link>;
	accept(link: Link): void;
	used: boolean;
}

async function privateDirectory(path: string) {
	await mkdir(path, { recursive: true, mode: 0o700 });
	const meta = await lstat(path);
	if (
		!meta.isDirectory() ||
		(meta.mode & 0o077) !== 0 ||
		meta.uid !== process.getuid?.()
	)
		throw new Error(
			`Server directory must be a private owned directory: ${path}`,
		);
}

function listen(server: Server, path: string) {
	return new Promise<void>((resolve, reject) => {
		server.once("error", reject);
		server.listen(path, () => resolve());
	});
}

const bearer = (request: IncomingMessage) =>
	Buffer.from(
		String(request.headers.authorization ?? "").replace(/^Bearer /, ""),
	);
const same = (a: Buffer, b: Buffer) =>
	a.length === b.length && timingSafeEqual(a, b);

export async function startServer(options: ServerOptions): Promise<OqtServer> {
	await privateDirectory(options.directory);
	await privateDirectory(join(options.directory, "sessions"));
	const workersDirectory = join(options.directory, "workers");
	await privateDirectory(workersDirectory);
	const token = randomBytes(32).toString("hex");
	const clientToken = Buffer.from(token);
	const host = createHost<Principal>({
		name: "server",
		self: { kind: "host" },
		principalKey,
		onError: (error) => console.error("[oqt server]", error),
	});
	const current = new Map<string, Generation>();
	/** Serializes start/stop per Session: one generation at a time. */
	const transitions = new Map<string, Promise<unknown>>();
	let generations = 0;
	let closed = false;
	const sessionOf = (hostName: string) => {
		const session = hostName.slice("session:".length);
		if (!hostName.startsWith("session:") || !SESSION_ID.test(session))
			throw new Error("Invalid Session host");
		return session;
	};
	const serialized = <T>(
		session: string,
		step: () => Promise<T>,
	): Promise<T> => {
		const next = (transitions.get(session) ?? Promise.resolve()).then(
			step,
			step,
		);
		transitions.set(
			session,
			next.catch(() => {}),
		);
		return next;
	};
	const retire = async (entry: Generation) => {
		if (current.get(entry.session) === entry) current.delete(entry.session);
		entry.link?.close("worker retired");
		entry.process.kill();
		await entry.process.exited;
	};
	const start = async (session: string): Promise<Generation> => {
		const previous = current.get(session);
		// Never two owners of one storage: the old process must be gone first.
		if (previous) await retire(previous);
		if (closed) throw new Error("Server closed");
		const generation = ++generations;
		const secret = randomBytes(32).toString("hex");
		let accept!: (link: Link) => void;
		let fail!: (error: Error) => void;
		const connected = new Promise<Link>((resolve, reject) => {
			accept = resolve;
			fail = reject;
		});
		connected.catch(() => {});
		const child = options.launch({
			session,
			generation,
			database: join(options.directory, "sessions", `${session}.sqlite`),
			socket: workerSocketPath,
			token: secret,
		});
		const entry: Generation = {
			session,
			generation,
			token: Buffer.from(secret),
			process: child,
			connected,
			accept,
			used: false,
		};
		current.set(session, entry);
		const timer = setTimeout(
			() => fail(new Error("Session worker did not connect")),
			options.connectTimeout ?? 30_000,
		);
		void child.exited.then(() => {
			fail(new Error("Session worker exited"));
			if (current.get(session) === entry) current.delete(session);
		});
		try {
			entry.link = await connected;
			entry.link.onClose(() => {
				// A worker without its link is retired: no stale owner survives.
				if (current.get(session) === entry)
					void serialized(session, () => retire(entry));
			});
			return entry;
		} catch (error) {
			await retire(entry);
			throw error;
		} finally {
			clearTimeout(timer);
		}
	};
	host.route("session:", {
		// Only the server's own facade reaches workers; clients go through it.
		authorize: (principal) => principal.kind === "host",
		resolve: (hostName) => {
			const session = sessionOf(hostName);
			return serialized(session, async () => {
				const live = current.get(session);
				if (live?.link && !live.link.closed) return live.link;
				return (await start(session)).link as Link;
			});
		},
		principal: (hostName) => ({
			kind: "worker",
			session: sessionOf(hostName),
			generation: current.get(sessionOf(hostName))?.generation ?? 0,
		}),
	});
	serveFacade(host, {
		sessionsDirectory: join(options.directory, "sessions"),
		...(options.input ? { input: options.input } : {}),
	});
	// Worker socket: a connection is accepted only with the current, unused
	// token of a Session's generation. Anything else is closed without a link.
	const workerHttp = createServer((_, response) =>
		response.writeHead(404).end(),
	);
	const workerSockets = new WebSocketServer({
		server: workerHttp,
		maxPayload: 16 * 1024 * 1024,
	});
	workerSockets.on("connection", (socket, request) => {
		const presented = bearer(request);
		const entry = [...current.values()].find(
			(candidate) => !candidate.used && same(candidate.token, presented),
		);
		if (!entry) {
			socket.close(1008, "unauthorized");
			return;
		}
		entry.used = true;
		entry.accept(webSocketLink(socket as never, { compress: true }));
	});
	const clientHttp = createServer((_, response) =>
		response.writeHead(404).end(),
	);
	const clientSockets = new WebSocketServer({
		server: clientHttp,
		maxPayload: 16 * 1024 * 1024,
	});
	acceptWebSockets<IncomingMessage, Principal>(clientSockets as never, host, {
		principal: (request) =>
			same(bearer(request), clientToken)
				? { kind: "owner", client: randomUUID() }
				: undefined,
	});
	const socketPath = join(options.directory, "server.sock");
	const workerSocketPath = join(workersDirectory, "workers.sock");
	await listen(clientHttp, socketPath);
	await listen(workerHttp, workerSocketPath);
	await chmod(socketPath, 0o600);
	await chmod(workerSocketPath, 0o600);
	return {
		host,
		socketPath,
		workerSocketPath,
		token,
		worker(session) {
			const entry = current.get(session);
			return (
				entry && {
					generation: entry.generation,
					pid: entry.process.pid,
					connected: Boolean(entry.link && !entry.link.closed),
				}
			);
		},
		stopWorker(session) {
			return serialized(session, async () => {
				const entry = current.get(session);
				if (entry) await retire(entry);
			});
		},
		async close() {
			closed = true;
			clientSockets.close();
			workerSockets.close();
			clientHttp.close();
			workerHttp.close();
			await host.close();
			await Promise.all([...current.values()].map((entry) => retire(entry)));
		},
	};
}

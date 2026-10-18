// Bounded newline-delimited JSON over local Unix sockets. Framing is not
// authentication: who can reach a socket is decided by where the guard binds it.
import { connect, createServer, type Server, type Socket } from "node:net";
import { setTimeout as sleep } from "node:timers/promises";

export const MAX_FRAME = 64 * 1024;

export type Handler = (
	method: string,
	params: Record<string, unknown>,
	signal: AbortSignal,
	peer: Peer,
) => Promise<unknown>;

export interface Peer {
	readonly socket: Socket;
	push(event: string, data: unknown): void;
	readonly closed: AbortSignal;
}

function frames(socket: Socket, onFrame: (value: unknown) => void) {
	let buffer = "";
	socket.setEncoding("utf8");
	socket.on("data", (chunk: string) => {
		buffer += chunk;
		for (;;) {
			const newline = buffer.indexOf("\n");
			if (newline < 0) break;
			const line = buffer.slice(0, newline);
			buffer = buffer.slice(newline + 1);
			if (!line) continue;
			if (Buffer.byteLength(line) + 1 > MAX_FRAME) {
				socket.destroy();
				return;
			}
			try {
				onFrame(JSON.parse(line));
			} catch {
				socket.destroy();
				return;
			}
		}
		if (Buffer.byteLength(buffer) >= MAX_FRAME) socket.destroy();
	});
}

function write(socket: Socket, value: unknown) {
	const line = `${JSON.stringify(value)}\n`;
	if (Buffer.byteLength(line) > MAX_FRAME)
		throw new Error("Guard frame exceeds limit");
	if (
		socket.destroyed ||
		socket.writableLength + Buffer.byteLength(line) > MAX_FRAME * 4
	)
		throw new Error("Guard peer closed or output bound exceeded");
	socket.write(line);
}

/** One handler per connection; at most `concurrency` calls in flight. */
export function serve(
	path: string,
	handler: Handler,
	options: {
		concurrency?: number;
		onConnect?: (peer: Peer) => void;
		onClose?: (peer: Peer) => void;
	} = {},
): Promise<Server> {
	const server = createServer((socket) => {
		const closed = new AbortController();
		const calls = new Map<unknown, AbortController>();
		const queue: (() => void)[] = [];
		let running = 0;
		const peer: Peer = {
			socket,
			closed: closed.signal,
			push(event, data) {
				try {
					write(socket, { event, data });
				} catch (error) {
					socket.destroy();
					throw error;
				}
			},
		};
		socket.on("error", () => socket.destroy());
		socket.once("close", () => {
			closed.abort();
			for (const call of calls.values()) call.abort();
			options.onClose?.(peer);
		});
		frames(socket, (value) => {
			if (
				!value ||
				typeof value !== "object" ||
				!("id" in value) ||
				!("method" in value) ||
				typeof value.method !== "string"
			) {
				socket.destroy();
				return;
			}
			const id = value.id;
			if (value.method === "cancel") {
				calls.get(id)?.abort();
				return;
			}
			// Bounded: `concurrency` calls run, at most 256 more wait; beyond
			// that (or on a reused id) the peer is misbehaving and is dropped.
			if (calls.has(id) || calls.size >= (options.concurrency ?? 16) + 256) {
				socket.destroy();
				return;
			}
			const params =
				"params" in value &&
				value.params &&
				typeof value.params === "object" &&
				!Array.isArray(value.params)
					? (value.params as Record<string, unknown>)
					: {};
			const call = new AbortController();
			calls.set(id, call);
			const method = value.method;
			const run = () => {
				if (call.signal.aborted || closed.signal.aborted) {
					calls.delete(id);
					queue.shift()?.();
					return;
				}
				running++;
				handler(method, params, call.signal, peer)
					.then(
						(result) => write(socket, { id, ok: true, result }),
						(error: unknown) =>
							write(socket, {
								id,
								ok: false,
								error: error instanceof Error ? error.message : String(error),
							}),
					)
					.catch(() => socket.destroy())
					.finally(() => {
						running--;
						calls.delete(id);
						queue.shift()?.();
					});
			};
			if (running < (options.concurrency ?? 16)) run();
			else queue.push(run);
		});
		options.onConnect?.(peer);
	});
	return new Promise((resolve, reject) => {
		server.once("error", reject);
		server.listen(path, () => {
			server.off("error", reject);
			resolve(server);
		});
	});
}

export class WireClient {
	readonly socket: Socket;
	#next = 0;
	readonly #calls = new Map<
		number,
		{ resolve: (value: unknown) => void; reject: (error: Error) => void }
	>();
	readonly #listeners = new Map<string, Set<(data: unknown) => void>>();
	readonly closed: Promise<void>;
	private constructor(socket: Socket) {
		this.socket = socket;
		this.closed = new Promise((resolve) =>
			socket.once("close", () => {
				for (const call of this.#calls.values())
					call.reject(new Error("Guard connection closed"));
				this.#calls.clear();
				resolve();
			}),
		);
		socket.on("error", () => socket.destroy());
		frames(socket, (value) => {
			if (!value || typeof value !== "object") return;
			if ("event" in value && typeof value.event === "string") {
				for (const listener of this.#listeners.get(value.event) ?? [])
					listener("data" in value ? value.data : undefined);
				return;
			}
			if (!("id" in value) || typeof value.id !== "number") return;
			const call = this.#calls.get(value.id);
			if (!call) return;
			this.#calls.delete(value.id);
			if ("ok" in value && value.ok === true)
				call.resolve("result" in value ? value.result : undefined);
			else
				call.reject(
					new Error(
						"error" in value ? String(value.error) : "Guard call failed",
					),
				);
		});
	}
	static async connect(path: string, deadlineMs = 10_000): Promise<WireClient> {
		const deadline = Date.now() + deadlineMs;
		for (;;) {
			try {
				const socket = await new Promise<Socket>((resolve, reject) => {
					const socket = connect(path);
					socket.once("connect", () => {
						socket.off("error", reject);
						resolve(socket);
					});
					socket.once("error", reject);
				});
				return new WireClient(socket);
			} catch (error) {
				if (Date.now() >= deadline) throw error;
				await sleep(25);
			}
		}
	}
	on(event: string, listener: (data: unknown) => void) {
		let set = this.#listeners.get(event);
		if (!set) {
			set = new Set();
			this.#listeners.set(event, set);
		}
		set.add(listener);
		return () => set.delete(listener);
	}
	call(
		method: string,
		params: Record<string, unknown> = {},
		signal?: AbortSignal,
	): Promise<unknown> {
		if (this.socket.destroyed)
			return Promise.reject(new Error("Guard connection closed"));
		signal?.throwIfAborted();
		const id = ++this.#next;
		return new Promise((resolve, reject) => {
			const abort = () => {
				this.#calls.delete(id);
				try {
					write(this.socket, { id, method: "cancel" });
				} catch {}
				reject(signal?.reason ?? new Error("Aborted"));
			};
			signal?.addEventListener("abort", abort, { once: true });
			this.#calls.set(id, {
				resolve: (value) => {
					signal?.removeEventListener("abort", abort);
					resolve(value);
				},
				reject: (error) => {
					signal?.removeEventListener("abort", abort);
					reject(error);
				},
			});
			try {
				write(this.socket, { id, method, params });
			} catch (error) {
				this.#calls.delete(id);
				signal?.removeEventListener("abort", abort);
				reject(error);
			}
		});
	}
	close() {
		this.socket.end();
		this.socket.destroy();
	}
}

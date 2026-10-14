import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer, type Socket } from "node:net";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { Connection, RemoteExecutionEnv } from "@earendil-works/pi-env";

/** Owns only processes this supervisor spawned. No PID-file/reused-PID kills.
 * Each epoch has fresh Connection/RemoteExecutionEnv objects: retained handles
 * cannot reconnect after stop. bwrap PID namespace teardown fences descendants.
 */
export class HandsSupervisor {
	readonly children = new Set<ChildProcessWithoutNullStreams>();
	readonly sockets = new Set<Socket>();
	readonly server = createServer((socket) => this.accept(socket));
	readonly directory: string;
	readonly spec: string;
	readonly socketPath: string;
	readonly launcher: string;
	readonly workspace: string;
	#active = false;
	#closed = false;
	#epoch = 0;
	#replacement: Promise<void> | undefined;
	#connection: Connection | undefined;
	#execution: RemoteExecutionEnv | undefined;
	constructor(directory: string, launcher: string, workspace: string) {
		this.directory = directory;
		this.launcher = launcher;
		this.workspace = workspace;
		this.spec = join(directory, "hands.json");
		this.socketPath = join(directory, "transport.sock");
	}
	static async open(options: {
		launcher: string;
		workspace: string;
		daemon: string;
		bpf: string;
		protectedRoots: string[];
	}) {
		const directory = await mkdtemp("/var/tmp/oqh-");
		await chmod(directory, 0o700);
		const supervisor = new HandsSupervisor(
			directory,
			options.launcher,
			options.workspace,
		);
		try {
			await writeFile(
				supervisor.spec,
				JSON.stringify({
					workspace: options.workspace,
					daemon: options.daemon,
					bpf: options.bpf,
					protected_roots: [...options.protectedRoots, directory],
				}),
				{ mode: 0o600 },
			);
			await new Promise<void>((resolve, reject) => {
				supervisor.server.once("error", reject);
				supervisor.server.listen(supervisor.socketPath, () => {
					supervisor.server.off("error", reject);
					resolve();
				});
			});
			await chmod(supervisor.socketPath, 0o600);
			supervisor.install();
			return supervisor;
		} catch (error) {
			await supervisor.close();
			throw error;
		}
	}
	install() {
		if (this.#closed || this.#active || this.children.size !== 0)
			throw new Error("Cannot install hands before confirmed stop");
		this.#epoch++;
		this.#connection = new Connection({
			command: [
				process.execPath,
				fileURLToPath(new URL("./hands-proxy.mjs", import.meta.url)),
				this.socketPath,
			],
		});
		this.#execution = new RemoteExecutionEnv({
			connection: this.#connection,
			id: `oqt-hands-${this.#epoch}`,
			cwd: this.workspace,
		});
		this.#active = true;
	}
	get execution() {
		if (!this.#active || !this.#execution) throw new Error("Hands are frozen");
		return this.#execution;
	}
	accept(socket: Socket) {
		if (!this.#active || this.sockets.size >= 8) {
			socket.destroy();
			return;
		}
		this.sockets.add(socket);
		let header = Buffer.alloc(0);
		let child: ChildProcessWithoutNullStreams | undefined;
		const timer = setTimeout(() => socket.destroy(), 2000);
		socket.once("close", () => {
			clearTimeout(timer);
			this.sockets.delete(socket);
			child?.kill("SIGTERM");
		});
		socket.on("error", () => socket.destroy());
		const handshake = (chunk: Buffer) => {
			header = Buffer.concat([header, chunk]);
			const newline = header.indexOf(10);
			if (newline < 0) {
				if (header.length > 128) socket.destroy();
				return;
			}
			if (newline > 128 || !this.#active) {
				socket.destroy();
				return;
			}
			try {
				const value: unknown = JSON.parse(
					header.subarray(0, newline).toString(),
				);
				if (
					!value ||
					typeof value !== "object" ||
					!("token" in value) ||
					typeof value.token !== "string" ||
					!/^[a-f0-9]{32}$/.test(value.token)
				)
					throw new Error("Invalid daemon transport token");
				socket.off("data", handshake);
				clearTimeout(timer);
				child = spawn(
					this.launcher,
					[this.spec, "serve", "--token", value.token],
					{
						stdio: ["pipe", "pipe", "pipe"],
						env: {
							PATH: "/usr/local/bin:/usr/bin:/bin",
							HOME: homedir(),
							OQTO_SANDBOX_SHIM_BIN: process.env.OQT_SANDBOX_SHIM,
						},
					},
				);
				const owned = child;
				this.children.add(owned);
				owned.once("exit", () => {
					this.children.delete(owned);
					socket.destroy();
				});
				owned.once("error", () => {
					this.children.delete(owned);
					socket.destroy();
				});
				owned.stdin.on("error", () => socket.destroy());
				owned.stderr.resume(); // untrusted executor diagnostics are not host/model logs
				owned.stdout.pipe(socket);
				socket.pipe(owned.stdin);
				const rest = header.subarray(newline + 1);
				if (rest.length) owned.stdin.write(rest);
			} catch {
				socket.destroy();
			}
		};
		socket.on("data", handshake);
	}
	async replace() {
		if (!this.#replacement) {
			this.#replacement = (async () => {
				await this.stop();
				this.install();
			})();
		}
		const current = this.#replacement;
		try {
			await current;
		} finally {
			if (this.#replacement === current) this.#replacement = undefined;
		}
	}
	async stop() {
		this.#active = false; // freeze before closing transport/admitting replacements
		this.#connection?.close();
		this.#connection = undefined;
		this.#execution = undefined;
		const children = [...this.children];
		for (const socket of this.sockets) socket.destroy();
		await Promise.all(
			children.map(
				(child) =>
					new Promise<void>((resolve, reject) => {
						if (child.exitCode !== null || child.signalCode !== null) {
							resolve();
							return;
						}
						const kill = setTimeout(() => child.kill("SIGKILL"), 1000);
						const deadline = setTimeout(() => {
							clearTimeout(kill);
							reject(
								new Error(
									"Executor stop not confirmed; replacement remains disabled",
								),
							);
						}, 4000);
						child.once("exit", () => {
							clearTimeout(kill);
							clearTimeout(deadline);
							resolve();
						});
						child.kill("SIGTERM");
					}),
			),
		);
		if (this.children.size !== 0) throw new Error("Executor drain incomplete");
	}
	async close() {
		this.#closed = true;
		await this.stop();
		await new Promise<void>((resolve) => this.server.close(() => resolve()));
		await rm(this.directory, { recursive: true, force: true });
	}
}

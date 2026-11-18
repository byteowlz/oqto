import {
	type ChildProcessWithoutNullStreams,
	execFile,
	spawn,
} from "node:child_process";
import { randomBytes } from "node:crypto";
import { writeFileSync } from "node:fs";
import {
	chmod,
	copyFile,
	mkdir,
	mkdtemp,
	rm,
	writeFile,
} from "node:fs/promises";
import { createServer, type Socket } from "node:net";
import { homedir } from "node:os";
import { join } from "node:path";

/** macOS confines with Seatbelt and has no PID namespace (and no seccomp). */
export const DARWIN = process.platform === "darwin";

/** Create the private launch marker pair (see oqto_sandbox::process_tree)
 * in an existing private directory. */
export function createMarker(directory: string) {
	for (const name of ["marker-allow", "marker-deny"])
		writeFileSync(join(directory, name), "", { mode: 0o600, flag: "wx" });
}

/** macOS: SIGKILL and confirm gone every process carrying the marker in
 * `directory`, whatever its ancestry. Rejects unless a complete scan found
 * no live or undecidable member. */
export function confirmReaped(launcher: string, directory: string) {
	return new Promise<void>((resolve, reject) => {
		execFile(
			launcher,
			["reap", directory],
			{ timeout: 10_000, env: { PATH: "/usr/bin:/bin", HOME: homedir() } },
			(error, stdout) => {
				let report: unknown;
				try {
					report = JSON.parse(stdout);
				} catch {}
				if (
					!error &&
					report &&
					typeof report === "object" &&
					"confirmed" in report &&
					report.confirmed === true
				)
					resolve();
				else
					reject(
						new Error(
							`Executor descendants not confirmed stopped: ${stdout.trim() || error?.message}`,
						),
					);
			},
		);
	});
}

/** Guard-side owner of confined executor processes. Owns only processes it
 * spawned (no PID-file/reused-PID kills). Each generation has a fresh random
 * epoch; the transport handshake must name the current epoch, so a retained
 * brain-side Connection cannot reconnect after stop/replacement.
 *
 * Linux: bwrap PID namespace teardown fences descendants. macOS: the launcher
 * supervises its Seatbelt child and every spawned process carries this
 * supervisor's launch marker; stop() succeeds only after a marker scan
 * confirms that no member survived, including descendants that called
 * setsid and were reparented. Otherwise stop() throws and replacement stays
 * disabled (the authority remains frozen).
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
	/** macOS: private TMPDIR/HOME, wiped for every generation. */
	scratch: string | undefined;
	/** Remote hands (ADR-0086): a guard-admitted, one-use per-call token that
	 * routes this connection to the paired link instead of local hands. */
	remoteRoute:
		| ((
				epoch: string,
		  ) => ((socket: Socket, token: string, rest: Buffer) => void) | undefined)
		| undefined;
	#active = false;
	#closed = false;
	#epoch = "";
	#replacement: Promise<void> | undefined;
	constructor(
		directory: string,
		launcher: string,
		workspace: string,
		socketPath?: string,
	) {
		this.directory = directory;
		this.launcher = launcher;
		this.workspace = workspace;
		this.spec = join(directory, "hands.json");
		this.socketPath = socketPath ?? join(directory, "transport.sock");
	}
	static async open(options: {
		launcher: string;
		workspace: string;
		daemon: string;
		/** Compiled seccomp filter (Linux). There is no seccomp on macOS. */
		bpf?: string;
		protectedRoots: string[];
		/** Directory for the private spec; defaults to a fresh /var/tmp/oqh-*. */
		parent?: string;
		/** Brain-visible transport socket; defaults inside the private directory. */
		socketPath?: string;
		readPaths?: string[];
		writePaths?: string[];
		network?: boolean;
		/** macOS: private scratch directory; created here when absent. */
		scratch?: string;
	}) {
		const directory = await mkdtemp(join(options.parent ?? "/var/tmp", "oqh-"));
		await chmod(directory, 0o700);
		const supervisor = new HandsSupervisor(
			directory,
			options.launcher,
			options.workspace,
			options.socketPath,
		);
		try {
			if (DARWIN) {
				createMarker(directory);
				supervisor.scratch =
					options.scratch ?? (await mkdtemp(join("/var/tmp", "oqx-")));
				await chmod(supervisor.scratch, 0o700);
			}
			await supervisor.writeSpec(options);
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
	#base:
		| { daemon: string; bpf: string | undefined; protectedRoots: string[] }
		| undefined;
	/** Replace the trusted spec. Only valid while frozen (between stop and install). */
	async writeSpec(options: {
		daemon?: string;
		bpf?: string;
		protectedRoots?: string[];
		readPaths?: string[];
		writePaths?: string[];
		network?: boolean;
		mask?: string[];
	}) {
		if (this.#active) throw new Error("Cannot change hands spec while active");
		// The trusted base (daemon, policy, protected roots) is fixed at open;
		// later replacements may only change the approved resource grants.
		this.#base ??= {
			daemon: options.daemon ?? "",
			bpf: options.bpf,
			protectedRoots: [...(options.protectedRoots ?? []), this.directory],
		};
		const base = this.#base;
		await writeFile(
			this.spec,
			JSON.stringify({
				workspace: this.workspace,
				daemon: base.daemon,
				// Linux: compiled seccomp filter. macOS: no seccomp; the launch
				// marker and a private scratch directory instead.
				...(DARWIN
					? { marker: this.directory, scratch: this.scratch }
					: { bpf: base.bpf ?? "" }),
				protected_roots: base.protectedRoots,
				read_paths: options.readPaths ?? [],
				write_paths: options.writePaths ?? [],
				network: options.network ?? false,
				mask_paths: options.mask ?? [],
			}),
			{ mode: 0o600 },
		);
		// Keep the last spec for forensics (private dir, never brain-visible).
		await copyFile(this.spec, join(this.directory, "hands-last.json")).catch(
			() => {},
		);
		if (process.env.OQT_DEBUG)
			console.error(
				`oqt hands spec: +r${options.readPaths?.length ?? 0} +w${options.writePaths?.length ?? 0} mask${options.mask?.length ?? 0}`,
			);
	}
	install() {
		if (this.#closed || this.#active || this.children.size !== 0)
			throw new Error("Cannot install hands before confirmed stop");
		this.#epoch = randomBytes(16).toString("hex");
		this.#active = true;
	}
	get active() {
		return this.#active;
	}
	/** Current generation for the brain-side transport; throws while frozen. */
	get epoch() {
		if (!this.#active) throw new Error("Hands are frozen");
		return this.#epoch;
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
				if (header.length > 160) socket.destroy();
				return;
			}
			if (newline > 160 || !this.#active) {
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
					!/^[a-f0-9]{32}$/.test(value.token) ||
					!("epoch" in value) ||
					typeof value.epoch !== "string"
				)
					throw new Error("Invalid daemon transport handshake");
				if (value.epoch !== this.#epoch) {
					const route = this.remoteRoute?.(value.epoch);
					if (!route) throw new Error("Stale daemon transport handshake");
					socket.off("data", handshake);
					clearTimeout(timer);
					// The remote connection is the guard's to track and close.
					this.sockets.delete(socket);
					route(socket, value.token, header.subarray(newline + 1));
					return;
				}
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
				// Untrusted executor diagnostics are not host/model logs; only the
				// OQT_DEBUG private log sees them.
				if (process.env.OQT_DEBUG)
					owned.stderr.on("data", (chunk: Buffer) =>
						console.error(
							`oqt hands[${this.#epoch.slice(0, 6)}]:`,
							String(chunk),
						),
					);
				else owned.stderr.resume();
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
	/** Freeze, confirm stop, optionally swap the spec, then install a fresh epoch. */
	async replace(spec?: Parameters<HandsSupervisor["writeSpec"]>[0]) {
		while (this.#replacement) await this.#replacement.catch(() => {});
		const current = (async () => {
			await this.stop();
			// Fresh hands start with an empty scratch, as bwrap's private /tmp.
			if (this.scratch) {
				await rm(this.scratch, { recursive: true, force: true });
				await mkdir(this.scratch, { mode: 0o700 });
			}
			if (spec) await this.writeSpec(spec);
			this.install();
		})();
		this.#replacement = current;
		try {
			await current;
		} finally {
			if (this.#replacement === current) this.#replacement = undefined;
		}
	}
	async stop() {
		this.#active = false; // freeze before closing transport/admitting replacements
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
		// The launchers reap their own launch; confirm independently, because
		// a killed launcher could not, and a setsid descendant has no parent
		// we could wait for.
		if (DARWIN) await confirmReaped(this.launcher, this.directory);
	}
	async close() {
		this.#closed = true;
		await this.stop();
		await new Promise<void>((resolve) => this.server.close(() => resolve()));
		await rm(this.directory, { recursive: true, force: true });
		if (this.scratch) await rm(this.scratch, { recursive: true, force: true });
	}
}

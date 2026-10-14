// Private source-only composition. Stock Pi/default local tools are never selected.
import { spawn } from "node:child_process";
import { closeSync, openSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { connect } from "node:net";
import { join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import lockfile from "proper-lockfile";
import { runClient } from "./client.ts";
import { runClientTui } from "./client-tui.ts";
import { startServer } from "./server.ts";

const args = process.argv.slice(2);
const smoke = args.includes("--smoke");
const coding = args.includes("--coding");
if (coding && args.includes("--demo"))
	throw new Error("Choose --coding or --demo, not both");
if ((process.env.OQT_MODE === "coding") !== coding)
	throw new Error("Launcher mode mismatch; refusing fallback");
const continuing = args.includes("--continue") || args.includes("-c");
if (
	args.some(
		(arg) =>
			!["--demo", "--coding", "--continue", "-c", "--smoke"].includes(arg),
	)
)
	throw new Error("Unsupported oqt prototype option");
if (!smoke && (!process.stdin.isTTY || !process.stdout.isTTY))
	throw new Error(
		"oqt requires a real terminal (use --smoke only for qualification)",
	);
const directory = process.env.OQT_SERVER_DIR;
if (!directory || !process.env.OQT_CARRIER || !process.env.OQT_SECCOMP_BPF)
	throw new Error("Missing protected launcher context; no fallback");
const packageRoot = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const plugin = join(packageRoot, "examples/plugins/oq-permissions");

interface Host {
	pid: number;
	socketPath: string;
	operator: string;
}

function reachable(path: string) {
	return new Promise<boolean>((done) => {
		const socket = connect(path);
		socket.once("connect", () => {
			socket.destroy();
			done(true);
		});
		socket.once("error", () => done(false));
	});
}

async function readHost(): Promise<Host | undefined> {
	try {
		const value: unknown = JSON.parse(
			await readFile(join(directory as string, "host.json"), "utf8"),
		);
		if (
			!value ||
			typeof value !== "object" ||
			!("pid" in value) ||
			typeof value.pid !== "number" ||
			!("socketPath" in value) ||
			typeof value.socketPath !== "string" ||
			!("operator" in value) ||
			typeof value.operator !== "string"
		)
			return undefined;
		try {
			process.kill(value.pid, 0);
		} catch {
			return undefined;
		}
		const host = value as Host;
		return (await reachable(host.socketPath)) &&
			(await reachable(host.operator))
			? host
			: undefined;
	} catch {
		return undefined;
	}
}

/** Attach to this workspace profile's live host or start one. Serialized by a
 * profile lock so concurrent launchers share one host (and one guard). */
async function ensureHost(): Promise<Host> {
	const release = await lockfile.lock(join(directory as string, "host"), {
		realpath: false,
		stale: 60_000,
		retries: { retries: 600, factor: 1, minTimeout: 50, maxTimeout: 50 },
	});
	try {
		const existing = await readHost();
		if (existing) return existing;
		const log = openSync(join(directory as string, "host.log"), "a", 0o600);
		try {
			const child = spawn(
				process.execPath,
				[
					"--import",
					pathToFileURL(
						fileURLToPath(new URL("./source-resolver.ts", import.meta.url)),
					).href,
					fileURLToPath(new URL("./host.ts", import.meta.url)),
				],
				{ detached: true, stdio: ["ignore", log, log], env: process.env },
			);
			child.unref();
			const deadline = Date.now() + 30_000;
			for (;;) {
				const host = await readHost();
				if (host && host.pid === child.pid) return host;
				if (child.exitCode !== null || child.signalCode !== null)
					throw new Error(
						`oqt host exited during startup; see ${join(directory as string, "host.log")}`,
					);
				if (Date.now() > deadline)
					throw new Error("Timed out waiting for the oqt host");
				await sleep(50);
			}
		} finally {
			closeSync(log);
		}
	} finally {
		await release();
	}
}

if (coding) {
	const host = await ensureHost();
	// The TUI permission facet talks to the guard directly; brains cannot.
	process.env.OQT_GUARD_OPERATOR = host.operator;
	const command = {
		command: "client" as const,
		connect: { transport: "unix" as const, path: host.socketPath },
		pluginPackages: [plugin],
		...(continuing ? { continue: true } : {}),
	};
	if (smoke) {
		const result = await runClient({
			...command,
			prompt: "Do not use tools. Reply exactly oqt-coding-ready.",
		});
		if (result.kind !== "prompted" || !result.text.includes("oqt-coding-ready"))
			throw new Error(`Coding smoke failed: ${JSON.stringify(result)}`);
		console.log(`oqt coding smoke PASS: ${result.text}`);
	} else {
		await runClientTui(command, { directory });
	}
} else {
	const runtime = await startServer({
		directory,
		sessionDir: join(directory, "sessions"),
		pluginPackages: [plugin],
	});
	try {
		const command = {
			command: "client" as const,
			connect: { transport: "unix" as const, path: runtime.socketPath },
			pluginPackages: [plugin],
			...(continuing ? { continue: true } : {}),
		};
		if (smoke) {
			const result = await runClient({ ...command, prompt: "own history" });
			if (
				result.kind !== "prompted" ||
				!result.text.includes("own fixture-token")
			)
				throw new Error(`Demo smoke failed: ${JSON.stringify(result)}`);
			console.log(`oqt demo smoke PASS: ${result.text}`);
		} else {
			await runClientTui(command, { directory });
		}
	} finally {
		await runtime.close();
	}
}

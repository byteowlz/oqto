// Detached oqt coding host: upstream server + guard for one workspace profile.
// Launchers attach to it; it retires when no client or worker remains.
import { existsSync, lstatSync, readdirSync } from "node:fs";
import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { userInfo } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import lockfile from "proper-lockfile";
import { Guard } from "./guard.ts";
import { OwnerBroker } from "./owner-broker.ts";
import { ownerModelDefaults } from "./owner-models.ts";
import { ownerRefresh } from "./owner-refresh.ts";
import { readOwnedJson } from "./readonly-profile.ts";
import { startServer } from "./server.ts";
import { installGuard } from "./spawn-hook.ts";

function required(name: string) {
	const value = process.env[name];
	if (!value) throw new Error(`Missing protected host context: ${name}`);
	return value;
}
function rejectCommands(value: unknown) {
	if (typeof value === "string" && value.startsWith("!"))
		throw new Error(
			"Command-valued model configuration is unavailable in oqt; use static owning credentials",
		);
	if (Array.isArray(value)) value.forEach(rejectCommands);
	else if (value && typeof value === "object")
		Object.values(value).forEach(rejectCommands);
}

const directory = required("OQT_SERVER_DIR");
const owner = required("OQT_OWNER_AGENT_DIR");
const agent = required("PI_CODING_AGENT_DIR");
const trusted = required("OQT_TRUSTED_SOURCE_ROOT");
const checkout = required("OQT_SOURCE_CHECKOUT");
const runtimeRoot = required("OQT_RUNTIME_ROOT");
const bpf = required("OQT_SECCOMP_BPF");
const experimental = dirname(fileURLToPath(import.meta.url));
const snapshot = resolve(experimental, "../../../..");
const plugin = join(
	snapshot,
	"packages/coding-agent/examples/plugins/oq-permissions",
);
const sessionDir = join(directory, "sessions");
const manifest = join(directory, "host.json");

const defaults = await ownerModelDefaults(owner);
const modelsPath = join(owner, "models.json");
const storePath = join(owner, "models-store.json");
if (existsSync(modelsPath))
	rejectCommands(await readOwnedJson(modelsPath, false));
await mkdir(sessionDir, { recursive: true, mode: 0o700 });

// A host killed outright leaves its private guard state behind (its carriers
// and hands die with it). Sweep directories whose host liveness lock is stale.
async function sweepDeadGuards() {
	for (const name of readdirSync("/var/tmp")) {
		if (!name.startsWith("oqg-")) continue;
		const dir = join("/var/tmp", name);
		try {
			const meta = lstatSync(dir);
			if (
				!meta.isDirectory() ||
				meta.uid !== userInfo().uid ||
				(meta.mode & 0o077) !== 0
			)
				continue;
			// A guard that has not yet taken its lock is younger than this.
			if (Date.now() - meta.mtimeMs < 60_000) continue;
			// Dead when its recorded host process is gone (or the pid was reused
			// by something else), or when its liveness lock went stale.
			let dead = false;
			try {
				const pid = Number(await readFile(join(dir, "host.pid"), "utf8"));
				const command = await readFile(`/proc/${pid}/cmdline`, "utf8").catch(
					() => "",
				);
				dead = !command.includes("host.ts");
			} catch {}
			if (
				!dead &&
				(await lockfile.check(join(dir, "alive"), {
					realpath: false,
					stale: 15_000,
				}))
			)
				continue;
			let roots: unknown = [];
			try {
				roots = JSON.parse(await readFile(join(dir, "roots.json"), "utf8"));
			} catch {}
			for (const root of Array.isArray(roots) ? roots : [])
				if (
					typeof root === "string" &&
					/^\/var\/tmp\/oqs-[A-Za-z0-9]+$/.test(root)
				)
					await rm(root, { recursive: true, force: true });
			await rm(dir, { recursive: true, force: true });
		} catch {
			// Best effort; never touch anything we cannot positively identify.
		}
	}
}
await sweepDeadGuards();

// Owner's global ceiling (read-only). Workspace .oqt/policy.toml may only narrow it.
const home = userInfo().homedir;
const globalCeiling = join(home, ".config", "oqt", "policy.toml");
const guard = await Guard.open({
	ceiling: {
		global: existsSync(globalCeiling)
			? await readFile(globalCeiling, "utf8")
			: null,
		home,
		hardDeny: [agent],
	},
	authorityRoot: join(directory, "authority"),
	protectedRoots: [owner, trusted, checkout, runtimeRoot],
	hands: {
		launcher: required("OQT_HANDS"),
		daemon: required("OQT_ENV_DAEMON"),
		bpf,
	},
	broker: new OwnerBroker(owner, ownerRefresh(owner)),
	brain: {
		launcher: required("OQT_BRAIN"),
		node: process.execPath,
		bpf,
		shim: required("OQT_SANDBOX_SHIM"),
		ro: [
			snapshot,
			checkout,
			...[modelsPath, storePath].filter((path) => existsSync(path)),
		],
		rw: [sessionDir, agent],
		forbidden: [join(owner, "auth.json"), join(owner, "auth.json.oqt-backup")],
		env: {
			PI_EXPERIMENTAL: "1",
			PI_OFFLINE: "1",
			PI_CODING_AGENT_DIR: agent,
			OQT_MODE: "coding",
			NODE_OPTIONS: "--max-old-space-size=512",
			OQT_OWNER_DEFAULTS: JSON.stringify(defaults),
			OQT_OWNER_AGENT_DIR: owner,
			...(existsSync(modelsPath) ? { OQT_OWNER_MODELS: modelsPath } : {}),
			...(process.env.OQT_DEBUG ? { OQT_DEBUG: "1" } : {}),
		},
		trustedSource: trusted,
		experimental,
		log: join(directory, "brain.log"),
	},
});
installGuard(guard);
await writeFile(join(guard.directory, "host.pid"), String(process.pid), {
	mode: 0o600,
});
const releaseAlive = await lockfile.lock(join(guard.directory, "alive"), {
	realpath: false,
	stale: 15_000,
	update: 5_000,
});
let runtime: Awaited<ReturnType<typeof startServer>> | undefined;
try {
	runtime = await startServer({
		directory,
		sessionDir,
		pluginPackages: [plugin],
		keepAlive: false,
	});
	const active = runtime;
	for (const signal of ["SIGTERM", "SIGINT"] as const)
		process.once(signal, () => void active.close());
	const pending = `${manifest}.tmp`;
	await writeFile(
		pending,
		JSON.stringify({
			pid: process.pid,
			socketPath: runtime.socketPath,
			operator: guard.operatorPath,
		}),
		{ mode: 0o600 },
	);
	await rename(pending, manifest);
	await runtime.closed;
} finally {
	await runtime?.close().catch(() => {});
	await rm(manifest, { force: true });
	installGuard(undefined);
	await releaseAlive().catch(() => {});
	await guard.close();
}
process.exit(0);

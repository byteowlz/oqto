/** Installed oqt entry: everything that used to happen in launch.py, in node.
 *
 * `oqt` is now `exec node <this file>` (three lines of sh). Nothing runs before
 * node except resolving `current`. Full content verification is no longer on the
 * launch path — see `oqt verify` (ADR-0072): at install time the tree is hashed,
 * published read-only and named by that hash; re-hashing 892 MB on every start
 * detects accidents, not attackers, since everything that runs first is written
 * by the same user anyway.
 */
import { spawnSync } from "node:child_process";
import { lstatSync, readFileSync, realpathSync, statSync } from "node:fs";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { chdir, cwd, getuid } from "node:process";
import { fileURLToPath } from "node:url";
import { prepareState, StateError } from "./installed-state.ts";

function fail(reason: string): never {
	process.stderr.write(`oqt: ${reason}\n`);
	process.exit(2);
}

/** The runtime this file lives in: <runtime>/pi-source/packages/coding-agent/src/experimental/ */
function runtimeRoot(): string {
	const declared = process.env.OQT_CURRENT_RUNTIME;
	if (declared !== undefined && isAbsolute(declared)) {
		const verified = realpathSync(declared);
		if (realpathSync(fileURLToPath(import.meta.url)).startsWith(`${verified}/`))
			return verified;
		fail("Runtime entry does not belong to the launched runtime");
	}
	let current = dirname(fileURLToPath(import.meta.url));
	while (true) {
		if (lstatSync(join(current, "runtime.json")).isFile())
			return realpathSync(current);
		const parent = dirname(current);
		if (parent === current) fail("Not inside an installed runtime");
		current = parent;
	}
}

/** Cheap launch check: the runtime is the artifact it says it is, and it is the
 * immutable, read-only copy the installer created. Content hashing is `oqt verify`. */
function checkRuntime(runtime: string): void {
	const meta = lstatSync(runtime);
	if (
		!meta.isDirectory() ||
		meta.uid !== getuid() ||
		(meta.mode & 0o777) !== 0o500
	)
		fail("Unsafe runtime root; reinstall oqt");
	const manifest = join(runtime, "manifest.json");
	const manifestMeta = lstatSync(manifest);
	if (
		!manifestMeta.isFile() ||
		manifestMeta.uid !== getuid() ||
		(manifestMeta.mode & 0o777) !== 0o400
	)
		fail("Unsafe runtime manifest; reinstall oqt");
	// Parsing (not hashing) the manifest: a truncated or foreign manifest is
	// caught here; proving the tree still hashes to its name is `oqt verify`.
	const saved = JSON.parse(readFileSync(manifest, "utf8")) as {
		schema?: unknown;
		files?: unknown;
	};
	if (
		saved.schema !== 1 ||
		saved.files === undefined ||
		typeof saved.files !== "object"
	)
		fail("Unsupported runtime manifest; reinstall oqt");
}

interface Options {
	readonly runtimeInfo: boolean;
	readonly workdir: string;
	readonly owner: string;
	readonly mode: "coding" | "demo";
	/** Flags forwarded to the launcher, in its own order. */
	readonly launcher: string[];
}

function parse(argv: readonly string[]): Options {
	let runtimeInfo = false;
	let workdir = cwd();
	let owner =
		process.env.OQT_OWNER_AGENT_DIR ??
		process.env.PI_CODING_AGENT_DIR ??
		`${process.env.HOME}/.pi/agent`;
	let mode: "coding" | "demo" | undefined;
	const launcher: string[] = [];
	for (let index = 0; index < argv.length; index++) {
		const flag = argv[index] ?? fail("Missing option");
		const value = () => {
			const next = argv[++index];
			if (next === undefined || next.startsWith("--"))
				fail(`${flag} requires a value`);
			return next;
		};
		switch (flag) {
			case "--runtime-info":
				runtimeInfo = true;
				break;
			case "--workdir":
				workdir = value();
				break;
			case "--owner-profile":
				owner = value();
				break;
			case "--coding":
				if (mode === "demo") fail("Choose --coding or --demo, not both");
				mode = "coding";
				break;
			case "--demo":
				if (mode === "coding") fail("Choose --coding or --demo, not both");
				mode = "demo";
				launcher.push(flag);
				break;
			case "--continue":
			case "-c":
			case "--smoke":
			case "--json":
				launcher.push(flag);
				break;
			case "--import-classic":
			case "--origin":
			case "--leaf":
				launcher.push(flag, value());
				break;
			default:
				fail(`Unsupported oqt option: ${flag}`);
		}
	}
	return { runtimeInfo, workdir, owner, mode: mode ?? "coding", launcher };
}

const runtime = runtimeRoot();
checkRuntime(runtime);
const metadata = JSON.parse(
	readFileSync(join(runtime, "runtime.json"), "utf8"),
) as Record<string, string>;
const path = (name: string, kind: "file" | "directory"): string => {
	const value = resolve(
		runtime,
		metadata[name] ?? fail(`Missing runtime path: ${name}`),
	);
	const meta = lstatSync(value);
	if (meta.uid !== getuid() || (meta.mode & 0o077) !== 0)
		fail(`Unsafe runtime path: ${name}`);
	if (kind === "file" ? !meta.isFile() : !meta.isDirectory())
		fail(`Unsafe runtime path: ${name}`);
	return value;
};
const asset = (name: string) => path(name, "file");
const source = path("source", "directory");
const node = asset("node");
const options = parse(process.argv.slice(2));

// Classic history inspection is read-only and needs no profile, state or host.
if (
	process.argv[2] === "inspect-classic" ||
	process.argv[2] === "list-classic"
) {
	const result = spawnSync(
		node,
		[
			join(source, "packages/coding-agent/src/experimental/classic-inspect.ts"),
			...process.argv.slice(2),
		],
		{ stdio: "inherit", env: process.env },
	);
	process.exit(result.status ?? 1);
}

if (options.runtimeInfo) {
	process.stdout.write(
		`${JSON.stringify({ runtime, artifact: runtime.slice(runtime.lastIndexOf("/") + 1), source, node })}\n`,
	);
	process.exit(0);
}

if (process.platform !== "linux" || process.arch !== "x64")
	fail("This experimental runtime requires Linux x86_64");

const workdir =
	realpathSync(options.workdir) ?? fail("Work directory must exist");
if (!statSync(workdir).isDirectory())
	fail("Work directory must be a directory");
const stateHome = resolve(
	process.env.XDG_STATE_HOME ?? `${process.env.HOME}/.local/state`,
);
const installStore = dirname(dirname(runtime));
// The state home may not exist yet on a first launch; resolve lexically.
const stateStore = resolve(stateHome);
const artifact = runtime.slice(runtime.lastIndexOf("/") + 1);
const overlaps = (left: string, right: string) =>
	left === right ||
	left.startsWith(`${right}/`) ||
	right.startsWith(`${left}/`);
for (const protectedPath of [installStore, stateStore]) {
	if (overlaps(workdir, protectedPath))
		fail(
			"Workspace overlaps protected installed runtime or state; choose a narrower workspace",
		);
}

let state: ReturnType<typeof prepareState>;
try {
	state = prepareState({
		artifact,
		workdir,
		mode: options.mode,
		owner: options.mode === "demo" ? null : realpathSync(options.owner),
		stateHome,
	});
} catch (error) {
	if (error instanceof StateError) fail(error.message);
	throw error;
}
if (overlaps(workdir, realpathSync(state.runtimeRoot)))
	fail(
		"Workspace overlaps protected installed state; choose a narrower workspace",
	);

// The confined brain, host and guard inherit exactly this environment — never
// the ambient one (same whitelist the Python launcher published).
const environment: Record<string, string> = {
	PATH: process.env.PATH ?? "/usr/bin:/bin",
	HOME: state.home,
	LANG: process.env.LANG ?? "C.UTF-8",
	TERM: process.env.TERM ?? "xterm-256color",
	COLORTERM: process.env.COLORTERM ?? "",
	NODE_OPTIONS: "--max-old-space-size=512",
	PI_EXPERIMENTAL: "1",
	PI_OFFLINE: "1",
	PI_CODING_AGENT_DIR: state.agent,
	OQT_MODE: options.mode,
	OQT_OWNER_AGENT_DIR: realpathSync(options.owner),
	OQT_RUNTIME_ROOT: state.runtimeRoot,
	OQT_TRUSTED_SOURCE_ROOT: installStore,
	OQT_SOURCE_CHECKOUT: source,
	OQT_INSTALLED_BIN_ROOT: `${process.env.HOME}/.local/bin`,
	OQT_SERVER_DIR: state.profile,
	OQT_CARRIER: asset("carrier"),
	OQT_HANDS: asset("hands"),
	OQT_BRAIN: asset("brain"),
	OQT_ENV_DAEMON: asset("daemon"),
	OQT_SANDBOX_SHIM: asset("shim"),
	OQT_SECCOMP_BPF: asset("bpf"),
	OQT_SANDBOX_CONFIG:
		process.env.OQT_SANDBOX_CONFIG ??
		`${process.env.HOME}/.config/oqto/sandbox.toml`,
	OQT_INPUT_TRACE: join(state.profile, "input-trace.log"),
	OQT_STARTUP_TRACE: join(state.profile, "startup-trace.log"),
	// Seconds, matching what the shell/launcher trace expects.
	OQT_SHELL_START:
		process.env.OQT_SHELL_START ?? (Date.now() / 1000).toFixed(3),
	NODE_COMPILE_CACHE:
		process.env.NODE_COMPILE_CACHE ??
		join(state.runtimeRoot, "node-compile-cache"),
};
for (const key of ["OQT_DEBUG", "OQT_HOST_IDLE_SECONDS"]) {
	const value = process.env[key];
	if (value !== undefined) environment[key] = value;
}
// Anything else the shell had is dropped before any child can inherit it.
for (const key of Object.keys(process.env)) {
	if (environment[key] === undefined) delete process.env[key];
}
Object.assign(process.env, environment);

chdir(workdir);
// The launcher reads its own flags from argv, as it did when Python exec'd it.
process.argv = [
	process.argv[0] ?? "",
	join(dirname(fileURLToPath(import.meta.url)), "oqt-launcher.ts"),
	...options.launcher,
];
// prepare.py publishes tools/oqt/launcher.ts under this name.
await import("./oqt-launcher.ts");

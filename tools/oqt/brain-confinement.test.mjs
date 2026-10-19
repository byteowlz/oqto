import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import {
	chmod,
	mkdir,
	mkdtemp,
	readFile,
	rm,
	symlink,
	writeFile,
} from "node:fs/promises";
import { createServer } from "node:net";
import { join } from "node:path";
import { test } from "node:test";
import { Guard } from "./guard.ts";
import { OwnerBroker } from "./owner-broker.ts";

// Drives the real Guard.spawnBrain + oqt_brain with a probe entrypoint in
// place of the session worker, and reports what the confined brain can reach.
test(
	"confined brain sees only its binds: no owner credentials, ledger, guard state or other sessions",
	{ timeout: 30000 },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-brain-proof-");
		await chmod(root, 0o700);
		const owner = join(root, "owner");
		const workspace = join(root, "workspace");
		const sessions = join(root, "sessions");
		const agent = join(root, "agent");
		const experimental = join(root, "experimental");
		const authorityRoot = join(root, "authority");
		for (const dir of [owner, workspace, sessions, agent, experimental])
			await mkdir(dir, { mode: 0o700 });
		await writeFile(
			join(owner, "auth.json"),
			'{"secret":{"type":"api_key","key":"owner-secret-7731"}}',
			{ mode: 0o600 },
		);
		await writeFile(join(owner, "models.json"), '{"providers":{}}', {
			mode: 0o600,
		});
		await writeFile(join(owner, "settings.json"), '{"secretish":true}', {
			mode: 0o600,
		});
		await writeFile(join(workspace, "workspace-file"), "workspace-data");
		await writeFile(join(experimental, "source-resolver.ts"), "export {};\n");
		const control = join(root, "control.sock");
		const coordinator = createServer((socket) => {
			socket.on("error", () => {});
			socket.end("coordinator-ok\n");
		});
		await new Promise((resolve) => coordinator.listen(control, resolve));
		const report = join(sessions, "report.json");
		await writeFile(
			join(experimental, "session-worker.ts"),
			`import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
const args = JSON.parse(process.argv[2]);
const listing = (path) => { try { return readdirSync(path); } catch (e) { return e.code; } };
const probe = (path) => { try { readFileSync(path); return "readable"; } catch (e) { return e.code; } };
const once = (path) => new Promise((done) => { const s = connect(path); s.once("connect", () => { s.destroy(); done("connected"); }); s.once("error", (e) => done(e.code)); });
// The guard opens a session asynchronously; real brains retry the same way.
const reach = async (path) => { let last; for (let i = 0; i < 100; i++) { last = await once(path); if (last === "connected") return last; await new Promise((r) => setTimeout(r, 50)); } return last; };
const result = {
  ownerAuth: probe(${JSON.stringify(join(owner, "auth.json"))}),
  ownerSettings: probe(${JSON.stringify(join(owner, "settings.json"))}),
  ownerModels: probe(${JSON.stringify(join(owner, "models.json"))}),
  authority: listing(${JSON.stringify(authorityRoot)}),
  workspace: probe(${JSON.stringify(join(workspace, "workspace-file"))}),
  ssh: listing(${JSON.stringify(join(process.env.HOME ?? "/home", ".ssh"))}),
  pi: listing(${JSON.stringify(join(process.env.HOME ?? "/home", ".pi"))}),
  coordinator: await reach(process.env.PI_SESSION_WORKER_CONTROL_ADDRESS),
  tool: await reach(process.env.OQT_GUARD_SOCKET),
  guardDir: listing(process.env.OQT_PROBE_GUARD_DIR ?? "/nonexistent"),
  env: Object.keys(process.env).filter((k) => !/^(PATH|HOME|LANG|SHELL|PWD|SHLVL|_)$/.test(k)).sort(),
  session: args.metadata.id,
};
writeFileSync(${JSON.stringify(report)}, JSON.stringify(result));
`,
		);
		const guard = await Guard.open({
			authorityRoot,
			protectedRoots: [owner],
			hands: {
				launcher: process.env.OQT_HANDS,
				daemon: process.env.OQT_ENV_DAEMON,
				bpf: process.env.OQT_SECCOMP_BPF,
			},
			broker: new OwnerBroker(owner),
			brain: {
				launcher: process.env.OQT_BRAIN,
				node: process.execPath,
				bpf: process.env.OQT_SECCOMP_BPF,
				shim: process.env.OQT_SANDBOX_SHIM,
				ro: [experimental, join(owner, "models.json")],
				rw: [sessions, agent],
				forbidden: [join(owner, "auth.json")],
				env: { PI_EXPERIMENTAL: "1", OQT_MODE: "coding" },
				trustedSource: join(root, "trusted"),
				experimental,
				log: join(root, "brain.log"),
			},
		});
		try {
			const child = guard.spawnBrain(
				{
					metadata: {
						id: "public-session-a",
						cwd: workspace,
						path: join(sessions, "a"),
					},
					pluginManifestPaths: [],
				},
				{
					PI_SESSION_WORKER_CONTROL_ADDRESS: control,
					PI_SESSION_WORKER_CONTROL_TOKEN: "token",
					OQT_PROBE_GUARD_DIR: guard.directory,
				},
			);
			const code = await new Promise((resolve) =>
				child.once("exit", (c, s) => resolve(s ?? c)),
			);
			const log = await readFile(join(root, "brain.log"), "utf8").catch(
				() => "",
			);
			assert.equal(code, 0, `brain probe failed: ${log}`);
			const result = JSON.parse(await readFile(report, "utf8"));
			assert.ok(
				["ENOENT", "EACCES"].includes(result.ownerAuth),
				`owner credentials must be unreadable: ${result.ownerAuth}`,
			);
			assert.ok(
				["ENOENT", "EACCES"].includes(result.ownerSettings),
				"unbound owner files are unreadable",
			);
			assert.equal(
				result.ownerModels,
				"readable",
				"explicit read-only catalog bind",
			);
			// Masked paths may exist as empty mountpoints; their contents never do.
			const hidden = (value) =>
				value === "ENOENT" ||
				value === "EACCES" ||
				(Array.isArray(value) && value.length === 0);
			assert.ok(
				hidden(result.authority),
				`permission ledger invisible: ${JSON.stringify(result.authority)}`,
			);
			assert.ok(
				hidden(result.guardDir),
				`guard state invisible: ${JSON.stringify(result.guardDir)}`,
			);
			assert.ok(
				["ENOENT", "EACCES"].includes(result.workspace),
				"the brain never touches workspace files; hands do",
			);
			assert.ok(
				hidden(result.ssh),
				`home secrets invisible: ${JSON.stringify(result.ssh)}`,
			);
			assert.ok(
				hidden(result.pi),
				`owner Pi profile invisible: ${JSON.stringify(result.pi)}`,
			);
			assert.equal(result.coordinator, "connected");
			assert.equal(result.tool, "connected", "own tool channel reachable");
			assert.equal(result.session, "public-session-a");
			assert.ok(!result.env.includes("OQTO_SANDBOX_SHIM_BIN"));
			assert.ok(
				result.env.every((key) => /^(PI_|OQT_|__PI_|NODE_OPTIONS)/.test(key)),
				result.env.join(","),
			);
			// Forbidden binds fail closed at launch, never silently dropped.
			await assert.rejects(
				Guard.open({
					authorityRoot: join(root, "authority2"),
					protectedRoots: [owner],
					hands: guard.options.hands,
					broker: new OwnerBroker(owner),
					brain: {
						...guard.options.brain,
						ro: [owner],
						log: join(root, "brain2.log"),
					},
				}).then(async (other) => {
					try {
						const bad = other.spawnBrain(
							{
								metadata: {
									id: "public-session-b",
									cwd: workspace,
									path: join(sessions, "b"),
								},
								pluginManifestPaths: [],
							},
							{ PI_SESSION_WORKER_CONTROL_ADDRESS: control },
						);
						const status = await new Promise((resolve) =>
							bad.once("exit", (c, s) => resolve(s ?? c)),
						);
						if (status !== 0) throw new Error("refused");
					} finally {
						await other.close();
					}
				}),
				/refused/,
			);
			assert.match(
				await readFile(join(root, "brain2.log"), "utf8"),
				/overlaps protected/,
			);
		} finally {
			await guard.close();
			coordinator.close();
			await rm(root, { recursive: true, force: true });
		}
	},
);

// Exercise the native spec boundary, retaining the installed runtime's short
// /var/tmp IPC alias while the real state remains in a separate persistent tree.
test(
	"native brain preserves short aliases without granting their parents",
	{ timeout: 30000 },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-brain-alias-state-");
		const alias = `/var/tmp/qi${process.getuid()}-${root.split("-").at(-1)}`;
		const state = join(root, "persistent", "runtimes", "fixture-hash");
		const session = join(state, "sessions");
		const runtime = join(root, "runtime");
		const specDir = join(root, "spec");
		const forbidden = join(root, "protected");
		let server;
		try {
			for (const dir of [
				session,
				join(state, "catalog"),
				join(runtime, ".oqto"),
				specDir,
				forbidden,
			])
				await mkdir(dir, { recursive: true, mode: 0o700 });
			await symlink(state, alias);
			await writeFile(join(root, "persistent", "sibling-secret"), "hidden");
			await writeFile(join(state, "unbound-secret"), "hidden");
			await writeFile(join(state, "catalog", "models.json"), "public-catalog");
			await writeFile(join(forbidden, "secret"), "hidden");
			await symlink(forbidden, join(state, "protected-alias"));
			await writeFile(
				join(runtime, "probe.mjs"),
				`
import { readFileSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
const probe = p => { try { readFileSync(p); return "readable"; } catch(e) { return e.code; } };
const catalog = readFileSync(${JSON.stringify(join(alias, "catalog", "models.json"))}, "utf8");
let catalogWrite;
try { writeFileSync(${JSON.stringify(join(alias, "catalog", "models.json"))}, "changed"); catalogWrite = "writable"; } catch(e) { catalogWrite = e.code; }
const socket = await new Promise(resolve => {
 const s = connect(${JSON.stringify(join(alias, "sessions", "c.sock"))});
 s.on("data", b => { resolve(b.toString()); s.destroy(); });
 s.on("error", e => resolve(e.code));
});
writeFileSync(${JSON.stringify(join(alias, "sessions", "report.json"))}, JSON.stringify({socket, catalog, catalogWrite,
 parent: probe(${JSON.stringify(join(root, "persistent", "sibling-secret"))}),
 sibling: probe(${JSON.stringify(join(alias, "unbound-secret"))}),
 canonicalSibling: probe(${JSON.stringify(join(state, "unbound-secret"))}),
 protected: probe(${JSON.stringify(join(alias, "protected-alias", "secret"))})}));
`,
			);
			server = createServer((s) => {
				s.on("error", () => {});
				s.end("short-ipc-ok");
			});
			await new Promise((resolve) =>
				server.listen(join(alias, "sessions", "c.sock"), resolve),
			);
			const spec = {
				node: process.execPath,
				cwd: runtime,
				ro: [runtime, join(alias, "catalog")],
				rw: [join(alias, "sessions")],
				forbidden: [forbidden],
				bpf: process.env.OQT_SECCOMP_BPF,
				env: {},
				args: [join(runtime, "probe.mjs")],
			};
			const launch = async (value) => {
				const path = join(specDir, "brain.json");
				await writeFile(path, JSON.stringify(value), { mode: 0o600 });
				const child = spawn(process.env.OQT_BRAIN, [path], {
					env: {
						...process.env,
						OQTO_SANDBOX_SHIM_BIN: process.env.OQT_SANDBOX_SHIM,
					},
					stdio: ["ignore", "pipe", "pipe"],
					timeout: 10000,
					killSignal: "SIGKILL",
				});
				let log = "";
				child.stdout.on("data", (b) => {
					log += b;
				});
				child.stderr.on("data", (b) => {
					log += b;
				});
				const code = await new Promise((resolve, reject) => {
					child.once("error", reject);
					child.once("exit", (c, s) => resolve(s ?? c));
				});
				return { code, log };
			};
			const result = await launch(spec);
			assert.equal(result.code, 0, result.log);
			const report = JSON.parse(
				await readFile(join(session, "report.json"), "utf8"),
			);
			assert.equal(report.socket, "short-ipc-ok");
			assert.equal(report.catalog, "public-catalog");
			assert.ok(
				["EROFS", "EACCES"].includes(report.catalogWrite),
				report.catalogWrite,
			);
			for (const key of ["parent", "sibling", "canonicalSibling", "protected"])
				assert.ok(
					["ENOENT", "EACCES"].includes(report[key]),
					`${key}: ${report[key]}`,
				);
			for (const kind of ["ro", "rw"]) {
				const denied = await launch({
					...spec,
					[kind]: [...spec[kind], join(alias, "protected-alias")],
				});
				assert.notEqual(denied.code, 0);
				assert.match(denied.log, /overlaps protected/);
			}
		} finally {
			if (server) await new Promise((resolve) => server.close(resolve));
			await rm(alias, { force: true });
			await rm(root, { recursive: true, force: true });
		}
	},
);

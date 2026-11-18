// Remote hands (ADR-0086) through the real stack: guard, ledger, brain
// composition and gated tools (faux model only), the real oqt_link adapter,
// pinned TLS, personal pairing with a hands delegation, the real foreground
// oqto-hands-host and the platform's confined oqt_hands launcher. The
// "remote" machine is a loopback hands host with its own workspace, so every
// effect can be checked on disk. Disposable state only.
import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { join } from "node:path";
import { test } from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import { promisify } from "node:util";
import {
	call,
	fauxAssistantMessage,
	toolResults,
	turn,
	withGuard,
} from "./guard-fixture.mjs";

const run = promisify(execFile);
const HOST = process.env.OQTO_HANDS_HOST;
const LINK = process.env.OQT_LINK;
const LINUX = process.platform === "linux";

async function freePort() {
	return new Promise((resolve) => {
		const server = createServer();
		server.listen(0, "127.0.0.1", () => {
			const { port } = server.address();
			server.close(() => resolve(port));
		});
	});
}

/** A loopback destination: config, foreground host, pairing, delegation. */
async function destination(root) {
	const remote = join(root, "remote-work");
	const protectedRoot = join(root, "protected");
	await mkdir(remote);
	await mkdir(protectedRoot);
	const port = await freePort();
	const state = join(root, "host-state");
	const config = join(root, "hands-host.toml");
	await writeFile(
		config,
		[
			"version = 1",
			`listen = "127.0.0.1:${port}"`,
			`origin = "https://127.0.0.1:${port}/"`,
			`state_dir = "${state}"`,
			"[launcher]",
			`oqt_hands = "${process.env.OQT_HANDS}"`,
			`daemon = "${process.env.OQT_ENV_DAEMON}"`,
			...(LINUX
				? [
						`bpf = "${process.env.OQT_SECCOMP_BPF}"`,
						`shim = "${process.env.OQT_SANDBOX_SHIM}"`,
					]
				: []),
			`protected_roots = ["${protectedRoot}"]`,
			"[bindings.workspace]",
			`root = "${remote}"`,
			"max_lease_ms = 10000",
			"max_concurrent_calls = 4",
			"",
		].join("\n"),
	);
	let host;
	const start = async () => {
		host = spawn(HOST, ["--config", config, "serve"], {
			stdio: ["ignore", "ignore", "pipe"],
			env: { PATH: "/usr/bin:/bin", HOME: process.env.HOME, RUST_LOG: "info" },
		});
		host.log = "";
		host.stderr.on("data", (chunk) => {
			host.log += chunk;
		});
		for (let i = 0; i < 100 && !host.log.includes("serving"); i++)
			await sleep(100);
		assert.ok(host.log.includes("serving"), host.log);
	};
	await start();
	const owner = async (...args) =>
		JSON.parse(
			(await run(HOST, ["--config", config, ...args])).stdout || "null",
		);
	const qr = join(root, "qr.json");
	await run(HOST, ["--config", config, "pair", "--out", qr]);
	const linkState = join(root, "link-state");
	const paired = JSON.parse(
		(
			await run(LINK, [
				"--state",
				linkState,
				"pair",
				"--name",
				"dest",
				"--qr-file",
				qr,
			])
		).stdout,
	);
	const linkConfig = join(root, "hands-link.toml");
	await writeFile(
		linkConfig,
		'version = 1\n[machines.far]\npairing = "dest"\nbinding = "workspace"\nlease_ms = 10000\nrenew_ms = 3000\nmax_concurrent_calls = 4\n',
	);
	return {
		remote,
		config,
		linkConfig,
		linkState,
		device: paired.device_id,
		owner,
		delegate: () =>
			owner("delegate", "--device", paired.device_id, "--binding", "workspace"),
		stop: async () => {
			if (host && host.exitCode === null) {
				host.kill("SIGTERM");
				await new Promise((resolve) => host.once("exit", resolve));
			}
		},
		start,
	};
}

async function withRemote(body, options = {}) {
	const root = await mkdtemp("/var/tmp/oqt-remote-proof-");
	const dest = await destination(root);
	if (options.delegate !== false) await dest.delegate();
	process.env.OQT_HANDS_LINK_CONFIG = dest.linkConfig;
	process.env.OQT_HANDS_LINK_STATE = dest.linkState;
	try {
		await withGuard((context) => body({ ...context, dest }), {
			link: { binary: LINK, profile: root },
			...options.guard,
		});
	} finally {
		delete process.env.OQT_HANDS_LINK_CONFIG;
		delete process.env.OQT_HANDS_LINK_STATE;
		await dest.stop();
		await rm(root, { recursive: true, force: true });
	}
}

/** Tool results by call id (entries are not in call order). */
const byId = (entries) =>
	Object.fromEntries(toolResults(entries).map((m) => [m.toolCallId, m]));

const text = (message) =>
	(message.content ?? [])
		.map((part) => (part.type === "text" ? part.text : ""))
		.join("");

test(
	"read/write/edit/bash run on the named machine and name machine and cwd",
	{ timeout: 90_000, skip: !HOST || !LINK },
	async () => {
		await withRemote(
			async ({ dest, workspace, faux, conversation, requests }) => {
				faux.setResponses([
					call(
						"write",
						{ path: "r.txt", content: "before\n", machine: "far" },
						"w",
					),
					call(
						"edit",
						{
							path: "r.txt",
							oldText: "before",
							newText: "after",
							machine: "far",
						},
						"e",
					),
					call("read", { path: "r.txt", machine: "far" }, "r"),
					call(
						"bash",
						{ command: "pwd; cat r.txt; uname -s", machine: "far" },
						"b",
					),
					call("write", { path: "local.txt", content: "here\n" }, "l"),
					fauxAssistantMessage("done"),
				]);
				const entries = await turn(conversation);
				const id = byId(entries);
				const results = ["w", "e", "r", "b", "l"].map((key) => id[key]);
				assert.ok(
					results.every((m) => m && !m.isError),
					JSON.stringify(results),
				);
				// Effects only in the destination's own workspace.
				assert.equal(
					await readFile(join(dest.remote, "r.txt"), "utf8"),
					"after\n",
				);
				assert.equal(existsSync(join(workspace, "r.txt")), false);
				assert.equal(
					await readFile(join(workspace, "local.txt"), "utf8"),
					"here\n",
				);
				const remoteCwd = await (await import("node:fs/promises")).realpath(
					dest.remote,
				);
				for (const m of results.slice(0, 4))
					assert.ok(
						text(m).includes(`[machine far · cwd ${remoteCwd}]`),
						text(m),
					);
				assert.ok(text(results[3]).includes(remoteCwd));
				assert.ok(text(results[3]).includes(LINUX ? "Linux" : "Darwin"));
				assert.ok(text(results[4]).includes("[machine local · cwd"));
				// The local guard decided: remote edits and shell asked, labelled.
				const remote = requests.filter((r) => r.resource.includes("on far"));
				assert.deepEqual(
					remote.map((r) => r.capability),
					["write", "edit", "bash"],
				);
				assert.ok(remote.every((r) => r.preview.startsWith("[on far]")));
				// One destination receipt per call, stop confirmed, none replayed.
				const recent = await dest.owner("recent", "--limit", "16");
				assert.equal(recent.length, 4);
				assert.ok(
					recent.every((r) => r.state === "released" && r.stop === "confirmed"),
				);
			},
		);
	},
);

test(
	"unknown, undelegated and offline machines fail before any effect, no local fallback",
	{ timeout: 90_000, skip: !HOST || !LINK },
	async () => {
		await withRemote(
			async ({ dest, workspace, faux, conversation, requests }) => {
				faux.setResponses([
					call(
						"write",
						{ path: "x.txt", content: "x", machine: "nowhere" },
						"u",
					),
					call("bash", { command: "touch y.txt", machine: "far" }, "d"),
					fauxAssistantMessage("done"),
				]);
				const entries = await turn(conversation);
				const id = byId(entries);
				const results = [id.u, id.d];
				assert.ok(results.every((m) => m?.isError));
				assert.match(text(results[0]), /Unknown machine/);
				// Paired but not delegated: refused before the operator is asked.
				assert.match(text(results[1]), /offline, unpaired, unbound or refused/);
				assert.equal(requests.length, 0);
				assert.equal(existsSync(join(workspace, "x.txt")), false);
				assert.equal(existsSync(join(workspace, "y.txt")), false);
				assert.equal(existsSync(join(dest.remote, "y.txt")), false);
				// Delegated, then the machine goes offline.
				await dest.delegate();
				await dest.stop();
				faux.setResponses([
					call("bash", { command: "touch z.txt", machine: "far" }, "o"),
					fauxAssistantMessage("done"),
				]);
				const offline = byId(await turn(conversation, "offline-turn")).o;
				assert.ok(offline.isError);
				assert.match(text(offline), /offline/);
				assert.equal(existsSync(join(workspace, "z.txt")), false);
			},
			{ delegate: false },
		);
	},
);

test(
	"code-mode fan-out keeps machine with one receipt per call and settles siblings",
	{ timeout: 90_000, skip: !HOST || !LINK },
	async () => {
		await withRemote(async ({ dest, workspace, faux, conversation }) => {
			const cell = `
const results = await Promise.allSettled([
  tools.write({ path: "a.txt", content: "A", machine: "far" }),
  tools.write({ path: "b.txt", content: "B" }),
  tools.read({ path: "missing.txt", machine: "far" }),
]);
return results.map((r) => r.status);`;
			faux.setResponses([
				call("code", { code: cell }, "c"),
				fauxAssistantMessage("done"),
			]);
			const entries = await turn(conversation);
			const result = byId(entries).c;
			const output = text(result);
			assert.match(output, /Result: \["fulfilled","fulfilled","rejected"\]/);
			assert.match(output, /#0 write on far: ok/);
			assert.match(output, /#1 write on local: ok/);
			assert.match(output, /#2 read on far: error/);
			assert.equal(await readFile(join(dest.remote, "a.txt"), "utf8"), "A");
			assert.equal(await readFile(join(workspace, "b.txt"), "utf8"), "B");
			assert.equal(existsSync(join(workspace, "a.txt")), false);
			// The destination journaled exactly its two calls, each once.
			const recent = await dest.owner("recent", "--limit", "16");
			assert.equal(recent.length, 2, JSON.stringify(recent));
			assert.equal(new Set(recent.map((r) => r.call_id)).size, 2);
			assert.ok(
				recent.some((r) => r.operation === "write" && r.state === "released"),
			);
		});
	},
);

test(
	"a dropped link makes the running call unknown and it is never replayed",
	{ timeout: 90_000, skip: !HOST || !LINK },
	async () => {
		await withRemote(async ({ dest, faux, conversation, guard }) => {
			faux.setResponses([
				call(
					"bash",
					{ command: "echo mark >> marks.txt; sleep 20", machine: "far" },
					"k",
				),
				fauxAssistantMessage("done"),
			]);
			const pending = turn(conversation);
			const marks = join(dest.remote, "marks.txt");
			for (let i = 0; i < 100 && !existsSync(marks); i++) await sleep(100);
			assert.ok(existsSync(marks));
			// The guard's link adapter dies mid-call (cut after the effect).
			guard.link.child.kill("SIGKILL");
			const result = byId(await pending).k;
			assert.ok(result.isError);
			assert.match(text(result), /uncertain|unknown/i);
			await sleep(1500);
			assert.equal(await readFile(marks, "utf8"), "mark\n");
			const recent = await dest.owner("recent", "--limit", "4");
			assert.equal(recent.length, 1);
			assert.ok(["unknown", "expired", "fenced"].includes(recent[0].state));
			assert.equal(recent[0].stop, "confirmed");
		});
	},
);

test(
	"revoke freezes locally, fences the destination and confirms the stop",
	{ timeout: 90_000, skip: !HOST || !LINK },
	async () => {
		await withRemote(
			async ({ dest, faux, conversation, operator, session }) => {
				faux.setResponses([
					call(
						"bash",
						{ command: "echo go > started.txt; sleep 25", machine: "far" },
						"s",
					),
					fauxAssistantMessage("done"),
				]);
				const pending = turn(conversation);
				const started = join(dest.remote, "started.txt");
				for (let i = 0; i < 100 && !existsSync(started); i++) await sleep(100);
				assert.ok(existsSync(started));
				const began = Date.now();
				await operator.call("revoke_all", { session });
				const result = byId(await pending).s;
				assert.ok(result.isError);
				assert.ok(Date.now() - began < 15_000);
				const recent = await dest.owner("recent", "--limit", "4");
				assert.equal(recent[0].state, "fenced");
				assert.equal(recent[0].stop, "confirmed");
				const status = await dest.owner("status");
				assert.equal(status.calls.length, 0);
			},
		);
	},
);

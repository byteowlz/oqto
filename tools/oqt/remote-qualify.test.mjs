// Physical qualification of remote hands (ADR-0086 "Acceptance") between two
// real machines, through the real guard, ledger, gated tools (faux model
// only), oqt_link, pinned TLS and the destination's foreground
// oqto-hands-host. Everything machine-specific comes from the environment;
// nothing here names a host, address or home path.
//
//   HL_Q_MACHINE      configured machine name (OQT_HANDS_LINK_CONFIG/STATE set, paired, delegated)
//   HL_Q_REMOTE_OS    expected `uname -s` of the destination
//   HL_Q_REMOTE_SH    optional shell prefix that runs a command on the destination
//   HL_Q_REMOTE_ROOT  destination binding root (with HL_Q_REMOTE_SH)
//   HL_Q_OWNER        optional destination owner CLI (run through HL_Q_REMOTE_SH)
//   HL_Q_HOST_PATTERN optional pkill -f pattern of the destination host (suspend proof)
//   HL_Q_ONLY         optional comma list of scenario names
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
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
const MACHINE = process.env.HL_Q_MACHINE;
const SH = process.env.HL_Q_REMOTE_SH;
const ROOT = process.env.HL_Q_REMOTE_ROOT;
const OWNER = process.env.HL_Q_OWNER;
const PATTERN = process.env.HL_Q_HOST_PATTERN;
const ONLY = process.env.HL_Q_ONLY?.split(",");
const enabled = (name, needs = true) =>
	Boolean(MACHINE) && needs && (!ONLY || ONLY.includes(name));
const trace = [];
const note = (event) => {
	trace.push({ at: new Date().toISOString(), ...event });
	console.log(`TRACE ${JSON.stringify(event)}`);
};

const quote = (text) => `'${text.replaceAll("'", "'\\''")}'`;
async function remote(command) {
	const { stdout } = await run("/bin/sh", ["-c", `${SH} ${quote(command)}`], {
		timeout: 60_000,
	});
	return stdout;
}
async function owner(...args) {
	return JSON.parse(await remote(`${OWNER} ${args.join(" ")}`));
}

const byId = (entries) =>
	Object.fromEntries(toolResults(entries).map((m) => [m.toolCallId, m]));
const text = (message) =>
	(message?.content ?? [])
		.map((part) => (part.type === "text" ? part.text : ""))
		.join("");

async function withQualify(body) {
	const profile = await mkdtemp("/var/tmp/oqt-qualify-");
	try {
		await withGuard(body, {
			link: { binary: process.env.OQT_LINK, profile },
		});
	} finally {
		await rm(profile, { recursive: true, force: true });
	}
}

let counter = 0;
const scratch = () => `q-${Date.now()}-${++counter}`;
const m = (args) => ({ ...args, machine: MACHINE });

/** Wait until a remote file exists, through the link itself when no shell
 * reaches the destination (a guarded read-only call). */
async function remoteExists(path, deadline = 20_000) {
	const end = Date.now() + deadline;
	while (Date.now() < end) {
		if (SH) {
			if (
				(await remote(`test -e ${ROOT}/${path} && echo yes || true`)).includes(
					"yes",
				)
			)
				return true;
		} else return true;
		await sleep(200);
	}
	return false;
}

test(
	"guarded read/write/edit/bash run on the named machine",
	{ timeout: 120_000, skip: !enabled("tools") },
	async () => {
		await withQualify(async ({ workspace, faux, conversation, requests }) => {
			const dir = scratch();
			faux.setResponses([
				call("write", m({ path: `${dir}/r.txt`, content: "before\n" }), "w"),
				call(
					"edit",
					m({ path: `${dir}/r.txt`, oldText: "before", newText: "after" }),
					"e",
				),
				call("read", m({ path: `${dir}/r.txt` }), "r"),
				call("bash", m({ command: `pwd; cat ${dir}/r.txt; uname -s` }), "b"),
				call("bash", { command: "uname -s" }, "l"),
				fauxAssistantMessage("done"),
			]);
			const started = Date.now();
			const id = byId(await turn(conversation));
			const results = ["w", "e", "r", "b", "l"].map((key) => id[key]);
			note({
				scenario: "tools",
				ms: Date.now() - started,
				errors: results.map((r) => r?.isError),
			});
			assert.ok(
				results.every((r) => r && !r.isError),
				JSON.stringify(results),
			);
			assert.match(text(id.r), /after/);
			assert.match(text(id.b), new RegExp(process.env.HL_Q_REMOTE_OS));
			assert.match(text(id.b), new RegExp(`\\[machine ${MACHINE} · cwd `));
			assert.match(text(id.l), /\[machine local · cwd /);
			note({ scenario: "tools", remote: text(id.b).split("\n").slice(0, 4) });
			if (SH) assert.equal(await remote(`cat ${ROOT}/${dir}/r.txt`), "after\n");
			assert.equal(
				requests.filter(
					(r) => r.resource === `confined workspace on ${MACHINE}`,
				).length,
				3,
			);
			void workspace;
		});
	},
);

test(
	"unknown machine fails before any effect with no local fallback",
	{ timeout: 60_000, skip: !enabled("routing") },
	async () => {
		await withQualify(async ({ workspace, faux, conversation, requests }) => {
			faux.setResponses([
				call("write", { path: "u.txt", content: "x", machine: "nowhere" }, "u"),
				fauxAssistantMessage("done"),
			]);
			const result = byId(await turn(conversation)).u;
			assert.ok(result.isError);
			assert.match(text(result), /Unknown machine/);
			assert.equal(requests.length, 0);
			const { existsSync } = await import("node:fs");
			assert.equal(existsSync(`${workspace}/u.txt`), false);
			note({ scenario: "routing", result: text(result).slice(0, 120) });
		});
	},
);

test(
	"link cut after the effect: unknown, one effect, never replayed",
	{ timeout: 120_000, skip: !enabled("cut") },
	async () => {
		await withQualify(async ({ faux, conversation, guard }) => {
			const dir = scratch();
			faux.setResponses([
				call(
					"bash",
					m({
						command: `mkdir -p ${dir}; echo mark >> ${dir}/marks.txt; sleep 20`,
					}),
					"k",
				),
				fauxAssistantMessage("done"),
			]);
			const pending = turn(conversation);
			if (SH) assert.ok(await remoteExists(`${dir}/marks.txt`));
			else await sleep(4000);
			guard.link.child.kill("SIGKILL");
			const result = byId(await pending).k;
			assert.ok(result.isError);
			assert.match(text(result), /uncertain|unknown/i);
			note({ scenario: "cut", result: text(result).slice(0, 160) });
			if (SH) {
				await sleep(2000);
				assert.equal(await remote(`cat ${ROOT}/${dir}/marks.txt`), "mark\n");
			}
		});
		// Inspect from a fresh guard: the marker count stays one.
		if (!SH)
			await withQualify(async ({ faux, conversation }) => {
				faux.setResponses([
					call(
						"bash",
						m({
							command:
								"cat q-*/marks.txt 2>/dev/null | tail -1; ls -d q-* | tail -1",
						}),
						"c",
					),
					fauxAssistantMessage("done"),
				]);
				note({
					scenario: "cut-inspect",
					output: text(byId(await turn(conversation)).c).slice(0, 300),
				});
			});
	},
);

test(
	"operator revoke freezes locally, fences the destination, confirms the stop",
	{ timeout: 120_000, skip: !enabled("revoke") },
	async () => {
		await withQualify(async ({ faux, conversation, operator, session }) => {
			const dir = scratch();
			faux.setResponses([
				call(
					"bash",
					m({
						command: `mkdir -p ${dir}; echo go > ${dir}/s.txt; sleep 25; echo late >> ${dir}/s.txt`,
					}),
					"s",
				),
				fauxAssistantMessage("done"),
			]);
			const pending = turn(conversation);
			if (SH) assert.ok(await remoteExists(`${dir}/s.txt`));
			else await sleep(4000);
			const began = Date.now();
			await operator.call("revoke_all", { session });
			const revokedMs = Date.now() - began;
			const result = byId(await pending).s;
			note({
				scenario: "revoke",
				revokedMs,
				result: text(result).slice(0, 160),
			});
			assert.ok(result.isError);
			if (SH) {
				await sleep(27_000);
				assert.equal(await remote(`cat ${ROOT}/${dir}/s.txt`), "go\n");
			}
			if (OWNER) {
				const recent = await owner("recent", "--limit", "3");
				note({ scenario: "revoke", receipt: recent[0] });
				assert.equal(recent[0].state, "fenced");
				assert.equal(recent[0].stop, "confirmed");
			}
		});
	},
);

test(
	"code-mode fan-out across local and the named machine, one receipt per call",
	{ timeout: 120_000, skip: !enabled("code") },
	async () => {
		await withQualify(async ({ workspace, faux, conversation }) => {
			const dir = scratch();
			const cell = `
const results = await Promise.allSettled([
  tools.write({ path: "${dir}/a.txt", content: "A", machine: "${MACHINE}" }),
  tools.bash({ command: "uname -s", machine: "${MACHINE}" }),
  tools.write({ path: "b.txt", content: "B" }),
  tools.read({ path: "${dir}/missing.txt", machine: "${MACHINE}" }),
]);
return results.map((r) => r.status);`;
			faux.setResponses([
				call("code", { code: cell }, "c"),
				fauxAssistantMessage("done"),
			]);
			const output = text(byId(await turn(conversation)).c);
			note({ scenario: "code", output: output.split("\n") });
			assert.match(
				output,
				/Result: \["fulfilled","fulfilled","fulfilled","rejected"\]/,
			);
			assert.match(output, new RegExp(`#0 write on ${MACHINE}: ok`));
			assert.match(output, new RegExp(`#1 bash on ${MACHINE}: ok`));
			assert.match(output, /#2 write on local: ok/);
			assert.match(output, new RegExp(`#3 read on ${MACHINE}: error`));
			const { readFile } = await import("node:fs/promises");
			assert.equal(await readFile(`${workspace}/b.txt`, "utf8"), "B");
			if (SH) assert.equal(await remote(`cat ${ROOT}/${dir}/a.txt`), "A");
		});
	},
);

test(
	"origin partition: no renewals, the fixed lease expires and the generation stops",
	{ timeout: 120_000, skip: !enabled("partition") },
	async () => {
		await withQualify(async ({ faux, conversation, guard }) => {
			const dir = scratch();
			faux.setResponses([
				call(
					"bash",
					m({
						command: `mkdir -p ${dir}; echo go > ${dir}/p.txt; sleep 40; echo late >> ${dir}/p.txt`,
					}),
					"p",
				),
				fauxAssistantMessage("done"),
			]);
			const pending = turn(conversation);
			if (SH) assert.ok(await remoteExists(`${dir}/p.txt`));
			else await sleep(4000);
			// Blackhole the origin side: the adapter stops reading and renewing
			// while its TCP connections stay open.
			guard.link.child.kill("SIGSTOP");
			const stopped = Date.now();
			let receipt;
			if (OWNER)
				for (let i = 0; i < 60 && !receipt; i++) {
					await sleep(500);
					const recent = await owner("recent", "--limit", "1");
					if (recent[0]?.state !== "running") receipt = recent[0];
				}
			// Without destination access hold the partition past the lease; the
			// destination owner reads the expired receipt on its side.
			else await sleep(13_000);
			const expiredMs = Date.now() - stopped;
			guard.link.child.kill("SIGCONT");
			const result = byId(await pending).p;
			note({
				scenario: "partition",
				expiredMs,
				receipt,
				result: text(result).slice(0, 160),
			});
			assert.ok(result.isError);
			if (OWNER) {
				assert.ok(
					["expired", "unknown"].includes(receipt?.state),
					JSON.stringify(receipt),
				);
				assert.equal(receipt.stop, "confirmed");
				assert.ok(expiredMs < 16_000, `${expiredMs}`);
			}
			if (SH) {
				await sleep(2000);
				assert.equal(await remote(`cat ${ROOT}/${dir}/p.txt`), "go\n");
			}
		});
	},
);

test(
	"destination suspend: a stale lease is dead before any new I/O on resume",
	{
		timeout: 120_000,
		skip: !enabled(
			"suspend",
			Boolean(SH && PATTERN && OWNER) ||
				Boolean(process.env.HL_Q_SUSPEND_EXTERNAL),
		),
	},
	async () => {
		await withQualify(async ({ faux, conversation }) => {
			const dir = scratch();
			faux.setResponses([
				call(
					"bash",
					m({
						command: `mkdir -p ${dir}; echo go > ${dir}/z.txt; sleep 30; echo late >> ${dir}/z.txt`,
					}),
					"z",
				),
				fauxAssistantMessage("done"),
			]);
			const pending = turn(conversation);
			if (process.env.HL_Q_SUSPEND_EXTERNAL) {
				// The destination owner suspends its host when z.txt appears and
				// checks the receipt and z.txt on its side.
				const result = byId(await pending).z;
				note({
					scenario: "suspend-external",
					result: text(result).slice(0, 160),
				});
				assert.ok(result.isError);
				return;
			}
			assert.ok(await remoteExists(`${dir}/z.txt`));
			await remote(`pkill -STOP -f ${quote(PATTERN)}`);
			await sleep(13_000);
			await remote(`pkill -CONT -f ${quote(PATTERN)}`);
			const result = byId(await pending).z;
			const recent = await owner("recent", "--limit", "1");
			note({
				scenario: "suspend",
				receipt: recent[0],
				result: text(result).slice(0, 160),
			});
			assert.ok(result.isError);
			assert.ok(
				["expired", "unknown"].includes(recent[0].state),
				JSON.stringify(recent[0]),
			);
			assert.equal(recent[0].stop, "confirmed");
			await sleep(20_000);
			assert.equal(await remote(`cat ${ROOT}/${dir}/z.txt`), "go\n");
		});
	},
);

test(
	"destination-owner device revoke during a call stops it and refuses the device",
	{ timeout: 120_000, skip: !enabled("device-revoke") },
	async () => {
		await withQualify(async ({ faux, conversation }) => {
			const dir = scratch();
			faux.setResponses([
				call(
					"bash",
					m({
						command: `mkdir -p ${dir}; echo go > ${dir}/d.txt; sleep 25; echo late >> ${dir}/d.txt`,
					}),
					"d",
				),
				fauxAssistantMessage("done"),
			]);
			const pending = turn(conversation);
			if (SH && OWNER) {
				assert.ok(await remoteExists(`${dir}/d.txt`));
				const devices = await owner("devices");
				const device = devices.devices.find((d) => !d.revoked);
				const revoked = await owner("revoke", "--device", device.device_id);
				note({ scenario: "device-revoke", revoked });
				assert.equal(revoked.confirmed, true);
			}
			// Without a destination shell, the destination owner revokes on its
			// own side when the call starts (see the qualification runner).
			const result = byId(await pending).d;
			note({ scenario: "device-revoke", result: text(result).slice(0, 160) });
			assert.ok(result.isError);
			faux.setResponses([
				call("bash", m({ command: "echo again" }), "a"),
				fauxAssistantMessage("done"),
			]);
			const again = byId(await turn(conversation, "after-revoke")).a;
			note({ scenario: "device-revoke", again: text(again).slice(0, 160) });
			assert.ok(again.isError);
			assert.match(text(again), /not started|not_started|offline|refused/i);
		});
	},
);

test(
	"ambiguous destination stop freezes that machine with attention",
	{
		timeout: 120_000,
		skip: !enabled("ambiguous", process.env.HL_Q_REMOTE_OS === "Darwin"),
	},
	async () => {
		await withQualify(async ({ faux, conversation }) => {
			const dir = scratch();
			// The destination owner's watcher removes this generation's launch
			// markers when amb.txt appears, so the reap cannot confirm the stop.
			faux.setResponses([
				call(
					"bash",
					m({ command: `mkdir -p ${dir}; echo go > ${dir}/amb.txt; sleep 8` }),
					"x",
				),
				call("bash", m({ command: "echo next" }), "n"),
				fauxAssistantMessage("done"),
			]);
			const id = byId(await turn(conversation));
			note({
				scenario: "ambiguous",
				first: text(id.x).slice(0, 200),
				next: text(id.n).slice(0, 200),
			});
			assert.ok(id.x.isError);
			assert.match(text(id.x), /uncertain|unknown/i);
			assert.ok(id.n.isError);
			assert.match(text(id.n), /attention|refused|not started/i);
		});
	},
);

test(
	"destination confinement: outside files, symlink escapes and network are unreachable",
	{
		timeout: 120_000,
		skip: !enabled("confinement", Boolean(process.env.HL_Q_SENTINEL)),
	},
	async () => {
		// HL_Q_SENTINEL: absolute destination path outside the binding root
		// holding HL_Q_SENTINEL_TEXT; HL_Q_SENTINEL_LINK: a workspace-relative
		// symlink to it, both prepared by the destination owner.
		const sentinel = process.env.HL_Q_SENTINEL;
		const secret = process.env.HL_Q_SENTINEL_TEXT;
		const link = process.env.HL_Q_SENTINEL_LINK;
		await withQualify(async ({ faux, conversation }) => {
			faux.setResponses([
				call("read", m({ path: link }), "l"),
				call("read", m({ path: sentinel }), "a"),
				call(
					"bash",
					m({
						command: `cat ${sentinel} 2>&1 | head -2; cat ${link} 2>&1 | head -2; (curl -m 4 -s https://example.com >/dev/null 2>&1 || nc -z -w 3 1.1.1.1 443 2>/dev/null) && echo NET_OPEN || echo NET_CLOSED; ls -d ~/.ssh 2>&1 | head -1; echo HOME=$HOME`,
					}),
					"b",
				),
				fauxAssistantMessage("done"),
			]);
			const id = byId(await turn(conversation));
			const shell = text(id.b);
			note({
				scenario: "confinement",
				readLink: text(id.l).slice(0, 160),
				readAbsolute: text(id.a).slice(0, 160),
				shell: shell.split("\n").slice(0, 8),
			});
			for (const key of ["l", "a", "b"])
				assert.ok(
					!text(id[key]).includes(secret),
					`${key} leaked the sentinel`,
				);
			assert.match(shell, /NET_CLOSED/);
		});
	},
);

test.after(() => {
	if (process.env.HL_Q_TRACE)
		import("node:fs").then(({ writeFileSync }) =>
			writeFileSync(
				process.env.HL_Q_TRACE,
				`${JSON.stringify(trace, null, 1)}\n`,
			),
		);
});

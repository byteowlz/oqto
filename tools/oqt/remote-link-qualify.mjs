// Link-level physical qualification of remote hands (ADR-0086 "Acceptance"):
// scenarios that need two concurrent calls or two guard incarnations, driven
// through the real oqt_link adapter (as the guard drives it) and upstream
// pi-env clients. Machine specifics come only from the environment:
//   OQT_PI_SOURCE, OQT_LINK, OQT_HANDS_LINK_CONFIG, OQT_HANDS_LINK_STATE,
//   HL_Q_MACHINE, optional HL_Q_REMOTE_SH + HL_Q_OWNER (destination receipts).
// Usage: node --experimental-strip-types --import <pi>/.../source-resolver.ts
//        tools/oqt/remote-link-qualify.mjs [claim-cut|flood|takeover ...]
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { promisify } from "node:util";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import { Connection, RemoteExecutionEnv } from "@earendil-works/pi-env";
import { HandsLink, remoteCallId, remoteDigest } from "./hands-link.ts";

const run = promisify(execFile);
const MACHINE = process.env.HL_Q_MACHINE;
const quote = (text) => `'${text.replaceAll("'", "'\\''")}'`;
const owner = async (...args) =>
	process.env.HL_Q_REMOTE_SH && process.env.HL_Q_OWNER
		? JSON.parse(
				(
					await run("/bin/sh", [
						"-c",
						`${process.env.HL_Q_REMOTE_SH} ${quote(`${process.env.HL_Q_OWNER} ${args.join(" ")}`)}`,
					])
				).stdout,
			)
		: undefined;
const log = (event) => console.log(`TRACE ${JSON.stringify(event)}`);
const temporary = [];
const scratch = () => {
	const dir = mkdtempSync("/var/tmp/oqt-link-q-");
	temporary.push(dir);
	return dir;
};
const proxy = join(scratch(), "proxy.mjs");
// pi-env's Connection spawns this with `serve --token T`; it attaches the
// one-use local token and pipes bytes. STALL=1 stops reading the daemon.
writeFileSync(
	proxy,
	`import { connect } from "node:net";
const [socketPath, callId, attach, , , token] = process.argv.slice(2);
const socket = connect(socketPath, () => {
  socket.write(JSON.stringify({ call_id: callId, attach, sync_token: token }) + "\\n");
  process.stdin.pipe(socket);
  if (process.env.STALL === "1") {
    let seen = 0;
    socket.on("data", (chunk) => {
      if (seen < 4096) process.stdout.write(chunk);
      seen += chunk.length;
      if (seen > 4096) socket.pause();
    });
  } else socket.pipe(process.stdout);
});
socket.on("close", () => process.exit(0));
socket.on("error", () => process.exit(1));
`,
);

async function open(profile) {
	return HandsLink.open({
		binary: process.env.OQT_LINK,
		dataSocket: join(profile, "d.sock"),
		profile,
	});
}

let n = 0;
async function claim(link, tool = "bash") {
	const id = remoteCallId(link.namespace, `link-qualify:${Date.now()}:${++n}`);
	const reply = await link.claim(
		MACHINE,
		id,
		remoteDigest(MACHINE, "q", tool, { n }),
		tool,
	);
	return { id, reply };
}

function env(link, call, reply, stall = false) {
	const connection = new Connection({
		command: [
			"/usr/bin/env",
			`STALL=${stall ? 1 : 0}`,
			process.execPath,
			proxy,
			link.dataSocket,
			call,
			reply.attach,
		],
	});
	return {
		connection,
		env: new RemoteExecutionEnv({ connection, id: call, cwd: reply.cwd }),
	};
}

const scenarios = {
	/** Claim persisted, link killed before attach: nothing can run. */
	async "claim-cut"() {
		const profile = scratch();
		const link = await open(profile);
		const { id, reply } = await claim(link);
		assert.equal(reply.status, "claimed");
		link.child.kill("SIGKILL");
		await sleep(3000);
		const receipt = (await owner("recent", "--limit", "8"))?.find(
			(r) => r.call_id === id,
		);
		log({ scenario: "claim-cut", receipt });
		if (receipt) {
			assert.ok(["unknown", "expired"].includes(receipt.state));
			assert.equal(receipt.stop, "not_started");
			assert.equal(receipt.bytes_in, 0);
		}
	},
	/** One stream flooded with a stalled reader while a sibling is cancelled. */
	async flood() {
		const profile = scratch();
		const link = await open(profile);
		const x = await claim(link);
		const y = await claim(link);
		assert.equal(x.reply.status, "claimed");
		assert.equal(y.reply.status, "claimed");
		const flood = env(link, x.id, x.reply, true);
		const flooding = flood.env
			.exec("yes flood | head -c 400000000", {}, BACKGROUND_CONTEXT)
			.catch((error) => error);
		const quiet = env(link, y.id, y.reply);
		const sleeping = quiet.env
			.exec("sleep 30", {}, BACKGROUND_CONTEXT)
			.catch((error) => error);
		await sleep(3000);
		let started = Date.now();
		const cancelled = await link.cancel(y.id);
		const cancelMs = Date.now() - started;
		started = Date.now();
		const stopped = await link.cancel(x.id);
		const floodCancelMs = Date.now() - started;
		log({
			scenario: "flood",
			cancelMs,
			floodCancelMs,
			quiet: cancelled.receipt,
			flood: stopped.receipt,
		});
		assert.equal(cancelled.receipt?.state, "cancelled");
		assert.equal(cancelled.receipt?.stop, "confirmed");
		assert.ok(cancelMs < 3000, `${cancelMs}`);
		assert.equal(stopped.receipt?.stop, "confirmed");
		// Stalled reader: at most a few credit windows left the destination.
		assert.ok(
			stopped.receipt.bytes_out < 4 * 1024 * 1024,
			`${stopped.receipt.bytes_out}`,
		);
		quiet.connection.close();
		flood.connection.close();
		await Promise.race([Promise.all([flooding, sleeping]), sleep(5000)]);
		await link.close();
	},
	/** A second guard incarnation takes over: the old one is fenced, no race. */
	async takeover() {
		const profile = scratch();
		const first = await open(profile);
		const a = await claim(first);
		assert.equal(a.reply.status, "claimed");
		const old = env(first, a.id, a.reply);
		const running = old.env.exec("sleep 30", {}, BACKGROUND_CONTEXT).then(
			(value) => ({
				ok: value.ok,
				error: value.ok ? undefined : value.error?.code,
			}),
			(error) => ({ ok: false, error: String(error) }),
		);
		await sleep(2000);
		// Same profile, same persisted controller state: epoch + 1.
		const second = await open(profile);
		const probe = await second.probe(MACHINE);
		assert.equal(probe.ok, true);
		const outcome = await Promise.race([
			running,
			sleep(15000).then(() => "still running"),
		]);
		const stale = await claim(first);
		const fresh = await claim(second);
		const receipt = (await owner("recent", "--limit", "8"))?.find(
			(r) => r.call_id === a.id,
		);
		log({
			scenario: "takeover",
			old: outcome,
			oldReceipt: receipt,
			staleClaim: stale.reply,
			freshClaim: fresh.reply.status,
		});
		assert.notEqual(outcome, "still running");
		assert.equal(outcome.ok, false);
		assert.notEqual(stale.reply.status, "claimed");
		assert.equal(fresh.reply.status, "claimed");
		if (receipt) assert.equal(receipt.state, "fenced");
		await second.release(fresh.id);
		old.connection.close();
		await first.close();
		await second.close();
	},
};

const chosen = process.argv.slice(2).length
	? process.argv.slice(2)
	: Object.keys(scenarios);
let failed = 0;
for (const name of chosen) {
	try {
		await scenarios[name]();
		console.log(`PASS ${name}`);
	} catch (error) {
		failed++;
		console.log(`FAIL ${name}: ${error?.stack ?? error}`);
	}
}
for (const dir of temporary) rmSync(dir, { recursive: true, force: true });
process.exit(failed ? 1 : 0);

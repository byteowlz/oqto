import assert from "node:assert/strict";
import {
	copyFile,
	mkdir,
	mkdtemp,
	readFile,
	rm,
	writeFile,
} from "node:fs/promises";
import { createRequire } from "node:module";
import { join } from "node:path";
import { test } from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import { pathToFileURL } from "node:url";
import { withGuard } from "./guard-fixture.mjs";
import { HandsClient } from "./hands-client.ts";
import { confirmReaped, DARWIN, HandsSupervisor } from "./hands-supervisor.ts";

const require = createRequire(join(process.env.OQT_PI_SOURCE, "package.json"));
const { BACKGROUND_CONTEXT: context } = await import(
	pathToFileURL(require.resolve("@earendil-works/chord/context"))
);

test(
	"guard supervisor confirms confined descendant stop before replacement and fences stale epochs",
	{ timeout: 15000 },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-stop-proof-");
		const workspace = join(root, "workspace");
		await mkdir(workspace);
		const daemon = join(root, "pi-env");
		await copyFile(process.env.OQT_ENV_DAEMON, daemon);
		const supervisor = await HandsSupervisor.open({
			workspace,
			daemon,
			launcher: process.env.OQT_HANDS,
			bpf: process.env.OQT_SECCOMP_BPF,
			protectedRoots: [],
		});
		const client = new HandsClient(supervisor.socketPath, workspace);
		const retained = new HandsClient(supervisor.socketPath, workspace);
		try {
			const oldEpoch = supervisor.epoch;
			const stale = client.use(oldEpoch);
			assert.equal(
				(await stale.writeFile("hello", "before", context)).ok,
				true,
			);
			const effect = stale.exec(
				"(while true; do echo tick >> heartbeat; sleep 0.05; done) & wait",
				{ timeout: 10000 },
				context,
			);
			for (let i = 0; i < 40; i++) {
				try {
					if ((await readFile(join(workspace, "heartbeat"))).length) break;
				} catch {}
				await sleep(25);
			}
			assert.ok((await readFile(join(workspace, "heartbeat"))).length > 0);
			await supervisor.stop();
			assert.equal(supervisor.children.size, 0);
			assert.throws(() => supervisor.epoch, /frozen/);
			const settled = await effect;
			assert.equal(
				settled.ok,
				false,
				"interrupted mutation must not be reported as success",
			);
			const before = await readFile(join(workspace, "heartbeat"), "utf8");
			await sleep(250);
			assert.equal(
				await readFile(join(workspace, "heartbeat"), "utf8"),
				before,
				"detached descendant survived stop",
			);
			supervisor.install();
			assert.notEqual(supervisor.epoch, oldEpoch);
			// A brain that keeps the old epoch cannot reconnect to fresh hands.
			const retainedEnv = retained.use(oldEpoch);
			assert.equal(
				(await retainedEnv.writeFile("stale-write", "bad", context)).ok,
				false,
			);
			await assert.rejects(readFile(join(workspace, "stale-write")), {
				code: "ENOENT",
			});
			assert.equal(
				(
					await client
						.use(supervisor.epoch)
						.writeFile("fresh-write", "ok", context)
				).ok,
				true,
			);
		} finally {
			client.close();
			retained.close();
			await supervisor.close();
			await rm(root, { recursive: true, force: true });
		}
	},
);

// macOS has no PID namespace. A confined command can call setsid, let its
// parent exit and be reparented to launchd: no process group or tree walk
// from the launcher finds it. The launch marker does.
const ESCAPE = `/usr/bin/perl -MPOSIX -e "if (fork() == 0) { POSIX::setsid(); if (fork() == 0) { exec '/bin/sh', '-c', 'while true; do echo tick >> escaped; sleep 0.05; done' } exit 0 } exit 0"`;
const MAC_ONLY =
	!DARWIN &&
	"macOS only: on Linux the bwrap PID namespace ends every descendant";

async function grows(file) {
	for (let i = 0; i < 80; i++) {
		try {
			if ((await readFile(file)).length) break;
		} catch {}
		await sleep(25);
	}
	const before = await readFile(file, "utf8");
	await sleep(250);
	return (await readFile(file, "utf8")) !== before;
}

async function macSupervisor(root) {
	const workspace = join(root, "workspace");
	await mkdir(workspace);
	const daemon = join(root, "pi-env");
	await copyFile(process.env.OQT_ENV_DAEMON, daemon);
	const supervisor = await HandsSupervisor.open({
		workspace,
		daemon,
		launcher: process.env.OQT_HANDS,
		protectedRoots: [],
	});
	return { supervisor, workspace };
}

test(
	"macOS stop finds a setsid-escaped descendant by its launch marker before fresh hands install",
	{ timeout: 20000, skip: MAC_ONLY },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-escape-proof-");
		const { supervisor, workspace } = await macSupervisor(root);
		const client = new HandsClient(supervisor.socketPath, workspace);
		try {
			const epoch = supervisor.epoch;
			const result = await client
				.use(epoch)
				.exec(ESCAPE, { timeout: 10 }, context);
			assert.equal(result.ok, true, JSON.stringify(result));
			const escaped = join(workspace, "escaped");
			assert.ok(await grows(escaped), "escaped descendant runs on");
			// The launcher would reap its own launch; kill it outright so only
			// the guard's marker scan stands between the escapee and new hands.
			for (const child of supervisor.children) child.kill("SIGKILL");
			assert.ok(await grows(escaped), "escapee outlives its launcher");
			await supervisor.replace();
			assert.notEqual(supervisor.epoch, epoch);
			assert.equal(
				await grows(escaped),
				false,
				"escapee stopped before install",
			);
		} finally {
			client.close();
			await supervisor.close();
			await rm(root, { recursive: true, force: true });
		}
	},
);

test(
	"macOS stop that cannot be confirmed keeps hands frozen and the Session needing attention",
	{ timeout: 30000, skip: MAC_ONLY },
	async () => {
		await withGuard(async ({ guardSession, operator, session, workspace }) => {
			const supervisor = guardSession.supervisor;
			const client = new HandsClient(guardSession.handsPath, workspace);
			try {
				const result = await client
					.use(supervisor.epoch)
					.exec(ESCAPE, { timeout: 10 }, context);
				assert.equal(result.ok, true, JSON.stringify(result));
				assert.ok(await grows(join(workspace, "escaped")));
				// Without its marker the guard cannot tell members from bystanders.
				// That is not "nothing left": revoke must fail closed.
				const deny = join(supervisor.directory, "marker-deny");
				await rm(deny);
				await assert.rejects(operator.call("revoke_all", { session }));
				assert.equal(supervisor.active, false, "no fresh hands");
				assert.throws(() => supervisor.epoch, /frozen/);
				assert.match(
					guardSession.state().attention ?? "",
					/not be confirmed stopped/,
				);
				await assert.rejects(
					guardSession.authority.ready(),
					"no new grants or calls",
				);
				// Restore the marker so the proof cleans up after itself.
				await writeFile(deny, "", { mode: 0o600 });
				await confirmReaped(process.env.OQT_HANDS, supervisor.directory);
				assert.equal(await grows(join(workspace, "escaped")), false);
			} finally {
				client.close();
			}
		});
	},
);

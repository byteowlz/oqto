import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { createRequire } from "node:module";
import { join } from "node:path";
import { test } from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import { pathToFileURL } from "node:url";
import { HandsClient } from "./hands-client.ts";
import { HandsSupervisor } from "./hands-supervisor.ts";

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

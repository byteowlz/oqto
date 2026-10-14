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
import { openCodingRuntime } from "./coding-runtime.ts";

const require = createRequire(join(process.env.OQT_PI_SOURCE, "package.json"));
const load = (name) => import(pathToFileURL(require.resolve(name)));
const { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } =
	await load("@earendil-works/pi-ai");
const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);

test(
	"composed native coding runtime revokes active hands, fences output, replaces and continues the same turn",
	{ timeout: 20000 },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-runtime-proof-");
		const workspace = join(root, "workspace"),
			authority = join(root, "authority");
		await mkdir(workspace);
		await mkdir(authority, { mode: 0o700 });
		const daemon = join(root, "pi-env");
		await copyFile(process.env.OQT_ENV_DAEMON, daemon);
		await writeFile(join(workspace, "private.txt"), "composed-secret-1962");
		const faux = fauxProvider(),
			models = createModels();
		models.setProvider(faux.provider);
		const call = (name, args, id) =>
			fauxAssistantMessage([fauxToolCall(name, args, { id })], {
				stopReason: "toolUse",
			});
		faux.setResponses([
			call(
				"bash",
				{
					command:
						"cat private.txt; (while true; do echo tick >> heartbeat; sleep 0.05; done) & wait",
					timeout: 10,
				},
				"active-shell",
			),
			call(
				"write",
				{ path: "after-revoke", content: "fresh epoch" },
				"fresh-write",
			),
			fauxAssistantMessage("done"),
		]);
		const seen = new Set(),
			controls = [];
		let runtime;
		try {
			runtime = await openCodingRuntime({
				databasePath: join(authority, "native.sqlite"),
				directory: join(authority, "permissions"),
				session: "runtime-public-session",
				workspace,
				launcher: process.env.OQT_HANDS,
				daemon,
				bpf: process.env.OQT_SECCOMP_BPF,
				protectedRoots: [authority],
				models,
				model: { provider: "faux", modelId: "faux-1" },
				policy: "ask",
				publish(requests) {
					for (const request of requests)
						if (!seen.has(request.id)) {
							seen.add(request.id);
							controls.push(
								(async () => {
									await runtime.authority.decide(request, true, "session");
									if (request.operation.arguments.tool === "bash") {
										for (let i = 0; i < 80; i++) {
											try {
												if (
													(await readFile(join(workspace, "heartbeat"))).length
												)
													break;
											} catch {}
											await sleep(25);
										}
										await runtime.authority.revokeAll();
									}
								})(),
							);
						}
				},
			});
			const turn = await runtime.conversation.submit(
				{ type: "input", content: "run coding", requestId: "runtime-turn" },
				context,
			);
			assert.equal((await turn.wait(context)).status, "done");
			await Promise.all(controls);
			assert.equal(seen.size, 2);
			assert.equal(
				await readFile(join(workspace, "after-revoke"), "utf8"),
				"fresh epoch",
			);
			const before = await readFile(join(workspace, "heartbeat"), "utf8");
			await sleep(150);
			assert.equal(
				await readFile(join(workspace, "heartbeat"), "utf8"),
				before,
			);
			const entries = (
				await runtime.conversation.entries({}, 1000, undefined, context)
			).items;
			assert.ok(!JSON.stringify(entries).includes("composed-secret-1962"));
			assert.ok(
				entries
					.flatMap((entry) => entry.model ?? [])
					.some(
						(message) =>
							message.toolCallId === "active-shell" && message.isError,
					),
			);
		} finally {
			await runtime?.harness.close(context);
			await runtime?.cleanup();
			await rm(root, { recursive: true, force: true });
		}
	},
);

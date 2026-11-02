// Synthetic model, real Durable conversation, actual extension tool, signed TLS
// peer CLI, destination owner CLI, confined Runner. No real Pi profile/history.
import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import {
	chmod,
	mkdir,
	mkdtemp,
	readFile,
	rm,
	writeFile,
} from "node:fs/promises";
import { createServer } from "node:net";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { promisify } from "node:util";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import {
	createModels,
	fauxAssistantMessage,
	fauxProvider,
	fauxToolCall,
} from "@earendil-works/pi-ai";
import {
	createRegistry,
	defineExtension,
	Harness,
} from "@earendil-works/pi-durable";
import { openNodeSqliteStorage } from "@earendil-works/pi-durable/storage/sqlite/node";
import piHands from "./index.ts";

const exec = promisify(execFile);

test(
	"a Durable Session executes the pi-hands tool through owner-approved remote Runner",
	{ timeout: 60000 },
	async () => {
		const runner = resolve(
			process.env.OQT_TEST_RUNNER_BINARY ?? "backend/target/debug/oqto-runner",
		);
		const bpf = resolve(
			process.env.OQT_HANDS_TEST_BPF ??
				"backend/target/oqt-prepared/bpf/default-x86_64.bpf",
		);
		const shim = resolve(
			process.env.OQTO_SANDBOX_SHIM_BIN ??
				"backend/target/debug/examples/oqt_shim",
		);
		const root = await mkdtemp("/var/tmp/pi-hands-native-");
		await chmod(root, 0o700);
		let child;
		let harness;
		const prior = {
			runner: process.env.PI_HANDS_RUNNER,
			identity: process.env.PI_HANDS_IDENTITY,
			environment: process.env.PI_HANDS_ENVIRONMENT,
		};
		try {
			const home = join(root, "home");
			const work = join(root, "work");
			const state = join(root, "state");
			await mkdir(join(home, "tmp"), { recursive: true });
			await mkdir(work);
			const config = join(root, "config.toml");
			await writeFile(
				config,
				`[local]\nsingle_user=true\nworkspace_dir=${JSON.stringify(work)}\n[local.linux_users]\nenabled=false\n[runner]\nrunner_id="hands-fixture"\n`,
			);
			const reserve = createServer();
			reserve.listen(0, "127.0.0.1");
			await once(reserve, "listening");
			const port = reserve.address().port;
			await new Promise((done) => reserve.close(done));
			const env = {
				PATH: "/usr/bin:/bin",
				HOME: home,
				USER: process.env.USER ?? "fixture",
				XDG_CONFIG_HOME: join(home, ".config"),
				TMPDIR: join(home, "tmp"),
				OQTO_SANDBOX_SHIM_BIN: shim,
			};
			child = spawn(
				runner,
				[
					"--config",
					config,
					"--no-sandbox",
					"--personal-listen",
					`127.0.0.1:${port}`,
					"--personal-origin",
					`https://127.0.0.1:${port}`,
					"--personal-state",
					state,
					"--pair-output",
					"json",
					"--personal-hands-bpf",
					bpf,
				],
				{ env, stdio: ["ignore", "pipe", "pipe"] },
			);
			let stderr = "";
			child.stderr.on("data", (chunk) => {
				stderr += chunk;
			});
			const lines = createInterface({ input: child.stdout });
			const [qr] = await once(lines, "line", {
				signal: AbortSignal.timeout(20_000),
			});
			lines.close();
			const qrPath = join(root, "ticket.json");
			const identity = join(root, "peer.json");
			await writeFile(qrPath, qr, { mode: 0o600 });
			await exec(
				runner,
				[
					"personal",
					"peer",
					"enroll",
					"--qr",
					qrPath,
					"--identity",
					identity,
					"--name",
					"native-session-fixture",
				],
				{ env },
			);
			process.env.PI_HANDS_RUNNER = runner;
			process.env.PI_HANDS_IDENTITY = identity;
			process.env.PI_HANDS_ENVIRONMENT = "linux";
			let registered;
			let shutdown;
			piHands({
				registerTool: (tool) => {
					registered = tool;
				},
				on: (_event, handler) => {
					shutdown = handler;
				},
			});
			assert.ok(registered);
			await assert.rejects(
				registered.execute(
					"bad",
					{ environment: "mac", command: "echo no" },
					undefined,
					undefined,
					{},
				),
				/no fallback/,
			);
			const registry = createRegistry();
			registry.install(
				defineExtension({
					name: "pi-hands-proof",
					tools: [
						{
							name: registered.name,
							description: registered.description,
							parameters: registered.parameters,
							async execute(args, api, context) {
								let approval;
								const result = await registered.execute(
									String(api.taskId),
									args,
									context.abortSignal,
									(update) => {
										approval = exec(
											runner,
											[
												"personal",
												"hands-approve",
												update.details.invocation,
												"--root",
												work,
												"--state",
												state,
											],
											{ env },
										);
									},
									{
										sessionManager: {
											getSessionId: () => "disposable-native-session",
										},
									},
								);
								await approval;
								return { output: result.content, details: result.details };
							},
						},
					],
				}),
			);
			const faux = fauxProvider();
			const models = createModels();
			models.setProvider(faux.provider);
			faux.setResponses([
				fauxAssistantMessage(
					[
						fauxToolCall(
							"hands_bash",
							{
								environment: "linux",
								command: "printf session-remote-ok > proof; cat proof",
							},
							{ id: "remote-call" },
						),
					],
					{ stopReason: "toolUse" },
				),
				fauxAssistantMessage("done"),
			]);
			harness = await Harness.open(
				await openNodeSqliteStorage(join(root, "session.sqlite")),
				{ models, registry },
				BACKGROUND_CONTEXT,
			);
			const conversation = await harness.root(BACKGROUND_CONTEXT, {
				agent: { cwd: work, model: { provider: "faux", modelId: "faux-1" } },
			});
			const submitted = await conversation.submit(
				{
					type: "input",
					content: "use the Linux environment",
					requestId: "remote-proof",
				},
				BACKGROUND_CONTEXT,
			);
			assert.equal(
				(await submitted.wait(BACKGROUND_CONTEXT)).status,
				"done",
				stderr,
			);
			const entries = (
				await conversation.entries({}, 100, undefined, BACKGROUND_CONTEXT)
			).items;
			const result = entries
				.flatMap((entry) => entry.model ?? [])
				.find((m) => m.role === "toolResult" && m.toolCallId === "remote-call");
			assert.ok(result && !result.isError, JSON.stringify(result));
			assert.ok(JSON.stringify(result).includes("session-remote-ok"));
			assert.equal(
				await readFile(join(work, "proof"), "utf8"),
				"session-remote-ok",
			);
			await shutdown();
		} finally {
			await harness?.close(BACKGROUND_CONTEXT);
			for (const [name, value] of [
				["PI_HANDS_RUNNER", prior.runner],
				["PI_HANDS_IDENTITY", prior.identity],
				["PI_HANDS_ENVIRONMENT", prior.environment],
			]) {
				if (value === undefined) delete process.env[name];
				else process.env[name] = value;
			}
			if (child && child.exitCode === null) {
				child.kill("SIGTERM");
				await once(child, "exit");
			}
			await rm(root, { recursive: true, force: true });
		}
	},
);

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import {
	chmod,
	mkdir,
	mkdtemp,
	readFile,
	rm,
	writeFile,
} from "node:fs/promises";
import { createRequire } from "node:module";
import { getPriority } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import { pathToFileURL } from "node:url";

const source = process.env.OQ_ADAPTIVE_PI_SOURCE;
const carrier = process.env.OQ_ADAPTIVE_CARRIER;
assert.ok(
	source && carrier,
	"Run the bounded run.sh with the qualified upstream source",
);
const require = createRequire(join(source, "package.json"));
const load = (name) => import(pathToFileURL(require.resolve(name)));
const { Harness, createRegistry, defineExtension, defineTool } = await load(
	"@earendil-works/pi-durable",
);
const { openNodeSqliteStorage } = await load(
	"@earendil-works/pi-durable/storage/sqlite/node",
);
const { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } =
	await load("@earendil-works/pi-ai");
const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);

class Pipe {
	constructor(role, directory) {
		this.pending = [];
		this.child = spawn(carrier, [role, directory], {
			stdio: ["pipe", "pipe", "inherit"],
		});
		this.lines = createInterface({ input: this.child.stdout });
		this.lines.on("line", (line) => {
			const pending = this.pending.shift();
			if (!pending) return;
			clearTimeout(pending.timer);
			try {
				pending.resolve(JSON.parse(line));
			} catch (error) {
				pending.reject(error);
			}
		});
		this.child.on("exit", () => {
			for (const pending of this.pending.splice(0)) {
				clearTimeout(pending.timer);
				pending.reject(new Error("carrier disconnected"));
			}
		});
	}
	call(command) {
		return new Promise((resolve, reject) => {
			const timer = setTimeout(
				() => reject(new Error("carrier response deadline")),
				5000,
			);
			this.pending.push({ resolve, reject, timer });
			this.child.stdin.write(`${JSON.stringify(command)}\n`);
		});
	}
	async close() {
		if (this.child.exitCode !== null) return;
		this.child.stdin.end();
		await new Promise((resolve) => {
			const timer = setTimeout(() => this.child.kill("SIGKILL"), 1000);
			this.child.once("exit", () => {
				clearTimeout(timer);
				resolve();
			});
		});
		this.lines.close();
	}
}

async function setup() {
	assert.equal(getPriority(), 19, "native lane must remain low priority");
	const status = await readFile("/proc/self/status", "utf8");
	const cpus = status.match(/^Cpus_allowed_list:\s*(.+)$/m)?.[1];
	assert.match(cpus, /^\d+$/, "native lane must use exactly one CPU");
	const directory = await mkdtemp("/var/tmp/oq-adaptive-native-");
	await chmod(directory, 0o700);
	await mkdir(join(directory, "workdir"));
	await mkdir(join(directory, "extra"));
	await writeFile(
		join(directory, "extra/token"),
		"confined extra fixture-token",
	);
	const faux = fauxProvider();
	const models = createModels();
	models.setProvider(faux.provider);
	const registry = createRegistry();
	const f = {
		directory,
		faux,
		models,
		registry,
		seen: [],
		reads: 0,
		tools: new Pipe("tools", directory),
		operator: new Pipe("operator", directory),
	};
	const gated = async (target, argumentsValue, api, callContext) => {
		const operation = {
			request_id: `fixture-public-session:store-one:${api.conversationId}:${api.taskId}`,
			target,
			arguments: argumentsValue,
		};
		for (;;) {
			callContext.abortSignal?.throwIfAborted();
			const reply = await f.tools.call({ command: "invoke", operation });
			assert.equal(reply.ok, true, reply.error);
			if (reply.result.status === "allowed") {
				f.reads = reply.result.fixture_reads;
				return { output: [{ type: "text", text: reply.result.text }] };
			}
			if (reply.result.admission.status === "denied")
				throw new Error("permission denied");
			assert.equal(
				reply.result.fixture_reads,
				f.reads,
				"pending call touched fixture history",
			);
			const request = reply.result.admission.value;
			if (!f.seen.some((old) => old.digest === request.digest))
				f.seen.push(request);
			await sleep(25, undefined, { signal: callContext.abortSignal });
		}
	};
	registry.install(
		defineExtension({
			name: "qualified-services",
			tools: [
				defineTool({
					name: "history_search",
					description:
						"Bounded fixture history via runner permission authority",
					parameters: Type.Object({
						scope: Type.Union([Type.Literal("own"), Type.Literal("account")]),
					}),
					replay: "safe",
					execute: (args, api, callContext) =>
						gated(
							{
								capability: "history_search",
								resource_id:
									args.scope === "own" ? "own-history" : "account-history",
							},
							{ query: "fixture-token" },
							api,
							callContext,
						),
				}),
				defineTool({
					name: "file_read",
					description: "Read a registered fixture file through confined hands",
					parameters: Type.Object({}),
					replay: "safe",
					execute: (_args, api, callContext) =>
						gated(
							{ capability: "file_read", resource_id: "extra-resource" },
							{ relative_path: "token" },
							api,
							callContext,
						),
				}),
			],
		}),
	);
	// Only the registered read-only operations are replay-safe. No effectful
	// command inherits this declaration or automatic retry behavior.
	f.open = async () => {
		const h = await Harness.open(
			await openNodeSqliteStorage(join(directory, "native.sqlite")),
			{ models, registry },
			context,
		);
		const root = await h.root(context, {
			agent: { model: { provider: "faux", modelId: "faux-1" } },
		});
		f.harness = h;
		f.root = root;
		return { h, root };
	};
	f.cleanup = async () => {
		await f.harness?.close(context);
		await f.tools.close();
		await f.operator.close();
		await rm(directory, { recursive: true, force: true });
	};
	return f;
}
const call = (scope, id) =>
	fauxAssistantMessage([fauxToolCall("history_search", { scope }, { id })], {
		stopReason: "toolUse",
	});
async function until(predicate) {
	const deadline = Date.now() + 5000;
	while (!predicate()) {
		assert.ok(Date.now() < deadline, "native observation deadline");
		await sleep(10);
	}
}
async function approve(f, request, allow = true) {
	const decision = await f.operator.call({
		command: "decide",
		id: request.operation.request_id,
		digest: request.digest,
		revision: request.revision,
		lifetime: "once",
		allow,
	});
	assert.equal(decision.ok, true, decision.error);
	return decision.result.request;
}

test(
	"real Durable: tool waits inside turn, operator approves, same brain resumes",
	{ timeout: 12000 },
	async () => {
		const f = await setup();
		try {
			f.faux.setResponses([
				call("own", "own-call"),
				fauxAssistantMessage("own answer"),
				call("account", "global-call"),
				fauxAssistantMessage("approved answer"),
			]);
			const { h, root } = await f.open();
			const pid = process.pid;
			const own = await root.submit(
				{
					type: "input",
					content: "search own history",
					requestId: "prompt-own",
				},
				context,
			);
			assert.equal((await own.wait(context)).status, "done");
			assert.equal(f.seen.length, 0);
			assert.equal(f.reads, 1);
			const submission = await root.submit(
				{
					type: "input",
					content: "search broader history",
					requestId: "prompt-one",
				},
				context,
			);
			await until(() => f.seen.length === 1);
			assert.equal(
				f.faux.state.callCount,
				3,
				"model ran again before permission decision",
			);
			const request = f.seen[0];
			assert.equal(request.state, "pending");
			const deniedControl = await f.tools.call({
				command: "decide",
				id: request.operation.request_id,
				digest: request.digest,
				revision: 1,
				lifetime: "once",
				allow: true,
			});
			assert.equal(
				deniedControl.ok,
				false,
				"tool channel exposed operator approval",
			);
			await approve(f, request);
			const settled = await submission.wait(context);
			assert.equal(settled.status, "done");
			assert.equal(process.pid, pid);
			assert.equal(f.faux.state.callCount, 4);
			const { items } = await root.entries({}, 1000, undefined, context);
			assert.ok(JSON.stringify(items).includes("broader fixture-token"));
			assert.equal(items.filter((e) => e.kind === "pi.tool-result").length, 2);
			assert.equal(f.reads, 2);
			assert.equal(h, f.harness);
		} finally {
			await f.cleanup();
		}
	},
);

test(
	"real Durable: pending request survives harness + permission carrier reopen",
	{ timeout: 12000 },
	async () => {
		const f = await setup();
		try {
			f.faux.setResponses([
				call("account", "recovery-call"),
				fauxAssistantMessage("recovered answer"),
			]);
			const first = await f.open();
			const submission = await first.root.submit(
				{
					type: "input",
					content: "search and recover",
					requestId: "prompt-recovery",
				},
				context,
			);
			await until(() => f.seen.length === 1);
			const request = f.seen[0];
			const conversationId = first.root.id;
			await first.h.close(context);
			f.harness = undefined;
			await f.tools.close();
			await f.operator.close();
			f.tools = new Pipe("tools", f.directory);
			f.operator = new Pipe("operator", f.directory);
			const inspection = await f.operator.call({
				command: "inspect",
				id: request.operation.request_id,
			});
			assert.equal(inspection.ok, true, inspection.error);
			assert.equal(inspection.result.request.digest, request.digest);
			assert.equal(inspection.result.request.state, "pending");
			const second = await f.open();
			assert.equal(second.root.id, conversationId);
			second.h.resume();
			await approve(f, request);
			const restored = await second.h.submission(submission.id, context);
			assert.ok(restored);
			const settled = await restored.wait(context);
			assert.equal(settled.status, "done");
			assert.equal(
				f.seen.length,
				1,
				"replayed read created another permission request",
			);
			assert.equal(
				f.faux.state.callCount,
				2,
				"recovery regenerated the original tool call",
			);
			const { items } = await second.root.entries({}, 1000, undefined, context);
			assert.equal(items.filter((e) => e.kind === "pi.tool-result").length, 1);
		} finally {
			await f.cleanup();
		}
	},
);

test(
	"real Durable: filesystem approval launches confined hands, revocation gates next read",
	{ timeout: 12000 },
	async () => {
		const f = await setup();
		try {
			const fileCall = (id) =>
				fauxAssistantMessage([fauxToolCall("file_read", {}, { id })], {
					stopReason: "toolUse",
				});
			f.faux.setResponses([
				fileCall("file-one"),
				fauxAssistantMessage("read answer"),
				fileCall("file-two"),
				fauxAssistantMessage("denied answer"),
			]);
			const { root } = await f.open();
			const pid = process.pid,
				conversationId = root.id;
			const first = await root.submit(
				{
					type: "input",
					content: "read approved external file",
					requestId: "file-prompt",
				},
				context,
			);
			await until(() => f.seen.length === 1);
			assert.equal(f.reads, 0);
			const decision = await approve(f, f.seen[0]);
			assert.equal((await first.wait(context)).status, "done");
			assert.equal(f.reads, 1);
			assert.equal(process.pid, pid);
			assert.equal(root.id, conversationId);
			const { items } = await root.entries({}, 1000, undefined, context);
			assert.ok(JSON.stringify(items).includes("confined extra fixture-token"));
			const revoked = await f.operator.call({
				command: "revoke",
				grant_id: decision.grant_id,
			});
			assert.equal(revoked.ok, true, revoked.error);
			const second = await root.submit(
				{
					type: "input",
					content: "try external file again",
					requestId: "file-prompt-two",
				},
				context,
			);
			await until(() => f.seen.length === 2);
			await approve(f, f.seen[1], false);
			assert.equal((await second.wait(context)).status, "done");
			assert.equal(f.reads, 1, "denial entered filesystem provider");
			const finalEntries = (await root.entries({}, 1000, undefined, context))
				.items;
			const results = finalEntries.filter((e) => e.kind === "pi.tool-result");
			assert.equal(results.length, 2);
			const deniedResult = results.find(
				(entry) => entry.model[0].toolCallId === "file-two",
			);
			assert.ok(deniedResult);
			assert.equal(deniedResult.model[0].isError, true);
		} finally {
			await f.cleanup();
		}
	},
);

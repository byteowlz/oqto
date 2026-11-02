/** host2 with the real guard and a confined brain (oqt_brain + bwrap), real
 * gated coding tools and guard-owned hands. Only the model provider is
 * scripted. Requires built confinement assets (see run-tests.sh). */
import assert from "node:assert/strict";
import {
	copyFile,
	mkdir,
	mkdtemp,
	readdir,
	readFile,
	readlink,
	rm,
	writeFile,
} from "node:fs/promises";
import { createServer, type Server } from "node:http";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { Guard } from "../experimental/guard.ts";
import { WireClient } from "../experimental/guard-wire.ts";
import { OwnerBroker } from "../experimental/owner-broker.ts";
import { connectClient } from "./client.ts";
import { Conversation, Session } from "./contract.ts";
import { guardLauncher } from "./guard-launcher.ts";
import { type OqtServer, startServer } from "./server.ts";

const assets = [
	"OQT_BRAIN",
	"OQT_HANDS",
	"OQT_SANDBOX_SHIM",
	"OQT_CARRIER",
	"OQT_ENV_DAEMON",
	"OQT_SECCOMP_BPF",
];
const missing = assets.filter((name) => !process.env[name]);
const snapshot = process.env.OQT_HOST2_SNAPSHOT;
const upstream = process.env.OQT_PI_UPSTREAM_ROOT;
const skip =
	missing.length > 0 || !snapshot || !upstream
		? `needs ${[...missing, "OQT_HOST2_SNAPSHOT"].join(", ")}`
		: false;

type Step = { tool: string; args: object } | { text: string };
let script: (request: {
	messages: { role: string; content: unknown }[];
}) => Step;
const requests: unknown[] = [];

let root: string;
let provider: Server;
let guard: Guard;
let server: OqtServer;
let operator: WireClient;
let decide: (row: { id: string }) => "allow" | "deny" = () => "allow";
const decided: { id: string; allow: boolean }[] = [];
let workspace: string;

function startProvider(): Promise<number> {
	provider = createServer((request, response) => {
		let body = "";
		request.on("data", (chunk) => {
			body += chunk;
		});
		request.on("end", () => {
			const parsed = JSON.parse(body);
			requests.push(parsed);
			const step = script(parsed);
			const delta =
				"tool" in step
					? {
							tool_calls: [
								{
									index: 0,
									id: `call_${requests.length}`,
									type: "function",
									function: {
										name: step.tool,
										arguments: JSON.stringify(step.args),
									},
								},
							],
						}
					: { content: step.text };
			const finish = "tool" in step ? "tool_calls" : "stop";
			response.writeHead(200, {
				"content-type": "text/event-stream",
				connection: "close",
			});
			for (const [data, reason] of [
				[delta, null],
				[{}, finish],
			] as const) {
				const chunk = {
					id: "fixture",
					object: "chat.completion.chunk",
					created: 1,
					model: "owner-model",
					choices: [{ index: 0, delta: data, finish_reason: reason }],
				};
				response.write(`data: ${JSON.stringify(chunk)}\n\n`);
			}
			response.end("data: [DONE]\n\n");
		});
	});
	return new Promise((resolve) =>
		provider.listen(0, "127.0.0.1", () =>
			resolve((provider.address() as { port: number }).port),
		),
	);
}

const toolResults = (request: { messages: { role: string }[] }) =>
	request.messages.filter((message) => message.role === "tool");
type View = {
	entries: {
		model?: { role: string; content: { type: string; text?: string }[] }[];
	}[];
};
const texts = (view: unknown) =>
	(view as View).entries.flatMap((entry) =>
		(entry.model ?? []).flatMap((message) =>
			message.role === "assistant"
				? message.content.flatMap((part) =>
						part.type === "text" ? [part.text] : [],
					)
				: [],
		),
	);
const until = async (check: () => boolean, label: string, ms = 60_000) => {
	const end = Date.now() + ms;
	while (!check()) {
		if (Date.now() > end) throw new Error(`timed out: ${label}`);
		await new Promise((resolve) => setTimeout(resolve, 25));
	}
};

before(async () => {
	if (skip) return;
	root = await mkdtemp("/var/tmp/oqt-host2-brain-");
	const owner = join(root, "owner");
	const agent = join(root, "agent");
	const serverDirectory = join(root, "server");
	workspace = join(root, "workspace");
	await mkdir(owner, { mode: 0o700 });
	await mkdir(agent, { mode: 0o700 });
	await mkdir(workspace);
	const port = await startProvider();
	const files: Record<string, unknown> = {
		"auth.json": { fixture: { type: "api_key", key: "fixture-owner-key" } },
		"settings.json": {
			defaultProvider: "fixture",
			defaultModel: "owner-model",
			defaultThinkingLevel: "off",
		},
		"models.json": {
			providers: {
				fixture: {
					api: "openai-completions",
					baseUrl: `http://127.0.0.1:${port}/v1`,
					models: [
						{
							id: "owner-model",
							name: "Owner model",
							reasoning: false,
							input: ["text"],
							cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
							contextWindow: 8192,
							maxTokens: 1024,
						},
					],
				},
			},
		},
	};
	for (const [name, value] of Object.entries(files))
		await writeFile(join(owner, name), JSON.stringify(value), { mode: 0o600 });
	const daemon = join(root, "pi-env");
	await copyFile(process.env.OQT_ENV_DAEMON as string, daemon);
	const experimental = join(
		snapshot as string,
		"packages/coding-agent/src/experimental",
	);
	guard = await Guard.open({
		authorityRoot: join(root, "authority"),
		protectedRoots: [owner],
		hands: {
			launcher: process.env.OQT_HANDS as string,
			daemon,
			bpf: process.env.OQT_SECCOMP_BPF as string,
		},
		broker: new OwnerBroker(owner),
		ceiling: {
			userConfig: join(root, "sandbox.toml"),
			home: root,
			hardDeny: [],
		},
		parent: root,
		brain: {
			launcher: process.env.OQT_BRAIN as string,
			node: process.execPath,
			bpf: process.env.OQT_SECCOMP_BPF as string,
			shim: process.env.OQT_SANDBOX_SHIM as string,
			ro: [snapshot as string, upstream as string, join(owner, "models.json")],
			rw: [join(serverDirectory, "sessions"), agent],
			forbidden: [join(owner, "auth.json")],
			env: {
				PI_EXPERIMENTAL: "1",
				PI_OFFLINE: "1",
				PI_CODING_AGENT_DIR: agent,
				OQT_MODE: "coding",
				NODE_OPTIONS: "--max-old-space-size=512",
				OQT_OWNER_DEFAULTS: JSON.stringify({
					provider: "fixture",
					model: "owner-model",
				}),
				OQT_OWNER_AGENT_DIR: owner,
				OQT_OWNER_MODELS: join(owner, "models.json"),
				...(process.env.OQT_DEBUG ? { OQT_DEBUG: "1" } : {}),
			},
			trustedSource: snapshot as string,
			experimental,
			log: join(root, "brain.log"),
		},
	});
	operator = await WireClient.connect(guard.operatorPath);
	type OperatorState = {
		sessions: {
			id: string;
			pending: {
				id: string;
				digest: string;
				revision: number;
				session: string;
			}[];
		}[];
	};
	operator.on("state", (data: unknown) => {
		const state = data as OperatorState;
		for (const session of state.sessions)
			for (const row of session.pending) {
				if (decided.some((entry) => entry.id === row.id)) continue;
				const allow = decide(row) === "allow";
				decided.push({ id: row.id, allow });
				void operator.call("decide", {
					id: row.id,
					digest: row.digest,
					revision: row.revision,
					session: row.session,
					allow,
					lifetime: "once",
				});
			}
	});
	await operator.call("subscribe");
	await mkdir(serverDirectory, { mode: 0o700 });
	server = await startServer({
		directory: serverDirectory,
		launch: guardLauncher(guard, {
			entry: join(
				snapshot as string,
				"packages/coding-agent/src/oqt-host2/brain-worker.ts",
			),
			workspace: () => workspace,
		}),
		connectTimeout: 60_000,
	});
});

after(async () => {
	if (skip) return;
	await server?.close();
	operator?.close();
	await guard?.close();
	provider?.close();
	// Keep the private brain log for diagnosis (synthetic fixture data only).
	await copyFile(join(root, "brain.log"), "/tmp/oqt-host2-brain.log").catch(
		() => {},
	);
	await rm(root, { recursive: true, force: true });
});

test(
	"confined brain: gated tool runs in hands only after operator approval",
	{ skip },
	async () => {
		decide = () => "allow";
		script = (request) =>
			toolResults(request).length === 0
				? {
						tool: "bash",
						args: { command: "echo confined-ok > proof.txt; echo confined-ok" },
					}
				: { text: "brain-done" };
		const client = await connectClient(server.socketPath, server.token);
		try {
			const session = await client.host.get(Session, { session: "B1" });
			const conversation = await client.host.get(Conversation, {
				session: "B1",
				conversation: session.root.value,
			});
			if (conversation.variant !== "participant")
				throw new Error("expected participant");
			await conversation.prompt({ text: "write the proof file" });
			await until(
				() => texts(conversation.view.value).includes("brain-done"),
				"brain finished the turn",
			);
			const seen = JSON.stringify(
				requests.map((request) => toolResults(request as never)),
			);
			const proof = await readFile(join(workspace, "proof.txt"), "utf8").catch(
				(error) => `missing (${error.code}); tool results: ${seen}`,
			);
			assert.equal(proof.trim(), "confined-ok");
			assert.equal(decided.length, 1, "exactly one operator decision");
			assert.ok(decided[0]?.allow);
			// The launcher stays in the host namespace by design; the brain itself is
			// the node process running brain-worker.ts, which must be namespaced.
			const ours = await readlink("/proc/self/ns/pid");
			const brains: string[] = [];
			for (const entry of await readdir("/proc")) {
				if (!/^\d+$/.test(entry)) continue;
				const argv = (
					await readFile(`/proc/${entry}/cmdline`, "utf8").catch(() => "")
				).split("\0");
				// Only the node process itself; bwrap carries the same arguments outside.
				if (
					!/(^|\/)node$/.test(argv[0] ?? "") ||
					!argv.some((arg) => arg.endsWith("oqt-host2/brain-worker.ts"))
				)
					continue;
				if (!argv.some((arg) => arg.startsWith(snapshot as string))) continue;
				brains.push(
					await readlink(`/proc/${entry}/ns/pid`).catch(() => "unreadable"),
				);
			}
			assert.ok(brains.length >= 1, "a live brain process exists");
			assert.ok(
				brains.every((namespace) => namespace !== ours),
				"every brain runs in its own PID namespace",
			);
			session.release();
			conversation.release();
		} finally {
			await client.close();
		}
	},
);

test(
	"confined brain: denied call has no effect and the model sees a denial",
	{ skip },
	async () => {
		decide = () => "deny";
		const before = decided.length;
		script = (request) =>
			toolResults(request).length === 0
				? {
						tool: "bash",
						args: { command: "echo must-not-exist > denied.txt" },
					}
				: { text: "denial-seen" };
		const client = await connectClient(server.socketPath, server.token);
		try {
			const session = await client.host.get(Session, { session: "B2" });
			const conversation = await client.host.get(Conversation, {
				session: "B2",
				conversation: session.root.value,
			});
			if (conversation.variant !== "participant")
				throw new Error("expected participant");
			await conversation.prompt({ text: "try the denied write" });
			await until(
				() => texts(conversation.view.value).includes("denial-seen"),
				"brain saw the denial",
			);
			await assert.rejects(
				readFile(join(workspace, "denied.txt")),
				"denied effect never ran",
			);
			assert.equal(decided.length - before, 1);
			assert.equal(decided.at(-1)?.allow, false);
			const lastToolResult = JSON.stringify(
				toolResults(requests.at(-1) as never),
			);
			assert.match(lastToolResult, /denied|declined|not approved/i);
			session.release();
			conversation.release();
		} finally {
			await client.close();
		}
	},
);

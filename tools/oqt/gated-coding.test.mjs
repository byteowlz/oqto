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
import { pathToFileURL } from "node:url";
import { CodingAuthority } from "./coding-authority.ts";
import { confinedCoding } from "./confined-coding.ts";

const require = createRequire(join(process.env.OQT_PI_SOURCE, "package.json"));
const load = (name) => import(pathToFileURL(require.resolve(name)));
const { Harness } = await load("@earendil-works/pi-durable");
const { openNodeSqliteStorage } = await load(
	"@earendil-works/pi-durable/storage/sqlite/node",
);
const { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } =
	await load("@earendil-works/pi-ai");
const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);

async function fixture(run, policy = "ask") {
	const root = await mkdtemp("/var/tmp/oqt-gated-proof-");
	const workspace = join(root, "workspace"),
		ledger = join(root, "ledger");
	await mkdir(workspace);
	await mkdir(ledger, { mode: 0o700 });
	const daemon = join(root, "pi-env");
	await copyFile(process.env.OQT_ENV_DAEMON, daemon);
	const spec = join(root, "hands.json");
	await writeFile(
		spec,
		JSON.stringify({
			workspace,
			daemon,
			bpf: process.env.OQT_SECCOMP_BPF,
			protected_roots: [ledger],
		}),
		{ mode: 0o600 },
	);
	const session = "coding-public-session";
	const binding = {
		account_id: "local-owner",
		principal_id: "local-principal",
		workspace_id: "workspace",
		work_directory_id: "workdir",
		session_id: session,
		audience: "local-owner-only",
	};
	const requests = [],
		seen = new Set(),
		decisions = [];
	let onRequest = (request) => authority.decide(request, true, "once");
	const authority = new CodingAuthority(
		session,
		ledger,
		binding,
		(pending) => {
			for (const request of pending)
				if (!seen.has(request.id)) {
					seen.add(request.id);
					requests.push(request);
					decisions.push(Promise.resolve().then(() => onRequest(request)));
				}
		},
		policy,
	);
	const coding = confinedCoding({
		workspace,
		launcher: process.env.OQT_HANDS,
		spec,
		gate: authority,
		publicSession: session,
	});
	const faux = fauxProvider(),
		models = createModels();
	models.setProvider(faux.provider);
	let harness;
	try {
		harness = await Harness.open(
			await openNodeSqliteStorage(join(ledger, "native.sqlite")),
			{ models, registry: coding.registry, env: coding.env },
			context,
		);
		const conversation = await harness.root(context, {
			agent: { model: { provider: "faux", modelId: "faux-1" }, cwd: workspace },
		});
		await run({
			root,
			workspace,
			authority,
			coding,
			faux,
			conversation,
			requests,
			setDecision: (handler) => {
				onRequest = handler;
			},
		});
		await Promise.all(decisions);
	} finally {
		await harness?.close(context);
		await coding.cleanup(context);
		await authority.close();
		await rm(root, { recursive: true, force: true });
	}
}
const call = (name, args, id) =>
	fauxAssistantMessage([fauxToolCall(name, args, { id })], {
		stopReason: "toolUse",
	});
async function turn(conversation) {
	const handle = await conversation.submit(
		{ type: "input", content: "code", requestId: "gated-turn" },
		context,
	);
	assert.equal((await handle.wait(context)).status, "done");
	return (await conversation.entries({}, 1000, undefined, context)).items;
}

test(
	"native coding waits for exact ledger approvals and continues the same turn",
	{ timeout: 20000 },
	async () => {
		await fixture(
			async ({ workspace, faux, conversation, requests, authority }) => {
				faux.setResponses([
					call("write", { path: "code.txt", content: "before\n" }, "write"),
					call(
						"edit",
						{ path: "code.txt", oldText: "before", newText: "after" },
						"edit",
					),
					call("read", { path: "code.txt" }, "read"),
					call("bash", { command: "cat code.txt" }, "bash"),
					fauxAssistantMessage("done"),
				]);
				const entries = await turn(conversation);
				assert.equal(requests.length, 4);
				assert.equal(
					await readFile(join(workspace, "code.txt"), "utf8"),
					"after\n",
				);
				assert.ok(JSON.stringify(entries).includes("after"));
				assert.ok(
					entries
						.flatMap((entry) => entry.model ?? [])
						.filter((message) => message.role === "toolResult")
						.every((message) => !message.isError),
				);
				await assert.rejects(
					authority.tools.call({ command: "revoke_all" }),
					/unavailable/,
				);
				await assert.rejects(
					authority.tools.call({
						command: "check",
						operation: requests[0].operation,
					}),
					/no live/,
				);
			},
		);
	},
);

test(
	"revocation during actual shell output withholds progress and final secret from durable history",
	{ timeout: 20000 },
	async () => {
		await fixture(
			async ({ workspace, faux, conversation, authority, setDecision }) => {
				await writeFile(
					join(workspace, "private.txt"),
					"revocation-secret-7249",
				);
				let revoked;
				setDecision(async (request) => {
					await authority.decide(request, true, "session");
					revoked = new Promise((resolve, reject) => {
						setTimeout(() => authority.revokeAll().then(resolve, reject), 600);
					});
				});
				faux.setResponses([
					call(
						"bash",
						{ command: "cat private.txt; sleep 1; echo finished" },
						"revoked-shell",
					),
					fauxAssistantMessage("done"),
				]);
				const entries = await turn(conversation);
				await revoked;
				assert.ok(!JSON.stringify(entries).includes("revocation-secret-7249"));
				const result = entries
					.flatMap((entry) => entry.model ?? [])
					.find((message) => message.toolCallId === "revoked-shell");
				assert.ok(result?.isError);
				assert.ok(JSON.stringify(result).includes("uncertain"));
			},
		);
	},
);

test(
	"hard coding ceiling denies before filesystem effects and ignores operator approval",
	{ timeout: 20000 },
	async () => {
		await fixture(async ({ workspace, faux, conversation, requests }) => {
			faux.setResponses([
				call(
					"write",
					{ path: "forbidden.txt", content: "bad" },
					"denied-write",
				),
				fauxAssistantMessage("done"),
			]);
			const entries = await turn(conversation);
			assert.equal(requests.length, 0);
			await assert.rejects(readFile(join(workspace, "forbidden.txt")), {
				code: "ENOENT",
			});
			assert.ok(
				entries
					.flatMap((entry) => entry.model ?? [])
					.some(
						(message) =>
							message.toolCallId === "denied-write" && message.isError,
					),
			);
		}, "deny");
	},
);

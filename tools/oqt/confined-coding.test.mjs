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

test(
	"native Durable coding registry routes read/write/edit/bash exclusively through confined hands",
	{ timeout: 20000 },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-coding-proof-");
		const workspace = join(root, "workspace"),
			authority = join(root, "authority");
		await mkdir(workspace);
		await mkdir(authority);
		await writeFile(join(authority, "secret"), "must-not-disclose");
		const daemon = join(root, "pi-env");
		await copyFile(process.env.OQT_ENV_DAEMON, daemon);
		const spec = join(root, "hands.json");
		await writeFile(
			spec,
			JSON.stringify({
				workspace,
				daemon,
				bpf: process.env.OQT_SECCOMP_BPF,
				protected_roots: [authority],
			}),
			{ mode: 0o600 },
		);
		const coding = confinedCoding({
			workspace,
			launcher: process.env.OQT_HANDS,
			spec,
		});
		const faux = fauxProvider(),
			models = createModels();
		models.setProvider(faux.provider);
		const call = (name, args, id) =>
			fauxAssistantMessage([fauxToolCall(name, args, { id })], {
				stopReason: "toolUse",
			});
		faux.setResponses([
			call("write", { path: "code.txt", content: "before\n" }, "write-one"),
			call(
				"edit",
				{ path: "code.txt", oldText: "before", newText: "after" },
				"edit-one",
			),
			call("read", { path: "code.txt" }, "read-one"),
			call(
				"bash",
				{
					command:
						"cat code.txt > shell-result.txt; test ! -r ../authority/secret",
				},
				"bash-one",
			),
			call("read", { path: join(authority, "secret") }, "denied-read"),
			fauxAssistantMessage("coding complete"),
		]);
		let harness;
		try {
			assert.throws(() => coding.env({ cwd: authority }), /owner-bound/);
			assert.throws(() => coding.env({}), /owner-bound/);
			harness = await Harness.open(
				await openNodeSqliteStorage(join(authority, "native.sqlite")),
				{ models, registry: coding.registry, env: coding.env },
				context,
			);
			const conversation = await harness.root(context, {
				agent: {
					model: { provider: "faux", modelId: "faux-1" },
					cwd: workspace,
				},
			});
			const turn = await conversation.submit(
				{
					type: "input",
					content: "perform the coding sequence",
					requestId: "native-coding",
				},
				context,
			);
			assert.equal((await turn.wait(context)).status, "done");
			assert.equal(faux.state.callCount, 6);
			const entries = (await conversation.entries({}, 1000, undefined, context))
				.items;
			assert.ok(
				!JSON.stringify(entries).includes("must-not-disclose"),
				"authority content must never enter durable/model history",
			);
			const denied = entries
				.flatMap((entry) => entry.model ?? [])
				.find((message) => message.toolCallId === "denied-read");
			assert.ok(
				denied?.isError,
				"outside read must produce a tool error, not a fabricated success",
			);
			assert.equal(
				await readFile(join(workspace, "code.txt"), "utf8"),
				"after\n",
			);
			assert.equal(
				await readFile(join(workspace, "shell-result.txt"), "utf8"),
				"after\n",
			);
			assert.equal(
				await readFile(join(authority, "secret"), "utf8"),
				"must-not-disclose",
			);
		} finally {
			await harness?.close(context);
			await coding.cleanup(context);
			await rm(root, { recursive: true, force: true });
		}
	},
);

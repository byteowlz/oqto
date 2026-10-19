// Qualification of the pinned engine seam, NOT the guard/TUI receipt pipeline.
// No provider calls, real sessions, owner settings, or external effects.
import assert from "node:assert/strict";
import { mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

const source = process.env.OQT_PI_SOURCE;
if (!source)
	throw new Error(
		"OQT_PI_SOURCE must name the pinned disposable test dependency",
	);
const modules = await mkdtemp(join(tmpdir(), "oqt-native-context-modules-"));
await symlink(join(source, "node_modules"), join(modules, "node_modules"));
await writeFile(
	join(modules, "loader.mjs"),
	"export const load = name => import(name);",
);
const { load } = await import(pathToFileURL(join(modules, "loader.mjs")));
const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);
const { createModels, fauxProvider } = await load("@earendil-works/pi-ai");
const { Harness, createRegistry } = await load("@earendil-works/pi-durable");
const { openNodeSqliteStorage } = await load(
	"@earendil-works/pi-durable/storage/sqlite/node",
);

test("passive operator context survives SQLite reopen and identical retry without starting inference", async () => {
	const directory = await mkdtemp(join(tmpdir(), "oqt-native-operator-"));
	const database = join(directory, "session.sqlite");
	const faux = fauxProvider();
	const models = createModels();
	models.setProvider(faux.provider);
	let harness;
	try {
		const open = async () =>
			Harness.open(
				await openNodeSqliteStorage(database),
				{ models, registry: createRegistry() },
				context,
			);
		harness = await open();
		let root = await harness.root(context, {
			agent: { model: { provider: "faux", modelId: "faux-1" } },
		});
		const conversationId = root.id;
		const draft = {
			type: "write",
			requestId: "fixture-session:operator:receipt-1",
			entry: {
				kind: "oq.operator-result.fixture",
				data: {
					invocation: "receipt-1",
					source: "disposable-fixture",
					excluded: false,
				},
				model: [
					{
						role: "user",
						content: "Operator output: fixture-value",
						timestamp: 1,
					},
				],
			},
		};
		const admitted = await root.submit(draft, context);
		const placed = await admitted.wait(context);
		assert.equal(placed.status, "done");
		assert.equal(placed.type, "write");
		assert.equal(
			faux.state.callCount,
			0,
			"a passive context entry must not run the model",
		);
		await harness.close(context);
		harness = undefined;

		harness = await open();
		root = await harness.root(context);
		assert.equal(root.id, conversationId);
		const retried = await root.submit(draft, context);
		assert.equal(
			retried.id,
			admitted.id,
			"identical receipt retry must use the original native submission",
		);
		const after = await retried.wait(context);
		assert.equal(after.entry, placed.entry);
		const view = await root.context(context);
		assert.equal(JSON.stringify(view.messages).includes("fixture-value"), true);
		assert.equal(
			faux.state.callCount,
			0,
			"reopen and retry must not initiate inference",
		);
	} finally {
		if (harness) await harness.close(context);
		await rm(directory, { recursive: true, force: true });
		await rm(modules, { recursive: true, force: true });
	}
});

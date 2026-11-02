import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { openCodingHarness } from "./coding-runtime.ts";
import { ExecutionBindings } from "./execution-bindings.ts";
import {
	call,
	context,
	fauxAssistantMessage,
	toolResults,
	turn,
	withGuard,
} from "./guard-fixture.mjs";

test(
	"one Durable conversation writes through two independently confined guards",
	{ timeout: 60000 },
	async () => {
		await withGuard(async (first) => {
			await withGuard(async (second) => {
				const environments = new ExecutionBindings();
				environments.bind("first", {
					gate: first.gate,
					description: "First isolated work directory",
				});
				environments.bind("second", {
					gate: second.gate,
					description: "Second isolated work directory",
				});
				const runtime = await openCodingHarness({
					databasePath: join(first.root, "dual.sqlite"),
					session: first.session,
					workspace: first.workspace,
					models: first.models,
					model: { provider: "faux", modelId: "faux-1" },
					gate: first.gate,
					hands: first.hands,
					refresh: () => first.gate.refresh(),
					environments,
				});
				try {
					first.faux.setResponses([
						call(
							"first_write",
							{ path: "target.txt", content: "first-only" },
							"a",
						),
						call(
							"second_write",
							{ path: "target.txt", content: "second-only" },
							"b",
						),
						call("first_bash", { command: "cat target.txt" }, "c"),
						call("second_bash", { command: "cat target.txt" }, "d"),
						fauxAssistantMessage("done"),
					]);
					const entries = await turn(runtime.conversation);
					const results = toolResults(entries);
					assert.equal(results.length, 4);
					assert.ok(results.every((result) => !result.isError));
					assert.ok(
						JSON.stringify(
							results.find((result) => result.toolCallId === "c"),
						).includes("first-only"),
					);
					assert.ok(
						JSON.stringify(
							results.find((result) => result.toolCallId === "d"),
						).includes("second-only"),
					);
					assert.equal(
						await readFile(join(first.workspace, "target.txt"), "utf8"),
						"first-only",
					);
					assert.equal(
						await readFile(join(second.workspace, "target.txt"), "utf8"),
						"second-only",
					);
				} finally {
					await runtime.harness.close(context);
				}
			});
		});
	},
);

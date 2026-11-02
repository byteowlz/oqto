import assert from "node:assert/strict";
import { test } from "node:test";
import { environmentTools } from "./environment-tools.ts";
import { ExecutionBindings } from "./execution-bindings.ts";

function target(machine) {
	return {
		description: machine,
		gate: {
			async prepare() {},
			async check() {},
			async finish() {},
			env: () => ({ machine }),
		},
	};
}
const context = {};
const tool = {
	name: "bash",
	description: "Execute",
	parameters: {},
	async execute(_args, api) {
		api.output(api.env.machine);
		return {};
	},
};

test("one tool set per target, delivery uses the explicitly bound environment", async () => {
	const bindings = new ExecutionBindings();
	bindings.bind("mac", target("mac"));
	bindings.bind("linux", target("linux"));
	const tools = environmentTools([tool], bindings, "session");
	assert.deepEqual(
		tools.map((t) => t.name),
		["mac_bash", "linux_bash"],
	);
	for (const [index, expected] of ["mac", "linux"].entries()) {
		const result = await tools[index].execute(
			{},
			{ conversationId: 1, taskId: index },
			context,
		);
		assert.ok(JSON.stringify(result).includes(expected));
	}
});

test("revoked named tool cannot redirect to a replacement or leak buffered output", async () => {
	const bindings = new ExecutionBindings();
	bindings.bind("linux", target("old"));
	const [old] = environmentTools([tool], bindings, "session");
	bindings.bind("linux", target("new"));
	await assert.rejects(
		old.execute({}, { conversationId: 1, taskId: 1 }, context),
	);
});

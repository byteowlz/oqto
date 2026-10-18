import test from "node:test";
import assert from "node:assert/strict";
import { gatedTool } from "./gated-tool.ts";

test("uncertain teardown retains the invocation journal", async () => {
	let finished = false;
	const tool = gatedTool(
		{ name: "bash", execute: async () => ({ output: [] }) },
		{
			prepare: async () => {},
			check: async () => {},
			drain: async () => {
				throw new Error("stop unconfirmed");
			},
			finish: async () => {
				finished = true;
			},
		},
		"session",
	);
	await assert.rejects(
		tool.execute({}, { conversationId: "c", taskId: "t" }, {}),
	);
	assert.equal(finished, false);
});

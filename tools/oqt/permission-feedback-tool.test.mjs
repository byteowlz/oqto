import assert from "node:assert/strict";
import test from "node:test";
import { gatedTool } from "./gated-tool.ts";

const context = () => ({ abortSignal: new AbortController().signal });
const operationApi = (details) => ({ conversationId: 7, taskId: 9, details });

test("failed admission retains typed UI feedback and never executes the tool", async () => {
	let ran = false;
	const recorded = [];
	const tool = gatedTool({ name: "read", async execute() { ran = true; } }, {
		async prepare() { throw Object.assign(new Error("private internal location"), { feedback: { code: "protected_safety" } }); },
		async check() { throw new Error("must not check"); },
		async finish() { throw new Error("must not finish"); },
	}, "session-fixture");
	await assert.rejects(tool.execute({ path: "/home/fixture" }, operationApi(async (value) => recorded.push(value)), context()),
		(error) => /Blocked by a safety rule/.test(error.message) && !/private internal location/.test(error.message));
	assert.equal(ran, false);
	assert.equal(recorded.length, 1);
	assert.equal(recorded[0].permissionFeedback.code, "protected_safety");
	assert.equal(recorded[0].permissionFeedback.execution, "not_started");
});

test("provider failures cannot forge operator-decline feedback or expose private output", async () => {
	const recorded = [];
	let drained = 0;
	let finished = 0;
	const tool = gatedTool({ name: "bash", async execute(_args, api) {
		api.output("private output must stay withheld");
		throw Object.assign(new Error("private exception"), { feedback: { code: "operator_denied" } });
	} }, {
		async prepare() {}, async check() {}, async drain() { drained++; }, async finish() { finished++; },
	}, "session-fixture");
	await assert.rejects(tool.execute({}, operationApi(async (value) => recorded.push(value)), context()),
		(error) => /outcome is uncertain/.test(error.message) && !/private exception|private output|Declined/.test(error.message));
	assert.equal(recorded[0].permissionFeedback.code, "unknown");
	assert.equal(recorded[0].permissionFeedback.execution, "uncertain");
	assert.equal(drained, 1);
	assert.equal(finished, 1);
});

test("UI metadata failure does not replace the authoritative denial or leak its own error", async () => {
	const tool = gatedTool({ name: "read", async execute() { throw new Error("not executed"); } }, {
		async prepare() { throw Object.assign(new Error("hidden"), { feedback: { code: "configured_ceiling" } }); },
		async check() {}, async finish() {},
	}, "session-fixture");
	await assert.rejects(tool.execute({}, operationApi(async () => { throw new Error("private metadata exception"); }), context()),
		(error) => /Blocked by configured policy/.test(error.message) && !/private metadata exception/.test(error.message));
});

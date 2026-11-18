import assert from "node:assert/strict";
import { test } from "node:test";
import { classify, remoteMachine } from "./classify.ts";
import { withMachine } from "./machine-tools.ts";

test("remote calls are their own ledger resources and never silently local", () => {
	assert.equal(
		classify("bash", { command: "npm test" }).resource,
		"shell:npm test",
	);
	const remote = classify("bash", { command: "npm test", machine: "linux-box" });
	assert.equal(remote.resource, "remote:linux-box:shell:npm test");
	assert.equal(remote.machine, "linux-box");
	assert.ok(remote.preview.startsWith("[on linux-box] "));
	assert.equal(
		classify("read", { path: "a", machine: "local" }).resource,
		"workspace-read",
	);
	assert.equal(remoteMachine({ machine: undefined }), undefined);
	for (const bad of ["Arch", "a b", "../x", "", 7, "x".repeat(33)])
		assert.throws(
			() => classify("write", { path: "a", machine: bad }),
			/Invalid machine/,
		);
});

test("machine argument is added only when machines exist and stripped for the tool", async () => {
	let seen;
	const tool = {
		name: "read",
		description: "Read a file",
		parameters: {
			type: "object",
			properties: { path: { type: "string" } },
			required: ["path"],
		},
		async execute(args) {
			seen = args;
			return { output: [] };
		},
	};
	const plain = withMachine(tool, []);
	assert.equal(
		plain.parameters,
		tool.parameters,
		"no machine argument without machines",
	);
	assert.match(plain.description, /\/machines \(alias \/hands\)/);
	assert.match(plain.description, /None is set up now/);
	const wrapped = withMachine(tool, [
		{ machine: "linux-box", platform: "linux", cwd: null },
	]);
	assert.ok(wrapped.parameters.properties.machine);
	assert.deepEqual(wrapped.parameters.required, ["path"]);
	assert.match(wrapped.description, /linux-box \(linux\)/);
	assert.match(wrapped.description, /Machines are the owner's computers/);
	assert.doesNotMatch(wrapped.description, /None is set up now/);
	await wrapped.execute({ path: "a", machine: "linux-box" }, {}, {});
	assert.deepEqual(seen, { path: "a" });
});

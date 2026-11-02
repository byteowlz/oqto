import assert from "node:assert/strict";
import { test } from "node:test";
import { ExecutionBindings } from "./execution-bindings.ts";

test("two named environments remain distinct; no missing-target fallback", () => {
	const bindings = new ExecutionBindings();
	const mac = bindings.bind("mac", { machine: "mac" });
	const linux = bindings.bind("linux", { machine: "linux" });
	assert.equal(bindings.capture("mac"), mac);
	assert.equal(bindings.capture("linux"), linux);
	assert.throws(() => bindings.capture("unknown"), /unavailable/);
	bindings.revoke("linux");
	assert.throws(() => bindings.capture("linux"), /unavailable/);
	bindings.check(mac);
});

test("replacement fences old calls and revocation cannot reuse generations", () => {
	const bindings = new ExecutionBindings();
	const old = bindings.bind("linux", {});
	const next = bindings.bind("linux", {});
	assert.throws(() => bindings.check(old), /retired/);
	bindings.check(next);
	bindings.revoke("linux");
	assert.throws(() => bindings.check(next), /retired/);
	const restored = bindings.bind("linux", {});
	assert.equal(restored.generation, 3);
	assert.throws(() => bindings.check(next), /retired/);
});

test("a binding from another Session cannot authorize delivery", () => {
	const a = new ExecutionBindings();
	const b = new ExecutionBindings();
	const foreign = a.bind("linux", {});
	b.bind("linux", {});
	assert.throws(() => b.check(foreign), /retired/);
	assert.throws(() => a.bind("https://machine", {}), /Invalid/);
});

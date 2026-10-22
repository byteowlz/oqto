import assert from "node:assert/strict";
import test from "node:test";
import {
	BrainAccessDenied,
	createBrainGrantController,
} from "./brain-grants.ts";

const identity = { session: "session-a", conversation: "conversation-a" };
const grant = (resource, delegate = false, operation = "history.read") => ({
	resource,
	delegate,
	operation,
});
const read = (client, resources) =>
	client.run("history.read", resources, async (scope) => scope.resources);
const deferred = () => {
	let resolve;
	const promise = new Promise((done) => {
		resolve = done;
	});
	return { promise, resolve };
};

test("omitted scope is permitted resources, explicit scope cannot widen", async () => {
	const controller = createBrainGrantController();
	const client = controller.create(identity, [grant("A"), grant("B")]);
	assert.deepEqual(await read(client), ["A", "B"]);
	assert.deepEqual(await read(client, ["B"]), ["B"]);
	await assert.rejects(read(client, ["A", "secret"]), BrainAccessDenied);
	await assert.rejects(
		client.run("memory.record", ["A"], async () => "written"),
		BrainAccessDenied,
	);
});

test("identity and grants are captured, copied and not forgeable", async () => {
	const controller = createBrainGrantController();
	const mutable = [grant("A")];
	const owner = { ...identity };
	const client = controller.create(owner, mutable);
	mutable[0].resource = "secret";
	owner.session = "forged";
	assert.equal(
		await client.run("history.read", ["A"], async (scope) => scope.session),
		identity.session,
	);
	assert.equal(Object.isFrozen(client), true);
	assert.throws(
		() => controller.child({ run() {} }, identity, []),
		BrainAccessDenied,
	);
});

test("children intersect recursively with parent delegation, not parent access", async () => {
	const controller = createBrainGrantController();
	const parent = controller.create(identity, [grant("A", true), grant("B")]);
	const child = controller.child(
		parent,
		{ session: "child", conversation: "child-conv" },
		[grant("A", true), grant("B", true), grant("secret", true)],
	);
	const grandchild = controller.child(
		child,
		{ session: "grandchild", conversation: "grandchild-conv" },
		[grant("A"), grant("B")],
	);
	assert.deepEqual(await read(child), ["A"]);
	assert.deepEqual(await read(grandchild), ["A"]);
	await controller.update(parent, [grant("B")]);
	await assert.rejects(read(child), BrainAccessDenied);
	await assert.rejects(read(grandchild), BrainAccessDenied);
});

test("updates work without restart and operations stay separate", async () => {
	const controller = createBrainGrantController();
	const client = controller.create(identity, []);
	await assert.rejects(read(client, ["A"]), BrainAccessDenied);
	await controller.update(client, [grant("A")]);
	assert.deepEqual(await read(client), ["A"]);
	await assert.rejects(
		client.run("history.search", ["A"], async () => []),
		BrainAccessDenied,
	);
});

test("revocation fences in-flight results and ACK waits for adapter settlement", async () => {
	const controller = createBrainGrantController();
	const parent = controller.create(identity, [grant("A", true)]);
	const child = controller.child(
		parent,
		{ session: "child", conversation: "child-conv" },
		[grant("A")],
	);
	const pending = deferred();
	let signal;
	const result = child.run("history.read", ["A"], async (scope) => {
		signal = scope.signal;
		await pending.promise;
		return "secret";
	});
	const rejected = assert.rejects(result, BrainAccessDenied);
	let acknowledged = false;
	const update = controller.update(parent, []).then(() => {
		acknowledged = true;
	});
	assert.equal(signal.aborted, true);
	await Promise.resolve();
	assert.equal(acknowledged, false);
	await assert.rejects(read(child, ["A"]), BrainAccessDenied);
	pending.resolve();
	await rejected;
	await update;
	assert.equal(acknowledged, true);
});

test("revoke then regrant does not release an older result", async () => {
	const controller = createBrainGrantController();
	const client = controller.create(identity, [grant("A")]);
	const pending = deferred();
	const result = client.run("history.read", ["A"], async () => {
		await pending.promise;
		return "old";
	});
	const rejected = assert.rejects(result, BrainAccessDenied);
	const revoked = controller.update(client, []);
	const granted = controller.update(client, [grant("A")]);
	pending.resolve();
	await Promise.all([rejected, revoked, granted]);
	assert.deepEqual(await read(client), ["A"]);
});

test("closed parents retire descendants and retained clients fail closed", async () => {
	const controller = createBrainGrantController();
	const parent = controller.create(identity, [grant("A", true)]);
	const child = controller.child(
		parent,
		{ session: "child", conversation: "child-conv" },
		[grant("A")],
	);
	await controller.close(parent);
	await assert.rejects(read(child), BrainAccessDenied);
	assert.throws(
		() => controller.update(child, [grant("secret")]),
		BrainAccessDenied,
	);
});

test("invalid grants reject before changing current authority", async () => {
	const controller = createBrainGrantController();
	const client = controller.create(identity, [grant("A")]);
	assert.throws(
		() => controller.update(client, [grant("A"), grant("A")]),
		/Duplicate/,
	);
	assert.throws(
		() => controller.update(client, [grant("A", false, "shell.exec")]),
		/Invalid/,
	);
	assert.deepEqual(await read(client), ["A"]);
});

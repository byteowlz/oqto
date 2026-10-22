import assert from "node:assert/strict";
import test from "node:test";
import {
	BrainAccessDenied,
	createBrainGrantController,
} from "./brain-grants.ts";

test("revoked adapter errors cannot disclose old resource details", async () => {
	const controller = createBrainGrantController();
	const client = controller.create(
		{ session: "session", conversation: "conversation" },
		[{ operation: "memory.read", resource: "collection" }],
	);
	let release;
	const pending = new Promise((resolve) => {
		release = resolve;
	});
	const result = client.run("memory.read", ["collection"], async () => {
		await pending;
		throw new Error("secret resource contents");
	});
	const rejected = assert.rejects(result, BrainAccessDenied);
	const revoked = controller.update(client, []);
	release();
	await Promise.all([rejected, revoked]);
});

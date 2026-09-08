import assert from "node:assert/strict";
import { test } from "node:test";
import { ProviderLoginBroker } from "./broker.mjs";
const tick = () => new Promise((resolve) => setImmediate(resolve));
function fixture() {
	let committed = false;
	const broker = new ProviderLoginBroker({
		machine: "mac",
		requireCommit: true,
		authorize: async (actor) => actor === "alice",
		runtime: {
			providers: async () => [{ id: "codex", methods: ["oauth"] }],
			credentialsCommitted: () => false,
			login: async (_provider, _method, _interaction, beforeCommit) => {
				await beforeCommit();
				committed = true;
			},
		},
	});
	return { broker, committed: () => committed };
}
test("remote credential persistence requires a fresh owner-scoped commit permit", async () => {
	const { broker, committed } = fixture();
	const start = await broker.start("alice", "codex", "oauth");
	await tick();
	const ready = await broker.status("alice", start.id);
	assert.equal(ready.state, "awaiting_commit");
	assert.equal(committed(), false);
	await assert.rejects(broker.commit("bob", start.id, ready.commitNonce));
	await assert.rejects(broker.commit("alice", start.id, "wrong"));
	await broker.commit("alice", start.id, ready.commitNonce);
	await tick();
	assert.equal(committed(), true);
	assert.equal((await broker.status("alice", start.id)).state, "saved");
	await assert.rejects(broker.commit("alice", start.id, ready.commitNonce));
});
test("cancel before commit discards provider result without persisting it", async () => {
	const { broker, committed } = fixture();
	const start = await broker.start("alice", "codex", "oauth");
	await tick();
	await broker.cancel("alice", start.id);
	await tick();
	assert.equal(committed(), false);
	assert.equal((await broker.status("alice", start.id)).state, "cancelled");
});

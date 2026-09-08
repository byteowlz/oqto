import assert from "node:assert/strict";
import { test } from "node:test";
import { ProviderLoginBroker } from "./broker.mjs";

const tick = () => new Promise((resolve) => setImmediate(resolve));
function fixture(
	run,
	authorize = async (actor) => actor === "alice",
	ttlMs = 1000,
) {
	const runtime = {
		providers: async () => [{ id: "codex", methods: ["oauth"] }],
		login: run,
		credentialsCommitted: (error) => error?.committed === true,
	};
	return new ProviderLoginBroker({ runtime, machine: "mac", authorize, ttlMs });
}
test("private prompt, device link, and completion never return credential/input", async () => {
	let answer;
	const broker = fixture(async (_id, _type, interaction, beforeCommit) => {
		interaction.notify({
			type: "device_code",
			userCode: "ABC",
			verificationUri: "https://example.test/verify",
		});
		answer = await interaction.prompt({ type: "secret", message: "Code" });
		await beforeCommit();
		return { access: "NEVER-RETURN-THIS" };
	});
	const attempt = await broker.start("alice", "codex", "oauth");
	assert.equal(attempt.machine, "mac");
	assert.equal(attempt.events[0].userCode, "ABC");
	await assert.rejects(broker.status("bob", attempt.id));
	await assert.rejects(
		broker.answer("alice", attempt.id, "stale", "private-code"),
	);
	await broker.answer("alice", attempt.id, attempt.prompt.id, "private-code");
	await tick();
	const final = await broker.status("alice", attempt.id);
	assert.equal(final.state, "saved");
	assert.equal(answer, "private-code");
	assert(!JSON.stringify(final).includes("private-code"));
	assert(!JSON.stringify(final).includes("NEVER"));
	assert.deepEqual(final.events, []);
});
test("concurrent starts admit exactly one attempt", async () => {
	const broker = fixture(async (_id, _type, interaction) =>
		interaction.prompt({ type: "text", message: "Wait" }),
	);
	const starts = await Promise.allSettled([
		broker.start("alice", "codex", "oauth"),
		broker.start("alice", "codex", "oauth"),
	]);
	assert.equal(
		starts.filter((value) => value.status === "fulfilled").length,
		1,
	);
	broker.close();
	await tick();
});
test("expiry cancels pending input and prevents commit", async () => {
	let committed = false;
	const broker = fixture(
		async (_id, _type, interaction, beforeCommit) => {
			await interaction.prompt({
				type: "manual_code",
				message: "Paste callback",
			});
			await beforeCommit();
			committed = true;
		},
		undefined,
		10,
	);
	const attempt = await broker.start("alice", "codex", "oauth");
	await new Promise((resolve) => setTimeout(resolve, 30));
	assert.equal((await broker.status("alice", attempt.id)).state, "expired");
	assert.equal(committed, false);
	await assert.rejects(
		broker.answer("alice", attempt.id, attempt.prompt.id, "late"),
	);
});
test("revocation is checked immediately before credential commit", async () => {
	let allowed = true;
	let finish;
	let committed = false;
	const broker = fixture(
		async (_id, _type, _interaction, beforeCommit) => {
			await new Promise((resolve) => {
				finish = resolve;
			});
			await beforeCommit();
			committed = true;
		},
		async () => allowed,
	);
	await broker.start("alice", "codex", "oauth");
	allowed = false;
	finish();
	await tick();
	assert.equal(committed, false);
	broker.close();
});
test("provider callback can cancel a manual prompt without cancelling login", async () => {
	const callback = new AbortController();
	const broker = fixture(async (_id, _type, interaction) => {
		const input = interaction.prompt({
			type: "manual_code",
			message: "Optional callback",
			signal: callback.signal,
		});
		callback.abort();
		await assert.rejects(input);
	});
	const attempt = await broker.start("alice", "codex", "oauth");
	await tick();
	assert.equal((await broker.status("alice", attempt.id)).state, "saved");
});
test("raw failures are redacted; committed-but-unsynchronized credentials are explicit", async () => {
	for (const committed of [false, true]) {
		const broker = fixture(async () => {
			throw Object.assign(new Error("SECRET-TOKEN"), { committed });
		});
		const attempt = await broker.start("alice", "codex", "oauth");
		await tick();
		const final = await broker.status("alice", attempt.id);
		assert.equal(final.state, committed ? "saved_refresh_required" : "failed");
		assert(!JSON.stringify(final).includes("SECRET"));
	}
});
test("unsafe auth links fail closed and cannot execute browser schemes", async () => {
	const broker = fixture(async (_id, _type, interaction) =>
		interaction.notify({ type: "auth_url", url: "javascript:alert(1)" }),
	);
	const attempt = await broker.start("alice", "codex", "oauth");
	await tick();
	assert.equal((await broker.status("alice", attempt.id)).state, "failed");
});
test("cancelled attempts cannot answer or retain private challenges", async () => {
	const broker = fixture(async (_id, _type, interaction) =>
		interaction.prompt({ type: "secret", message: "Code" }),
	);
	const attempt = await broker.start("alice", "codex", "oauth");
	await broker.cancel("alice", attempt.id);
	await tick();
	assert.equal((await broker.status("alice", attempt.id)).state, "cancelled");
	await assert.rejects(
		broker.answer("alice", attempt.id, attempt.prompt.id, "late"),
	);
});

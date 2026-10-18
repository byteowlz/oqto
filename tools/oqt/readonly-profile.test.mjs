import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { readOnlyCredentials } from "./readonly-profile.ts";

test("read-only credentials never execute key commands, mutate the owning file or rotate OAuth", async () => {
	const root = await mkdtemp("/var/tmp/oqt-auth-proof-");
	const path = join(root, "auth.json");
	const original = JSON.stringify({
		static: { type: "api_key", key: "fixture-secret" },
		oauth: {
			type: "oauth",
			access: "fixture-access",
			refresh: "must-not-rotate",
			expires: 0,
		},
		command: { type: "api_key", key: "!touch /tmp/unsafe-key-command" },
	});
	await writeFile(path, original, { mode: 0o600 });
	const store = readOnlyCredentials(root);
	let invoked = false;
	try {
		assert.deepEqual(await store.read("static"), {
			type: "api_key",
			key: "fixture-secret",
		});
		assert.equal(await store.read("missing"), undefined);
		assert.equal((await store.list()).length, 3);
		await assert.rejects(store.read("command"), /commands/);
		assert.deepEqual(await store.read("oauth"), {
			type: "oauth",
			access: "fixture-access",
			refresh: "",
			expires: 0,
		});
		await assert.rejects(
			store.modify("oauth", async () => {
				invoked = true;
				return undefined;
			}),
			/read-only/,
		);
		await assert.rejects(store.delete("static"), /read-only/);
		assert.equal(invoked, false);
		assert.equal(await readFile(path, "utf8"), original);
		await rm(path);
		await writeFile(join(root, "other"), original);
		await symlink(join(root, "other"), path);
		await assert.rejects(store.read("static"), { code: "ELOOP" });
	} finally {
		await rm(root, { recursive: true, force: true });
	}
});

import assert from "node:assert/strict";
import { mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { ownerModelDefaults, ownerModelsStore } from "./owner-models.ts";

test("owner defaults/catalog preserved; runtime catalog writes and deletions are memory-only", async () => {
	const root = await mkdtemp("/var/tmp/oqt-owner-models-");
	const catalog = JSON.stringify({
		owner: { models: [], etag: "fixture" },
		unknown: { models: [] },
	});
	const settings = JSON.stringify({
		defaultProvider: "owner",
		defaultModel: "custom-model",
		defaultThinkingLevel: "minimal",
		unknown: { keep: true },
	});
	await writeFile(join(root, "models-store.json"), catalog, { mode: 0o644 });
	await writeFile(join(root, "settings.json"), settings, { mode: 0o644 });
	try {
		assert.deepEqual(await ownerModelDefaults(root), {
			provider: "owner",
			model: "custom-model",
			thinking: "minimal",
		});
		const store = ownerModelsStore(root);
		assert.deepEqual(await store.read("owner"), {
			models: [],
			etag: "fixture",
		});
		const entry = { models: [], etag: "memory-only" };
		await store.write("owner", entry);
		entry.etag = "mutated";
		assert.equal((await store.read("owner")).etag, "memory-only");
		const read = await store.read("owner");
		read.etag = "mutated-read";
		assert.equal((await store.read("owner")).etag, "memory-only");
		await store.delete("unknown");
		assert.equal(await store.read("unknown"), undefined);
		assert.equal(
			await readFile(join(root, "models-store.json"), "utf8"),
			catalog,
		);
		assert.equal(await readFile(join(root, "settings.json"), "utf8"), settings);
		assert.deepEqual((await readdir(root)).sort(), [
			"models-store.json",
			"settings.json",
		]);
		assert.deepEqual(await ownerModelsStore(root).read("unknown"), {
			models: [],
		});
		const signal = AbortSignal.abort();
		await assert.rejects(store.write("owner", { models: [] }, { signal }));
		assert.equal((await store.read("owner")).etag, "memory-only");
		await writeFile(
			join(root, "models-store.json"),
			JSON.stringify({ bad: { models: "not-array" } }),
		);
		await assert.rejects(ownerModelsStore(root).read("bad"), /Invalid/);
	} finally {
		await rm(root, { recursive: true, force: true });
	}
});

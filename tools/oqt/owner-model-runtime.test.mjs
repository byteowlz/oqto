import assert from "node:assert/strict";
import { mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { ownerModelsStore } from "./owner-models.ts";
import { readOnlyCredentials } from "./readonly-profile.ts";

const { ModelRuntime } = await import(
	pathToFileURL(
		join(
			process.env.OQT_PI_SOURCE,
			"packages/coding-agent/src/core/model-runtime.ts",
		),
	)
);

test(
	"exact upstream ModelRuntime composes owner custom config without writing its profile",
	{ timeout: 15000 },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-owner-runtime-");
		const original = JSON.stringify({
			providers: {
				fixture: {
					baseUrl: "http://127.0.0.1:1/v1",
					api: "openai-completions",
					apiKey: "fixture-model-key",
					models: [
						{
							id: "owner-model",
							name: "Fixture owner model",
							reasoning: false,
							input: ["text"],
							cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
							contextWindow: 8192,
							maxTokens: 1024,
						},
					],
				},
			},
		});
		await writeFile(join(root, "models.json"), original, { mode: 0o600 });
		await writeFile(join(root, "auth.json"), "{}", { mode: 0o600 });
		try {
			const runtime = await ModelRuntime.create({
				credentials: readOnlyCredentials(root),
				modelsPath: join(root, "models.json"),
				modelsStore: ownerModelsStore(root),
				allowModelNetwork: false,
			});
			const model = runtime
				.getAllModels("fixture")
				.find((model) => model.id === "owner-model");
			assert.ok(model, "owner custom model must be restored");
			assert.equal(
				(await runtime.getAuth(model)).auth.apiKey,
				"fixture-model-key",
			);
			assert.equal(await readFile(join(root, "models.json"), "utf8"), original);
			assert.equal(await readFile(join(root, "auth.json"), "utf8"), "{}");
			assert.deepEqual((await readdir(root)).sort(), [
				"auth.json",
				"models.json",
			]);
		} finally {
			await rm(root, { recursive: true, force: true });
		}
	},
);

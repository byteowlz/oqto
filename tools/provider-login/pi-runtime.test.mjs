import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { createPiLoginRuntime } from "./pi-runtime.mjs";
import { dispatch } from "./worker.mjs";

test("old Pi API is rejected without attempting a compatibility auth-file write", async () => {
	await assert.rejects(
		createPiLoginRuntime({}, "/unused"),
		/lacks provider-auth SDK/,
	);
});
test("worker rejects actor/path injection and unknown request fields", async () => {
	for (const field of ["actor", "account", "machine", "authPath", "extra"]) {
		await assert.rejects(
			dispatch({}, "alice", {
				id: "1",
				command: "providers",
				[field]: "injection",
			}),
		);
	}
	await assert.rejects(
		dispatch({}, "alice", { id: "bad\nframe", command: "providers" }),
	);
});
test(
	"real Pi SDK writes only the selected credential and preserves unrelated entries",
	{ skip: !process.env.PI_LOGIN_TEST_SDK },
	async () => {
		const home = await mkdtemp(join(tmpdir(), "oqto-login-sdk-"));
		try {
			const sdk = await import(
				pathToFileURL(process.env.PI_LOGIN_TEST_SDK).href
			);
			const unrelated = { type: "api_key", key: "untouched-fixture" };
			await writeFile(join(home, "auth.json"), JSON.stringify({ unrelated }), {
				mode: 0o600,
			});
			const runtime = await createPiLoginRuntime(sdk, home);
			assert(
				(await runtime.providers()).some(
					(provider) =>
						provider.id === "openai-codex" &&
						provider.methods.includes("oauth"),
				),
			);
			await runtime.login(
				"openai",
				"api_key",
				{
					signal: new AbortController().signal,
					notify() {},
					prompt: async () => "sk-test-only-not-a-real-key",
				},
				async () => {},
			);
			const credentials = JSON.parse(
				await readFile(join(home, "auth.json"), "utf8"),
			);
			assert.deepEqual(credentials.unrelated, unrelated);
			assert.equal(credentials.openai.key, "sk-test-only-not-a-real-key");
			assert.equal((await stat(join(home, "auth.json"))).mode & 0o777, 0o600);
			const before = await readFile(join(home, "auth.json"), "utf8");
			for (const value of [
				"!printf SHOULD_NOT_EXECUTE",
				"$PRIVATE_RUNNER_TOKEN",
				" ${PRIVATE_RUNNER_TOKEN}",
			]) {
				await assert.rejects(
					runtime.login(
						"openai",
						"api_key",
						{
							signal: new AbortController().signal,
							notify() {},
							prompt: async () => value,
						},
						async () => {},
					),
				);
				assert.equal(await readFile(join(home, "auth.json"), "utf8"), before);
			}
			await assert.rejects(
				runtime.login(
					"openai",
					"api_key",
					{
						signal: new AbortController().signal,
						notify() {},
						prompt: async () => "must-not-be-saved",
					},
					async () => {
						throw new Error("revoked");
					},
				),
			);
			assert.equal(await readFile(join(home, "auth.json"), "utf8"), before);
		} finally {
			await rm(home, { recursive: true, force: true });
		}
	},
);

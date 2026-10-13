import assert from "node:assert/strict";
import {
	chmod,
	mkdtemp,
	readFile,
	rename,
	rm,
	writeFile,
} from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { readOnlyCredentials } from "./readonly-profile.ts";

const { resolveProviderAuth } = await import(
	pathToFileURL(
		join(process.env.OQT_PI_SOURCE, "packages/ai/src/auth/resolve.ts"),
	)
);

test("upstream OAuth resolver uses valid owning tokens, fails closed before refresh, and observes owner replacements", async () => {
	const root = await mkdtemp("/var/tmp/oqt-broker-proof-");
	const path = join(root, "auth.json");
	const original = JSON.stringify({
		owner: {
			type: "oauth",
			access: "valid-fixture",
			refresh: "never-deliver-refresh",
			expires: Date.now() + 3600000,
			accountId: "fixture-account",
		},
		unrelated: { type: "api_key", key: "keep-unchanged" },
	});
	await writeFile(path, original, { mode: 0o600 });
	let refreshes = 0,
		ambient = 0;
	const provider = {
		id: "owner",
		auth: {
			oauth: {
				toAuth: async (credential) => {
					assert.equal(credential.refresh, "");
					return {
						apiKey: credential.access,
						headers: { "account-id": credential.accountId },
					};
				},
				refresh: async () => {
					refreshes++;
					throw new Error("must never refresh");
				},
			},
			apiKey: {
				resolve: async () => {
					ambient++;
					return { auth: { apiKey: "must-not-fallback" } };
				},
			},
		},
	};
	const context = {
		env: async () => {
			ambient++;
			return "must-not-read-env";
		},
		fileExists: async () => {
			ambient++;
			return false;
		},
	};
	const store = readOnlyCredentials(root);
	try {
		const auth = await resolveProviderAuth(provider, store, context);
		assert.equal(auth.auth.apiKey, "valid-fixture");
		assert.equal(auth.auth.headers["account-id"], "fixture-account");
		assert.equal(await readFile(path, "utf8"), original);
		// Expiry: mutation is refused before the upstream refresh callback executes.
		const expired = JSON.stringify({
			owner: {
				type: "oauth",
				access: "expired",
				refresh: "must-not-rotate",
				expires: 0,
			},
		});
		await writeFile(path, expired);
		await assert.rejects(
			resolveProviderAuth(provider, store, context),
			/modify failed/,
		);
		assert.equal(refreshes, 0);
		assert.equal(ambient, 0);
		assert.equal(await readFile(path, "utf8"), expired);
		// Existing Pi is allowed to renew its own credential. No long-lived snapshot:
		// an atomic owner replacement becomes visible to the very next invocation.
		const replacement = join(root, "next.json");
		await writeFile(
			replacement,
			original.replace("valid-fixture", "owner-renewed"),
			{ mode: 0o600 },
		);
		await rename(replacement, path);
		assert.equal(
			(await resolveProviderAuth(provider, store, context)).auth.apiKey,
			"owner-renewed",
		);
		assert.equal(refreshes, 0);
		assert.equal(ambient, 0);
		await chmod(path, 0o644);
		await assert.rejects(store.read("owner"), /private/);
	} finally {
		await rm(root, { recursive: true, force: true });
	}
});

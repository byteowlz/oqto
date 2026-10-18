import assert from "node:assert/strict";
import {
	chmod,
	mkdir,
	mkdtemp,
	readFile,
	rm,
	stat,
	writeFile,
} from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { guardCredentials } from "./brain-client.ts";
import { Guard } from "./guard.ts";
import { WireClient } from "./guard-wire.ts";
import { OwnerBroker, REFRESH_WINDOW_MS } from "./owner-broker.ts";

const { AuthStorage } = await import(
	pathToFileURL(
		join(
			process.env.OQT_PI_SOURCE,
			"packages/coding-agent/src/core/auth-storage.ts",
		),
	).href
);

async function owner() {
	const root = await mkdtemp("/var/tmp/oqt-broker-proof-");
	await chmod(root, 0o700);
	const dir = join(root, "owner");
	await mkdir(dir, { mode: 0o700 });
	return { root, dir };
}

test(
	"guard broker renews near-expiry OAuth once under the owner lock, backs up first and never hands out refresh secrets",
	{ timeout: 20000 },
	async () => {
		const { root, dir } = await owner();
		const auth = join(dir, "auth.json");
		const original = {
			"openai-codex": {
				type: "oauth",
				access: "old-access",
				refresh: "old-refresh-secret",
				expires: Date.now() + 60_000,
				accountId: "acct-1",
			},
			"other-provider": { type: "api_key", key: "keep-me" },
			"unknown-future-entry": { type: "something-new", data: [1, 2, 3] },
		};
		await writeFile(auth, JSON.stringify(original, null, 2), { mode: 0o600 });
		const before = await readFile(auth, "utf8");
		let refreshes = 0;
		// Upstream Pi's own file-locked store performs the write, exactly as a
		// provider refresh inside Models.getAuth would. Only the token-endpoint
		// HTTP exchange is substituted.
		const storage = AuthStorage.create(auth);
		const broker = new OwnerBroker(dir, async (providerId) => {
			refreshes++;
			await new Promise((resolve) => setTimeout(resolve, 100));
			await storage.modify(providerId, async (current) => ({
				...current,
				access: "new-access",
				refresh: "rotated-refresh-secret",
				expires: Date.now() + 3_600_000,
			}));
		});
		try {
			const [a, b, c] = await Promise.all([
				broker.read("openai-codex"),
				broker.read("openai-codex"),
				broker.read("openai-codex"),
			]);
			assert.equal(refreshes, 1, "concurrent readers share one renewal");
			for (const credential of [a, b, c]) {
				assert.equal(credential.access, "new-access");
				assert.equal(
					credential.refresh,
					"",
					"refresh secret never leaves the guard",
				);
				assert.equal(
					credential.accountId,
					"acct-1",
					"account metadata retained",
				);
			}
			const backup = join(dir, "auth.json.oqt-backup");
			assert.equal(
				await readFile(backup, "utf8"),
				before,
				"pre-refresh backup",
			);
			assert.equal((await stat(backup)).mode & 0o777, 0o600);
			const after = JSON.parse(await readFile(auth, "utf8"));
			assert.equal(after["openai-codex"].refresh, "rotated-refresh-secret");
			assert.deepEqual(after["other-provider"], original["other-provider"]);
			assert.deepEqual(
				after["unknown-future-entry"],
				original["unknown-future-entry"],
			);
			// Valid tokens are served without another renewal.
			await broker.read("openai-codex");
			assert.equal(refreshes, 1);
			assert.deepEqual(await broker.read("other-provider"), {
				type: "api_key",
				key: "keep-me",
			});
		} finally {
			await rm(root, { recursive: true, force: true });
		}
	},
);

test(
	"renewal failure fails closed, and without a broker near-expiry tokens are refused",
	{ timeout: 10000 },
	async () => {
		const { root, dir } = await owner();
		await writeFile(
			join(dir, "auth.json"),
			JSON.stringify({
				p: {
					type: "oauth",
					access: "a",
					refresh: "r",
					expires: Date.now() + REFRESH_WINDOW_MS / 2,
				},
				cmd: { type: "api_key", key: "!security find-generic-password" },
			}),
			{ mode: 0o600 },
		);
		try {
			const failing = new OwnerBroker(dir, async () => {
				throw new Error("token endpoint rejected refresh");
			});
			await assert.rejects(failing.read("p"), /rejected refresh/);
			await assert.rejects(new OwnerBroker(dir).read("p"), /no refresh broker/);
			await assert.rejects(failing.read("cmd"), /not executed/);
		} finally {
			await rm(root, { recursive: true, force: true });
		}
	},
);

test(
	"the brain receives broker credentials only over its tool channel and cannot write them",
	{ timeout: 20000 },
	async () => {
		const { root, dir } = await owner();
		await writeFile(
			join(dir, "auth.json"),
			JSON.stringify({
				p: {
					type: "oauth",
					access: "acc",
					refresh: "secret",
					expires: Date.now() + 3_600_000,
				},
			}),
			{ mode: 0o600 },
		);
		const workspace = join(root, "ws");
		await mkdir(workspace);
		const guard = await Guard.open({
			authorityRoot: join(root, "authority"),
			protectedRoots: [dir],
			hands: {
				launcher: process.env.OQT_HANDS,
				daemon: process.env.OQT_ENV_DAEMON,
				bpf: process.env.OQT_SECCOMP_BPF,
			},
			broker: new OwnerBroker(dir),
			parent: root,
		});
		try {
			const session = guard.session("broker-session", workspace);
			await session.ready;
			const client = await WireClient.connect(session.toolPath);
			try {
				const store = guardCredentials(client);
				const credential = await store.read("p");
				assert.equal(credential.access, "acc");
				assert.equal(credential.refresh, "");
				assert.deepEqual(await store.list(), [
					{ providerId: "p", type: "oauth" },
				]);
				await assert.rejects(
					store.modify("p", async () => undefined),
					/cannot modify/,
				);
				await assert.rejects(store.delete("p"), /cannot delete/);
				const operator = await WireClient.connect(guard.operatorPath);
				try {
					await assert.rejects(
						operator.call("credential", { provider: "p" }),
						/Unknown operator method/,
					);
				} finally {
					operator.close();
				}
			} finally {
				client.close();
			}
		} finally {
			await guard.close();
			await rm(root, { recursive: true, force: true });
		}
	},
);

test(
	"real upstream openai-codex refresh path renews through the broker (token endpoint stubbed at fetch)",
	{ timeout: 20000 },
	async () => {
		const { ownerRefreshWith } = await import("./owner-refresh-core.ts");
		const { ModelRuntime } = await import(
			pathToFileURL(
				join(
					process.env.OQT_PI_SOURCE,
					"packages/coding-agent/src/core/model-runtime.ts",
				),
			).href
		);
		const { root, dir } = await owner();
		const jwt = (account) =>
			`h.${Buffer.from(JSON.stringify({ "https://api.openai.com/auth": { chatgpt_account_id: account } })).toString("base64url")}.s`;
		await writeFile(
			join(dir, "auth.json"),
			JSON.stringify({
				"openai-codex": {
					type: "oauth",
					access: jwt("acct-owner"),
					refresh: "owner-refresh-secret",
					expires: Date.now() + 30_000,
					accountId: "acct-owner",
				},
				keep: { type: "api_key", key: "untouched" },
			}),
			{ mode: 0o600 },
		);
		const realFetch = globalThis.fetch;
		const exchanges = [];
		globalThis.fetch = async (url, init) => {
			if (String(url) !== "https://auth.openai.com/oauth/token")
				throw new Error(`unexpected network access: ${url}`);
			const body = new URLSearchParams(String(init.body));
			exchanges.push(Object.fromEntries(body));
			return new Response(
				JSON.stringify({
					access_token: jwt("acct-owner"),
					refresh_token: "rotated-secret",
					expires_in: 3600,
				}),
				{ status: 200, headers: { "content-type": "application/json" } },
			);
		};
		try {
			const broker = new OwnerBroker(
				dir,
				ownerRefreshWith({ AuthStorage, ModelRuntime })(dir),
			);
			const credential = await broker.read("openai-codex");
			assert.equal(exchanges.length, 1);
			assert.equal(exchanges[0].grant_type, "refresh_token");
			assert.equal(exchanges[0].refresh_token, "owner-refresh-secret");
			assert.equal(
				credential.refresh,
				"",
				"rotated secret stays in the owner file only",
			);
			assert.ok(credential.expires > Date.now() + 3_000_000);
			const stored = JSON.parse(await readFile(join(dir, "auth.json"), "utf8"));
			assert.equal(
				stored["openai-codex"].refresh,
				"rotated-secret",
				"owning Pi keeps working after rotation",
			);
			assert.deepEqual(stored.keep, { type: "api_key", key: "untouched" });
		} finally {
			globalThis.fetch = realFetch;
			await rm(root, { recursive: true, force: true });
		}
	},
);

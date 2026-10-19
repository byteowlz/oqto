import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
	copyFile,
	mkdir,
	mkdtemp,
	readdir,
	readFile,
	rm,
	symlink,
	writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

const source = process.env.OQT_PI_SOURCE;
assert.ok(source, "OQT_PI_SOURCE is required for pinned native dependencies");
const root = await mkdtemp(join(tmpdir(), "oqt-classic-host-"));
await symlink(join(source, "node_modules"), join(root, "node_modules"));
for (const name of (await readdir(new URL(".", import.meta.url))).filter(
	(name) => name.endsWith(".ts"),
))
	await copyFile(new URL(name, import.meta.url), join(root, name));
const api = await import(pathToFileURL(join(root, "classic-import-host.ts")));
const { applyClassicImport } = await import(
	pathToFileURL(join(root, "classic-import-worker.ts"))
);
await writeFile(
	join(root, "loader.mjs"),
	"export const load = name => import(name);",
);
const { load } = await import(pathToFileURL(join(root, "loader.mjs")));
const { Harness, createRegistry } = await load("@earendil-works/pi-durable");
const { createModels, fauxProvider } = await load("@earendil-works/pi-ai");
const { openNodeSqliteStorage } = await load(
	"@earendil-works/pi-durable/storage/sqlite/node",
);
const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);
const faux = fauxProvider();
const models = createModels();
models.setProvider(faux.provider);
const { Guard, GuardSession } = await import(
	pathToFileURL(join(root, "guard.ts"))
);
const { WireClient } = await import(pathToFileURL(join(root, "guard-wire.ts")));
const guard = await Guard.open({
	authorityRoot: join(root, "guard-authority"),
	ceiling: { home: root, userConfig: join(root, "absent.toml"), hardDeny: [] },
	hands: {
		launcher: "/unavailable",
		daemon: "/unavailable",
		bpf: "/unavailable",
	},
	broker: {
		async read() {},
		async list() {
			return [];
		},
	},
	protectedRoots: [],
	parent: root,
});
const operator = await WireClient.connect(guard.operatorPath);
let forbiddenEffects = 0;
test("platform import IDs are destination-catalog scoped, not source UUIDs", () => {
	const sourceId = "pi:fixture:21bf2aa6-3de2-4c0f-8f8e-c3c061534379";
	const sourceHash = "a".repeat(64);
	const first = api.classicImportTargetId(
		join(root, "owner-a", "sessions"),
		sourceId,
		sourceHash,
		"a",
	);
	const second = api.classicImportTargetId(
		join(root, "owner-b", "sessions"),
		sourceId,
		sourceHash,
		"a",
	);
	assert.notEqual(first, second);
	assert.equal(
		first,
		api.classicImportTargetId(
			join(root, "owner-a", "sessions", "."),
			sourceId,
			sourceHash,
			"a",
		),
	);
	assert.notEqual(
		first,
		api.classicImportTargetId(
			join(root, "owner-a", "sessions"),
			sourceId,
			sourceHash,
			null,
		),
	);
	assert.notEqual(first, sourceId);
});
const directory = join(root, "sessions");
await mkdir(directory);
let failBootstrap = false;
let opens = 0;
let closes = 0;
api.configureClassicImportHost(join(root, "protected"));
api.bindClassicImportBackend({
	directory,
	cwd: "/approved",
	async open(metadata) {
		opens++;
		assert.equal(api.canAdmitSession(metadata.id), false);
		// Real operator socket dispatch with an inert Session double. If admission
		// fails open, the forbidden shell counter records an effect attempt.
		guard.sessions.set(metadata.id, {
			id: metadata.id,
			assertAdmission: GuardSession.prototype.assertAdmission,
			async operatorShell() {
				forbiddenEffects++;
			},
			async close() {},
		});
		for (const method of ["shell", "input_request", "input_decide", "decide"]) {
			await assert.rejects(
				operator.call(method, {
					session: metadata.id,
					command: "echo forbidden",
				}),
				/import staging/,
			);
		}
		assert.equal(forbiddenEffects, 0);
		assert.equal(api.isClassicImportStaged(metadata.path), true);
		assert.equal(
			JSON.parse(
				await readFile(api.classicImportPublicationPath(metadata.path), "utf8"),
			).state,
			"staged",
		);
		const seed = JSON.parse(
			await readFile(api.classicImportSeedPath(metadata.path), "utf8"),
		);
		assert.equal(seed.sourceHash, digest(seed.source));
		if (failBootstrap)
			throw new Error("simulated worker crash before publication");
		// Test worker lane: the production host never opens storage itself.
		const harness = await Harness.open(
			await openNodeSqliteStorage(join(metadata.path, "session.sqlite")),
			{ models, registry: createRegistry() },
			context,
		);
		try {
			await applyClassicImport(harness, seed, context);
		} finally {
			await harness.close(context);
		}
		await writeFile(join(metadata.path, "worker-proof"), seed.sourceHash);
	},
	async close() {
		closes++;
	},
	async refresh() {},
});
const peer = {};
const digest = (text) => createHash("sha256").update(text).digest("hex");
const header = {
	type: "session",
	version: 3,
	id: "21bf2aa6-3de2-4c0f-8f8e-c3c061534379",
	cwd: "/untrusted",
	timestamp: "2026-10-18T00:00:00Z",
};
const row = (id, parentId, content) => ({
	type: "message",
	id,
	parentId,
	timestamp: header.timestamp,
	message: { role: "user", content, timestamp: 1 },
});
const text = `${[header, row("a", null, "large".repeat(16000))].map(JSON.stringify).join("\n")}\n`;
async function begin(
	value = text,
	owner = peer,
	origin = "fixture",
	sourceHash = digest(value),
) {
	return api.operateClassicImport(
		"import_classic_begin",
		{ origin, sourceHash, byteLength: Buffer.byteLength(value) },
		owner,
	);
}
async function upload(
	value,
	owner = peer,
	origin = "fixture",
	sourceHash = digest(value),
) {
	const { uploadId } = await begin(value, owner, origin, sourceHash);
	const bytes = Buffer.from(value);
	for (let offset = 0; offset < bytes.length; offset += 16384) {
		const result = await api.operateClassicImport(
			"import_classic_chunk",
			{
				uploadId,
				offset,
				base64: bytes.subarray(offset, offset + 16384).toString("base64"),
			},
			owner,
		);
		assert.equal(result.offset, Math.min(offset + 16384, bytes.length));
	}
	return () =>
		api.operateClassicImport("import_classic_commit", { uploadId }, owner);
}
test("bounded peer-scoped upload, hidden recovery, immutable append snapshots and rewrite fences", async () => {
	try {
		const a = await begin();
		const b = await begin();
		await assert.rejects(begin(), /busy/);
		await assert.rejects(
			api.operateClassicImport(
				"import_classic_commit",
				{ uploadId: a.uploadId },
				{},
			),
			/Unknown/,
		);
		api.retireClassicImportPeer(peer);
		await assert.rejects(
			api.operateClassicImport(
				"import_classic_commit",
				{ uploadId: b.uploadId },
				peer,
			),
			/Unknown/,
		);
		const bad = await upload(text, peer, "bad-hash", "0".repeat(64));
		await assert.rejects(bad(), /hash mismatch/);
		assert.equal(opens, 0);
		failBootstrap = true;
		const commit = await upload(text);
		await assert.rejects(commit(), /simulated worker crash/);
		const staged = (await readdir(directory)).filter((name) =>
			name.startsWith("classic-"),
		);
		assert.equal(staged.length, 1);
		assert.equal(
			api.canAdmitSession(staged[0]),
			false,
			"worker failure cannot lift the host admission fence",
		);
		assert.equal(api.isClassicImportStaged(join(directory, staged[0])), true);
		await assert.rejects(
			readFile(join(directory, staged[0], "meta.json")),
			/ENOENT/,
		);
		// A fresh module models host restart: no in-memory Set survives. A brain
		// may remove .pending and forge meta.json, but cannot publish its receipt.
		const failedPath = join(directory, staged[0]);
		await rm(join(failedPath, ".oqt-import-pending"));
		await writeFile(
			join(failedPath, "meta.json"),
			JSON.stringify({ cwd: "/approved", createdAt: 1 }),
		);
		const restarted = await import(
			`${pathToFileURL(join(root, "classic-import-host.ts"))}?restart=1`
		);
		restarted.bindClassicImportBackend({
			directory,
			cwd: "/approved",
			async open() {
				throw new Error("restart test must not launch");
			},
			async close() {},
			async refresh() {},
		});
		assert.equal(restarted.isClassicImportStaged(failedPath), true);
		assert.equal(
			restarted.canAdmitSession(staged[0]),
			false,
			"durable publication fence survives missing marker and host restart",
		);
		// Exercise the pinned native catalog's read/list with the exact reviewed
		// publication-fence patch, rather than merely inspecting marker existence.
		let catalogCode = await readFile(
			join(source, "packages/coding-agent/src/experimental/session-catalog.ts"),
			"utf8",
		);
		catalogCode = catalogCode.replace(
			'import { randomUUID } from "node:crypto";',
			'import { randomUUID } from "node:crypto";\nimport { isClassicImportStaged } from "./classic-import-host.ts";',
		);
		catalogCode = catalogCode.replace(
			"\tconst path = join(sessionDir, id);\n\tlet value: unknown;",
			"\tconst path = join(sessionDir, id);\n\tif (isClassicImportStaged(path)) return undefined;\n\tlet value: unknown;",
		);
		await writeFile(join(root, "session-catalog-test.ts"), catalogCode);
		const catalog = await import(
			pathToFileURL(join(root, "session-catalog-test.ts"))
		);
		assert.equal(await catalog.readSession(directory, staged[0]), undefined);
		assert.equal(
			(await catalog.listSessions(directory)).some((s) => s.id === staged[0]),
			false,
		);
		failBootstrap = false;
		const receipt = await (await upload(text))();
		assert.equal(
			restarted.canAdmitSession(receipt.sessionId),
			true,
			"protected publish is visible to a rebound host",
		);
		assert.equal(
			(await catalog.readSession(directory, receipt.sessionId)).id,
			receipt.sessionId,
		);
		assert.equal(receipt.status, "imported");
		assert.equal(
			api.isClassicImportStaged(join(directory, receipt.sessionId)),
			false,
		);
		const originalProof = await readFile(
			join(directory, receipt.sessionId, "worker-proof"),
			"utf8",
		);
		const beforeRetry = opens;
		const retry = await (await upload(text))();
		assert.equal(retry.sessionId, receipt.sessionId);
		assert.equal(retry.status, "unchanged");
		assert.equal(opens, beforeRetry);
		const appended = `${text}${JSON.stringify(row("b", "a", "new branch snapshot"))}\n`;
		const next = await (await upload(appended))();
		assert.notEqual(next.sessionId, receipt.sessionId);
		assert.equal(next.mode, "new_snapshot");
		assert.equal(next.predecessorSessionId, receipt.sessionId);
		assert.equal(
			await readFile(
				join(directory, receipt.sessionId, "worker-proof"),
				"utf8",
			),
			originalProof,
		);
		assert.equal(
			(await (await upload(text))()).sessionId,
			receipt.sessionId,
			"old revision retry must remain idempotent",
		);
		const rewrite = await upload(appended.replace("large", "other"));
		await assert.rejects(rewrite(), /rewritten|reused/);
		// A failed reservation is revision evidence, never a published predecessor.
		failBootstrap = true;
		await assert.rejects(
			(await upload(text, peer, "failed-only"))(),
			/simulated worker crash/,
		);
		failBootstrap = false;
		const recoveredAppend = await (
			await upload(appended, peer, "failed-only")
		)();
		assert.equal(recoveredAppend.predecessorSessionId, undefined);
		const failedRevision = `${appended}${JSON.stringify(row("c", "b", "failed reservation"))}\n`;
		failBootstrap = true;
		await assert.rejects(
			(await upload(failedRevision))(),
			/simulated worker crash/,
		);
		failBootstrap = false;
		const recoveredRevision = `${failedRevision}${JSON.stringify(row("d", "c", "after failed reservation"))}\n`;
		const recovered = await (await upload(recoveredRevision))();
		assert.equal(
			recovered.predecessorSessionId,
			next.sessionId,
			"predecessor is last published, not failed hidden target",
		);
		const toolCall = row("call", "a", "unused");
		toolCall.message = {
			role: "assistant",
			timestamp: 2,
			provider: "recorded",
			model: "historic",
			api: "faux",
			stopReason: "toolUse",
			usage: {
				input: 0,
				output: 0,
				cacheRead: 0,
				cacheWrite: 0,
				totalTokens: 0,
				cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
			},
			content: [
				{
					type: "toolCall",
					id: "call-1",
					name: "never-execute",
					arguments: {},
				},
			],
		};
		const toolResult = row("result", "call", "unused");
		toolResult.message = {
			role: "toolResult",
			timestamp: 3,
			toolCallId: "wrong-call",
			toolName: "never-execute",
			isError: false,
			content: [],
		};
		for (const [index, invalid] of [
			[header, row("a", null, "prompt"), toolCall],
			[header, row("a", null, "prompt"), toolCall, toolResult],
		].entries()) {
			const invalidText = `${invalid.map(JSON.stringify).join("\n")}\n`;
			await assert.rejects(
				(await upload(invalidText, peer, `bad-tools-${index}`))(),
				/Classic tool pairing/,
			);
			const target = api.classicImportTargetId(
				directory,
				`pi:bad-tools-${index}:${header.id}`,
				digest(invalidText),
				invalid.at(-1).id,
			);
			assert.equal(api.isClassicImportStaged(join(directory, target)), true);
			await assert.rejects(
				readFile(join(directory, target, "meta.json")),
				/ENOENT/,
			);
			if (index === 0) {
				const completedResult = {
					...toolResult,
					message: { ...toolResult.message, toolCallId: "call-1" },
				};
				const completedSource = `${invalidText}${JSON.stringify(completedResult)}\n`;
				const completed = await (
					await upload(completedSource, peer, "bad-tools-0")
				)();
				assert.equal(completed.status, "imported");
				assert.equal(
					completed.predecessorSessionId,
					undefined,
					"incomplete tool leaf was never published",
				);
			}
		}
		assert.equal(faux.state.callCount, 0);
		assert.equal(opens, closes);
		assert.equal(api.canAdmitSession(receipt.sessionId), true);
		assert.equal(api.canAdmitSession("existing-normal-session"), true);
		assert.equal(forbiddenEffects, 0);
	} finally {
		operator.close();
		await guard.close();
		await rm(root, { recursive: true, force: true });
	}
});

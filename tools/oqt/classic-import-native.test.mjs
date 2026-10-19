import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { copyFile, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

const source = process.env.OQT_PI_SOURCE;
const pin = "cdaf6c227d4718a72b59ccd02152c9614df44cfc";
assert.equal(
	execFileSync("git", ["-C", source, "rev-parse", "HEAD"], {
		encoding: "utf8",
	}).trim(),
	pin,
);
const modules = await mkdtemp(join(tmpdir(), "oqt-classic-native-"));
await symlink(join(source, "node_modules"), join(modules, "node_modules"));
await writeFile(
	join(modules, "loader.mjs"),
	"export const load = name => import(name);",
);
const { load } = await import(pathToFileURL(join(modules, "loader.mjs")));
for (const name of ["classic-import.ts", "classic-import-worker.ts"])
	await copyFile(new URL(name, import.meta.url), join(modules, name));
const { classicSnapshot } = await import(
	pathToFileURL(join(modules, "classic-import.ts"))
);
const {
	importClassicSnapshot,
	applyClassicImport,
	validateClassicImportSeed,
	ClassicImportDoc,
} = await import(pathToFileURL(join(modules, "classic-import-worker.ts")));
const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);
const { Harness, createRegistry } = await load("@earendil-works/pi-durable");
const { createModels, fauxProvider } = await load("@earendil-works/pi-ai");
const { openNodeSqliteStorage } = await load(
	"@earendil-works/pi-durable/storage/sqlite/node",
);
const { buildSessionContext, buildSessionProjection } = await import(
	pathToFileURL(
		join(source, "packages/coding-agent/src/core/session-manager.ts"),
	)
);
const { convertToLlm } = await import(
	pathToFileURL(join(source, "packages/coding-agent/src/core/messages.ts"))
);
const header = {
	type: "session",
	version: 3,
	id: "9267f005-48a0-4c42-920e-3277aa2c6070",
	cwd: "/inert",
	timestamp: "2026-10-17T00:00:00Z",
};
const row = (id, parentId, type, extra) => ({
	id,
	parentId,
	type,
	timestamp: header.timestamp,
	...extra,
});
const msg = (id, parentId) =>
	row(id, parentId, "message", {
		message: { role: "user", content: id, timestamp: 1 },
	});
const rows = [
	msg("a", null),
	msg("b", "a"),
	row("opaque", "b", "custom", {
		customType: "inert",
		data: { secret: [1, 2] },
	}),
	row("c1", "opaque", "compaction", {
		summary: "first",
		firstKeptEntryId: "b",
		tokensBefore: 100,
	}),
	row("custom", "c1", "custom_message", {
		customType: "note",
		content: "custom",
		display: false,
	}),
	row("edit", "custom", "context_edit", {
		targetId: "custom",
		replacement: { content: "edited" },
	}),
	row("c2", "edit", "compaction", {
		summary: "second",
		firstKeptEntryId: "c1",
		tokensBefore: 200,
		systemMessage: { role: "system", content: "preserved", timestamp: 2 },
	}),
	row("branch", "c2", "branch_summary", {
		summary: "other branch",
		fromId: "b",
	}),
	msg("sibling", "a"),
];
const usage = {
	input: 1,
	output: 2,
	cacheRead: 3,
	cacheWrite: 4,
	totalTokens: 10,
	cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
};
rows.splice(
	rows.length - 1,
	0,
	row("model", "branch", "model_change", {
		provider: "recorded",
		modelId: "not-approved",
	}),
	row("thinking", "model", "thinking_level_change", { thinkingLevel: "high" }),
	row("answer", "thinking", "message", {
		message: {
			role: "assistant",
			timestamp: 3,
			provider: "recorded",
			model: "historic",
			api: "faux",
			stopReason: "toolUse",
			usage,
			content: [
				{ type: "thinking", thinking: "reason", thinkingSignature: "opaque" },
				{
					type: "toolCall",
					id: "call-1",
					name: "never-execute",
					arguments: { raw: [1, 2] },
				},
			],
		},
	}),
	row("result", "answer", "message", {
		message: {
			role: "toolResult",
			timestamp: 4,
			toolCallId: "call-1",
			toolName: "never-execute",
			isError: false,
			details: { diagnostic: "opaque" },
			content: [{ type: "text", text: "historical" }],
		},
	}),
	row("image", "result", "message", {
		message: {
			role: "user",
			timestamp: 5,
			content: [{ type: "image", data: "aGVsbG8=", mimeType: "image/png" }],
		},
	}),
	row("omit", "edit", "context_edit", {
		targetId: "custom",
		replacement: null,
	}),
	row("none", "image", "compaction", {
		summary: "retain none",
		tokensBefore: 300,
		firstKeptEntryId: "none",
	}),
);
const snap = (rs) =>
	classicSnapshot(
		`${[header, ...rs].map(JSON.stringify).join("\n")}\n`,
		"fixture",
	);

test("SQLite archive/map and selected contexts equal pinned classic, survive reopen, never infer", async () => {
	const faux = fauxProvider();
	const models = createModels();
	models.setProvider(faux.provider);
	let harness;
	try {
		for (const leaf of [
			"b",
			"edit",
			"omit",
			"c2",
			"branch",
			"image",
			"none",
			"sibling",
			null,
		]) {
			const database = join(modules, `case-${leaf}.sqlite`);
			const open = async () =>
				Harness.open(
					await openNodeSqliteStorage(database),
					{ models, registry: createRegistry() },
					context,
				);
			harness = await open();
			let conversation = await harness.root(context);
			const snapshot = snap(rows);
			const result = await importClassicSnapshot(
				{ harness, conversation, snapshot, leaf },
				context,
			);
			assert.equal(result.mappings.length, rows.length);
			assert.equal(
				(
					await harness.commit(
						(tx) => tx.scanTasks({ conversationId: conversation.id }, 1),
						context,
					)
				).items.length,
				0,
			);
			const expected = buildSessionContext(rows, leaf);
			assert.deepEqual(
				JSON.parse(
					(await harness.snapshot(ClassicImportDoc, conversation.id, context))
						.snapshotJson,
				).sourceText,
				snapshot.sourceText,
			);
			assert.deepEqual(
				(await conversation.context(context)).messages,
				convertToLlm(expected.messages),
			);
			const state = await harness.snapshot(
				ClassicImportDoc,
				conversation.id,
				context,
			);
			const view = await conversation.context(context);
			if (leaf === "edit")
				assert.equal(JSON.stringify(view.messages).includes("edited"), true);
			if (leaf === "omit")
				assert.equal(JSON.stringify(view.messages).includes("edited"), false);
			if (leaf === "none") assert.equal(view.messages.length, 1);
			if (leaf === "image")
				assert.equal(
					JSON.stringify(view.messages).includes("thinkingSignature"),
					true,
				);
			assert.equal(
				(await conversation.agent(context)).model,
				undefined,
				"historical models are not execution policy",
			);
			assert.deepEqual(
				JSON.parse(state.snapshotJson),
				JSON.parse(JSON.stringify(snapshot)),
			);
			let cursor;
			const entries = [];
			do {
				const page = await conversation.entries(
					{ order: "ascending" },
					2,
					cursor,
					context,
				);
				entries.push(...page.items);
				cursor = page.next;
			} while (cursor);
			assert.equal(
				entries.length,
				rows.length + 1 + convertToLlm(expected.messages).length,
			);
			const checkpoint = entries.find(
				(entry) => entry.kind === "oq.classic-projection.v2",
			);
			assert.ok(checkpoint);
			assert.equal(
				checkpoint.model,
				undefined,
				"checkpoint must not hide all model history in an unknown kind",
			);
			assert.deepEqual(
				checkpoint.data.classicContext,
				JSON.parse(JSON.stringify(expected)),
			);
			assert.deepEqual(checkpoint.data.recordedSettings, {
				model: expected.model,
				thinkingLevel: expected.thinkingLevel,
			});
			for (let i = 0; i < rows.length; i++) {
				assert.deepEqual(entries[i].data.record, rows[i]);
				assert.equal(result.mappings[i].entryId, entries[i].id);
			}
			const transcript = await conversation.viewState(context);
			const materialized = transcript.value.entries.filter((entry) =>
				[
					"pi.user",
					"pi.assistant",
					"pi.tool-result",
					"pi.system",
					"oq.classic-hidden-context.v2",
				].includes(entry.kind),
			);
			assert.deepEqual(
				materialized.flatMap((entry) => entry.model ?? []),
				convertToLlm(expected.messages),
			);
			assert.equal(result.watermark, entries.at(-1).id);
			assert.equal(state.watermark, entries.at(-1).id);
			const oracleProjection = buildSessionProjection(rows, leaf);
			const visible = materialized.filter(
				(entry) => entry.kind !== "oq.classic-hidden-context.v2",
			);
			assert.deepEqual(
				visible.flatMap((entry) => entry.model ?? []),
				oracleProjection.entries
					.filter(
						(selected) =>
							!(
								selected.sourceEntry.type === "custom_message" &&
								selected.sourceEntry.display === false
							),
					)
					.flatMap((selected) => convertToLlm(selected.messages)),
			);
			if (leaf === "edit") {
				assert.equal(
					JSON.stringify(visible).includes("edited"),
					false,
					"display:false stays hidden in native role views",
				);
				assert.equal(
					JSON.stringify(
						materialized.filter(
							(entry) => entry.kind === "oq.classic-hidden-context.v2",
						),
					).includes("edited"),
					true,
				);
			}
			const byNativeId = new Map(
				result.mappings.map((mapping) => [mapping.nativeId, mapping]),
			);
			const byDurableId = new Map(entries.map((entry) => [entry.id, entry]));
			for (const selected of oracleProjection.entries) {
				const mapping = byNativeId.get(selected.sourceEntry.id);
				assert.deepEqual(
					mapping.projectionEntryIds.flatMap(
						(id) => byDurableId.get(id).model ?? [],
					),
					convertToLlm(selected.messages),
				);
			}
			assert.deepEqual(
				new Set(
					result.mappings.flatMap((mapping) => mapping.projectionEntryIds),
				),
				new Set(materialized.map((entry) => entry.id)),
			);
			for (const entry of materialized) {
				if (entry.kind === "pi.tool-result")
					assert.deepEqual(entry.data.diagnostics, []);
				assert.equal(entry.data.provenance.sourceId, snapshot.sourceId);
				assert.ok(
					byNativeId
						.get(entry.data.provenance.sourceEntryId)
						.projectionEntryIds.includes(entry.id),
				);
			}
			transcript.dispose();
			await harness.close(context);
			harness = await open();
			conversation = await harness.root(context);
			assert.equal(
				(
					await importClassicSnapshot(
						{ harness, conversation, snapshot, leaf },
						context,
					)
				).kind,
				"unchanged",
			);
			assert.deepEqual(
				(await conversation.context(context)).messages,
				convertToLlm(expected.messages),
			);
			const appended = snap([...rows, msg("next", "sibling")]);
			const beforeAppend = await harness.snapshot(
				ClassicImportDoc,
				conversation.id,
				context,
			);
			await assert.rejects(
				importClassicSnapshot(
					{ harness, conversation, snapshot: appended },
					context,
				),
				/new snapshot destination/,
			);
			assert.deepEqual(
				await harness.snapshot(ClassicImportDoc, conversation.id, context),
				beforeAppend,
			);
			await conversation.commit(
				(tx) => tx.appendEntry(conversation.id, { kind: "local", data: {} }),
				context,
			);
			await assert.rejects(
				importClassicSnapshot(
					{ harness, conversation, snapshot: appended },
					context,
				),
				/diverged/,
			);
			await harness.close(context);
			harness = undefined;
		}
		const open = async (file) =>
			Harness.open(
				await openNodeSqliteStorage(join(modules, file)),
				{ models, registry: createRegistry() },
				context,
			);
		// Explicit completed-source fixture: the stock native Transcript kinds
		// carry historical user text, assistant calls, real results, and final answer.
		const completedSource = [
			row("public-custom", null, "custom_message", {
				customType: "inert-custom",
				display: true,
				content: "public-custom-marker",
			}),
			row("historical-user", "public-custom", "message", {
				message: {
					role: "user",
					content: "historical-user-marker",
					timestamp: 1,
				},
			}),
			row("historical-call", "historical-user", "message", {
				message: {
					role: "assistant",
					content: [
						{
							type: "toolCall",
							id: "historical-call-id",
							name: "read",
							arguments: { path: "inert-history.txt" },
						},
					],
					timestamp: 2,
					provider: "recorded",
					model: "historic",
					api: "faux",
					usage,
					stopReason: "toolUse",
				},
			}),
			row("historical-result", "historical-call", "message", {
				message: {
					role: "toolResult",
					toolCallId: "historical-call-id",
					toolName: "read",
					content: [{ type: "text", text: "historical-result-marker" }],
					isError: false,
					details: { unchanged: true },
					timestamp: 3,
				},
			}),
			row("historical-done", "historical-result", "message", {
				message: {
					role: "assistant",
					content: [{ type: "text", text: "historical-completed-marker" }],
					timestamp: 4,
					provider: "recorded",
					model: "historic",
					api: "faux",
					usage,
					stopReason: "stop",
				},
			}),
		];
		harness = await open("completed-source.sqlite");
		let completed = await harness.root(context);
		const completedSnapshot = snap(completedSource);
		const completedReceipt = await importClassicSnapshot(
			{ harness, conversation: completed, snapshot: completedSnapshot },
			context,
		);
		let completedView = await completed.viewState(context);
		let completedEntries = completedView.value.entries.filter((entry) =>
			["pi.user", "pi.assistant", "pi.tool-result"].includes(entry.kind),
		);
		assert.deepEqual(
			completedEntries.map((entry) => entry.kind),
			["pi.user", "pi.user", "pi.assistant", "pi.tool-result", "pi.assistant"],
		);
		assert.deepEqual(
			completedEntries.flatMap((entry) => entry.model),
			convertToLlm(
				buildSessionContext(completedSource, "historical-done").messages,
			),
		);
		assert.match(JSON.stringify(completedEntries), /historical-user-marker/);
		assert.match(
			JSON.stringify(completedEntries),
			/public-custom-marker/,
			"display:true reuses the native user-role view, not an extension renderer",
		);
		assert.match(JSON.stringify(completedEntries), /historical-result-marker/);
		assert.match(
			JSON.stringify(completedEntries),
			/historical-completed-marker/,
		);
		assert.deepEqual(
			completedEntries.find((entry) => entry.kind === "pi.tool-result").data
				.diagnostics,
			[],
		);
		assert.equal(completedReceipt.watermark, completedEntries.at(-1).id);
		assert.equal(
			(
				await harness.commit(
					(tx) => tx.scanTasks({ conversationId: completed.id }, 1),
					context,
				)
			).items.length,
			0,
			"historical tools must never create tasks",
		);
		const completedIds = completedEntries.map((entry) => entry.id);
		completedView.dispose();
		await harness.close(context);
		harness = await open("completed-source.sqlite");
		completed = await harness.root(context);
		assert.equal(
			(
				await importClassicSnapshot(
					{ harness, conversation: completed, snapshot: completedSnapshot },
					context,
				)
			).kind,
			"unchanged",
		);
		completedView = await completed.viewState(context);
		completedEntries = completedView.value.entries.filter((entry) =>
			["pi.user", "pi.assistant", "pi.tool-result"].includes(entry.kind),
		);
		assert.deepEqual(
			completedEntries.map((entry) => entry.id),
			completedIds,
			"reopen/retry must not duplicate rendered history",
		);
		completedView.dispose();
		await harness.close(context);
		harness = undefined;
		harness = await open("failure.sqlite");
		const conversation = await harness.root(context);
		const snapshot = snap(rows);
		assert.throws(
			() =>
				validateClassicImportSeed({
					source: snapshot.sourceText,
					sourceHash: "bad",
					origin: "fixture",
				}),
			/hash mismatch/,
		);
		await assert.rejects(
			importClassicSnapshot(
				{
					harness,
					conversation,
					snapshot: snap([
						...rows,
						row("unknown", "sibling", "future\u001b]52;c;AAAA", {}),
					]),
				},
				context,
			),
			(error) =>
				error.message.includes("Unsupported") &&
				!error.message.includes("\u001b"),
		);
		assert.equal(
			(await conversation.entries({}, 100, undefined, context)).items.length,
			0,
		);
		for (const bad of [
			row("bad", "sibling", "compaction", {
				summary: "bad",
				tokensBefore: 1,
				firstKeptEntryId: "missing",
			}),
			row("bad", "sibling", "context_edit", {
				targetId: "custom",
				replacement: null,
			}),
			row("bad", "sibling", "message", {
				message: {
					role: "extensionRole",
					timestamp: 1,
					content: "do not flatten",
				},
			}),
			row("bad", "sibling", "message", {
				message: {
					...rows.find((r) => r.id === "answer").message,
					stopReason: "deferred",
					deferred: { id: "never-poll" },
				},
			}),
		])
			await assert.rejects(
				importClassicSnapshot(
					{ harness, conversation, snapshot: snap([...rows, bad]) },
					context,
				),
				/Unsupported|not an ancestor/,
			);
		const answer = rows.find((r) => r.id === "answer");
		const result = rows.find((r) => r.id === "result");
		const pair = [msg("pair-root", null), { ...answer, parentId: "pair-root" }];
		const invalidPairs = [
			pair,
			[
				...pair,
				{
					...result,
					message: { ...result.message, toolCallId: "missing-call" },
				},
			],
			[
				...pair,
				{ ...result, message: { ...result.message, toolName: "wrong-tool" } },
			],
			[
				...pair,
				result,
				{ ...result, id: "duplicate-result", parentId: "result" },
			],
			[
				pair[0],
				{
					...pair[1],
					message: {
						...answer.message,
						content: [
							...answer.message.content,
							answer.message.content.find((b) => b.type === "toolCall"),
						],
					},
				},
				result,
			],
			[pair[0], { ...pair[1], message: { ...answer.message, content: [] } }],
		];
		for (const invalid of invalidPairs) {
			let transactions = 0;
			await assert.rejects(
				importClassicSnapshot(
					{
						harness: {
							commit(...args) {
								transactions++;
								return harness.commit(...args);
							},
						},
						conversation,
						snapshot: snap(invalid),
					},
					context,
				),
				/Classic tool pairing/,
			);
			assert.equal(
				transactions,
				0,
				"invalid pairing rejected before any Tx mutation",
			);
			assert.equal(
				(await conversation.entries({}, 100, undefined, context)).items.length,
				0,
			);
		}
		// An unresolved toolUse in an inactive branch remains inert archive;
		// selecting that branch is what makes incomplete execution unsafe.
		let reachedTransaction = false;
		await assert.rejects(
			importClassicSnapshot(
				{
					harness: {
						commit() {
							reachedTransaction = true;
							throw new Error("inert archive accepted");
						},
					},
					conversation,
					snapshot: snap([...pair, msg("safe-tip", "pair-root")]),
					leaf: "safe-tip",
				},
				context,
			),
			/inert archive accepted/,
		);
		assert.equal(reachedTransaction, true);
		// Throw after archives, checkpoint and one materialized message: the
		// entire native transcript and provenance document must roll back together.
		const injected = {
			commit: (fn, ctx) =>
				harness.commit((tx) => {
					let writes = 0;
					return fn(
						new Proxy(tx, {
							get(target, key) {
								if (key === "appendEntry")
									return async (...args) => {
										if (++writes === rows.length + 3)
											throw new Error("injected transaction fault");
										return target.appendEntry(...args);
									};
								const value = target[key];
								return typeof value === "function" ? value.bind(target) : value;
							},
						}),
					);
				}, ctx),
		};
		await assert.rejects(
			importClassicSnapshot(
				{ harness: injected, conversation, snapshot },
				context,
			),
			/injected/,
		);
		assert.equal(
			(await conversation.entries({}, 100, undefined, context)).items.length,
			0,
		);
		assert.equal(
			(await harness.snapshot(ClassicImportDoc, conversation.id, context))
				?.snapshotJson ?? null,
			null,
		);
		const seed = {
			source: snapshot.sourceText,
			sourceHash: snapshot.sourceDigest,
			origin: "fixture",
		};
		const concurrent = await Promise.all([
			applyClassicImport(harness, seed),
			applyClassicImport(harness, seed),
		]);
		assert.deepEqual(concurrent.map((r) => r.kind).sort(), [
			"imported",
			"unchanged",
		]);
		const before = await harness.snapshot(
			ClassicImportDoc,
			conversation.id,
			context,
		);
		await assert.rejects(
			importClassicSnapshot(
				{ harness, conversation, snapshot: snap(rows.slice(0, -1)) },
				context,
			),
			/new snapshot destination/,
		);
		const changed = rows.map((r) => (r.id === "a" ? msg("a", null) : r));
		changed[0].message.content = "changed";
		await assert.rejects(
			importClassicSnapshot(
				{ harness, conversation, snapshot: snap(changed) },
				context,
			),
			/new snapshot destination/,
		);
		await assert.rejects(
			importClassicSnapshot(
				{
					harness,
					conversation,
					snapshot: snap([...rows, msg("other-tip", "b")]),
				},
				context,
			),
			/new snapshot destination/,
		);
		assert.deepEqual(
			await harness.snapshot(ClassicImportDoc, conversation.id, context),
			before,
		);
		await conversation.configure({ cwd: "/approved-local-change" }, context);
		await assert.rejects(applyClassicImport(harness, seed), /diverged/);
		assert.equal(faux.state.callCount, 0);
	} finally {
		if (harness) await harness.close(context);
		await rm(modules, { recursive: true, force: true });
	}
});

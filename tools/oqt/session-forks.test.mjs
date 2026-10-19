// Native SQLite qualification only; no source profiles, JSONL, model requests or real sessions.
import assert from "node:assert/strict";
import { copyFile, mkdtemp, rm, symlink } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

const source = process.env.OQT_PI_SOURCE;
if (!source)
	throw new Error("OQT_PI_SOURCE must name the pinned test dependency");
const require = createRequire(join(source, "package.json"));
const load = (name) => import(pathToFileURL(require.resolve(name)));
const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);
const { Harness, createRegistry, defineDoc } = await load(
	"@earendil-works/pi-durable",
);
const { createModels, fauxProvider } = await load("@earendil-works/pi-ai");
const { openNodeSqliteStorage } = await load(
	"@earendil-works/pi-durable/storage/sqlite/node",
);

test(
	"native fork IDs, paged ancestry, initial-only security docs and SQLite selection recovery",
	{ timeout: 20000 },
	async () => {
		const dir = await mkdtemp(join(tmpdir(), "oqt-forks-"));
		let harness;
		try {
			await symlink(join(source, "node_modules"), join(dir, "node_modules"));
			await copyFile(
				new URL("./session-forks.ts", import.meta.url),
				join(dir, "session-forks.ts"),
			);
			const { createSessionForks } = await import(
				pathToFileURL(join(dir, "session-forks.ts"))
			);
			const faux = fauxProvider();
			const models = createModels();
			models.setProvider(faux.provider);
			const authority = defineDoc({
				kind: "fixture.authority",
				version: 1,
				scope: "conversation",
				history: "latest",
				fork: "initial",
				initial: () => ({ grants: [] }),
			});
			const receipts = defineDoc({
				kind: "fixture.receipts",
				version: 1,
				scope: "conversation",
				history: "latest",
				fork: "initial",
				initial: () => ({ receipts: [] }),
			});
			// Await storage explicitly: Harness.open accepts storage, not a promise.
			const openStorage = () =>
				openNodeSqliteStorage(join(dir, "session.sqlite"));
			const reopen = async () =>
				Harness.open(
					await openStorage(),
					{ models, registry: createRegistry() },
					context,
				);
			harness = await reopen();
			const root = await harness.root(context);
			const first = await root.commit(async (tx) => {
				(await tx.doc(authority, root.id)).grants.push("never inherit");
				(await tx.doc(receipts, root.id)).receipts.push("never inherit");
				return tx.appendEntry(root.id, {
					kind: "fixture.same",
					data: { text: "identical" },
				});
			}, context);
			const second = await root.commit(
				(tx) =>
					tx.appendEntry(root.id, {
						kind: "fixture.same",
						data: { text: "identical" },
					}),
				context,
			);
			let bound;
			let service = createSessionForks(harness, async (id) => {
				bound = id;
			});
			assert.equal(await service.selected(context), root.id);
			await assert.rejects(service.fork(root.id, 999999, context));
			await assert.rejects(service.entries(root.id, 101, null, context));
			await assert.rejects(service.select(999999, context));
			const archive = await root.commit(
				(tx) =>
					tx.appendEntry(root.id, {
						kind: "oq.classic-archive.v1",
						data: { sourceEntryId: "classic-source-entry" },
					}),
				context,
			);
			const beforeArchiveFork = await service.conversations(100, null, context);
			await assert.rejects(
				service.fork(root.id, archive.id, context),
				/inert source envelopes.*--leaf <classic-source-entry-id>/,
			);
			assert.equal(await service.selected(context), root.id);
			assert.equal(
				(await service.conversations(100, null, context)).items.length,
				beforeArchiveFork.items.length,
			);
			const child = await service.fork(root.id, first.id, context);
			assert.equal(bound, child);
			assert.equal(await service.selected(context), child);
			const rows = await service.entries(child, 1, null, context);
			assert.deepEqual(
				rows.items.map((row) => row.id),
				[first.id],
			);
			assert.equal(
				rows.items.some((row) => row.id === second.id),
				false,
			);
			assert.equal(
				await harness.snapshot(authority, child, context),
				undefined,
			);
			assert.equal(await harness.snapshot(receipts, child, context), undefined);
			const childHandle = await harness.conversation(child, context);
			await childHandle.commit(async (tx) => {
				assert.deepEqual((await tx.doc(authority, child)).grants, []);
				assert.deepEqual((await tx.doc(receipts, child)).receipts, []);
			}, context);
			const grandchild = await service.fork(child, first.id, context);
			assert.notEqual(child, grandchild);
			await assert.rejects(service.fork(child, second.id, context));
			const page1 = await service.conversations(1, null, context);
			assert.ok(page1.next);
			const page2 = await service.conversations(
				1,
				JSON.parse(JSON.stringify(page1.next)),
				context,
			);
			assert.notEqual(page1.items[0].id, page2.items[0].id);
			await harness.close(context);
			harness = undefined;
			harness = await reopen();
			service = createSessionForks(harness, async (id) => {
				bound = id;
			});
			assert.equal(await service.selected(context), grandchild);
			await service.select(child, context);
			assert.equal(bound, child);
			assert.equal(faux.state.callCount, 0);
			const brokenBinding = createSessionForks(harness, async () => {
				throw new Error("rebind failed");
			});
			await assert.rejects(
				brokenBinding.select(grandchild, context),
				/rebind failed/,
			);
			assert.equal(await service.selected(context), grandchild);
		} finally {
			if (harness) await harness.close(context);
			await rm(dir, { recursive: true, force: true });
		}
	},
);

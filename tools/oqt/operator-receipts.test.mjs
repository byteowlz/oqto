import assert from "node:assert/strict";
import {
	copyFile,
	mkdtemp,
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
if (!source) throw new Error("OQT_PI_SOURCE must point to cdaf6c22 dependency");
const modules = await mkdtemp(join(tmpdir(), "oqt-receipt-test-"));
await symlink(join(source, "node_modules"), join(modules, "node_modules"));
await writeFile(
	join(modules, "loader.mjs"),
	"export const load = name => import(name);",
);
const { load } = await import(pathToFileURL(join(modules, "loader.mjs")));
const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);
const { Harness, createRegistry } = await load("@earendil-works/pi-durable");
const { createModels, fauxProvider } = await load("@earendil-works/pi-ai");
const { openNodeSqliteStorage } = await load(
	"@earendil-works/pi-durable/storage/sqlite/node",
);
await copyFile(
	new URL("./operator-receipts.ts", import.meta.url),
	join(modules, "operator-receipts.ts"),
);
const {
	installOperatorReceipts,
	validateOperatorResult,
	digestOperatorResult,
	OperatorResultsDoc,
	MAX_OPERATOR_RECEIPTS,
	MAX_OPERATOR_RESULT_BYTES,
} = await import(pathToFileURL(join(modules, "operator-receipts.ts")));
function peer() {
	const events = new Map();
	const calls = [];
	return {
		calls,
		loseAck: false,
		on(name, fn) {
			events.set(name, fn);
			return () => events.delete(name);
		},
		async call(name, params) {
			if (name === "operator_results_ready")
				assert.ok(events.has("operator_result"));
			calls.push({ name, params });
			if (name === "operator_result_committed" && this.loseAck) {
				this.loseAck = false;
				throw new Error("lost ACK");
			}
		},
		emit(raw) {
			events.get("operator_result")(raw);
		},
	};
}
const receipt = (extra = {}) => ({
	version: 2,
	session: "Session-A",
	conversationId: 1,
	id: "result-1",
	command: "printf hello",
	exitCode: 0,
	output: "hello",
	truncated: false,
	excluded: false,
	completedAt: 123,
	...extra,
});

test(
	"SQLite v2 receipts pin forks, recover every branch without replay, keep privacy and Session quota",
	{ timeout: 15000 },
	async () => {
		const directory = await mkdtemp(join(tmpdir(), "oqt-receipts-"));
		const database = join(directory, "native.sqlite");
		const faux = fauxProvider();
		const models = createModels();
		models.setProvider(faux.provider);
		let harness;
		let handler;
		const open = async () => {
			harness = await Harness.open(
				await openNodeSqliteStorage(database),
				{ models, registry: createRegistry() },
				context,
			);
			return harness.root(context, {
				agent: { model: { provider: "faux", modelId: "faux-1" } },
			});
		};
		try {
			let root = await open();
			const anchor = await root.commit(
				(tx) => tx.appendEntry(root.id, { kind: "fixture", data: {} }),
				context,
			);
			let fork = await root.fork(
				anchor.id,
				{ ownership: { kind: "ownerless" } },
				context,
			);
			const forkId = fork.id;
			let selected = root.id;
			let guard = peer();
			const install = () =>
				installOperatorReceipts(
					{
						harness,
						selectedConversationId: () => selected,
						session: "Session-A",
						guard,
					},
					context,
				);
			handler = await install();
			assert.deepEqual(
				guard.calls.map((c) => c.name),
				["operator_results_ready", "operator_results_ready"],
			);
			assert.equal(guard.calls[0].params.recovering, true);
			const pinned = receipt({ conversationId: forkId });
			// Selection changes before publication, but the immutable envelope still owns forkId.
			selected = root.id;
			guard.loseAck = true;
			guard.emit(pinned);
			await handler.drain();
			const first = guard.calls.find(
				(c) => c.name === "operator_result_committed",
			);
			assert.equal(first.params.conversationId, forkId);
			assert.equal(first.params.digest, digestOperatorResult(pinned));
			assert.equal(guard.calls.at(-1).name, "operator_result_failed");
			assert.equal(
				JSON.stringify((await root.context(context)).messages).includes(
					"hello",
				),
				false,
			);
			assert.equal(
				JSON.stringify((await fork.context(context)).messages).includes(
					"hello",
				),
				true,
			);
			assert.equal(
				(
					await handler.list(
						{ session: "Session-A", conversationId: root.id },
						context,
					)
				).length,
				0,
			);
			assert.equal(
				(
					await handler.list(
						{ session: "Session-A", conversationId: forkId },
						context,
					)
				).length,
				1,
			);
			for (const bad of [
				receipt({ output: "changed", conversationId: forkId }),
				receipt(),
				receipt({ session: "Session-B", conversationId: forkId }),
				receipt({ id: "foreign", conversationId: 999999 }),
				receipt({ id: "badconv", conversationId: "1" }),
			]) {
				guard.emit(bad);
				await handler.drain();
				assert.equal(guard.calls.at(-1).name, "operator_result_failed");
			}
			await assert.rejects(
				handler.list(
					{ session: "Session-B", conversationId: root.id },
					context,
				),
			);
			await assert.rejects(handler.list({ session: "Session-A" }, context));
			assert.throws(
				() => validateOperatorResult(receipt({ version: 1 })),
				/Invalid/,
			);
			assert.throws(
				() => validateOperatorResult({ ...pinned, extra: true }),
				/Invalid/,
			);
			const privateRaw = receipt({
				id: "opaque",
				excluded: true,
				command: "PRIVATE-COMMAND-1917",
				output: "PRIVATE-OUTPUT-1917",
				conversationId: forkId,
			});
			guard.emit(privateRaw);
			await handler.drain();
			assert.equal(guard.calls.at(-1).name, "operator_result_failed");
			guard.emit({ ...privateRaw, command: "", output: "" });
			await handler.drain();
			const opaque = (
				await handler.list(
					{ session: "Session-A", conversationId: forkId },
					context,
				)
			).find((r) => r.id === "opaque");
			assert.equal(opaque.payload, undefined);
			assert.equal(opaque.status, "excluded");
			// Durable write-ahead state from crash windows: one missing submission, one
			// queued on another branch, one terminal/cancelled. Recovery scans all branches.
			const queuedPayload = receipt({ id: "queued", conversationId: forkId });
			const cancelledPayload = receipt({
				id: "cancelled",
				conversationId: root.id,
			});
			await harness.commit(async (tx) => {
				const state = await tx.doc(OperatorResultsDoc);
				for (const payload of [
					receipt({ id: "doc-only", conversationId: forkId }),
					queuedPayload,
					cancelledPayload,
				])
					state.records.push({
						id: payload.id,
						conversationId: payload.conversationId,
						digest: digestOperatorResult(payload),
						excluded: false,
						status: "pending",
						payload,
					});
				for (const payload of [queuedPayload, cancelledPayload])
					await tx.createSubmission({
						conversationId: payload.conversationId,
						type: "write",
						status: "queued",
						requestId: `oq.operator-result:${payload.id}`,
					});
			}, context);
			const cancelled = await harness.commit(
				(tx) => tx.submissionByRequest(root.id, "oq.operator-result:cancelled"),
				context,
			);
			await harness.abortSubmission(cancelled.id, context);
			await handler.close();
			handler = undefined;
			await harness.close(context);
			harness = undefined;
			root = await open();
			fork = await harness.conversation(forkId, context);
			selected = root.id;
			guard = peer();
			handler = await install();
			const recovery = guard.calls.filter(
				(c) => c.name === "operator_result_committed",
			);
			assert.ok(
				recovery.some(
					(c) =>
						c.params.id === "result-1" &&
						c.params.conversationId === forkId &&
						c.params.submission === first.params.submission,
				),
			);
			const recoveredFork = await handler.list(
				{ session: "Session-A", conversationId: forkId },
				context,
			);
			const recoveredRoot = await handler.list(
				{ session: "Session-A", conversationId: root.id },
				context,
			);
			assert.equal(
				recoveredRoot.find((r) => r.id === "cancelled").status,
				"cancelled",
			);
			assert.equal(
				recoveredRoot.find((r) => r.id === "cancelled").submission,
				cancelled.id,
			);
			assert.ok(recoveredFork.find((r) => r.id === "doc-only").submission);
			assert.equal(
				(
					await harness.commit(
						(tx) => tx.submissionByRequest(forkId, "oq.operator-result:queued"),
						context,
					)
				).status,
				"queued",
				"ACK does not wait for blocked native placement",
			);
			guard.emit(pinned);
			await handler.drain();
			assert.equal(
				guard.calls.at(-1).params.submission,
				first.params.submission,
			);
			const child = await root.fork(
				anchor.id,
				{ ownership: { kind: "ownerless" } },
				context,
			);
			assert.deepEqual(
				await handler.list(
					{ session: "Session-A", conversationId: child.id },
					context,
				),
				[],
				"receipt bindings are not inherited by forks",
			);
			guard.emit(
				receipt({
					id: "too-large",
					output: "x".repeat(MAX_OPERATOR_RESULT_BYTES),
				}),
			);
			await handler.drain();
			assert.equal(guard.calls.at(-1).name, "operator_result_failed");
			assert.equal(
				digestOperatorResult(
					Object.fromEntries(Object.entries(pinned).reverse()),
				),
				digestOperatorResult(pinned),
			);
			await harness.commit(async (tx) => {
				const state = await tx.doc(OperatorResultsDoc);
				while (state.records.length < MAX_OPERATOR_RECEIPTS)
					state.records.push({
						id: `capacity-${state.records.length}`,
						conversationId: state.records.length % 2 ? root.id : forkId,
						digest: "opaque-hash",
						excluded: true,
						status: "excluded",
					});
			}, context);
			selected = child.id;
			guard.emit(receipt({ id: "overflow", conversationId: child.id }));
			await handler.drain();
			assert.equal(
				guard.calls.at(-1).name,
				"operator_result_failed",
				"fork navigation cannot reset Session quota",
			);
			assert.equal(
				(await harness.snapshot(OperatorResultsDoc, context)).records.length,
				MAX_OPERATOR_RECEIPTS,
			);
			guard.emit(pinned);
			await handler.drain();
			assert.equal(
				guard.calls.at(-1).params.submission,
				first.params.submission,
				"dedup still works at global cap",
			);
			assert.equal(faux.state.callCount, 0);
			await handler.close();
			handler = undefined;
			await harness.close(context);
			harness = undefined;
			const bytes = await readFile(database);
			assert.equal(bytes.includes(Buffer.from("PRIVATE-COMMAND-1917")), false);
			assert.equal(bytes.includes(Buffer.from("PRIVATE-OUTPUT-1917")), false);
		} finally {
			if (handler) await handler.close();
			if (harness) await harness.close(context);
			await rm(directory, { recursive: true, force: true });
			await rm(modules, { recursive: true, force: true });
		}
	},
);

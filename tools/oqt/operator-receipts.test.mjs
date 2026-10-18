import assert from "node:assert/strict";
import { copyFile, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

const source = process.env.OQT_PI_SOURCE;
if (!source) throw new Error("OQT_PI_SOURCE must point to cdaf6c22 dependency");
const modules = await mkdtemp(join(source, ".receipt-test-"));
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
// Resolve the tested module against the pinned native package graph, not user config.
await copyFile(
	new URL("./operator-receipts.ts", import.meta.url),
	join(modules, "operator-receipts.ts"),
);
const {
	installOperatorReceipts,
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
		on(name, fn) {
			events.set(name, fn);
			return () => events.delete(name);
		},
		async call(name, params) {
			if (name === "operator_results_ready")
				assert.ok(events.has("operator_result"));
			calls.push({ name, params });
		},
		emit(raw) {
			events.get("operator_result")(raw);
		},
	};
}
const receipt = (extra = {}) => ({
	version: 1,
	session: "Session-A",
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
	"native SQLite receipts bind payloads, recover admission and keep excluded bytes opaque",
	{ timeout: 15000 },
	async () => {
		const directory = await mkdtemp(join(tmpdir(), "oqt-receipts-"));
		const database = join(directory, "native.sqlite");
		const faux = fauxProvider();
		const models = createModels();
		models.setProvider(faux.provider);
		let harness;
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
			let conversation = await open();
			let guard = peer();
			let handler = await installOperatorReceipts(
				{ harness, conversation, session: "Session-A", guard },
				context,
			);
			guard.emit(receipt());
			await handler.drain();
			const first = guard.calls.at(-1);
			assert.equal(first.name, "operator_result_committed");
			assert.equal(first.params.digest, digestOperatorResult(receipt()));
			guard.emit(receipt({ output: "changed" }));
			await handler.drain();
			assert.equal(guard.calls.at(-1).name, "operator_result_failed");
			guard.emit(receipt({ session: "Session-B" }));
			await handler.drain();
			assert.equal(guard.calls.at(-1).name, "operator_result_failed");
			await assert.rejects(handler.list({ session: "Session-B" }, context));
			guard.emit(
				receipt({
					id: "opaque",
					excluded: true,
					command: "PRIVATE-COMMAND-1917",
					output: "PRIVATE-OUTPUT-1917",
				}),
			);
			await handler.drain();
			const opaque = (
				await handler.list({ session: "Session-A" }, context)
			).find((r) => r.id === "opaque");
			assert.equal(opaque.payload, undefined);
			assert.equal(opaque.status, "excluded");
			// A real queued native write remains unplaced: ACK must not wait for placement.
			const queuedPayload = receipt({ id: "queued" });
			const cancelledPayload = receipt({ id: "cancelled" });
			await harness.commit(async (tx) => {
				const state = await tx.doc(OperatorResultsDoc, conversation.id);
				for (const payload of [
					receipt({ id: "doc-only" }),
					queuedPayload,
					cancelledPayload,
				]) {
					state.records.push({
						id: payload.id,
						digest: digestOperatorResult(payload),
						excluded: false,
						status: "pending",
						payload,
					});
				}
				for (const payload of [queuedPayload, cancelledPayload])
					await tx.createSubmission({
						conversationId: conversation.id,
						type: "write",
						status: "queued",
						requestId: `oq.operator-result:${payload.id}`,
					});
			}, context);
			const cancelled = await harness.commit(
				(tx) =>
					tx.submissionByRequest(
						conversation.id,
						"oq.operator-result:cancelled",
					),
				context,
			);
			await harness.abortSubmission(cancelled.id, context);
			await handler.close();
			await harness.close(context);
			harness = undefined;
			conversation = await open();
			guard = peer();
			handler = await installOperatorReceipts(
				{ harness, conversation, session: "Session-A", guard },
				context,
			);
			guard.emit(receipt());
			await handler.drain();
			assert.equal(
				guard.calls.at(-1).params.submission,
				first.params.submission,
			);
			const recovered = await handler.list({ session: "Session-A" }, context);
			assert.equal(
				recovered.find((r) => r.id === "cancelled").status,
				"cancelled",
			);
			assert.equal(
				recovered.find((r) => r.id === "cancelled").submission,
				cancelled.id,
			);
			assert.ok(recovered.find((r) => r.id === "doc-only").submission);
			const queued = await harness.commit(
				(tx) =>
					tx.submissionByRequest(conversation.id, "oq.operator-result:queued"),
				context,
			);
			assert.equal(
				queued.status,
				"queued",
				"ACK returned while native placement remains blocked",
			);
			const view = await conversation.context(context);
			assert.ok(JSON.stringify(view.messages).includes("hello"));
			assert.equal(faux.state.callCount, 0);
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
					Object.fromEntries(Object.entries(receipt()).reverse()),
				),
				digestOperatorResult(receipt()),
			);
			await harness.commit(async (tx) => {
				const state = await tx.doc(OperatorResultsDoc, conversation.id);
				while (state.records.length < MAX_OPERATOR_RECEIPTS)
					state.records.push({
						id: `capacity-${state.records.length}`,
						digest: "opaque-hash",
						excluded: true,
						status: "excluded",
					});
			}, context);
			guard.emit(receipt({ id: "overflow" }));
			await handler.drain();
			assert.equal(guard.calls.at(-1).name, "operator_result_failed");
			assert.equal(
				(await handler.list({ session: "Session-A" }, context)).length,
				MAX_OPERATOR_RECEIPTS,
			);
			guard.emit(receipt());
			await handler.drain();
			assert.equal(
				guard.calls.at(-1).params.submission,
				first.params.submission,
			);
			await handler.close();
			await harness.close(context);
			harness = undefined;
			const bytes = await readFile(database);
			assert.equal(bytes.includes(Buffer.from("PRIVATE-COMMAND-1917")), false);
			assert.equal(bytes.includes(Buffer.from("PRIVATE-OUTPUT-1917")), false);
		} finally {
			if (harness) await harness.close(context);
			await rm(directory, { recursive: true, force: true });
			await rm(modules, { recursive: true, force: true });
		}
	},
);

import { createHash } from "node:crypto";
import { type Context, defineService } from "@earendil-works/chord";
import {
	type Conversation,
	defineDoc,
	type Harness,
} from "@earendil-works/pi-durable";

export type OperatorResult = {
	version: 1;
	session: string;
	id: string;
	command: string;
	exitCode: number;
	output: string;
	truncated: boolean;
	excluded: boolean;
	completedAt: number;
};
export const MAX_OPERATOR_RECEIPTS = 128;
export const MAX_OPERATOR_RESULT_BYTES = 28 * 1024;
const fields = [
	"version",
	"session",
	"id",
	"command",
	"exitCode",
	"output",
	"truncated",
	"excluded",
	"completedAt",
] as const;
export function validateOperatorResult(raw: unknown): OperatorResult {
	const fail = () => {
		throw new Error("Invalid operator receipt");
	};
	if (!raw || typeof raw !== "object" || Array.isArray(raw)) return fail();
	const r = raw as Record<string, unknown>;
	if (
		Object.keys(r).length !== fields.length ||
		fields.some((key) => !Object.hasOwn(r, key))
	)
		return fail();
	if (
		r.version !== 1 ||
		typeof r.session !== "string" ||
		!r.session ||
		r.session.length > 256 ||
		typeof r.id !== "string" ||
		!r.id ||
		r.id.length > 256 ||
		typeof r.command !== "string" ||
		typeof r.output !== "string" ||
		!Number.isSafeInteger(r.exitCode) ||
		!Number.isSafeInteger(r.completedAt) ||
		(r.completedAt as number) < 0 ||
		typeof r.truncated !== "boolean" ||
		typeof r.excluded !== "boolean"
	)
		return fail();
	const result = Object.fromEntries(
		fields.map((key) => [key, r[key]]),
	) as OperatorResult;
	// Leave room for the native entry wrapper, digest, and model rendering.
	if (Buffer.byteLength(JSON.stringify(result)) >= MAX_OPERATOR_RESULT_BYTES)
		return fail();
	return result;
}
export function digestOperatorResult(raw: unknown): string {
	return createHash("sha256")
		.update(JSON.stringify(validateOperatorResult(raw)))
		.digest("hex");
}
type ReceiptRecord = {
	id: string;
	digest: string;
	excluded: boolean;
	status: string;
	payload?: OperatorResult;
	submission?: number;
};
export const OperatorResultsDoc = defineDoc<{
	session: string;
	records: ReceiptRecord[];
}>({
	kind: "oq.operator-results.v1",
	version: 1,
	scope: "conversation",
	history: "latest",
	fork: "initial",
	initial: () => ({ session: "", records: [] }),
});
export interface OperatorResults {
	list(params: { session: string }, context: Context): Promise<ReceiptRecord[]>;
}
export const OperatorResults = defineService<OperatorResults>(
	"oq.operator-results",
);
type Guard = {
	on(event: string, listener: (data: unknown) => void): () => void;
	call(method: string, params?: unknown): Promise<unknown>;
};

/** Harness-owned write-ahead binding. Native requestId dedup is NOT a payload binding. */
export async function installOperatorReceipts(
	options: {
		harness: Harness;
		conversation: Conversation;
		session: string;
		guard: Guard;
	},
	context: Context,
) {
	const { harness, conversation, session, guard } = options;
	const mutate = <T>(
		fn: (state: { session: string; records: ReceiptRecord[] }) => T,
	) =>
		harness.commit(async (tx) => {
			const state = await tx.doc(OperatorResultsDoc, conversation.id);
			if (state.session && state.session !== session)
				throw new Error("Operator receipt Session mismatch");
			state.session = session;
			return fn(state);
		}, context);
	const list = async (params: { session: string }, _context: Context) => {
		if (params.session !== session)
			throw new Error("Operator receipt Session mismatch");
		const state = await harness.snapshot(
			OperatorResultsDoc,
			conversation.id,
			_context,
		);
		if (state?.session && state.session !== session)
			throw new Error("Operator receipt Session mismatch");
		return structuredClone(state?.records ?? []);
	};
	const admit = async (record: ReceiptRecord) => {
		if (record.excluded)
			return {
				id: record.id,
				digest: record.digest,
				conversation: conversation.id,
			};
		const payload = record.payload;
		if (!payload) throw new Error("Operator receipt payload unavailable");
		const requestId = `oq.operator-result:${record.id}`;
		// Includes terminal unanswered/user-aborted writes: never requeue them.
		const existing = await harness.commit(
			(tx) => tx.submissionByRequest(conversation.id, requestId),
			context,
		);
		const submission =
			existing ??
			(await conversation.submit(
				{
					type: "write",
					requestId,
					entry: {
						kind: "oq.operator-result.v1",
						data: { ...payload, digest: record.digest },
						model: [
							{
								role: "user",
								content: `Operator shell result\n$ ${payload.command}\n${payload.output}\nExit code: ${payload.exitCode}${payload.truncated ? " (truncated)" : ""}`,
								timestamp: payload.completedAt,
							},
						],
					},
				},
				context,
			));
		await mutate((state) => {
			const stored = state.records.find((item) => item.id === record.id);
			if (!stored) throw new Error("Operator receipt binding unavailable");
			stored.submission = submission.id;
			stored.status =
				existing?.status === "unanswered" ? "cancelled" : "admitted";
		});
		return {
			id: record.id,
			digest: record.digest,
			conversation: conversation.id,
			submission: submission.id,
		};
	};
	let tail: Promise<unknown> = Promise.resolve();
	const processReceipt = async (raw: unknown) => {
		let id = "invalid";
		try {
			const receipt = validateOperatorResult(raw);
			id = receipt.id;
			if (receipt.session !== session) throw new Error("Session mismatch");
			const digest = digestOperatorResult(receipt);
			const record = await mutate((state) => {
				const existing = state.records.find((item) => item.id === id);
				if (existing) {
					if (existing.digest !== digest) throw new Error("Payload mismatch");
					return JSON.parse(JSON.stringify(existing)) as ReceiptRecord;
				}
				if (state.records.length >= MAX_OPERATOR_RECEIPTS)
					throw new Error("Capacity exhausted");
				const record: ReceiptRecord = receipt.excluded
					? { id, digest, excluded: true, status: "excluded" }
					: {
							id,
							digest,
							excluded: false,
							status: "pending",
							payload: receipt,
						};
				state.records.push(record);
				return JSON.parse(JSON.stringify(record)) as ReceiptRecord;
			});
			await guard.call("operator_result_committed", await admit(record));
		} catch {
			// Never reflect raw payloads or native diagnostics (especially !!) back into logs.
			await guard.call("operator_result_failed", {
				id,
				message: "Operator receipt rejected or durable admission unavailable",
			});
		}
	};
	const off = guard.on("operator_result", (raw) => {
		tail = tail.then(() => processReceipt(raw)).catch(() => {});
	});
	try {
		for (const record of await list({ session }, context)) {
			await guard.call("operator_result_committed", await admit(record));
		}
		await guard.call("operator_results_ready");
	} catch {
		off();
		await tail;
		throw new Error("Operator receipt recovery unavailable");
	}
	return {
		list,
		async close() {
			off();
			await tail;
		},
		drain: () => tail,
	};
}

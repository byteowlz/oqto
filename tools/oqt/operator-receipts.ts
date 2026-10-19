import { createHash } from "node:crypto";
import { type Context, defineService } from "@earendil-works/chord";
import {
	type ConversationId,
	defineDoc,
	type Harness,
} from "@earendil-works/pi-durable";

export type OperatorResult = {
	version: 2;
	session: string;
	conversationId: number;
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
	"conversationId",
	"id",
	"command",
	"exitCode",
	"output",
	"truncated",
	"excluded",
	"completedAt",
] as const;
export function validReceiptConversation(value: unknown): value is number {
	return Number.isSafeInteger(value) && (value as number) > 0;
}
export function validateOperatorResult(raw: unknown): OperatorResult {
	const fail = (): never => {
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
		r.version !== 2 ||
		typeof r.session !== "string" ||
		!r.session ||
		r.session.length > 256 ||
		!validReceiptConversation(r.conversationId) ||
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
	// Private operator commands never cross the worker boundary, even on retries.
	if (r.excluded && (r.command !== "" || r.output !== "")) return fail();
	const result = Object.fromEntries(
		fields.map((key) => [key, r[key]]),
	) as OperatorResult;
	if (Buffer.byteLength(JSON.stringify(result)) >= MAX_OPERATOR_RESULT_BYTES)
		return fail();
	return result;
}
export function digestOperatorResult(raw: unknown): string {
	return createHash("sha256")
		.update(JSON.stringify(validateOperatorResult(raw)))
		.digest("hex");
}
export type ReceiptRecord = {
	id: string;
	conversationId: number;
	digest: string;
	excluded: boolean;
	status: string;
	payload?: OperatorResult;
	submission?: number;
};
type ReceiptIndex = { session: string; records: ReceiptRecord[] };
/** One index per Session. It is never copied into a fork, and its quota cannot
 * be reset by navigating to another conversation. Excluded records are opaque. */
export const OperatorResultsDoc = defineDoc<ReceiptIndex>({
	kind: "oq.operator-results.v2",
	version: 2,
	scope: "session",
	initial: () => ({ session: "", records: [] }),
});
export interface OperatorResults {
	list(
		params: { session: string; conversationId: number },
		context: Context,
	): Promise<ReceiptRecord[]>;
}
export const OperatorResults = defineService<OperatorResults>(
	"oq.operator-results",
);
type Guard = {
	on(event: string, listener: (data: unknown) => void): () => void;
	call(method: string, params?: unknown): Promise<unknown>;
};

/** Payload binding and recovery are Session-owned; delivery always reacquires
 * the exact native destination recorded before effects, never current selection. */
export async function installOperatorReceipts(
	options: {
		harness: Harness;
		session: string;
		guard: Guard;
		selectedConversationId: () => ConversationId;
	},
	context: Context,
) {
	const { harness, session, guard, selectedConversationId } = options;
	const destination = async (id: number, ctx: Context) => {
		if (!validReceiptConversation(id))
			throw new Error("Invalid receipt conversation");
		const conversation = await harness.conversation(id as ConversationId, ctx);
		if (!conversation) throw new Error("Unknown receipt conversation");
		return conversation;
	};
	const mutate = <T>(fn: (state: ReceiptIndex) => T) =>
		harness.commit(async (tx) => {
			const state = await tx.doc(OperatorResultsDoc);
			if (state.session && state.session !== session)
				throw new Error("Operator receipt Session mismatch");
			state.session = session;
			return fn(state);
		}, context);
	const all = async (ctx: Context): Promise<ReceiptRecord[]> => {
		const state = await harness.snapshot(OperatorResultsDoc, ctx);
		if (state?.session && state.session !== session)
			throw new Error("Operator receipt Session mismatch");
		return JSON.parse(JSON.stringify(state?.records ?? []));
	};
	const list = async (
		params: { session: string; conversationId: number },
		ctx: Context,
	) => {
		if (params.session !== session)
			throw new Error("Operator receipt Session mismatch");
		await destination(params.conversationId, ctx);
		return (await all(ctx)).filter(
			(record) => record.conversationId === params.conversationId,
		);
	};
	const admit = async (record: ReceiptRecord) => {
		const conversation = await destination(record.conversationId, context);
		const ack = {
			id: record.id,
			digest: record.digest,
			conversationId: record.conversationId,
		};
		if (record.excluded) return ack;
		const payload = record.payload;
		if (
			!payload ||
			payload.conversationId !== record.conversationId ||
			payload.id !== record.id ||
			payload.session !== session ||
			digestOperatorResult(payload) !== record.digest
		)
			throw new Error("Operator receipt binding unavailable");
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
						kind: "oq.operator-result.v2",
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
			if (
				!stored ||
				stored.conversationId !== record.conversationId ||
				stored.digest !== record.digest
			)
				throw new Error("Operator receipt binding unavailable");
			stored.submission = submission.id;
			stored.status =
				existing?.status === "unanswered" ? "cancelled" : "admitted";
		});
		return { ...ack, submission: submission.id };
	};
	let tail: Promise<unknown> = Promise.resolve();
	const processReceipt = async (raw: unknown) => {
		let id = "invalid";
		try {
			const receipt = validateOperatorResult(raw);
			id = receipt.id;
			if (receipt.session !== session) throw new Error("Session mismatch");
			await destination(receipt.conversationId, context);
			const digest = digestOperatorResult(receipt);
			const record = await mutate((state) => {
				const existing = state.records.find((item) => item.id === id);
				if (existing) {
					if (
						existing.digest !== digest ||
						existing.conversationId !== receipt.conversationId
					)
						throw new Error("Payload mismatch");
					return JSON.parse(JSON.stringify(existing)) as ReceiptRecord;
				}
				if (state.records.length >= MAX_OPERATOR_RECEIPTS)
					throw new Error("Capacity exhausted");
				const record: ReceiptRecord = {
					id,
					conversationId: receipt.conversationId,
					digest,
					excluded: receipt.excluded,
					status: receipt.excluded ? "excluded" : "pending",
					...(!receipt.excluded ? { payload: receipt } : {}),
				};
				state.records.push(record);
				return JSON.parse(JSON.stringify(record)) as ReceiptRecord;
			});
			await guard.call("operator_result_committed", await admit(record));
		} catch {
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
		const conversationId = selectedConversationId();
		await destination(conversationId, context);
		await guard.call("operator_results_ready", {
			conversationId,
			recovering: true,
		});
		// Recover every branch, including sealed receipts whose ACK was lost while a
		// different conversation became selected. No grant or tool call is replayed.
		for (const record of await all(context))
			await guard.call("operator_result_committed", await admit(record));
		await guard.call("operator_results_ready", { conversationId });
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

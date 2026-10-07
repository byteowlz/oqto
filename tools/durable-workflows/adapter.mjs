import { createHash } from "node:crypto";
import { isAbsolute, join } from "node:path";

export const WORKFLOW_RUNTIME = "pi-durable@1.0.4";
const SCOPE_KEYS = [
	"accountId",
	"principalId",
	"workspaceId",
	"workDirectoryId",
	"sessionId",
];
const ID = /^[a-zA-Z0-9:_-]{1,200}$/;
const digest = (value) =>
	createHash("sha256").update(JSON.stringify(value)).digest("hex");
const opaque = (value) =>
	typeof value === "string" &&
	ID.test(value) &&
	!value.startsWith("pending-") &&
	!value.startsWith("tmp:");
// Durable1.0.4 IDs are positive branded numbers, scoped to a native store.
const nativeId = (value) => Number.isSafeInteger(value) && value > 0;
const validScope = (scope) =>
	scope &&
	typeof scope === "object" &&
	Object.keys(scope).length === SCOPE_KEYS.length &&
	SCOPE_KEYS.every((key) => opaque(scope[key]));
const sameScope = (left, right) =>
	validScope(left) &&
	validScope(right) &&
	SCOPE_KEYS.every((key) => left[key] === right[key]);
const scopeKey = (scope) => SCOPE_KEYS.map((key) => scope[key]);
const denied = (code, effects = "none") => ({
	ok: false,
	code,
	effects,
	recurrenceActive: false,
});

/**
 * Trusted host seam, not an authentication endpoint. The caller derives scope
 * from Oqto routing; authority reads protected records, never proposal reviews.
 * This module does not implement the protected repository or public transport.
 */
export function createWorkflowAdapter({
	authority,
	executor,
	validateReviewedProposal,
	models,
	storeRoot,
	maxConcurrent = 4,
	now = Date.now,
}) {
	const available =
		authority &&
		typeof authority.resolve === "function" &&
		typeof authority.recordReceipt === "function" &&
		executor?.capabilities?.runtime === WORKFLOW_RUNTIME &&
		typeof executor.runOccurrence === "function" &&
		typeof validateReviewedProposal === "function" &&
		models &&
		typeof storeRoot === "string" &&
		isAbsolute(storeRoot) &&
		storeRoot !== "/" &&
		Number.isSafeInteger(maxConcurrent) &&
		maxConcurrent >= 1 &&
		maxConcurrent <= 64;
	const active = new Set();

	function permits(record, scope, jobId, occurrenceId) {
		return (
			record?.approved === true &&
			sameScope(record.scope, scope) &&
			record.jobId === jobId &&
			record.occurrenceId === occurrenceId &&
			opaque(record.runId) &&
			opaque(record.bindingId) &&
			Number.isSafeInteger(record.grantRevision) &&
			record.grantRevision >= 0 &&
			Number.isSafeInteger(record.revision) &&
			record.revision >= 0 &&
			typeof record.digest === "string" &&
			/^[a-f0-9]{64}$/.test(record.digest) &&
			record.runtime === WORKFLOW_RUNTIME &&
			Number.isSafeInteger(record.expiresAt) &&
			record.expiresAt > now()
		);
	}

	return {
		capabilities: Object.freeze({
			manualRun: Boolean(available),
			scheduling: false,
			tools: Object.freeze([]),
			subagents: false,
			runtime: WORKFLOW_RUNTIME,
		}),

		async run(actingScope, request) {
			if (!available) return denied("adapter_unavailable");
			if (
				!validScope(actingScope) ||
				!request ||
				typeof request !== "object" ||
				Object.keys(request).length !== 2 ||
				!opaque(request.jobId) ||
				!opaque(request.occurrenceId)
			)
				return denied("request_invalid");
			// Snapshot authenticated facts; a caller cannot mutate routing during a run.
			const scope = Object.freeze({ ...actingScope });
			const { jobId, occurrenceId } = request;
			let record;
			try {
				record = structuredClone(
					await authority.resolve({
						scope,
						jobId,
						occurrenceId,
						phase: "prepare",
					}),
				);
			} catch {
				return denied("authority_unavailable");
			}
			if (!permits(record, scope, jobId, occurrenceId))
				return denied("admission_denied");
			let reviewed;
			try {
				reviewed = await validateReviewedProposal(record.proposal);
			} catch {
				return denied("review_invalid");
			}
			if (
				!reviewed.ok ||
				reviewed.value.revision !== record.revision ||
				reviewed.value.digest !== record.digest
			)
				return denied("review_invalid");
			const proposal = reviewed.value;
			if (
				proposal.definition.tools.length ||
				proposal.definition.artifacts.length ||
				proposal.definition.allowSubagents
			)
				return denied("capability_unavailable");
			const binding = [
				scopeKey(scope),
				record.bindingId,
				jobId,
				record.revision,
				record.digest,
			];
			const bindingKey = digest(binding);
			const ownerKey = digest([scopeKey(scope), record.bindingId, jobId]);
			if (active.has(ownerKey) || active.size >= maxConcurrent)
				return denied("run_busy");
			active.add(ownerKey);
			try {
				const result = await executor.runOccurrence(proposal, occurrenceId, {
					models,
					// Path is derived from a protected binding, NEVER native source.cwd.
					storeDir: join(storeRoot, bindingKey),
					async authorize(native) {
						if (native.phase !== "admit" && native.phase !== "request")
							return { approved: false };
						if (
							native.occurrenceId !== occurrenceId ||
							native.proposal.id !== proposal.id ||
							native.proposal.revision !== proposal.revision ||
							native.proposal.digest !== proposal.digest
						)
							return { approved: false };
						if (
							native.phase === "request" &&
							(!nativeId(native.conversationId) || !nativeId(native.taskId))
						)
							return { approved: false };
						try {
							const current = await authority.resolve({
								scope,
								jobId,
								occurrenceId,
								phase: native.phase,
								conversationId: native.conversationId,
								taskId: native.taskId,
							});
							return {
								approved:
									permits(current, scope, jobId, occurrenceId) &&
									current.runId === record.runId &&
									current.bindingId === record.bindingId &&
									current.grantRevision === record.grantRevision &&
									current.revision === record.revision &&
									current.digest === record.digest,
							};
						} catch {
							return { approved: false };
						}
					},
				});
				// Expose bounded native identities only as annotations. Failure messages
				// from SDK/hooks may contain private paths; do not forward them.
				const effects = result?.effects === "none" ? "none" : "possible";
				const nativeIdsValid =
					nativeId(result?.conversationId) &&
					nativeId(result?.submissionId) &&
					result.proposalId === proposal.id &&
					result.revision === proposal.revision &&
					result.digest === proposal.digest &&
					result.occurrenceId === occurrenceId;
				const completed =
					result?.ok === true &&
					result.status === "done" &&
					nativeIdsValid &&
					typeof result.output === "string" &&
					Buffer.byteLength(result.output) <=
						proposal.definition.limits.maxOutputBytes;
				const receipt = {
					schemaVersion: 1,
					scope,
					jobId,
					runId: record.runId,
					occurrenceId,
					revision: record.revision,
					digest: record.digest,
					runtime: WORKFLOW_RUNTIME,
					ok: completed,
					effects,
					recurrenceActive: false,
					status: completed ? "done" : "failed",
					code: completed ? undefined : "native_run_failed",
					native: nativeIdsValid
						? {
								bindingId: record.bindingId,
								conversationId: result.conversationId,
								submissionId: result.submissionId,
							}
						: undefined,
					output: completed ? result.output : undefined,
					truncated: completed ? result.truncated === true : undefined,
				};
				try {
					await authority.recordReceipt({
						scope,
						jobId,
						runId: record.runId,
						receipt,
					});
				} catch {
					return {
						...denied("receipt_unavailable", effects),
						jobId,
						runId: record.runId,
						native: receipt.native,
					};
				}
				return receipt;
			} catch {
				// An unhandled native exception has unknown effects, not safe-to-retry.
				return denied("native_run_failed", "possible");
			} finally {
				active.delete(ownerKey);
			}
		},
	};
}

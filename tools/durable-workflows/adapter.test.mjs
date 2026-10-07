import assert from "node:assert/strict";
import { test } from "node:test";
import { WORKFLOW_RUNTIME, createWorkflowAdapter } from "./adapter.mjs";

const scope = {
	accountId: "account",
	principalId: "principal",
	workspaceId: "workspace",
	workDirectoryId: "workdir",
	sessionId: "platform-session",
};
const request = { jobId: "public-job", occurrenceId: "manual-1" };
function fixture(overrides = {}) {
	const proposal = {
		id: "native-proposal",
		revision: 1,
		digest: "a".repeat(64),
		definition: {
			tools: [],
			artifacts: [],
			allowSubagents: false,
			source: { cwd: "/untrusted/model/selected/path" },
			limits: { maxOutputBytes: 100 },
		},
	};
	const record = {
		approved: true,
		scope: { ...scope },
		jobId: request.jobId,
		occurrenceId: request.occurrenceId,
		runId: "public-run",
		bindingId: "protected-binding",
		grantRevision: 1,
		revision: 1,
		digest: proposal.digest,
		runtime: WORKFLOW_RUNTIME,
		expiresAt: 2000,
		proposal,
	};
	const phases = [];
	const receipts = [];
	let executions = 0;
	let providerCalls = 0;
	let storeDir;
	const authority = {
		async resolve(input) {
			phases.push(input);
			return { ...record, occurrenceId: input.occurrenceId };
		},
		async recordReceipt(input) {
			receipts.push(input);
		},
	};
	const executor = {
		capabilities: { runtime: WORKFLOW_RUNTIME },
		async runOccurrence(p, occurrenceId, adapters) {
			executions++;
			storeDir = adapters.storeDir;
			if (
				!(
					await adapters.authorize({
						proposal: p,
						occurrenceId,
						phase: "admit",
					})
				).approved
			)
				return { ok: false, effects: "none" };
			if (
				!(
					await adapters.authorize({
						proposal: p,
						occurrenceId,
						phase: "request",
						conversationId: 1,
						taskId: 3,
					})
				).approved
			)
				return { ok: false, effects: "possible" };
			providerCalls++;
			return {
				ok: true,
				effects: "possible",
				status: "done",
				proposalId: p.id,
				occurrenceId,
				revision: p.revision,
				digest: p.digest,
				conversationId: 1,
				submissionId: 2,
				output: "garden",
				truncated: false,
			};
		},
	};
	const deps = {
		authority,
		executor,
		models: {},
		storeRoot: "/protected/runner/workflows",
		now: () => 1000,
		validateReviewedProposal: async (p) => ({ ok: true, value: p }),
		...overrides,
	};
	return {
		adapter: createWorkflowAdapter(deps),
		deps,
		proposal,
		record,
		phases,
		receipts,
		get executions() {
			return executions;
		},
		get providerCalls() {
			return providerCalls;
		},
		get storeDir() {
			return storeDir;
		},
	};
}

test("manual run binds public identities and private storage; no scheduler or tools", async () => {
	const f = fixture();
	const receipt = await f.adapter.run(scope, request);
	assert.equal(receipt.ok, true);
	assert.equal(receipt.jobId, "public-job");
	assert.equal(receipt.runId, "public-run");
	assert.equal(receipt.scope.sessionId, "platform-session");
	assert.equal(receipt.native.conversationId, 1);
	assert.equal(receipt.native.bindingId, "protected-binding");
	assert.equal(receipt.output, "garden");
	assert.deepEqual(
		f.phases.map((p) => p.phase),
		["prepare", "admit", "request"],
	);
	assert.match(f.storeDir, /^\/protected\/runner\/workflows\/[a-f0-9]{64}$/);
	assert.equal(f.storeDir.includes("/untrusted"), false);
	assert.equal(f.receipts.length, 1);
	assert.equal(f.adapter.capabilities.scheduling, false);
});

test("missing protected authority, wrong runtime and unsafe root fail closed", async () => {
	for (const override of [
		{ authority: null },
		{ executor: { capabilities: { runtime: "pi-durable@unknown" } } },
		{ storeRoot: "/" },
		{ storeRoot: "../store" },
		{ models: null },
	]) {
		const f = fixture(override);
		assert.equal(
			(await f.adapter.run(scope, request)).code,
			"adapter_unavailable",
		);
		assert.equal(f.executions, 0);
	}
});

test("request cannot supply actor identity, paths or provisional IDs", async () => {
	for (const bad of [
		{ ...request, accountId: "other" },
		{ ...request, storeDir: "/tmp" },
		{ ...request, jobId: "tmp:job" },
		{ ...request, occurrenceId: "../escape" },
	]) {
		const f = fixture();
		assert.equal((await f.adapter.run(scope, bad)).code, "request_invalid");
		assert.equal(f.executions, 0);
	}
	assert.equal(
		(
			await fixture().adapter.run(
				{ ...scope, sessionId: "pending-session" },
				request,
			)
		).code,
		"request_invalid",
	);
});

for (const key of Object.keys(scope)) {
	test(`denied cross-${key} before native storage/provider`, async () => {
		const f = fixture();
		assert.equal(
			(await f.adapter.run({ ...scope, [key]: "other" }, request)).code,
			"admission_denied",
		);
		assert.equal(f.executions, 0);
		assert.equal(f.providerCalls, 0);
	});
}

test("expired, revoked, wrong revision/digest and unsupported capability denied", async () => {
	for (const mutate of [
		(f) => {
			f.record.expiresAt = 1000;
		},
		(f) => {
			f.record.approved = false;
		},
		(f) => {
			f.record.revision++;
		},
		(f) => {
			f.record.digest = "b".repeat(64);
		},
		(f) => {
			f.proposal.definition.tools = ["bash"];
		},
		(f) => {
			f.proposal.definition.artifacts = ["generated-script"];
		},
		(f) => {
			f.proposal.definition.allowSubagents = true;
		},
	]) {
		const f = fixture();
		mutate(f);
		assert.equal((await f.adapter.run(scope, request)).ok, false);
		assert.equal(f.executions, 0);
	}
});

test("invalid shared-core review denied before executor", async () => {
	const f = fixture({ validateReviewedProposal: async () => ({ ok: false }) });
	assert.equal((await f.adapter.run(scope, request)).code, "review_invalid");
	assert.equal(f.executions, 0);
});

test("revoke, rotate or throw at native request denies actual provider", async () => {
	for (const change of ["revoke", "rotate", "scope", "expiry", "throw"]) {
		const f = fixture();
		f.deps.authority.resolve = async (input) => {
			if (input.phase === "request") {
				if (change === "throw") throw new Error("private credential/path");
				if (change === "revoke") f.record.approved = false;
				if (change === "rotate") f.record.grantRevision++;
				if (change === "scope") f.record.scope.accountId = "other";
				if (change === "expiry") f.record.expiresAt = 999;
			}
			return { ...f.record, occurrenceId: input.occurrenceId };
		};
		const result = await f.adapter.run(scope, request);
		assert.equal(result.ok, false);
		assert.equal(f.providerCalls, 0);
		assert.equal(JSON.stringify(result).includes("private credential"), false);
	}
});

test("concurrent same-binding run rejected; lease released after settlement", async () => {
	let release;
	const waiting = new Promise((resolve) => {
		release = resolve;
	});
	let entered;
	const started = new Promise((resolve) => {
		entered = resolve;
	});
	const f = fixture();
	const original = f.deps.executor.runOccurrence;
	f.deps.executor.runOccurrence = async (...args) => {
		entered();
		await waiting;
		return original(...args);
	};
	const first = f.adapter.run(scope, request);
	await started;
	assert.equal(
		(await f.adapter.run(scope, { ...request, occurrenceId: "second" })).code,
		"run_busy",
	);
	release();
	assert.equal((await first).ok, true);
	assert.equal((await f.adapter.run(scope, request)).ok, true);
});

test("receipt write failure exposes uncertain effects, never success or blind replay", async () => {
	const f = fixture();
	f.deps.authority.recordReceipt = async () => {
		throw new Error("private database secret");
	};
	const result = await f.adapter.run(scope, request);
	assert.equal(result.code, "receipt_unavailable");
	assert.equal(result.effects, "possible");
	assert.equal(result.native.submissionId, 2);
	assert.equal(f.providerCalls, 1);
	assert.equal(JSON.stringify(result).includes("private database"), false);
});

test("oversized or mismatched native result is not a successful receipt", async () => {
	for (const corrupt of [
		(r) => {
			r.output = "x".repeat(101);
		},
		(r) => {
			r.occurrenceId = "wrong";
		},
		(r) => {
			r.submissionId = "tmp:fake";
		},
		(r) => {
			r.conversationId = 0;
		},
		(r) => {
			r.conversationId = Number.MAX_SAFE_INTEGER + 1;
		},
	]) {
		const f = fixture();
		const original = f.deps.executor.runOccurrence;
		f.deps.executor.runOccurrence = async (...args) => {
			const result = await original(...args);
			corrupt(result);
			return result;
		};
		const result = await f.adapter.run(scope, request);
		assert.equal(result.ok, false);
		assert.equal(result.effects, "possible");
		assert.equal(result.output, undefined);
		if (result.native)
			assert.equal(result.native.bindingId, "protected-binding");
	}
});

test("authority must bind the exact occurrence, not merely a reviewed job", async () => {
	const f = fixture();
	f.deps.authority.resolve = async () => ({
		...f.record,
		occurrenceId: "other-occurrence",
	});
	assert.equal((await f.adapter.run(scope, request)).code, "admission_denied");
	assert.equal(f.executions, 0);
});

test("host concurrency is bounded across distinct protected bindings", async () => {
	const f = fixture({ maxConcurrent: 1 });
	const resolve = f.deps.authority.resolve;
	f.deps.authority.resolve = async (input) => ({
		...(await resolve(input)),
		jobId: input.jobId,
	});
	let release;
	const waiting = new Promise((r) => {
		release = r;
	});
	let entered;
	const started = new Promise((r) => {
		entered = r;
	});
	const execute = f.deps.executor.runOccurrence;
	f.deps.executor.runOccurrence = async (...args) => {
		entered();
		await waiting;
		return execute(...args);
	};
	const first = f.adapter.run(scope, request);
	await started;
	assert.equal(
		(await f.adapter.run(scope, { ...request, jobId: "another-job" })).code,
		"run_busy",
	);
	release();
	assert.equal((await first).ok, true);
});

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { WORKFLOW_RUNTIME, createWorkflowAdapter } from "./adapter.mjs";

const executorDir = process.env.OQTO_WORKFLOW_EXECUTOR_DIR;
assert.ok(
	executorDir,
	"Set OQTO_WORKFLOW_EXECUTOR_DIR to a disposable built exact1.0.4 executor",
);
const nativeRequire = createRequire(join(executorDir, "package.json"));
const executor = await import(pathToFileURL(join(executorDir, "executor.mjs")));
const core = await import(pathToFileURL(join(executorDir, "dist/core.js")));
const { createModels } = await import(
	pathToFileURL(nativeRequire.resolve("@earendil-works/pi-ai/models"))
);
const { fauxProvider, fauxAssistantMessage } = await import(
	pathToFileURL(nativeRequire.resolve("@earendil-works/pi-ai/providers/faux"))
);
assert.equal(executor.capabilities.runtime, WORKFLOW_RUNTIME);
const scope = {
	accountId: "account-1",
	principalId: "principal-1",
	workspaceId: "workspace-1",
	workDirectoryId: "workdir-1",
	sessionId: "public-session-1",
};

async function setup() {
	const dir = await mkdtemp(join(tmpdir(), "oqto-workflow-adapter-"));
	const faux = fauxProvider();
	const models = createModels();
	models.setProvider(faux.provider);
	const timestamp = new Date().toISOString();
	const proposal = core.reviewProposal(
		await core.createProposal(
			{
				name: "Synthetic manually admitted workflow",
				prompt: "Return the primed token.",
				intervalMs: 60000,
				source: {
					sessionId: "native-source-pi",
					cwd: "/not-a-storage-target",
					entryIds: ["source-entry"],
					context: "user: Remember token garden.",
					contextComplete: false,
					droppedEntries: 0,
				},
				model: { provider: "faux", id: "faux-1" },
				tools: [],
				artifacts: [],
				allowSubagents: false,
				limits: { maxRuns: 2, maxDurationMs: 5000, maxOutputBytes: 1000 },
			},
			"native-proposal-1",
			timestamp,
		),
		"approved",
		timestamp,
	);
	const phases = [];
	const receipts = [];
	let revokeRequest = false;
	const authority = {
		async resolve(input) {
			phases.push(input);
			return {
				approved: !(revokeRequest && input.phase === "request"),
				scope,
				jobId: "public-job-1",
				occurrenceId: input.occurrenceId,
				runId: `public-run-${input.occurrenceId}`,
				bindingId: "protected-binding-1",
				grantRevision: 1,
				revision: proposal.revision,
				digest: proposal.digest,
				runtime: WORKFLOW_RUNTIME,
				expiresAt: Date.now() + 60000,
				proposal,
			};
		},
		async recordReceipt(input) {
			receipts.push(input);
		},
	};
	const adapter = createWorkflowAdapter({
		authority,
		executor,
		validateReviewedProposal: core.validateReviewedProposal,
		models,
		storeRoot: join(dir, "private-store"),
	});
	return {
		dir,
		faux,
		phases,
		receipts,
		proposal,
		authority,
		adapter,
		revoke() {
			revokeRequest = true;
		},
		close: () => rm(dir, { recursive: true, force: true }),
	};
}

test("real Durable: manual run, same conversation, reopen dedup and frozen run budget", async () => {
	const f = await setup();
	try {
		const response = (context) => {
			assert.ok(
				JSON.stringify(context.messages).includes("Remember token garden"),
			);
			assert.equal(context.tools?.length ?? 0, 0);
			return fauxAssistantMessage("garden");
		};
		f.faux.setResponses([response, response]);
		const first = await f.adapter.run(scope, {
			jobId: "public-job-1",
			occurrenceId: "one",
		});
		assert.equal(first.ok, true, JSON.stringify(first));
		assert.equal(first.output, "garden");
		const duplicate = await f.adapter.run(scope, {
			jobId: "public-job-1",
			occurrenceId: "one",
		});
		assert.equal(duplicate.ok, true, JSON.stringify(duplicate));
		assert.equal(duplicate.runId, first.runId);
		assert.deepEqual(duplicate.native, first.native);
		assert.equal(f.faux.state.callCount, 1);
		const second = await f.adapter.run(scope, {
			jobId: "public-job-1",
			occurrenceId: "two",
		});
		assert.equal(second.ok, true, JSON.stringify(second));
		assert.equal(second.native.conversationId, first.native.conversationId);
		assert.notEqual(second.native.submissionId, first.native.submissionId);
		assert.equal(f.faux.state.callCount, 2);
		const exceeded = await f.adapter.run(scope, {
			jobId: "public-job-1",
			occurrenceId: "three",
		});
		assert.equal(exceeded.ok, false);
		assert.equal(f.faux.state.callCount, 2);
		assert.equal(f.receipts.length, 4);
		assert.ok(
			f.phases.some(
				(p) => p.phase === "request" && p.taskId && p.conversationId,
			),
		);
		assert.equal(first.recurrenceActive, false);
	} finally {
		await f.close();
	}
});

test("real transport denial after native admission: zero provider calls", async () => {
	const f = await setup();
	try {
		f.faux.setResponses([
			() => {
				throw new Error("Denied request reached provider");
			},
		]);
		const crossAccount = await f.adapter.run(
			{ ...scope, accountId: "other" },
			{ jobId: "public-job-1", occurrenceId: "denied" },
		);
		assert.equal(crossAccount.code, "admission_denied");
		assert.equal(f.faux.state.callCount, 0);
		f.revoke();
		const revoked = await f.adapter.run(scope, {
			jobId: "public-job-1",
			occurrenceId: "revoked",
		});
		assert.equal(revoked.ok, false);
		assert.equal(revoked.effects, "possible");
		assert.equal(f.faux.state.callCount, 0);
		assert.ok(f.phases.some((p) => p.phase === "request"));
	} finally {
		await f.close();
	}
});

test("real shared core refuses edited review even with cached approved digest", async () => {
	const f = await setup();
	try {
		f.proposal.definition.prompt = "Tampered after human review";
		const result = await f.adapter.run(scope, {
			jobId: "public-job-1",
			occurrenceId: "tampered",
		});
		assert.equal(result.code, "review_invalid");
		assert.equal(f.faux.state.callCount, 0);
		assert.equal(f.receipts.length, 0);
	} finally {
		await f.close();
	}
});

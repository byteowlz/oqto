import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import {
	bootstrapPolicyEditorAuthority,
	policyProposalDigest,
} from "./policy-editor-authority.ts";

// SIMULATED contract adapters only. No TOML parsing, real filesystem CAS,
// durable authority persistence, Account auth, or confinement qualification.
const hash = (value) => createHash("sha256").update(value).digest("hex");
const clone = (value) => JSON.parse(JSON.stringify(value));
function material() {
	return {
		binding: {
			host: "fixture-host",
			hostGeneration: "host-1",
			operatorNamespace: "fixture-local-os",
			workspace: "workspace-id",
			workspacePath: "/fixture/work",
			placementGeneration: "placement-1",
			target: "workspace",
			filePath: "/fixture/work/.oqto/sandbox.toml",
			fileIdentity: "dev:1:inode:2",
			policyRevision: "policy-1",
			compilerRevision: "compiler-1",
			schemaRevision: "schema-1",
			resolverRevision: "resolver-1",
			dependencies: [
				{
					identity: "/fixture/owner.toml",
					revision: "dep-1",
					fileIdentity: "dev:1:inode:3",
					sha256: hash("owner bytes"),
				},
			],
		},
		original: "# original\n",
		candidate: "# candidate\n",
		semanticDiff: "fixture diff",
		resolution: "complete",
		dependencySnapshots: [
			{
				identity: "/fixture/owner.toml",
				revision: "dep-1",
				fileIdentity: "dev:1:inode:3",
				original: "owner bytes",
			},
		],
	};
}
function fixture(overrides = {}) {
	let time = 1000;
	let builds = 0;
	const snapshot = {
		build() {
			builds++;
			return material();
		},
	};
	const authority = bootstrapPolicyEditorAuthority({
		snapshot,
		now: () => time,
		ttlMs: 100,
		...overrides,
	});
	return {
		...authority,
		advance: () => {
			time += 100;
		},
		builds: () => builds,
	};
}
function approved(f) {
	const p = f.client.propose({ request: "fixture" });
	const review = f.operator.review(p.envelope.id, p.envelope, p.digest);
	f.operator.decide(review, "approve");
	return p;
}
function simulatedPublication(overrides = {}) {
	const calls = [];
	const adapter = {
		async revalidate() {
			calls.push("revalidate");
		},
		async publish(envelope, _material, digest) {
			calls.push("publish");
			return {
				proposalDigest: digest,
				candidateSha256: envelope.candidateSha256,
				hostGeneration: envelope.binding.hostGeneration,
				publicationId: "simulated-durable-publication-1",
			};
		},
		async readPublished(receipt) {
			calls.push("readPublished");
			return clone(receipt);
		},
		async adopt(receipt) {
			calls.push("adopt");
			return { ...receipt, policyGeneration: "simulated-policy-generation-2" };
		},
		async readAdopted(receipt) {
			calls.push("readAdopted");
			return clone(receipt);
		},
		...overrides,
	};
	return { adapter, calls };
}

test("default explicit missing integrations; no effects, client carries no operator methods", () => {
	const { client } = bootstrapPolicyEditorAuthority();
	assert.equal(client.capabilities().canApply, false);
	assert.match(
		client.capabilities().blocker,
		/Absent real oqto-sandbox snapshot\/parser integration.*CAS.*adoption/,
	);
	assert.throws(
		() => client.propose({ role: "operator", approved: true }),
		/Absent real/,
	);
	assert.deepEqual(Object.keys(client).sort(), [
		"cancel",
		"capabilities",
		"inspect",
		"propose",
		"replace",
	]);
	assert.equal(client.operator, undefined);
	assert.equal(client.decide, undefined);
	assert.equal(client.apply, undefined);
	assert.ok(Object.isFrozen(client));
});

test("review exact bytes and envelope are deep frozen; caller input has no authority", () => {
	const f = fixture();
	const p = f.client.propose({
		role: "operator",
		approved: true,
		actionRevision: 1,
	});
	const view = f.operator.review(p.envelope.id, p.envelope, p.digest);
	assert.equal(view.original, "# original\n");
	assert.equal(view.candidate, "# candidate\n");
	assert.equal(view.envelope.originalSha256, hash(view.original));
	assert.equal(view.envelope.candidateSha256, hash(view.candidate));
	assert.equal(
		view.envelope.binding.dependencies[0].sha256,
		hash("owner bytes"),
	);
	assert.throws(() => {
		view.envelope.binding.host = "other";
	}, TypeError);
	assert.throws(
		() =>
			f.operator.decide(
				{ ...view, role: "operator", approved: true },
				"approve",
			),
		/capability/,
	);
	assert.throws(
		() => f.operator.decide(view, { role: "operator", allow: true }),
		/Invalid operator decision/,
	);
	assert.equal(f.operator.decide(view, "approve").phase, "approved");
	assert.throws(() => f.operator.decide(view, "approve"), /capability/);
});

function leaves(value, path = []) {
	return Object.entries(value).flatMap(([key, item]) =>
		item && typeof item === "object"
			? leaves(item, [...path, key])
			: [[...path, key]],
	);
}
const envelopeTemplate = fixture().client.propose({}).envelope;
for (const path of leaves(envelopeTemplate)) {
	test(`every envelope field is bound: ${path.join(".")}`, async () => {
		const f = fixture();
		const p = f.client.propose({});
		const changed = clone(p.envelope);
		let parent = changed;
		for (const part of path.slice(0, -1)) parent = parent[part];
		const key = path.at(-1);
		parent[key] =
			typeof parent[key] === "number"
				? parent[key] + 1
				: `${parent[key]}-mutated`;
		assert.notEqual(policyProposalDigest(changed), p.digest);
		assert.throws(
			() => f.operator.review(p.envelope.id, changed, p.digest),
			/mutated/,
		);
		assert.throws(
			() =>
				f.operator.review(
					p.envelope.id,
					changed,
					policyProposalDigest(changed),
				),
			/mutated/,
		);
		f.operator.decide(
			f.operator.review(p.envelope.id, p.envelope, p.digest),
			"approve",
		);
		await assert.rejects(
			f.operator.apply(p.envelope.id, changed, p.digest),
			/mutated/,
		);
	});
}

test("canonical key ordering, unknown fields, stale digest and arbitrary role flags", () => {
	const f = fixture();
	const p = f.client.propose({});
	const reordered = Object.fromEntries(Object.entries(p.envelope).reverse());
	assert.equal(policyProposalDigest(reordered), p.digest);
	assert.throws(
		() =>
			f.operator.review(
				p.envelope.id,
				{ ...p.envelope, role: "operator" },
				p.digest,
			),
		/mutated/,
	);
	assert.throws(
		() => f.operator.review(p.envelope.id, p.envelope, "old-digest"),
		/mutated/,
	);
	assert.throws(
		() => f.operator.decide({ role: "operator", digest: p.digest }, "approve"),
		/capability/,
	);
});

test("revision replacement invalidates all old reviews and approvals", () => {
	const f = fixture();
	const p = f.client.propose({});
	const review = f.operator.review(p.envelope.id, p.envelope, p.digest);
	const newer = f.client.replace(p.envelope.id, p.digest, {});
	assert.equal(newer.envelope.revision, 2);
	assert.notEqual(newer.envelope.nonce, p.envelope.nonce);
	assert.throws(() => f.operator.decide(review, "approve"), /Stale/);
	assert.throws(() => f.client.replace(p.envelope.id, p.digest, {}), /Stale/);
	assert.throws(
		() => f.operator.review(p.envelope.id, p.envelope, p.digest),
		/Stale/,
	);
});

test("dependency replacement requires new proposal and immutable source capture", () => {
	const current = material();
	const f = fixture({
		snapshot: {
			build() {
				return current;
			},
		},
	});
	const p = approved(f);
	current.binding.dependencies[0].revision = "dep-2";
	current.dependencySnapshots[0].revision = "dep-2";
	current.dependencySnapshots[0].original = "changed dependency";
	current.binding.dependencies[0].sha256 = hash("changed dependency");
	assert.equal(p.envelope.binding.dependencies[0].revision, "dep-1");
	const next = f.client.replace(p.envelope.id, p.digest, {});
	assert.equal(next.phase, "proposed");
	assert.notEqual(next.digest, p.digest);
});

test("cancellation, denial, expiry and bounded capacity are terminal", async () => {
	const f = fixture({ maxProposals: 1 });
	const p = f.client.propose({});
	const review = f.operator.review(p.envelope.id, p.envelope, p.digest);
	assert.equal(f.client.cancel(p.envelope.id, p.digest).phase, "cancelled");
	assert.throws(() => f.operator.decide(review, "approve"), /consumed/);
	assert.throws(
		() => f.client.replace(p.envelope.id, p.digest, {}),
		/terminal/,
	);
	assert.throws(() => f.client.propose({}), /capacity/);
	const expired = fixture();
	const e = approved(expired);
	expired.advance();
	await assert.rejects(
		expired.operator.apply(e.envelope.id, e.envelope, e.digest),
		/expired/,
	);
	assert.equal(expired.client.inspect(e.envelope.id).phase, "expired");
	const denied = fixture();
	const d = denied.client.propose({});
	denied.operator.decide(
		denied.operator.review(d.envelope.id, d.envelope, d.digest),
		"deny",
	);
	await assert.rejects(
		denied.operator.apply(d.envelope.id, d.envelope, d.digest),
		/approval/,
	);
});

test("incomplete resolver, diagnostic hashes and incomplete dependency snapshots rejected", () => {
	for (const mutate of [
		(m) => {
			m.resolution = "unavailable";
		},
		(m) => {
			m.binding.dependencies[0].sha256 = "fnv-fingerprint";
		},
		(m) => {
			m.dependencySnapshots = [];
		},
		(m) => {
			m.binding.compilerRevision = "";
		},
		(m) => {
			m.dependencySnapshots[0].fileIdentity = "other-inode";
		},
		(m) => {
			m.candidate = "\ud800";
		},
	]) {
		const f = fixture({
			snapshot: {
				build() {
					const m = material();
					mutate(m);
					return m;
				},
			},
		});
		assert.throws(() => f.client.propose({}));
	}
});

test("absent or incomplete publication integration fails before effects even approved", async () => {
	let effects = 0;
	for (const publication of [
		undefined,
		{
			publish: async () => {
				effects++;
				return true;
			},
		},
	]) {
		const f = fixture({ publication });
		const p = approved(f);
		assert.equal(f.client.capabilities().canApply, false);
		await assert.rejects(
			f.operator.apply(p.envelope.id, p.envelope, p.digest),
			/Absent real/,
		);
		assert.equal(f.client.inspect(p.envelope.id).phase, "approved");
	}
	assert.equal(effects, 0);
});

test("SIMULATED exact publication + independently read adoption ack; single winner/replay", async () => {
	const sim = simulatedPublication();
	const f = fixture({ publication: sim.adapter });
	const p = approved(f);
	const applying = f.operator.apply(p.envelope.id, p.envelope, p.digest);
	await assert.rejects(
		f.operator.apply(p.envelope.id, p.envelope, p.digest),
		/approval/,
	);
	const ack = await applying;
	assert.equal(ack.phase, "acked");
	assert.equal(ack.adoption.candidateSha256, p.envelope.candidateSha256);
	assert.equal(ack.adoption.policyGeneration, "simulated-policy-generation-2");
	assert.deepEqual(sim.calls, [
		"revalidate",
		"publish",
		"readPublished",
		"adopt",
		"readAdopted",
	]);
	f.advance(); // Expired decision cannot erase an already durable ack.
	await assert.rejects(
		f.operator.apply(p.envelope.id, p.envelope, p.digest),
		/approval/,
	);
	assert.equal(f.client.inspect(p.envelope.id).phase, "acked");
});

test("SIMULATED stale dependency revalidation fails before publication", async () => {
	const sim = simulatedPublication({
		async revalidate() {
			throw new Error("stale dependency inode");
		},
	});
	const f = fixture({ publication: sim.adapter });
	const p = approved(f);
	await assert.rejects(
		f.operator.apply(p.envelope.id, p.envelope, p.digest),
		/stale dependency/,
	);
	assert.deepEqual(sim.calls, []);
	assert.equal(f.client.inspect(p.envelope.id).phase, "denied");
});

for (const [name, overrides, phase] of [
	[
		"publish throw after possible effects",
		{
			async publish() {
				throw new Error("uncertain");
			},
		},
		"publication_uncertain",
	],
	[
		"boolean applied is not evidence",
		{
			async publish() {
				return true;
			},
		},
		"publication_uncertain",
	],
	[
		"wrong exact publication hash",
		{
			async readPublished(r) {
				return { ...r, candidateSha256: hash("other") };
			},
		},
		"publication_uncertain",
	],
	[
		"publication readback identity mismatch",
		{
			async readPublished(r) {
				return { ...r, publicationId: "other" };
			},
		},
		"publication_uncertain",
	],
	[
		"adoption failed",
		{
			async adopt() {
				throw new Error("not adopted");
			},
		},
		"published_not_adopted",
	],
	[
		"boolean adoption",
		{
			async adopt() {
				return { applied: true };
			},
		},
		"published_not_adopted",
	],
	[
		"adoption readback mismatch",
		{
			async readAdopted(r) {
				return { ...r, policyGeneration: "wrong" };
			},
		},
		"published_not_adopted",
	],
]) {
	test(`SIMULATED ${name} never claims success`, async () => {
		const sim = simulatedPublication(overrides);
		const f = fixture({ publication: sim.adapter });
		const p = approved(f);
		const state = await f.operator.apply(p.envelope.id, p.envelope, p.digest);
		assert.equal(state.phase, phase);
		assert.equal(state.adoption, null);
		if (phase === "published_not_adopted") assert.ok(state.publication);
		await assert.rejects(
			f.operator.apply(p.envelope.id, p.envelope, p.digest),
			/approval/,
		);
		assert.throws(() => f.client.cancel(p.envelope.id, p.digest), /terminal/);
	});
}

test("bounded envelopes reject large, deep, cyclic and noncanonical caller values", () => {
	const f = fixture();
	const p = f.client.propose({});
	const huge = { ...p.envelope, extra: "x".repeat(1024 * 1024 + 1) };
	assert.throws(
		() => f.operator.review(p.envelope.id, huge, p.digest),
		/bound/,
	);
	let nested = {};
	for (let i = 0; i < 30; i++) nested = { child: nested };
	assert.throws(() => policyProposalDigest(nested), /structural bound/);
	const cycle = {};
	cycle.self = cycle;
	assert.throws(() => policyProposalDigest(cycle), /structural bound/);
	assert.throws(
		() => policyProposalDigest({ value: Number.NaN }),
		/Noncanonical/,
	);
	assert.throws(
		() => policyProposalDigest({ value: undefined }),
		/Noncanonical/,
	);
});

test("SIMULATED expiry during revalidation has zero publication effects", async () => {
	const sim = simulatedPublication({
		async revalidate() {
			f.advance();
		},
	});
	const f = fixture({ publication: sim.adapter });
	const p = approved(f);
	await assert.rejects(
		f.operator.apply(p.envelope.id, p.envelope, p.digest),
		/expired/,
	);
	assert.equal(f.client.inspect(p.envelope.id).phase, "expired");
	assert.deepEqual(sim.calls, []);
});

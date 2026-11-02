// Trusted guard-bootstrap composition only. Never export this factory over a
// tool/agent RPC. OS namespace/socket exclusion is the local trust boundary;
// this module does not authenticate Accounts or protect unrestricted same UID.
import { createHash, randomUUID } from "node:crypto";

export interface PolicyBinding {
	host: string;
	hostGeneration: string;
	operatorNamespace: string;
	workspace: string;
	workspacePath: string;
	placementGeneration: string;
	target: "owner_global" | "workspace";
	filePath: string;
	fileIdentity: string;
	policyRevision: string;
	compilerRevision: string;
	schemaRevision: string;
	resolverRevision: string;
	dependencies: {
		identity: string;
		revision: string;
		fileIdentity: string;
		sha256: string;
	}[];
}
export interface ProposalMaterial {
	binding: PolicyBinding;
	original: string;
	candidate: string;
	semanticDiff: string;
	/** Trusted resolver output, never inferred from an empty dependency list. */
	resolution: "complete" | "unavailable";
	dependencySnapshots: {
		identity: string;
		revision: string;
		fileIdentity: string;
		original: string;
	}[];
}
/** Bridge to oqto-sandbox policy_editor snapshot/proposal validation. No TOML
 * evaluator is provided here. Caller input must never supply trusted binding. */
export interface SnapshotAdapter {
	build(input: unknown): ProposalMaterial;
}
export interface Envelope {
	protocol: "oqt-sandbox-policy-proposal-v1";
	action: "sandbox_policy_apply";
	id: string;
	revision: number;
	nonce: string;
	issuedAt: number;
	expiresAt: number;
	binding: PolicyBinding;
	originalSha256: string;
	candidateSha256: string;
	semanticDiffSha256: string;
}
export interface PublishedReceipt {
	proposalDigest: string;
	candidateSha256: string;
	hostGeneration: string;
	publicationId: string;
}
export interface AdoptionReceipt extends PublishedReceipt {
	policyGeneration: string;
}
/** Trusted integrations, NOT caller approval flags. A real implementation must
 * do strict parser/dependency/identity revalidation, backup + locked CAS and
 * durable publication, then independent authoritative readback and adoption.
 * No implementation ships here. Tests use explicitly simulated adapters. */
export interface PublicationAdapter {
	revalidate(
		envelope: Readonly<Envelope>,
		material: Readonly<ProposalMaterial>,
	): Promise<void>;
	publish(
		envelope: Readonly<Envelope>,
		material: Readonly<ProposalMaterial>,
		digest: string,
	): Promise<PublishedReceipt>;
	readPublished(receipt: Readonly<PublishedReceipt>): Promise<PublishedReceipt>;
	adopt(receipt: Readonly<PublishedReceipt>): Promise<AdoptionReceipt>;
	readAdopted(receipt: Readonly<AdoptionReceipt>): Promise<AdoptionReceipt>;
}
type Phase =
	| "proposed"
	| "reviewed"
	| "approved"
	| "denied"
	| "cancelled"
	| "expired"
	| "applying"
	| "publication_uncertain"
	| "published_not_adopted"
	| "acked";
interface Row {
	envelope: Envelope;
	digest: string;
	material: ProposalMaterial;
	phase: Phase;
	publication?: PublishedReceipt;
	adoption?: AdoptionReceipt;
}
const MISSING =
	"Absent real oqto-sandbox snapshot/parser integration and protected CAS publication/runtime adoption integration";
const MAX_BYTES = 1024 * 1024;
function fail(reason: string): never {
	throw new Error(reason);
}
function canonical(value: unknown): string {
	let remaining = MAX_BYTES;
	let nodes = 4096;
	function token(value: string) {
		remaining -= Buffer.byteLength(value);
		if (remaining < 0) fail("Proposal exceeds byte bound");
		return value;
	}
	function string(value: string) {
		if (Buffer.byteLength(value) > remaining)
			fail("Proposal exceeds byte bound");
		return token(JSON.stringify(value));
	}
	function visit(item: unknown, depth: number): string {
		if (--nodes < 0 || depth > 24) fail("Proposal exceeds structural bound");
		if (typeof item === "string") return string(item);
		if (item === null || typeof item === "boolean")
			return token(JSON.stringify(item));
		if (typeof item === "number" && Number.isSafeInteger(item))
			return token(String(item));
		if (Array.isArray(item)) {
			if (item.length > nodes) fail("Proposal exceeds structural bound");
			token(`[]${",".repeat(Math.max(0, item.length - 1))}`);
			return `[${item.map((child) => visit(child, depth + 1)).join(",")}]`;
		}
		if (
			typeof item === "object" &&
			item !== null &&
			Object.getPrototypeOf(item) === Object.prototype
		) {
			const keys = Object.keys(item).sort();
			if (keys.length > nodes) fail("Proposal exceeds structural bound");
			token(`{}${",".repeat(Math.max(0, keys.length - 1))}`);
			return `{${keys.map((key) => `${string(key)}${token(":")}${visit((item as Record<string, unknown>)[key], depth + 1)}`).join(",")}}`;
		}
		return fail("Noncanonical proposal value");
	}
	return visit(value, 0);
}
export function policyProposalDigest(value: Envelope): string {
	return sha256(canonical(value));
}
function sha256(value: string): string {
	return createHash("sha256").update(value, "utf8").digest("hex");
}
function copy<T>(value: T): T {
	return JSON.parse(canonical(value)) as T;
}
function freeze<T>(value: T): T {
	if (typeof value === "object" && value !== null) {
		for (const child of Object.values(value)) freeze(child);
		Object.freeze(value);
	}
	return value;
}
function text(value: unknown): asserts value is string {
	if (
		typeof value !== "string" ||
		value.length === 0 ||
		Buffer.byteLength(value) > MAX_BYTES
	)
		fail("Invalid bounded proposal text");
}
function exactKeys(value: object, keys: string[]) {
	if (Object.keys(value).sort().join("|") !== keys.sort().join("|"))
		fail("Unknown or missing proposal fields");
}
function materialChecked(value: ProposalMaterial): ProposalMaterial {
	const material = copy(value);
	exactKeys(material, [
		"binding",
		"original",
		"candidate",
		"semanticDiff",
		"resolution",
		"dependencySnapshots",
	]);
	if (material.resolution !== "complete")
		fail("Incomplete canonical policy resolver integration");
	for (const key of ["original", "candidate", "semanticDiff"] as const) {
		if (
			typeof material[key] !== "string" ||
			Buffer.from(material[key], "utf8").toString("utf8") !== material[key]
		)
			fail("Invalid exact UTF-8 proposal bytes");
	}
	if (Buffer.byteLength(canonical(material)) > MAX_BYTES)
		fail("Proposal exceeds byte bound");
	const binding = material.binding;
	exactKeys(binding, [
		"host",
		"hostGeneration",
		"operatorNamespace",
		"workspace",
		"workspacePath",
		"placementGeneration",
		"target",
		"filePath",
		"fileIdentity",
		"policyRevision",
		"compilerRevision",
		"schemaRevision",
		"resolverRevision",
		"dependencies",
	]);
	for (const [key, value] of Object.entries(binding))
		if (key !== "dependencies") text(value);
	if (binding.target !== "owner_global" && binding.target !== "workspace")
		fail("Invalid policy target");
	if (!Array.isArray(binding.dependencies) || binding.dependencies.length > 64)
		fail("Invalid dependency bound");
	if (
		!Array.isArray(material.dependencySnapshots) ||
		material.dependencySnapshots.length !== binding.dependencies.length
	)
		fail("Missing exact dependency snapshots");
	const identities = new Set<string>();
	for (const dependency of binding.dependencies) {
		exactKeys(dependency, ["identity", "revision", "fileIdentity", "sha256"]);
		text(dependency.identity);
		text(dependency.revision);
		text(dependency.fileIdentity);
		const captured = material.dependencySnapshots.filter(
			(item) => item.identity === dependency.identity,
		);
		if (captured.length !== 1) fail("Missing or duplicate dependency snapshot");
		const bytes = captured[0];
		exactKeys(bytes, ["identity", "revision", "fileIdentity", "original"]);
		if (
			typeof bytes.original !== "string" ||
			Buffer.from(bytes.original, "utf8").toString("utf8") !== bytes.original ||
			bytes.revision !== dependency.revision ||
			bytes.fileIdentity !== dependency.fileIdentity
		)
			fail("Dependency snapshot binding mismatch");
		// Never trust the diagnostic FNV fingerprint supplied by the Rust model.
		if (dependency.sha256 !== sha256(bytes.original))
			fail("Dependency exact SHA-256 mismatch");
		if (
			!/^[a-f0-9]{64}$/.test(dependency.sha256) ||
			identities.has(dependency.identity)
		)
			fail("Invalid dependency identity/hash");
		identities.add(dependency.identity);
	}
	binding.dependencies.sort((a, b) =>
		a.identity < b.identity ? -1 : a.identity > b.identity ? 1 : 0,
	);
	return freeze(material);
}

/** Only trusted guard bootstrap calls this and retains operator in its closure.
 * Giving an agent this factory/module's host execution privileges is NOT safe.
 * The client has propose/replace/cancel/inspect only, no decision capability. */
export function bootstrapPolicyEditorAuthority(
	options: {
		snapshot?: SnapshotAdapter;
		publication?: PublicationAdapter;
		now?: () => number;
		ttlMs?: number;
		maxProposals?: number;
	} = {},
) {
	const { snapshot, publication } = options;
	const now = options.now ?? Date.now;
	const ttl = options.ttlMs ?? 60_000;
	const limit = options.maxProposals ?? 64;
	if (
		!Number.isSafeInteger(ttl) ||
		ttl < 1 ||
		ttl > 300_000 ||
		!Number.isSafeInteger(limit) ||
		limit < 1 ||
		limit > 256
	)
		fail("Invalid authority bounds");
	const rows = new Map<string, Row>();
	const reviews = new WeakMap<object, { id: string; digest: string }>();
	const complete =
		!!snapshot &&
		!!publication &&
		["revalidate", "publish", "readPublished", "adopt", "readAdopted"].every(
			(key) =>
				typeof publication[key as keyof PublicationAdapter] === "function",
		);
	const capabilities = () =>
		freeze({
			canPropose: !!snapshot,
			canReview: !!snapshot,
			canApply: complete,
			blocker: complete ? null : MISSING,
		});
	function row(id: string) {
		return rows.get(id) ?? fail("Unknown proposal");
	}
	function fresh(value: Row) {
		if (now() >= value.envelope.expiresAt) {
			value.phase = "expired";
			fail("Proposal expired");
		}
	}
	function editable(value: Row) {
		if (!["proposed", "reviewed", "approved"].includes(value.phase))
			fail("Proposal consumed or terminal");
		fresh(value);
	}
	function build(input: unknown, id: string, revision: number): Row {
		if (!snapshot) fail(MISSING);
		const material = materialChecked(snapshot.build(input));
		const issuedAt = now();
		if (!Number.isSafeInteger(issuedAt) || issuedAt < 0)
			fail("Invalid authority clock");
		const envelope: Envelope = freeze({
			protocol: "oqt-sandbox-policy-proposal-v1",
			action: "sandbox_policy_apply",
			id,
			revision,
			nonce: randomUUID(),
			issuedAt,
			expiresAt: issuedAt + ttl,
			binding: material.binding,
			originalSha256: sha256(material.original),
			candidateSha256: sha256(material.candidate),
			semanticDiffSha256: sha256(material.semanticDiff),
		});
		return {
			envelope,
			material,
			digest: policyProposalDigest(envelope),
			phase: "proposed",
		};
	}
	function inspect(id: string) {
		const value = row(id);
		return freeze(
			copy({
				envelope: value.envelope,
				digest: value.digest,
				phase: value.phase,
				publication: value.publication ?? null,
				adoption: value.adoption ?? null,
			}),
		);
	}
	function match(value: Row, envelope: Envelope, digest: string) {
		if (
			digest !== value.digest ||
			policyProposalDigest(envelope) !== value.digest
		)
			fail("Stale or mutated proposal");
	}
	function checkPublication(
		receipt: PublishedReceipt,
		value: Row,
	): PublishedReceipt {
		if (!receipt || typeof receipt !== "object")
			fail("Missing exact publication receipt");
		exactKeys(receipt, [
			"proposalDigest",
			"candidateSha256",
			"hostGeneration",
			"publicationId",
		]);
		if (
			receipt.proposalDigest !== value.digest ||
			receipt.candidateSha256 !== value.envelope.candidateSha256 ||
			receipt.hostGeneration !== value.envelope.binding.hostGeneration
		)
			fail("Publication binding mismatch");
		text(receipt.publicationId);
		return freeze(copy(receipt));
	}
	const client = freeze({
		capabilities,
		propose(input: unknown) {
			if (rows.size >= limit) fail("Proposal capacity exhausted");
			const id = randomUUID();
			rows.set(id, build(input, id, 1));
			return inspect(id);
		},
		replace(id: string, digest: string, input: unknown) {
			const value = row(id);
			editable(value);
			if (digest !== value.digest) fail("Stale proposal");
			rows.set(id, build(input, id, value.envelope.revision + 1));
			return inspect(id);
		},
		cancel(id: string, digest: string) {
			const value = row(id);
			editable(value);
			if (digest !== value.digest) fail("Stale proposal");
			value.phase = "cancelled";
			return inspect(id);
		},
		inspect,
	});
	const operator = freeze({
		review(id: string, envelope: Envelope, digest: string) {
			const value = row(id);
			editable(value);
			match(value, envelope, digest);
			if (value.phase === "approved") fail("Already decided");
			const review = freeze({
				...inspect(id),
				original: value.material.original,
				candidate: value.material.candidate,
				semanticDiff: value.material.semanticDiff,
			});
			reviews.set(review, { id, digest });
			value.phase = "reviewed";
			return review;
		},
		decide(review: object, decision: "approve" | "deny") {
			const held =
				reviews.get(review) ?? fail("Operator review capability required");
			const value = row(held.id);
			if (value.phase !== "reviewed" || held.digest !== value.digest)
				fail("Stale or consumed review");
			fresh(value);
			if (decision !== "approve" && decision !== "deny")
				fail("Invalid operator decision");
			reviews.delete(review);
			value.phase = decision === "approve" ? "approved" : "denied";
			return inspect(held.id);
		},
		async apply(id: string, envelope: Envelope, digest: string) {
			const value = row(id);
			if (value.phase !== "approved") fail("Exact operator approval required");
			fresh(value);
			match(value, envelope, digest);
			if (!complete || !publication) fail(MISSING); // Fail before any adapter effects.
			value.phase = "applying"; // single winner before first await
			try {
				await publication.revalidate(value.envelope, value.material);
				fresh(value);
			} catch (error) {
				value.phase = now() >= value.envelope.expiresAt ? "expired" : "denied";
				throw error;
			}
			// Any throw after publication starts is uncertain, never an automatic retry.
			try {
				const published = checkPublication(
					await publication.publish(
						value.envelope,
						value.material,
						value.digest,
					),
					value,
				);
				const confirmed = checkPublication(
					await publication.readPublished(published),
					value,
				);
				if (canonical(published) !== canonical(confirmed))
					fail("Publication readback mismatch");
				value.publication = confirmed;
				value.phase = "published_not_adopted";
			} catch {
				value.phase = "publication_uncertain";
				return inspect(id);
			}
			try {
				const ack = await publication.adopt(value.publication);
				exactKeys(ack, [
					"proposalDigest",
					"candidateSha256",
					"hostGeneration",
					"publicationId",
					"policyGeneration",
				]);
				const { policyGeneration, ...base } = ack;
				text(policyGeneration);
				if (
					canonical(checkPublication(base, value)) !==
					canonical(value.publication)
				)
					fail("Adoption binding mismatch");
				if (
					canonical(await publication.readAdopted(freeze(copy(ack)))) !==
					canonical(ack)
				)
					fail("Adoption readback mismatch");
				value.adoption = freeze(copy(ack));
				value.phase = "acked";
			} catch {
				/* Durable publication is not rolled back or called applied. */
			}
			return inspect(id);
		},
	});
	return freeze({ client, operator });
}

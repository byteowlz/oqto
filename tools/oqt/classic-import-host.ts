// Protected local-operator upload and catalog publication. This module never
// opens a Durable store: only the normal Session worker may commit an import.
import { createHash, randomUUID } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { mkdir, open, readFile, rename, rm, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { planClassicUpdate } from "./classic-import.ts";
import {
	type ClassicImportSeed,
	validateClassicImportSeed,
} from "./classic-import-worker.ts";

const MAX_BYTES = 64 * 1024 * 1024;
const PENDING = ".oqt-import-pending";
export function classicImportSeedPath(sessionPath: string): string {
	return join(
		dirname(dirname(sessionPath)),
		"classic-seeds",
		`${sessionPath.split("/").at(-1)}.json`,
	);
}
export function classicImportPublicationPath(sessionPath: string): string {
	return join(
		dirname(dirname(sessionPath)),
		"classic-publications",
		`${sessionPath.split("/").at(-1)}.json`,
	);
}
type Metadata = { id: string; path: string; cwd: string; createdAt: number };
type Backend = {
	directory: string;
	cwd: string;
	open(metadata: Metadata): Promise<unknown>;
	close(metadata: Metadata): Promise<void>;
	refresh(): Promise<void>;
};
type Upload = {
	peer: object;
	seed: Omit<ClassicImportSeed, "source">;
	length: number;
	offset: number;
	path: string;
	touched: number;
	busy?: boolean;
};
let backend: Backend | undefined;
let uploadRoot: string | undefined;
export function configureClassicImportHost(privateDirectory: string): void {
	if (uploadRoot) throw new Error("Classic import host already configured");
	uploadRoot = join(privateDirectory, "classic-uploads");
}
const uploads = new Map<string, Upload>();
let committing = false;
// Host-owned admission fence survives marker/file changes during bootstrap.
const stagedSessions = new Set<string>();
export function canAdmitSession(sessionId: string): boolean {
	if (stagedSessions.has(sessionId)) return false;
	if (!/^classic-[0-9a-f]{48}$/.test(sessionId)) return true;
	return (
		backend !== undefined &&
		!isClassicImportStaged(join(backend.directory, sessionId))
	);
}

/** Bound only by startServerBackend, never by an agent service. */
export function bindClassicImportBackend(value: Backend): void {
	if (backend) throw new Error("Classic import backend already bound");
	backend = value;
}
export function isClassicImportStaged(path: string): boolean {
	if (existsSync(join(path, PENDING))) return true;
	const sessionId = path.split("/").at(-1);
	if (!sessionId || !/^classic-[0-9a-f]{48}$/.test(sessionId)) return false;
	// Imported identities are reserved. Only this protected receipt can publish
	// one, including after host restart or corruption of brain-writable metadata.
	try {
		const receipt = JSON.parse(
			readFileSync(classicImportPublicationPath(path), "utf8"),
		);
		return (
			receipt.state !== "published" ||
			receipt.sessionId !== sessionId ||
			receipt.catalogDirectory !== resolve(dirname(path)) ||
			!/^[0-9a-f]{64}$/.test(receipt.sourceHash)
		);
	} catch {
		return true;
	}
}
/** Platform identity is scoped to the destination catalog, never the classic UUID. */
export function classicImportTargetId(
	catalogDirectory: string,
	sourceId: string,
	sourceHash: string,
	leafId: string | null,
): string {
	return `classic-${hash(JSON.stringify([resolve(catalogDirectory), sourceId, sourceHash, leafId])).slice(0, 48)}`;
}
function hash(text: string) {
	return createHash("sha256").update(text, "utf8").digest("hex");
}
function string(value: unknown, name: string): string {
	if (typeof value !== "string") throw new Error(`Invalid ${name}`);
	return value;
}
function owned(uploadId: unknown, peer: object): [string, Upload] {
	const id = string(uploadId, "uploadId");
	const upload = uploads.get(id);
	if (!upload || upload.peer !== peer) throw new Error("Unknown import upload");
	upload.touched = Date.now();
	return [id, upload];
}
/** Peer-scoped upload cleanup: a disconnected operator cannot leave unbounded
 * authority-bearing buffers behind. An already sealed commit is host-owned. */
export function retireClassicImportPeer(peer: object): void {
	for (const [id, upload] of uploads)
		if (upload.peer === peer) {
			uploads.delete(id);
			void rm(upload.path, { force: true }).catch(() => {});
		}
}
export async function operateClassicImport(
	method: string,
	params: Record<string, unknown>,
	peer: object,
): Promise<unknown> {
	const active = backend;
	if (!active) throw new Error("Classic import backend unavailable");
	for (const [id, upload] of uploads)
		if (Date.now() - upload.touched > 10 * 60_000) {
			uploads.delete(id);
			await rm(upload.path, { force: true });
		}
	if (method === "import_classic_begin") {
		if (uploads.size >= 2 || committing) throw new Error("Classic import busy");
		const origin = string(params.origin, "origin");
		const sourceHash = string(params.sourceHash, "sourceHash");
		const length = params.byteLength;
		if (
			!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(origin) ||
			!/^[0-9a-f]{64}$/.test(sourceHash) ||
			typeof length !== "number" ||
			!Number.isSafeInteger(length) ||
			length < 1 ||
			length > MAX_BYTES ||
			(params.leafId !== undefined &&
				params.leafId !== null &&
				typeof params.leafId !== "string") ||
			params.cwd !== undefined
		)
			throw new Error(
				"Invalid classic import metadata (workspace is host-bound)",
			);
		const uploadId = randomUUID();
		// Uploads are outside the brain-writable Session tree.
		const directory = uploadRoot;
		if (!directory)
			throw new Error("Protected classic upload directory unavailable");
		const path = join(directory, uploadId);
		// Reserve capacity synchronously before any filesystem await.
		uploads.set(uploadId, {
			peer,
			seed: {
				origin,
				sourceHash,
				...(params.leafId !== undefined
					? { leafId: params.leafId as string | null }
					: {}),
			},
			length,
			offset: 0,
			path,
			touched: Date.now(),
		});
		try {
			await mkdir(directory, { recursive: true, mode: 0o700 });
			await writeFile(path, "", { flag: "wx", mode: 0o600 });
			if (!uploads.has(uploadId)) {
				await rm(path, { force: true });
				throw new Error("Import operator disconnected");
			}
		} catch (error) {
			uploads.delete(uploadId);
			throw error;
		}
		return { uploadId };
	}
	const [id, upload] = owned(params.uploadId, peer);
	if (method === "import_classic_chunk") {
		const encoded = string(params.base64, "base64");
		if (
			encoded.length > 21848 ||
			encoded.length % 4 !== 0 ||
			!/^[A-Za-z0-9+/]*={0,2}$/.test(encoded)
		)
			throw new Error("Invalid import chunk");
		const bytes = Buffer.from(encoded, "base64");
		if (
			!bytes.length ||
			bytes.length > 16384 ||
			bytes.toString("base64") !== encoded ||
			params.offset !== upload.offset ||
			upload.offset + bytes.length > upload.length
		)
			throw new Error("Invalid import chunk offset or length");
		if (upload.busy) throw new Error("Import upload write in progress");
		upload.busy = true;
		const offset = upload.offset;
		try {
			const file = await open(upload.path, "r+");
			try {
				let written = 0;
				while (written < bytes.length) {
					const result = await file.write(
						bytes,
						written,
						bytes.length - written,
						offset + written,
					);
					if (!result.bytesWritten)
						throw new Error("Import upload write failed");
					written += result.bytesWritten;
				}
				upload.offset += bytes.length;
			} finally {
				await file.close();
			}
		} finally {
			upload.busy = false;
		}
		return { offset: upload.offset };
	}
	if (method !== "import_classic_commit")
		throw new Error("Unknown classic import operation");
	if (committing || upload.busy || upload.offset !== upload.length)
		throw new Error("Classic import busy or incomplete");
	committing = true;
	uploads.delete(id);
	try {
		const bytes = await readFile(upload.path);
		const source = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
		const seed: ClassicImportSeed = { ...upload.seed, source };
		if (bytes.length !== upload.length || hash(source) !== seed.sourceHash)
			throw new Error("Classic source hash mismatch");
		const snapshot = validateClassicImportSeed(seed);
		const sessionId = classicImportTargetId(
			active.directory,
			snapshot.sourceId,
			seed.sourceHash,
			seed.leafId === undefined ? snapshot.activeLeaf : seed.leafId,
		);
		const existingPath = join(active.directory, sessionId);
		// Exact historical revision retries remain idempotent even after a newer
		// source snapshot was published. Never roll the source checkpoint back.
		if (
			!isClassicImportStaged(existingPath) &&
			existsSync(join(existingPath, "meta.json"))
		) {
			const savedSeed: ClassicImportSeed = JSON.parse(
				await readFile(classicImportSeedPath(existingPath), "utf8"),
			);
			const savedMeta = JSON.parse(
				await readFile(join(existingPath, "meta.json"), "utf8"),
			);
			if (
				savedSeed.source !== source ||
				savedSeed.sourceHash !== seed.sourceHash ||
				savedSeed.origin !== seed.origin ||
				savedMeta.cwd !== active.cwd
			)
				throw new Error("Classic import identity conflict");
			return { sessionId, sourceHash: seed.sourceHash, status: "unchanged" };
		}
		// Immutable source revisions never modify an advanced destination. Protected
		// checkpoints fence changed native IDs across revisions and staging recovery.
		const checkpointDir = join(active.directory, "..", "classic-sources");
		await mkdir(checkpointDir, { recursive: true, mode: 0o700 });
		const checkpointPath = join(
			checkpointDir,
			`${hash(JSON.stringify([resolve(active.directory), snapshot.sourceId]))}.json`,
		);
		let predecessorSessionId: string | undefined;
		let checkpoint:
			| { sessionId: string; publishedSessionId?: string }
			| undefined;
		let lastPublishedSessionId: string | undefined;
		try {
			checkpoint = JSON.parse(await readFile(checkpointPath, "utf8"));
		} catch (error) {
			if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
		}
		if (checkpoint) {
			if (!/^classic-[0-9a-f]{48}$/.test(checkpoint.sessionId))
				throw new Error("Invalid protected import checkpoint");
			const reservedPath = join(active.directory, checkpoint.sessionId);
			const publishedCandidate =
				!isClassicImportStaged(reservedPath) &&
				existsSync(join(reservedPath, "meta.json"))
					? checkpoint.sessionId
					: checkpoint.publishedSessionId;
			if (publishedCandidate !== undefined) {
				if (!/^classic-[0-9a-f]{48}$/.test(publishedCandidate))
					throw new Error("Invalid published import checkpoint");
				const publishedPath = join(active.directory, publishedCandidate);
				if (
					!isClassicImportStaged(publishedPath) &&
					existsSync(join(publishedPath, "meta.json"))
				)
					lastPublishedSessionId = publishedCandidate;
			}
			const previousSeed: ClassicImportSeed = JSON.parse(
				await readFile(
					classicImportSeedPath(join(active.directory, checkpoint.sessionId)),
					"utf8",
				),
			);
			const previous = validateClassicImportSeed(previousSeed);
			if (previous.sourceDigest !== snapshot.sourceDigest) {
				const update = planClassicUpdate(previous, snapshot, true);
				if (
					update.kind === "conflict" ||
					!source.startsWith(previous.sourceText)
				)
					throw new Error(
						"Classic source rewritten or native ID reused; reconciliation required",
					);
				predecessorSessionId = lastPublishedSessionId;
			}
		}
		const saveCheckpoint = async (published = false) => {
			const temporary = `${checkpointPath}.${randomUUID()}`;
			await writeFile(
				temporary,
				JSON.stringify({
					sessionId,
					sourceHash: seed.sourceHash,
					publishedSessionId: published ? sessionId : lastPublishedSessionId,
				}),
				{ flag: "wx", mode: 0o600 },
			);
			await rename(temporary, checkpointPath);
		};
		const path = join(active.directory, sessionId);
		const metadata: Metadata = {
			id: sessionId,
			path,
			cwd: active.cwd,
			createdAt: Date.now(),
		};
		stagedSessions.add(sessionId);
		const publicationPath = classicImportPublicationPath(path);
		await mkdir(dirname(publicationPath), { recursive: true, mode: 0o700 });
		const publication = {
			sessionId,
			sourceHash: seed.sourceHash,
			catalogDirectory: resolve(active.directory),
			state: "staged",
		};
		try {
			await writeFile(publicationPath, JSON.stringify(publication), {
				flag: "wx",
				mode: 0o600,
			});
		} catch (error) {
			if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
			const saved = JSON.parse(await readFile(publicationPath, "utf8"));
			if (
				saved.sessionId !== sessionId ||
				saved.sourceHash !== seed.sourceHash ||
				saved.catalogDirectory !== publication.catalogDirectory ||
				!["staged", "published"].includes(saved.state)
			)
				throw new Error("Protected import publication conflict");
		}
		let fresh = false;
		try {
			await mkdir(path, { mode: 0o700 });
			fresh = true;
		} catch (error) {
			if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
		}
		const seedPath = classicImportSeedPath(path);
		await mkdir(dirname(seedPath), { recursive: true, mode: 0o700 });
		if (fresh) {
			await writeFile(
				join(path, PENDING),
				JSON.stringify({ sourceHash: seed.sourceHash }),
				{ flag: "wx", mode: 0o600 },
			);
			await writeFile(seedPath, JSON.stringify(seed), {
				flag: "wx",
				mode: 0o600,
			});
		} else {
			const previous: unknown = JSON.parse(await readFile(seedPath, "utf8"));
			if (
				!previous ||
				typeof previous !== "object" ||
				!("sourceHash" in previous) ||
				previous.sourceHash !== seed.sourceHash ||
				!("source" in previous) ||
				previous.source !== source
			)
				throw new Error(
					"Classic source identity reused with changed content; target untouched",
				);
			if (!isClassicImportStaged(path)) {
				const published: unknown = JSON.parse(
					await readFile(join(path, "meta.json"), "utf8"),
				);
				if (
					!published ||
					typeof published !== "object" ||
					!("cwd" in published) ||
					published.cwd !== active.cwd
				)
					throw new Error("Classic import catalog mismatch");
				await saveCheckpoint(true);
				return {
					sessionId,
					sourceHash: seed.sourceHash,
					status: "unchanged",
					...(predecessorSessionId
						? { predecessorSessionId, mode: "new_snapshot" }
						: {}),
				};
			}
		}
		// Reserve the source revision before worker bootstrap as a recovery fence.
		// The checkpoint points only to protected bytes, never imported authority.
		await saveCheckpoint();
		// Bootstrap happens in the normal worker factory, before services/scheduler.
		// Close before publication: no operator can attach to the staging worker.
		try {
			await active.open(metadata);
		} finally {
			await active.close(metadata);
		}
		const temporary = join(path, ".oqt-import-meta");
		await writeFile(
			temporary,
			JSON.stringify({ createdAt: metadata.createdAt, cwd: metadata.cwd }),
			{ mode: 0o600 },
		);
		// Never replace an existing catalog file, even on recovery.
		try {
			await writeFile(join(path, "meta.json"), await readFile(temporary), {
				flag: "wx",
				mode: 0o600,
			});
		} catch (error) {
			if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
			const saved = JSON.parse(await readFile(join(path, "meta.json"), "utf8"));
			if (saved.cwd !== metadata.cwd)
				throw new Error("Classic import publication conflict");
		}
		await rm(join(path, PENDING), { force: true });
		// Atomic protected publication is the durable admission authority. A crash
		// before this rename remains hidden even if meta.json already exists.
		const publicationTemporary = `${publicationPath}.${randomUUID()}`;
		await writeFile(
			publicationTemporary,
			JSON.stringify({ ...publication, state: "published" }),
			{ flag: "wx", mode: 0o600 },
		);
		await rename(publicationTemporary, publicationPath);
		stagedSessions.delete(sessionId);
		await rm(temporary, { force: true });
		await saveCheckpoint(true);
		await active.refresh();
		return {
			sessionId,
			sourceHash: seed.sourceHash,
			status: "imported",
			...(predecessorSessionId
				? {
						predecessorSessionId,
						mode: "new_snapshot",
						reason: "source_append_destination_preserved",
					}
				: {}),
		};
	} finally {
		committing = false;
		await rm(upload.path, { force: true });
	}
}

/** Worker-side read of the host-sealed seed, never an external source path. */
export async function readClassicImportSeed(): Promise<
	ClassicImportSeed | undefined
> {
	const path = process.env.OQT_CLASSIC_IMPORT_SEED;
	if (!path) return undefined;
	const seed: ClassicImportSeed = JSON.parse(await readFile(path, "utf8"));
	validateClassicImportSeed(seed);
	return seed;
}

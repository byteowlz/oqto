// Local-operator source reader. Never opens classic SessionManager or repairs
// Pi JSONL; the host receives immutable bytes, never this original source path.

import { createHash } from "node:crypto";
import { constants, type Dir } from "node:fs";
import { lstat, open, opendir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { type ClassicSnapshot, classicSnapshot } from "./classic-import.ts";

export const MAX_CLASSIC_SOURCE_BYTES = 64 * 1024 * 1024;
export interface ClassicSourceSummary {
	readonly path: string;
	readonly sourceId: string;
	readonly sessionId: string;
	readonly cwd: string;
	readonly leaves: readonly string[];
	readonly sourceHash: string;
}
export interface ClassicSourceDiscovery {
	readonly sources: ClassicSourceSummary[];
	readonly skipped: { path: string; reason: string }[];
	readonly truncated: boolean;
}

/** Explicit-root discovery only. No getDefaultSessionDir/mkdir/migration and no
 * symlink traversal. Source labels are data, never destination/workspace scope. */
export async function discoverClassicSources(
	root: string,
	origin: string,
): Promise<ClassicSourceDiscovery> {
	if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(origin))
		throw new Error("An explicit stable source origin is required");
	const sources: ClassicSourceSummary[] = [];
	const skipped: { path: string; reason: string }[] = [];
	const queue = [{ path: resolve(root), depth: 0 }];
	let visited = 0;
	let truncated = false;
	while (queue.length) {
		const current = queue.shift();
		if (!current) break;
		let directory: Dir;
		try {
			const meta = await lstat(current.path);
			if (!meta.isDirectory() || meta.isSymbolicLink()) {
				skipped.push({
					path: current.path,
					reason: "Not a regular directory (symlinks are not traversed)",
				});
				continue;
			}
			directory = await opendir(current.path);
		} catch (error) {
			if ((error as NodeJS.ErrnoException).code === "ENOENT") continue;
			throw error;
		}
		for await (const entry of directory) {
			if (++visited > 1000 || sources.length >= 200) {
				truncated = true;
				break;
			}
			const path = join(current.path, entry.name);
			if (entry.isDirectory() && !entry.name.startsWith(".")) {
				if (current.depth < 4) queue.push({ path, depth: current.depth + 1 });
				else truncated = true;
			} else if (entry.isFile() && entry.name.endsWith(".jsonl")) {
				try {
					const source = await readClassicSource(path, origin);
					sources.push({
						path: source.path,
						sourceId: source.snapshot.sourceId,
						sessionId: String(source.snapshot.header.id),
						cwd: String(source.snapshot.header.cwd),
						leaves: source.snapshot.leaves,
						sourceHash: source.sourceHash,
					});
				} catch (error) {
					skipped.push({
						path,
						reason: error instanceof Error ? error.message : "Invalid source",
					});
				}
			}
		}
		if (visited > 1000 || sources.length >= 200) break;
	}
	sources.sort(
		(a, b) =>
			a.sourceId.localeCompare(b.sourceId) || a.path.localeCompare(b.path),
	);
	return { sources, skipped, truncated };
}
export interface FrozenClassicSource {
	readonly path: string;
	readonly bytes: Buffer;
	readonly text: string;
	readonly sourceHash: string;
	readonly snapshot: ClassicSnapshot;
}

export async function readClassicSource(
	path: string,
	origin: string,
): Promise<FrozenClassicSource> {
	if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(origin))
		throw new Error("An explicit stable source origin is required");
	const absolute = resolve(path);
	const entry = await lstat(absolute);
	if (!entry.isFile() || entry.isSymbolicLink())
		throw new Error("Classic source must be a regular non-symlink file");
	const file = await open(
		absolute,
		constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
	);
	try {
		const before = await file.stat({ bigint: true });
		if (!before.isFile())
			throw new Error("Classic source must be a regular file");
		if (before.size > BigInt(MAX_CLASSIC_SOURCE_BYTES))
			throw new Error("Classic source exceeds 64 MiB");
		const chunks: Buffer[] = [];
		let length = 0;
		for (;;) {
			const chunk = Buffer.alloc(
				Math.min(64 * 1024, MAX_CLASSIC_SOURCE_BYTES + 1 - length),
			);
			const { bytesRead } = await file.read(chunk, 0, chunk.length, null);
			if (!bytesRead) break;
			length += bytesRead;
			if (length > MAX_CLASSIC_SOURCE_BYTES)
				throw new Error("Classic source exceeds 64 MiB");
			chunks.push(chunk.subarray(0, bytesRead));
		}
		const after = await file.stat({ bigint: true });
		const named = await lstat(absolute, { bigint: true });
		if (
			named.isSymbolicLink() ||
			!named.isFile() ||
			named.ino !== before.ino ||
			named.dev !== before.dev ||
			before.size !== after.size ||
			before.mtimeNs !== after.mtimeNs ||
			before.ctimeNs !== after.ctimeNs ||
			named.size !== after.size ||
			named.mtimeNs !== after.mtimeNs ||
			named.ctimeNs !== after.ctimeNs ||
			BigInt(length) !== after.size
		)
			throw new Error("Classic source changed during read; retry after sync");
		const bytes = Buffer.concat(chunks, length);
		const text = new TextDecoder("utf-8", {
			fatal: true,
			ignoreBOM: true,
		}).decode(bytes);
		const snapshot = classicSnapshot(text, origin);
		return {
			path: absolute,
			bytes,
			text,
			sourceHash: createHash("sha256").update(bytes).digest("hex"),
			snapshot,
		};
	} finally {
		await file.close();
	}
}

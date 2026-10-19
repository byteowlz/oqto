// Read-only source validation/update planning. This module never opens storage,
// schedules a task or writes Pi-owned JSONL. A worker-owned importer must map
// these native identities and preserve raw records before activating context.
import { createHash } from "node:crypto";

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type Row = { [key: string]: Json };
export interface ClassicSnapshot {
	readonly sourceId: string;
	readonly sourceText: string;
	readonly sourceDigest: string;
	readonly header: Row;
	readonly records: readonly Row[];
	readonly entryDigests: Readonly<Record<string, string>>;
	readonly headerDigest: string;
	readonly leaves: readonly string[];
	readonly activeLeaf: string | null;
}
export type ClassicUpdate =
	| { kind: "unchanged" }
	| { kind: "append"; newIds: string[] }
	| { kind: "branch"; reason: string; newIds: string[] }
	| { kind: "conflict"; reason: string };

function row(value: unknown): value is Row {
	return value !== null && typeof value === "object" && !Array.isArray(value);
}
function canonical(value: Json): string {
	if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
	if (value !== null && typeof value === "object")
		return `{${Object.keys(value)
			.sort()
			.map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
			.join(",")}}`;
	return JSON.stringify(value);
}
function digest(value: Json): string {
	return createHash("sha256").update(canonical(value)).digest("hex");
}
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const ENTRY_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;

/** Strict v3 preflight. An unfinished synced file must be retried once stable,
 * not imported as if the missing suffix never existed. Legacy versions need an
 * explicit migration adapter; synthesizing IDs here would break reimport. */
export function classicSnapshot(
	text: string,
	sourceOrigin: string,
): ClassicSnapshot {
	if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(sourceOrigin))
		throw new Error(
			"Explicit source origin must be a stable name, not a filesystem path",
		);
	if (Buffer.byteLength(text, "utf8") > 64 * 1024 * 1024)
		throw new Error("Classic JSONL source exceeds the 64 MiB preflight limit");
	if (!text.endsWith("\n"))
		throw new Error(
			"Incomplete classic JSONL: final newline missing; retry after sync",
		);
	const parsed: Row[] = [];
	for (const [lineNumber, line] of text.split("\n").entries()) {
		if (!line.trim()) continue;
		let value: unknown;
		try {
			value = JSON.parse(line);
		} catch {
			throw new Error(`Invalid JSON at classic source line ${lineNumber + 1}`);
		}
		if (!row(value))
			throw new Error(
				`Expected object at classic source line ${lineNumber + 1}`,
			);
		if (parsed.length >= 100_001)
			throw new Error("Classic source exceeds 100000 entries");
		const pending: Array<{ value: Json; depth: number }> = [
			{ value, depth: 0 },
		];
		while (pending.length) {
			const current = pending.pop();
			if (!current) break;
			if (current.depth > 64)
				throw new Error("Classic source exceeds nesting limit");
			if (current.value !== null && typeof current.value === "object") {
				for (const child of Object.values(current.value))
					pending.push({ value: child, depth: current.depth + 1 });
			}
		}
		parsed.push(value);
	}
	const [header, ...records] = parsed;
	if (
		!header ||
		header.type !== "session" ||
		header.version !== 3 ||
		typeof header.id !== "string" ||
		!UUID.test(header.id) ||
		typeof header.cwd !== "string" ||
		typeof header.timestamp !== "string"
	)
		throw new Error(
			"Expected a classic Pi v3 session header with native UUID, cwd and timestamp",
		);
	const entryDigests: Record<string, string> = Object.create(null);
	const parents = new Set<string>();
	let activeLeaf: string | null = null;
	for (const record of records) {
		if (
			record.type === "session" ||
			typeof record.type !== "string" ||
			!record.type ||
			typeof record.id !== "string" ||
			!ENTRY_ID.test(record.id) ||
			typeof record.timestamp !== "string" ||
			!(record.parentId === null || typeof record.parentId === "string")
		)
			throw new Error(
				"Invalid classic entry: type, id, parentId and timestamp are required",
			);
		if (Object.hasOwn(entryDigests, record.id))
			throw new Error(`Duplicate classic entry ID ${record.id}`);
		if (record.parentId !== null) {
			if (!Object.hasOwn(entryDigests, record.parentId))
				throw new Error(
					`Unresolved or cyclic classic parent ${JSON.stringify(record.parentId)}`,
				);
			parents.add(record.parentId);
		}
		entryDigests[record.id] = digest(record);
		activeLeaf = record.id;
	}
	return {
		sourceId: `pi:${sourceOrigin}:${header.id.toLowerCase()}`,
		sourceText: text,
		sourceDigest: createHash("sha256").update(text, "utf8").digest("hex"),
		header,
		headerDigest: digest(header),
		records,
		entryDigests,
		activeLeaf,
		leaves: records
			.map((record) => record.id as string)
			.filter((id) => !parents.has(id)),
	};
}

/** Native ID/digest comparison, not a message-count/text heuristic. An append
 * on a different ancestor or a locally advanced destination requires a branch.
 * This only plans; it grants no authority to overwrite a destination store. */
export function planClassicUpdate(
	previous: ClassicSnapshot,
	incoming: ClassicSnapshot,
	destinationAdvanced: boolean,
): ClassicUpdate {
	if (
		previous.sourceId !== incoming.sourceId ||
		previous.headerDigest !== incoming.headerDigest
	)
		return {
			kind: "conflict",
			reason:
				"Source identity or header changed; explicit reconciliation required",
		};
	for (const [id, hash] of Object.entries(previous.entryDigests)) {
		if (incoming.entryDigests[id] !== hash)
			return {
				kind: "conflict",
				reason: `Source entry ${id} changed or disappeared`,
			};
	}
	const newIds = incoming.records
		.map((record) => record.id as string)
		.filter((id) => !Object.hasOwn(previous.entryDigests, id));
	if (newIds.length === 0) {
		if (previous.activeLeaf !== incoming.activeLeaf)
			return {
				kind: "branch",
				reason: "Source selected lineage changed",
				newIds,
			};
		return { kind: "unchanged" };
	}
	if (destinationAdvanced)
		return {
			kind: "branch",
			reason: "Destination has its own continuation",
			newIds,
		};
	const byId = new Map(incoming.records.map((record) => [record.id, record]));
	let ancestor = incoming.activeLeaf;
	while (ancestor !== null && !Object.hasOwn(previous.entryDigests, ancestor)) {
		ancestor = byId.get(ancestor)?.parentId as string | null;
	}
	if (ancestor !== previous.activeLeaf)
		return {
			kind: "branch",
			reason: "Source continues a different ancestor",
			newIds,
		};
	return { kind: "append", newIds };
}

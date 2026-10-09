/**
 * Versioned layout persistence codec (ADR-0041/0042/0043). Pure string <->
 * snapshot transformation with explicit migrations and provenance; storage
 * I/O lives in host adapters. Unknown future variants survive round trips
 * via preserved extension fields; corrupted documents fail with typed
 * reasons so hosts can fall back to last-known-good state.
 */

import { checkLayoutInvariants } from "../invariants";
import type { JsonValue, LayoutSnapshot } from "../model";
import { LAYOUT_SCHEMA_VERSION } from "../model";
import { isJsonObject } from "./json-guards";
import {
	LAYOUT_MIGRATIONS,
	type LayoutMigration,
	runLayoutMigrations,
} from "./migrations";
import { readSnapshot } from "./read";

export type DecodeFailureReason =
	| "parse-error"
	| "invalid-shape"
	| "unsupported-version"
	| "invariant-violation";

export type DecodeLayoutResult =
	| {
			readonly ok: true;
			readonly snapshot: LayoutSnapshot;
			readonly migratedFrom: number | null;
	  }
	| {
			readonly ok: false;
			readonly reason: DecodeFailureReason;
			readonly detail: string;
	  };

export interface RecoveredLayout {
	readonly snapshot: LayoutSnapshot;
	readonly sourceIndex: number | null;
	readonly failures: readonly {
		readonly index: number;
		readonly reason: DecodeFailureReason;
		readonly detail: string;
	}[];
}

export function encodeLayoutDocument(snapshot: LayoutSnapshot): string {
	return JSON.stringify({
		...snapshot.extensions,
		schemaVersion: snapshot.schemaVersion,
		revision: snapshot.revision,
		idSeed: snapshot.idSeed,
		strategy: snapshot.strategy,
		activeArrangementId: snapshot.activeArrangementId,
		activeWorkspace: snapshot.activeWorkspace,
		focusedContentId: snapshot.focusedContentId,
		arrangements: snapshot.arrangements.map((arrangement) => ({
			...arrangement.extensions,
			id: arrangement.id,
			...(arrangement.label !== undefined ? { label: arrangement.label } : {}),
			...(arrangement.binding !== undefined
				? { binding: arrangement.binding }
				: {}),
			scrollAnchorColumnId: arrangement.scrollAnchorColumnId,
			lastFocusedContentId: arrangement.lastFocusedContentId,
			grid: arrangement.grid,
		})),
		containers: snapshot.containers.map((container) => ({
			...container.extensions,
			id: container.id,
			...(container.role !== undefined ? { role: container.role } : {}),
			emptyBehavior: container.emptyBehavior,
			collapsed: container.collapsed,
			activeContentId: container.activeContentId,
			stack: container.stack.map((content) => ({
				...content.extensions,
				id: content.id,
				kind: content.kind,
			})),
		})),
	});
}

export function decodeLayoutDocument(
	raw: string,
	migrations: readonly LayoutMigration[] = LAYOUT_MIGRATIONS,
): DecodeLayoutResult {
	let parsed: JsonValue;
	try {
		parsed = JSON.parse(raw) as JsonValue;
	} catch (error) {
		return {
			ok: false,
			reason: "parse-error",
			detail: error instanceof Error ? error.message : String(error),
		};
	}
	if (!isJsonObject(parsed)) {
		return {
			ok: false,
			reason: "invalid-shape",
			detail: "layout document root must be an object",
		};
	}
	const version = parsed.schemaVersion;
	if (
		typeof version !== "number" ||
		!Number.isInteger(version) ||
		version < 1
	) {
		return {
			ok: false,
			reason: "invalid-shape",
			detail: "layout document is missing a valid schemaVersion",
		};
	}
	if (version > LAYOUT_SCHEMA_VERSION) {
		return {
			ok: false,
			reason: "unsupported-version",
			detail: `schemaVersion ${version} is newer than ${LAYOUT_SCHEMA_VERSION}`,
		};
	}
	const migrated = runLayoutMigrations(parsed, migrations);
	if (!migrated) {
		return {
			ok: false,
			reason: "unsupported-version",
			detail: `no migration path from schemaVersion ${version}`,
		};
	}
	const snapshot = readSnapshot(migrated.layoutDocument);
	if (!snapshot) {
		return {
			ok: false,
			reason: "invalid-shape",
			detail: "layout document fields failed validation",
		};
	}
	const violations = checkLayoutInvariants(snapshot);
	if (violations.length > 0) {
		return {
			ok: false,
			reason: "invariant-violation",
			detail: violations
				.map((violation) => `${violation.code}: ${violation.detail}`)
				.join("; "),
		};
	}
	return {
		ok: true,
		snapshot,
		migratedFrom:
			migrated.from === LAYOUT_SCHEMA_VERSION ? null : migrated.from,
	};
}

/**
 * Last-known-good recovery: decodes candidates in order (primary first,
 * then backups) and falls back to the provided default when every
 * candidate is corrupt or absent.
 */
export function recoverLayoutDocument(
	candidates: readonly (string | null | undefined)[],
	fallback: LayoutSnapshot,
): RecoveredLayout {
	const failures: {
		index: number;
		reason: DecodeFailureReason;
		detail: string;
	}[] = [];
	for (let index = 0; index < candidates.length; index += 1) {
		const candidate = candidates[index];
		if (candidate === null || candidate === undefined) continue;
		const result = decodeLayoutDocument(candidate);
		if (result.ok)
			return { snapshot: result.snapshot, sourceIndex: index, failures };
		failures.push({ index, reason: result.reason, detail: result.detail });
	}
	return { snapshot: fallback, sourceIndex: null, failures };
}

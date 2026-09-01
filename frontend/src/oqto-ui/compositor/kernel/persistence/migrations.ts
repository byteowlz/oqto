/**
 * Layout document migrations. Each step maps schema N to N+1 on raw JSON
 * before validation; unknown fields pass through untouched.
 */

import type { ExtensionFields, JsonValue } from "../model";
import { LAYOUT_SCHEMA_VERSION } from "../model";
import { isJsonObject } from "./json-guards";

export interface LayoutMigration {
	readonly from: number;
	readonly migrate: (layoutDocument: ExtensionFields) => ExtensionFields;
}

/**
 * v1 -> v2 (ADR-0042/0043): columns-of-cells become row/column tracks with
 * spanned placements inside one Arrangement. Each v1 column becomes a
 * column track; the row count is the largest cell count; a column with k
 * cells splits the rows evenly by index (cell block sizes are not carried
 * over — v1 never shipped to users, so this lossiness is documented, not
 * mitigated). Focus becomes the Arrangement's focus memory.
 */
function migrateV1ToV2(layoutDocument: ExtensionFields): ExtensionFields {
	const grid = layoutDocument.grid;
	const idSeed = layoutDocument.idSeed;
	if (!isJsonObject(grid) || !Array.isArray(grid.columns) || typeof idSeed !== "number") {
		return { ...layoutDocument, schemaVersion: 2 };
	}
	const v1Columns = grid.columns.filter(isJsonObject);
	const cellCounts = v1Columns.map((column) => (Array.isArray(column.cells) ? column.cells.length : 0));
	const rowCount = Math.max(1, ...cellCounts);
	let seed = Math.floor(idSeed);
	const rows: JsonValue[] = [];
	for (let index = 0; index < rowCount; index += 1) {
		rows.push({ id: `track-${seed}`, size: { unit: "fraction", value: 1 } });
		seed += 1;
	}
	const columns: JsonValue[] = [];
	const placements: JsonValue[] = [];
	v1Columns.forEach((column, columnIndex) => {
		columns.push({ id: String(column.id ?? `track-${seed + columnIndex}`), size: column.size ?? { unit: "fraction", value: 1 } });
		const cells = Array.isArray(column.cells) ? column.cells.filter(isJsonObject) : [];
		cells.forEach((cell, cellIndex) => {
			const start = Math.floor((cellIndex * rowCount) / cells.length);
			const end = Math.floor(((cellIndex + 1) * rowCount) / cells.length);
			placements.push({
				containerId: String(cell.containerId ?? ""),
				row: start,
				column: columnIndex,
				rowSpan: Math.max(1, end - start),
				colSpan: 1,
			});
		});
	});
	const arrangementId = `arrangement-${seed}`;
	const focus = layoutDocument.focusedContentId ?? null;
	const { grid: _v1Grid, ...rest } = layoutDocument;
	return {
		...rest,
		schemaVersion: 2,
		idSeed: seed + 1,
		activeArrangementId: arrangementId,
		activeWorkspace: { kind: "all" },
		arrangements: [
			{
				id: arrangementId,
				grid: { rows, columns, placements },
				scrollAnchorColumnId: null,
				lastFocusedContentId: typeof focus === "string" ? focus : null,
			},
		],
	};
}

/** Registered schema migrations; append one entry per version bump. */
export const LAYOUT_MIGRATIONS: readonly LayoutMigration[] = [{ from: 1, migrate: migrateV1ToV2 }];

/** Walks migrations from the document version upward. */
export function runLayoutMigrations(
	layoutDocument: ExtensionFields,
	migrations: readonly LayoutMigration[],
	targetVersion: number = LAYOUT_SCHEMA_VERSION,
): { readonly layoutDocument: ExtensionFields; readonly from: number } | null {
	const version = layoutDocument.schemaVersion;
	if (!(typeof version === "number" && Number.isInteger(version)) || version < 1) return null;
	let working = layoutDocument;
	let current = version;
	while (current < targetVersion) {
		const step = migrations.find((migration) => migration.from === current);
		if (!step) return null;
		working = { ...step.migrate(working), schemaVersion: current + 1 };
		current += 1;
	}
	return { layoutDocument: working, from: version };
}

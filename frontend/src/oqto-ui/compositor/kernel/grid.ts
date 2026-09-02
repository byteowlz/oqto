/**
 * Semantic grid topology operations, schema v2 (ADR-0042): row/column
 * tracks, placements with spans, deterministic insertion and repair. A Lane
 * is simply a row track. Placement and repair are internal parts of the
 * Layout Engine, not a public strategy seam.
 */

import type { ContainerId } from "./ids";
import type { GridPlacement, GridTopology, GridTrack } from "./model";

export interface ColumnInsertion {
	readonly containerId: ContainerId;
	readonly row: number;
	readonly rowSpan: number;
	/** When subdividing this placement's column band, neighbors covering the band grow. */
	readonly subdivide?: GridPlacement;
}

export interface RowInsertion {
	readonly containerId: ContainerId;
	readonly column: number;
	readonly colSpan: number;
	/** When subdividing this placement's row band, neighbors covering the band grow. */
	readonly subdivide?: GridPlacement;
}

/**
 * Whether `placement` must grow when a track is inserted at `at` along
 * `axis`: it straddles the insertion, or it fully covers the band being
 * subdivided (a full-height sidebar beside a vertically split editor).
 */
function grows(
	placement: GridPlacement,
	axis: "row" | "column",
	at: number,
	band: GridPlacement | undefined,
): boolean {
	const spanKey = axis === "row" ? "rowSpan" : "colSpan";
	const start = placement[axis];
	const end = start + placement[spanKey];
	if (start < at && at < end) return true;
	if (!band || band.containerId === placement.containerId) return false;
	return start <= band[axis] && end >= band[axis] + band[spanKey];
}

export function placementOf(
	grid: GridTopology,
	containerId: ContainerId,
): GridPlacement | null {
	return (
		grid.placements.find(
			(placement) => placement.containerId === containerId,
		) ?? null
	);
}

/** Container ids in deterministic reading order (row-major). */
export function containerOrder(grid: GridTopology): readonly ContainerId[] {
	return [...grid.placements]
		.sort((a, b) => a.row - b.row || a.column - b.column)
		.map((placement) => placement.containerId);
}

/** Inserts a column track at `index`, growing placements that span it. */
export function insertColumnTrack(
	grid: GridTopology,
	index: number,
	track: GridTrack,
	insertion: ColumnInsertion,
): GridTopology {
	const at = Math.max(0, Math.min(index, grid.columns.length));
	const columns = [...grid.columns];
	columns.splice(at, 0, track);
	const rows = grid.rows.length === 0 ? [placeholderRow(track)] : grid.rows;
	const placements = grid.placements.map((placement) => {
		if (grows(placement, "column", at, insertion.subdivide)) {
			return { ...placement, colSpan: placement.colSpan + 1 };
		}
		if (placement.column >= at) {
			return { ...placement, column: placement.column + 1 };
		}
		return placement;
	});
	placements.push({
		containerId: insertion.containerId,
		row: insertion.row,
		column: at,
		rowSpan: Math.max(1, insertion.rowSpan),
		colSpan: 1,
	});
	return { rows, columns, placements };
}

/** Inserts a row track (a Lane) at `index`, growing placements that span it. */
export function insertRowTrack(
	grid: GridTopology,
	index: number,
	track: GridTrack,
	insertion: RowInsertion,
): GridTopology {
	const at = Math.max(0, Math.min(index, grid.rows.length));
	const rows = [...grid.rows];
	rows.splice(at, 0, track);
	const columns =
		grid.columns.length === 0 ? [placeholderColumn(track)] : grid.columns;
	const placements = grid.placements.map((placement) => {
		if (grows(placement, "row", at, insertion.subdivide)) {
			return { ...placement, rowSpan: placement.rowSpan + 1 };
		}
		if (placement.row >= at) {
			return { ...placement, row: placement.row + 1 };
		}
		return placement;
	});
	placements.push({
		containerId: insertion.containerId,
		row: at,
		column: insertion.column,
		rowSpan: 1,
		colSpan: Math.max(1, insertion.colSpan),
	});
	return { rows, columns, placements };
}

function placeholderRow(track: GridTrack): GridTrack {
	return {
		id: `${track.id}-row` as GridTrack["id"],
		size: { unit: "fraction", value: 1 },
	};
}

function placeholderColumn(track: GridTrack): GridTrack {
	return {
		id: `${track.id}-column` as GridTrack["id"],
		size: { unit: "fraction", value: 1 },
	};
}

/** Removes a Container's placement and repairs the grid. */
export function removePlacement(
	grid: GridTopology,
	containerId: ContainerId,
): GridTopology {
	return repairGrid({
		...grid,
		placements: grid.placements.filter(
			(placement) => placement.containerId !== containerId,
		),
	});
}

/**
 * Drops tracks no placement needs. A track is removable when every
 * placement covering it also covers another track along the same axis
 * (or nothing covers it); spans shrink and indexes shift accordingly.
 * Iterates to a fixed point, so a grid with no placements ends empty.
 */
export function repairGrid(grid: GridTopology): GridTopology {
	let rows = grid.rows;
	let columns = grid.columns;
	let placements = grid.placements;
	for (;;) {
		const row = removableIndex(rows.length, placements, "row");
		if (row !== null) {
			rows = rows.filter((_, index) => index !== row);
			placements = shrinkAxis(placements, row, "row");
			continue;
		}
		const column = removableIndex(columns.length, placements, "column");
		if (column !== null) {
			columns = columns.filter((_, index) => index !== column);
			placements = shrinkAxis(placements, column, "column");
			continue;
		}
		break;
	}
	return { rows, columns, placements };
}

function removableIndex(
	count: number,
	placements: readonly GridPlacement[],
	axis: "row" | "column",
): number | null {
	const spanKey = axis === "row" ? "rowSpan" : "colSpan";
	for (let index = 0; index < count; index += 1) {
		const covering = placements.filter(
			(placement) =>
				placement[axis] <= index &&
				index < placement[axis] + placement[spanKey],
		);
		if (covering.every((placement) => placement[spanKey] > 1)) return index;
	}
	return null;
}

function shrinkAxis(
	placements: readonly GridPlacement[],
	index: number,
	axis: "row" | "column",
): GridPlacement[] {
	const spanKey = axis === "row" ? "rowSpan" : "colSpan";
	return placements.map((placement) => {
		if (placement[axis] > index) {
			return { ...placement, [axis]: placement[axis] - 1 };
		}
		if (index < placement[axis] + placement[spanKey]) {
			return { ...placement, [spanKey]: placement[spanKey] - 1 };
		}
		return placement;
	});
}

export function replaceTrack(
	grid: GridTopology,
	axis: "row" | "column",
	index: number,
	track: GridTrack,
): GridTopology {
	const key = axis === "row" ? "rows" : "columns";
	return {
		...grid,
		[key]: grid[key].map((existing, at) => (at === index ? track : existing)),
	};
}

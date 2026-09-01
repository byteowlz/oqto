/**
 * Deterministic creation of Containers and tracks from the snapshot's id
 * seed. Shared by open (auto-placement) and split (edge and
 * container-relative track insertion).
 */

import type { SplitEdge } from "./commands";
import {
	insertColumnTrack,
	insertRowTrack,
	placementOf,
	replaceTrack,
} from "./grid";
import { type ContainerId, containerIdFrom, trackIdFrom } from "./ids";
import type {
	Arrangement,
	Container,
	ContentRef,
	GridTopology,
	GridTrack,
	TrackSize,
} from "./model";

export interface MintedContainer {
	readonly container: Container;
	readonly grid: GridTopology;
	readonly idSeed: number;
}

function halved(size: TrackSize): { existing: TrackSize; created: TrackSize } {
	if (size.unit === "fixed") {
		return { existing: size, created: { unit: "fraction", value: 1 } };
	}
	const half = size.value / 2;
	return {
		existing: { ...size, value: half },
		created: { unit: "fraction", value: half },
	};
}

function newContainer(
	id: ContainerId,
	stack: readonly ContentRef[],
	role: string | undefined,
): Container {
	return {
		id,
		...(role !== undefined ? { role } : {}),
		emptyBehavior: "remove",
		stack,
		activeContentId: stack[0]?.id ?? null,
		collapsed: false,
	};
}

/**
 * Places a new Container at an Arrangement edge: a full-height column at
 * inline edges, a full-width Lane at block edges. An empty grid gets one
 * row and one column.
 */
export function mintEdgeContainer(
	arrangement: Arrangement,
	idSeed: number,
	edge: SplitEdge,
	stack: readonly ContentRef[],
	role?: string,
): MintedContainer {
	const container = newContainer(containerIdFrom(idSeed), stack, role);
	const grid = arrangement.grid;
	if (grid.rows.length === 0 || grid.columns.length === 0) {
		return {
			container,
			idSeed: idSeed + 3,
			grid: {
				rows: [{ id: trackIdFrom(idSeed + 1), size: { unit: "fraction", value: 1 } }],
				columns: [
					{ id: trackIdFrom(idSeed + 2), size: { unit: "fraction", value: 1 } },
				],
				placements: [
					{ containerId: container.id, row: 0, column: 0, rowSpan: 1, colSpan: 1 },
				],
			},
		};
	}
	const track: GridTrack = { id: trackIdFrom(idSeed + 1), size: { unit: "fraction", value: 1 } };
	if (edge === "inline-start" || edge === "inline-end") {
		return {
			container,
			idSeed: idSeed + 2,
			grid: insertColumnTrack(
				grid,
				edge === "inline-start" ? 0 : grid.columns.length,
				track,
				{ containerId: container.id, row: 0, rowSpan: grid.rows.length },
			),
		};
	}
	return {
		container,
		idSeed: idSeed + 2,
		grid: insertRowTrack(
			grid,
			edge === "block-start" ? 0 : grid.rows.length,
			{ ...track, size: { unit: "fixed", value: 240, min: 120 } },
			{ containerId: container.id, column: 0, colSpan: grid.columns.length },
		),
	};
}

/**
 * Splits next to an existing Container: inserts a track beside its
 * placement, halving the split track, and places the new Container in the
 * same rows (inline split) or columns (block split).
 */
export function mintSplitContainer(
	arrangement: Arrangement,
	idSeed: number,
	edge: SplitEdge,
	relativeTo: ContainerId,
	stack: readonly ContentRef[],
): MintedContainer | null {
	const placement = placementOf(arrangement.grid, relativeTo);
	if (!placement) return null;
	const container = newContainer(containerIdFrom(idSeed), stack, undefined);
	const trackId = trackIdFrom(idSeed + 1);
	if (edge === "inline-start" || edge === "inline-end") {
		const index =
			edge === "inline-start"
				? placement.column
				: placement.column + placement.colSpan;
		const sizes = halved(arrangement.grid.columns[placement.column].size);
		const halvedGrid = replaceTrack(arrangement.grid, "column", placement.column, {
			...arrangement.grid.columns[placement.column],
			size: sizes.existing,
		});
		return {
			container,
			idSeed: idSeed + 2,
			grid: insertColumnTrack(
				halvedGrid,
				index,
				{ id: trackId, size: sizes.created },
				{ containerId: container.id, row: placement.row, rowSpan: placement.rowSpan, subdivide: placement },
			),
		};
	}
	const index =
		edge === "block-start" ? placement.row : placement.row + placement.rowSpan;
	const sizes = halved(arrangement.grid.rows[placement.row].size);
	const halvedGrid = replaceTrack(arrangement.grid, "row", placement.row, {
		...arrangement.grid.rows[placement.row],
		size: sizes.existing,
	});
	return {
		container,
		idSeed: idSeed + 2,
		grid: insertRowTrack(
			halvedGrid,
			index,
			{ id: trackId, size: sizes.created },
			{ containerId: container.id, column: placement.column, colSpan: placement.colSpan, subdivide: placement },
		),
	};
}

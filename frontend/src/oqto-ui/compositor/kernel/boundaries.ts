/**
 * Where a resize boundary actually exists (ADR-0042). A track boundary is
 * not uniformly draggable: a full-height sidebar spans every row, so the
 * row boundary beside it separates nothing, and frame chrome such as the
 * status Lane is placed rather than resized. This module reduces a boundary
 * to the cross-axis segments where two different, resizable Containers
 * really meet, so the adapter can draw a handle exactly there and nowhere
 * else.
 */

import type { ContainerId } from "./ids";
import type { Arrangement, Container, GridPlacement } from "./model";

/** Roles whose Container is frame chrome: placed by the Preset, never resized. */
const CHROME_ROLES = new Set(["status"]);

export type BoundaryAxis = "inline" | "block";

/**
 * One draggable run of a track boundary. `index` is the track before the
 * boundary on its own axis; `start`/`end` are inclusive track indices on the
 * cross axis.
 */
export interface BoundarySegment {
	readonly index: number;
	readonly start: number;
	readonly end: number;
}

function covers(
	placement: GridPlacement,
	row: number,
	column: number,
): boolean {
	return (
		placement.row <= row &&
		row < placement.row + placement.rowSpan &&
		placement.column <= column &&
		column < placement.column + placement.colSpan
	);
}

function containerAt(
	arrangement: Arrangement,
	row: number,
	column: number,
): GridPlacement | null {
	return (
		arrangement.grid.placements.find((placement) =>
			covers(placement, row, column),
		) ?? null
	);
}

function resizable(
	containers: readonly Container[],
	containerId: ContainerId,
): boolean {
	const container = containers.find(
		(candidate) => candidate.id === containerId,
	);
	return container !== undefined && !CHROME_ROLES.has(container.role ?? "");
}

/**
 * The segments of every boundary along `axis` where two different resizable
 * Containers meet. A boundary crossed by one Container (a full-height
 * sidebar, a status Lane spanning its columns) yields no segment there, so a
 * boundary that separates nothing yields none at all.
 */
export function resizeBoundaries(
	arrangement: Arrangement,
	containers: readonly Container[],
	axis: BoundaryAxis,
): BoundarySegment[] {
	const along =
		axis === "inline" ? arrangement.grid.columns : arrangement.grid.rows;
	const across =
		axis === "inline" ? arrangement.grid.rows : arrangement.grid.columns;
	const segments: BoundarySegment[] = [];
	for (let index = 0; index + 1 < along.length; index += 1) {
		let start: number | null = null;
		for (let cross = 0; cross <= across.length; cross += 1) {
			const separated =
				cross < across.length &&
				divides(arrangement, containers, axis, index, cross);
			if (separated && start === null) start = cross;
			if (!separated && start !== null) {
				segments.push({ index, start, end: cross - 1 });
				start = null;
			}
		}
	}
	return segments;
}

/** True when the boundary separates two different resizable Containers here. */
function divides(
	arrangement: Arrangement,
	containers: readonly Container[],
	axis: BoundaryAxis,
	index: number,
	cross: number,
): boolean {
	const before =
		axis === "inline"
			? containerAt(arrangement, cross, index)
			: containerAt(arrangement, index, cross);
	const after =
		axis === "inline"
			? containerAt(arrangement, cross, index + 1)
			: containerAt(arrangement, index + 1, cross);
	if (!before || !after) return false;
	if (before.containerId === after.containerId) return false;
	return (
		resizable(containers, before.containerId) &&
		resizable(containers, after.containerId)
	);
}

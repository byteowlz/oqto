/**
 * Pure gesture-to-command helpers for the web adapter. Pointer geometry is
 * hit-tested here and turned into semantic commands; nothing in this module
 * touches the DOM or layout state, so every mapping is unit-testable.
 */

import type {
	Arrangement,
	ContainerId,
	ContentId,
	LayoutCommand,
	SolvedLayout,
	SplitEdge,
	TrackSize,
} from "../index";

export type DropZone = "center" | SplitEdge;

export interface PointerBox {
	readonly x: number;
	readonly y: number;
	readonly width: number;
	readonly height: number;
}

export const CONTENT_DRAG_TYPE = "application/x-oqto-content";

/** Center tabs the Content; the outer quarter on each side splits beside it. */
export function dropZoneAt(
	box: PointerBox,
	pointerX: number,
	pointerY: number,
	edgeFraction = 0.25,
): DropZone {
	if (box.width <= 0 || box.height <= 0) return "center";
	const fx = (pointerX - box.x) / box.width;
	const fy = (pointerY - box.y) / box.height;
	const candidates: { edge: SplitEdge; distance: number }[] = [
		{ edge: "inline-start", distance: fx },
		{ edge: "inline-end", distance: 1 - fx },
		{ edge: "block-start", distance: fy },
		{ edge: "block-end", distance: 1 - fy },
	];
	const nearest = candidates.reduce((best, candidate) =>
		candidate.distance < best.distance ? candidate : best,
	);
	return nearest.distance < edgeFraction ? nearest.edge : "center";
}

export function dropCommands(
	contentId: ContentId,
	containerId: ContainerId,
	zone: DropZone,
): LayoutCommand[] {
	if (zone === "center")
		return [{ type: "move", contentId, destination: { containerId } }];
	return [{ type: "split", contentId, relativeTo: containerId, edge: zone }];
}

/** Dropping on a viewport edge creates a full-width Lane or a full-height column. */
export function edgeDropCommands(
	contentId: ContentId,
	edge: SplitEdge,
): LayoutCommand[] {
	return [{ type: "split", contentId, edge }];
}

function containerInTrack(
	arrangement: Arrangement,
	axis: "column" | "row",
	index: number,
): ContainerId | null {
	return (
		arrangement.grid.placements.find((placement) => placement[axis] === index)
			?.containerId ?? null
	);
}

function resized(size: TrackSize, value: number): TrackSize {
	return size.min === undefined
		? { unit: size.unit, value }
		: { unit: size.unit, value, min: size.min };
}

/**
 * Pointer resize between adjacent tracks: the pair keeps its combined
 * length, minima are honored, fraction tracks stay fractions (re-weighted
 * to their new share), fixed tracks get their new logical length.
 */
export function resizeCommands(
	arrangement: Arrangement,
	geometry: SolvedLayout,
	axis: "inline" | "block",
	index: number,
	deltaPx: number,
): LayoutCommand[] {
	const tracks =
		axis === "inline" ? arrangement.grid.columns : arrangement.grid.rows;
	const sizes = axis === "inline" ? geometry.columnSizes : geometry.rowSizes;
	const a = tracks[index];
	const b = tracks[index + 1];
	if (!a || !b || deltaPx === 0) return [];
	const minA = a.size.min ?? 0;
	const minB = b.size.min ?? 0;
	const total = sizes[index] + sizes[index + 1];
	const newA = Math.min(
		Math.max(sizes[index] + deltaPx, minA),
		Math.max(minA, total - minB),
	);
	const newB = total - newA;
	if (newA === sizes[index]) return [];
	const trackAxis = axis === "inline" ? "column" : "row";
	const containerA = containerInTrack(arrangement, trackAxis, index);
	const containerB = containerInTrack(arrangement, trackAxis, index + 1);
	if (!containerA || !containerB) return [];
	const commands: LayoutCommand[] = [];
	if (a.size.unit === "fraction" && b.size.unit === "fraction") {
		const perPx = (a.size.value + b.size.value) / total;
		commands.push({
			type: "resize",
			containerId: containerA,
			axis,
			size: resized(a.size, newA * perPx),
		});
		commands.push({
			type: "resize",
			containerId: containerB,
			axis,
			size: resized(b.size, newB * perPx),
		});
		return commands;
	}
	if (a.size.unit === "fixed")
		commands.push({
			type: "resize",
			containerId: containerA,
			axis,
			size: resized(a.size, newA),
		});
	if (b.size.unit === "fixed")
		commands.push({
			type: "resize",
			containerId: containerB,
			axis,
			size: resized(b.size, newB),
		});
	return commands;
}

/** Discrete scroll by whole columns; null when already at the bound. */
export function scrollByColumns(
	arrangement: Arrangement,
	delta: number,
): LayoutCommand | null {
	const columns = arrangement.grid.columns;
	if (columns.length === 0) return null;
	const current = Math.max(
		0,
		columns.findIndex(
			(column) => column.id === arrangement.scrollAnchorColumnId,
		),
	);
	const next = Math.max(0, Math.min(columns.length - 1, current + delta));
	if (next === current) return null;
	return {
		type: "scroll",
		anchorColumnId: next === 0 ? null : columns[next].id,
	};
}

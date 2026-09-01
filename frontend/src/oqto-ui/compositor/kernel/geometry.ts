/**
 * Deterministic 2D geometry solver (ADR-0042). Solved geometry is derived
 * from logical topology and host constraints; it is renderer input, never
 * authoritative state. Columns may overflow the screen (scroll policy) or
 * be scaled as the terminal fallback (fit policy); rows always fit. The
 * solver never emits overlapping, negative, or silently inaccessible
 * geometry: every shortfall is reported. Flush Lanes are pinned to the
 * screen edge and ignore horizontal scrolling.
 */

import type { ContainerId, GridTrackId } from "./ids";
import type {
	Arrangement,
	Container,
	GridPlacement,
	GridTrack,
	LayoutSnapshot,
} from "./model";

export interface SafeAreaInsets {
	readonly top: number;
	readonly right: number;
	readonly bottom: number;
	readonly left: number;
}

export interface ViewportConstraints {
	readonly inlineSize: number;
	readonly blockSize: number;
	readonly safeArea?: SafeAreaInsets;
	/** Column overflow policy; rows always fit. Defaults to "scroll". */
	readonly overflow?: "scroll" | "fit";
}

export interface SolvedRect {
	readonly containerId: ContainerId;
	readonly x: number;
	readonly y: number;
	readonly inlineSize: number;
	readonly blockSize: number;
	readonly collapsed: boolean;
	readonly flush: boolean;
}

export type GeometryDegradation =
	| {
			readonly kind: "minima-unsatisfiable";
			readonly axis: "inline" | "block";
			readonly deficit: number;
	  }
	| { readonly kind: "overflow"; readonly axis: "inline"; readonly extent: number };

export interface SolvedLayout {
	readonly rects: readonly SolvedRect[];
	readonly degradations: readonly GeometryDegradation[];
	readonly scrollOffset: number;
	readonly inlineExtent: number;
	/** Solved track lengths in placement order; renderer input only. */
	readonly columnSizes: readonly number[];
	readonly rowSizes: readonly number[];
}

interface TrackRequest {
	readonly track: GridTrack;
	readonly collapsed: boolean;
}

interface TrackSolution {
	readonly sizes: readonly number[];
	readonly deficit: number;
}

/**
 * Distributes `available` logical length across tracks: collapsed tracks
 * consume nothing, fixed (and flush) tracks take their value, fractions
 * share the rest proportionally with minima pinned. In "fit" mode an
 * unsatisfiable total scales every visible track proportionally; in
 * "scroll" mode minima are honored and the total may exceed `available`.
 */
function solveTracks(
	tracks: readonly TrackRequest[],
	available: number,
	mode: "fit" | "scroll",
): TrackSolution {
	const sizes = tracks.map(() => 0);
	const pinned = tracks.map(() => false);
	const isFixed = (track: GridTrack) =>
		track.size.unit === "fixed" || track.flush === true;
	for (let pass = 0; pass <= tracks.length; pass += 1) {
		let reserved = 0;
		let fractionTotal = 0;
		tracks.forEach(({ track, collapsed }, index) => {
			if (collapsed) return;
			if (isFixed(track)) reserved += track.size.value;
			else if (pinned[index]) reserved += track.size.min ?? 0;
			else fractionTotal += track.size.value;
		});
		const remaining = Math.max(0, available - reserved);
		let repin = false;
		tracks.forEach(({ track, collapsed }, index) => {
			if (collapsed) {
				sizes[index] = 0;
				return;
			}
			if (isFixed(track)) {
				sizes[index] = track.size.value;
				return;
			}
			if (pinned[index]) {
				sizes[index] = track.size.min ?? 0;
				return;
			}
			const share =
				fractionTotal > 0 ? (track.size.value / fractionTotal) * remaining : 0;
			const min = track.size.min ?? 0;
			if (share < min) {
				pinned[index] = true;
				repin = true;
			}
			sizes[index] = Math.max(share, min);
		});
		if (!repin) break;
	}
	const needed = sizes.reduce((total, size) => total + size, 0);
	if (needed <= available || needed === 0 || mode === "scroll") {
		return { sizes, deficit: Math.max(0, needed - available) };
	}
	const scale = available / needed;
	return {
		sizes: sizes.map((size) => Math.max(0, size * scale)),
		deficit: needed - available,
	};
}

function collapsedIds(containers: readonly Container[]): ReadonlySet<ContainerId> {
	return new Set(
		containers.filter((container) => container.collapsed).map((container) => container.id),
	);
}

function trackRequests(
	tracks: readonly GridTrack[],
	placements: readonly GridPlacement[],
	axis: "row" | "column",
	collapsed: ReadonlySet<ContainerId>,
): TrackRequest[] {
	const spanKey = axis === "row" ? "rowSpan" : "colSpan";
	return tracks.map((track, index) => {
		const covering = placements.filter(
			(placement) =>
				placement[axis] <= index && index < placement[axis] + placement[spanKey],
		);
		return {
			track,
			collapsed:
				covering.length > 0 &&
				covering.every((placement) => collapsed.has(placement.containerId)),
		};
	});
}

function prefixSums(sizes: readonly number[]): number[] {
	const starts = [0];
	for (const size of sizes) starts.push(starts[starts.length - 1] + size);
	return starts;
}

function safeInsets(viewport: ViewportConstraints): SafeAreaInsets {
	return viewport.safeArea ?? { top: 0, right: 0, bottom: 0, left: 0 };
}

function columnSolution(
	arrangement: Arrangement,
	containers: readonly Container[],
	available: number,
	mode: "fit" | "scroll",
): TrackSolution {
	return solveTracks(
		trackRequests(
			arrangement.grid.columns,
			arrangement.grid.placements,
			"column",
			collapsedIds(containers),
		),
		available,
		mode,
	);
}

function anchorOffset(
	columns: readonly GridTrack[],
	starts: readonly number[],
	anchor: GridTrackId | null,
	available: number,
): number {
	const index = anchor === null ? -1 : columns.findIndex((column) => column.id === anchor);
	const extent = starts[starts.length - 1];
	return Math.min(index < 0 ? 0 : starts[index], Math.max(0, extent - available));
}

/**
 * Chooses the scroll anchor that makes `placement` visible with minimal
 * movement. With a viewport the choice is exact; without one the kernel
 * only scrolls left to the placement's column when it lies before the
 * current anchor, because it cannot know the right edge.
 */
export function anchorToReveal(
	arrangement: Arrangement,
	containers: readonly Container[],
	placement: GridPlacement,
	viewport: ViewportConstraints | undefined,
): GridTrackId | null {
	const columns = arrangement.grid.columns;
	const current = arrangement.scrollAnchorColumnId;
	const currentIndex =
		current === null ? 0 : Math.max(0, columns.findIndex((column) => column.id === current));
	if (!viewport) {
		return placement.column < currentIndex ? columns[placement.column].id : current;
	}
	const safe = safeInsets(viewport);
	const available = Math.max(0, viewport.inlineSize - safe.left - safe.right);
	const { sizes } = columnSolution(arrangement, containers, available, "scroll");
	const starts = prefixSums(sizes);
	const offset = anchorOffset(columns, starts, current, available);
	const start = starts[placement.column];
	const end = starts[placement.column + placement.colSpan];
	if (start >= offset && end <= offset + available) return current;
	if (start < offset) return columns[placement.column].id;
	for (let index = 0; index <= placement.column; index += 1) {
		if (end - starts[index] <= available) return columns[index].id;
	}
	return columns[placement.column].id;
}

export function solveLayoutGeometry(
	state: LayoutSnapshot,
	viewport: ViewportConstraints,
): SolvedLayout {
	const arrangement =
		state.arrangements.find((candidate) => candidate.id === state.activeArrangementId) ??
		state.arrangements[0];
	const safe = safeInsets(viewport);
	const availableInline = Math.max(0, viewport.inlineSize - safe.left - safe.right);
	const availableBlock = Math.max(0, viewport.blockSize - safe.top - safe.bottom);
	const mode = viewport.overflow ?? "scroll";
	if (!arrangement) {
		return { rects: [], degradations: [], scrollOffset: 0, inlineExtent: 0, columnSizes: [], rowSizes: [] };
	}
	const collapsed = collapsedIds(state.containers);
	const columns = columnSolution(arrangement, state.containers, availableInline, mode);
	const rows = solveTracks(
		trackRequests(arrangement.grid.rows, arrangement.grid.placements, "row", collapsed),
		availableBlock,
		"fit",
	);
	const degradations: GeometryDegradation[] = [];
	if (columns.deficit > 0) {
		degradations.push(
			mode === "scroll"
				? { kind: "overflow", axis: "inline", extent: availableInline + columns.deficit }
				: { kind: "minima-unsatisfiable", axis: "inline", deficit: columns.deficit },
		);
	}
	if (rows.deficit > 0) {
		degradations.push({ kind: "minima-unsatisfiable", axis: "block", deficit: rows.deficit });
	}
	const colStarts = prefixSums(columns.sizes);
	const rowStarts = prefixSums(rows.sizes);
	const scrollOffset =
		mode === "scroll"
			? anchorOffset(arrangement.grid.columns, colStarts, arrangement.scrollAnchorColumnId, availableInline)
			: 0;
	const rects: SolvedRect[] = [...arrangement.grid.placements]
		.sort((a, b) => a.row - b.row || a.column - b.column)
		.map((placement) => {
			const isCollapsed = collapsed.has(placement.containerId);
			const flush = arrangement.grid.rows[placement.row]?.flush === true;
			const inlineSize = flush
				? availableInline
				: colStarts[placement.column + placement.colSpan] - colStarts[placement.column];
			const blockSize = rowStarts[placement.row + placement.rowSpan] - rowStarts[placement.row];
			return {
				containerId: placement.containerId,
				x: flush ? safe.left : safe.left + colStarts[placement.column] - scrollOffset,
				y: safe.top + rowStarts[placement.row],
				inlineSize: isCollapsed ? 0 : inlineSize,
				blockSize: isCollapsed ? 0 : blockSize,
				collapsed: isCollapsed,
				flush,
			};
		});
	return {
		rects,
		degradations,
		scrollOffset,
		inlineExtent: colStarts[colStarts.length - 1],
		columnSizes: columns.sizes,
		rowSizes: rows.sizes,
	};
}

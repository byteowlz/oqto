/**
 * resize/flush/scroll appliers: semantic track sizes, flush Lanes, and the
 * Arrangement's scroll anchor. None of these touch Content, ownership, or
 * focus; the scroll anchor is a settled semantic position, never pixels.
 */

import { activeArrangement, arrangementOf, replaceArrangement } from "./arrangement";
import type { CommandOutcome, ResizeAxis } from "./commands";
import { placementOf, replaceTrack } from "./grid";
import type { ContainerId, GridTrackId } from "./ids";
import type { LayoutSnapshot, TrackSize } from "./model";
import { findContainer } from "./state";

const DEFAULT_FLUSH_SIZE: TrackSize = { unit: "fixed", value: 240, min: 120 };

function validTrackSize(size: TrackSize): boolean {
	return (
		(size.unit === "fraction" || size.unit === "fixed") &&
		Number.isFinite(size.value) &&
		size.value > 0 &&
		(size.min === undefined || (Number.isFinite(size.min) && size.min >= 0))
	);
}

export function applyResize(
	state: LayoutSnapshot,
	containerId: ContainerId,
	axis: ResizeAxis,
	size: TrackSize,
): CommandOutcome {
	const arrangement = arrangementOf(state, containerId);
	const placement = arrangement ? placementOf(arrangement.grid, containerId) : null;
	if (!arrangement || !placement || !findContainer(state, containerId)) {
		return { ok: false, rejection: { reason: "unknown-container", containerId } };
	}
	if (!validTrackSize(size)) {
		return {
			ok: false,
			rejection: { reason: "invalid-command", detail: "resize requires a finite positive track size" },
		};
	}
	const index = axis === "inline" ? placement.column : placement.row;
	const track = axis === "inline" ? arrangement.grid.columns[index] : arrangement.grid.rows[index];
	if (track.flush && size.unit !== "fixed") {
		return {
			ok: false,
			rejection: { reason: "invalid-command", detail: "a flush lane keeps a fixed size" },
		};
	}
	const grid = replaceTrack(arrangement.grid, axis === "inline" ? "column" : "row", index, {
		...track,
		size,
	});
	return {
		ok: true,
		state: replaceArrangement(state, { ...arrangement, grid }),
		events: [{ type: "resized", containerId, axis }],
	};
}

export function applyFlush(
	state: LayoutSnapshot,
	rowId: GridTrackId,
	flush: boolean,
): CommandOutcome {
	const arrangement = activeArrangement(state);
	const index = arrangement.grid.rows.findIndex((row) => row.id === rowId);
	if (index < 0) {
		return { ok: false, rejection: { reason: "invalid-command", detail: `unknown row ${rowId}` } };
	}
	if (flush && index !== 0 && index !== arrangement.grid.rows.length - 1) {
		return {
			ok: false,
			rejection: { reason: "invalid-command", detail: "only the first or last lane may be flush" },
		};
	}
	const spanning = arrangement.grid.placements.some(
		(placement) => placement.row <= index && index < placement.row + placement.rowSpan && placement.rowSpan > 1,
	);
	if (flush && spanning) {
		return {
			ok: false,
			rejection: { reason: "invalid-command", detail: "a flush lane cannot be spanned by other placements" },
		};
	}
	const row = arrangement.grid.rows[index];
	const size = flush && row.size.unit !== "fixed" ? DEFAULT_FLUSH_SIZE : row.size;
	const { flush: _previous, ...rest } = row;
	const grid = replaceTrack(arrangement.grid, "row", index, flush ? { ...rest, size, flush: true } : { ...rest, size });
	return {
		ok: true,
		state: replaceArrangement(state, { ...arrangement, grid }),
		events: [{ type: "flushed", rowId, flush }],
	};
}

export function applyScroll(
	state: LayoutSnapshot,
	anchorColumnId: GridTrackId | null,
): CommandOutcome {
	const arrangement = activeArrangement(state);
	if (
		anchorColumnId !== null &&
		!arrangement.grid.columns.some((column) => column.id === anchorColumnId)
	) {
		return {
			ok: false,
			rejection: { reason: "invalid-command", detail: `unknown column ${anchorColumnId}` },
		};
	}
	if (arrangement.scrollAnchorColumnId === anchorColumnId) {
		return { ok: true, state, events: [] };
	}
	return {
		ok: true,
		state: replaceArrangement(state, { ...arrangement, scrollAnchorColumnId: anchorColumnId }),
		events: [{ type: "scrolled", arrangementId: arrangement.id, anchorColumnId }],
	};
}

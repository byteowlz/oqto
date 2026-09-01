/**
 * Executable ADR-0041/0042/0043 invariants. Every accepted transition must
 * preserve these; the engine re-checks after each transaction as defense in
 * depth, and persistence rejects documents that violate them.
 */

import type { Arrangement, GridTrack, LayoutSnapshot, TrackSize } from "./model";

export interface InvariantViolation {
	readonly code: string;
	readonly detail: string;
}

function validSize(size: TrackSize): boolean {
	return (
		(size.unit === "fraction" || size.unit === "fixed") &&
		Number.isFinite(size.value) &&
		size.value > 0 &&
		(size.min === undefined || (Number.isFinite(size.min) && size.min >= 0))
	);
}

type Push = (code: string, detail: string) => void;

function checkTracks(tracks: readonly GridTrack[], label: string, push: Push): void {
	const ids = new Set<string>();
	tracks.forEach((track, index) => {
		if (ids.has(track.id)) push("track-identity", `duplicate track id ${track.id}`);
		ids.add(track.id);
		if (!validSize(track.size)) push("track-size", `${label} ${track.id} has an invalid size`);
		if (track.flush) {
			if (index !== 0 && index !== tracks.length - 1) {
				push("flush-edge", `flush lane ${track.id} is not the first or last row`);
			}
			if (track.size.unit !== "fixed") {
				push("flush-size", `flush lane ${track.id} must have a fixed size`);
			}
		}
	});
}

function checkArrangement(
	arrangement: Arrangement,
	containerIds: ReadonlySet<string>,
	seenContainers: Set<string>,
	push: Push,
): void {
	const { rows, columns, placements } = arrangement.grid;
	checkTracks(rows, "row", push);
	checkTracks(columns, "column", push);
	columns.forEach((column) => {
		if (column.flush) push("flush-axis", `column ${column.id} cannot be flush`);
	});
	const occupied = new Set<string>();
	const coveredRows = new Set<number>();
	const coveredColumns = new Set<number>();
	for (const placement of placements) {
		if (!containerIds.has(placement.containerId)) {
			push("container-placement", `grid references unknown container ${placement.containerId}`);
		}
		if (seenContainers.has(placement.containerId)) {
			push("container-placement", `container ${placement.containerId} is placed more than once`);
		}
		seenContainers.add(placement.containerId);
		const { row, column, rowSpan, colSpan } = placement;
		if (
			![row, column, rowSpan, colSpan].every(Number.isInteger) ||
			rowSpan < 1 ||
			colSpan < 1 ||
			row < 0 ||
			column < 0 ||
			row + rowSpan > rows.length ||
			column + colSpan > columns.length
		) {
			push("span-bounds", `placement of ${placement.containerId} is out of bounds`);
			continue;
		}
		for (let r = row; r < row + rowSpan; r += 1) {
			if (rows[r].flush && rowSpan !== 1) {
				push("flush-confined", `placement of ${placement.containerId} spans into flush lane ${rows[r].id}`);
			}
			coveredRows.add(r);
			for (let c = column; c < column + colSpan; c += 1) {
				coveredColumns.add(c);
				const key = `${r}:${c}`;
				if (occupied.has(key)) {
					push("placement-overlap", `placements overlap at row ${r}, column ${c}`);
				}
				occupied.add(key);
			}
		}
	}
	rows.forEach((row, index) => {
		if (!coveredRows.has(index)) push("empty-track", `row ${row.id} has no placement`);
	});
	columns.forEach((column, index) => {
		if (!coveredColumns.has(index)) push("empty-track", `column ${column.id} has no placement`);
	});
	if (
		arrangement.scrollAnchorColumnId !== null &&
		!columns.some((column) => column.id === arrangement.scrollAnchorColumnId)
	) {
		push("scroll-anchor", `arrangement ${arrangement.id} anchors an unknown column`);
	}
}

export function checkLayoutInvariants(
	state: LayoutSnapshot,
): readonly InvariantViolation[] {
	const violations: InvariantViolation[] = [];
	const push: Push = (code, detail) => violations.push({ code, detail });

	if (!Number.isInteger(state.schemaVersion) || state.schemaVersion < 1) {
		push("schema-version", `schemaVersion ${state.schemaVersion} is invalid`);
	}
	if (!Number.isInteger(state.revision) || state.revision < 0) {
		push("revision", `revision ${state.revision} is invalid`);
	}
	if (!Number.isInteger(state.idSeed) || state.idSeed < 0) {
		push("id-seed", `idSeed ${state.idSeed} is invalid`);
	}
	if (state.strategy !== "grid") {
		push("strategy", `unknown layout strategy ${String(state.strategy)}`);
	}
	if (
		state.activeWorkspace.kind !== "all" &&
		(state.activeWorkspace.kind !== "workspace" || state.activeWorkspace.id.length === 0)
	) {
		push("active-workspace", "active workspace is malformed");
	}

	const containerIds = new Set<string>();
	const seenContent = new Set<string>();
	const contentContainer = new Map<string, string>();
	for (const container of state.containers) {
		if (containerIds.has(container.id)) push("container-identity", `duplicate container id ${container.id}`);
		containerIds.add(container.id);
		for (const content of container.stack) {
			if (seenContent.has(content.id)) {
				push("content-single-container", `content ${content.id} is placed more than once`);
			}
			seenContent.add(content.id);
			contentContainer.set(content.id, container.id);
		}
		if (container.stack.length === 0) {
			if (container.activeContentId !== null) {
				push("active-membership", `empty container ${container.id} has active content`);
			}
		} else if (
			container.activeContentId === null ||
			!container.stack.some((content) => content.id === container.activeContentId)
		) {
			push("active-membership", `container ${container.id} active content is not in its stack`);
		}
	}

	if (state.arrangements.length === 0) push("arrangements", "a layout needs at least one arrangement");
	const arrangementIds = new Set<string>();
	const seenContainers = new Set<string>();
	for (const arrangement of state.arrangements) {
		if (arrangementIds.has(arrangement.id)) push("arrangement-identity", `duplicate arrangement id ${arrangement.id}`);
		arrangementIds.add(arrangement.id);
		checkArrangement(arrangement, containerIds, seenContainers, push);
		if (
			arrangement.lastFocusedContentId !== null &&
			!arrangement.grid.placements.some(
				(placement) => contentContainer.get(arrangement.lastFocusedContentId as string) === placement.containerId,
			)
		) {
			push("focus-memory", `arrangement ${arrangement.id} remembers unplaced focus`);
		}
	}
	for (const id of containerIds) {
		if (!seenContainers.has(id)) push("container-placement", `container ${id} is not placed in any arrangement`);
	}

	const active = state.arrangements.find((arrangement) => arrangement.id === state.activeArrangementId);
	if (!active) {
		push("active-arrangement", `active arrangement ${state.activeArrangementId} does not exist`);
		return violations;
	}
	const activeContainers = new Set<string>(active.grid.placements.map((placement) => placement.containerId));
	const activeContent = [...contentContainer.entries()]
		.filter(([, containerId]) => activeContainers.has(containerId))
		.map(([contentId]) => contentId);
	if (activeContent.length === 0) {
		if (state.focusedContentId !== null) {
			push("focus-empty", "an empty active arrangement must not have a focused target");
		}
	} else if (state.focusedContentId === null || !activeContent.includes(state.focusedContentId)) {
		push("focus-reachable", `focus ${String(state.focusedContentId)} is not placed in the active arrangement`);
	}
	return violations;
}

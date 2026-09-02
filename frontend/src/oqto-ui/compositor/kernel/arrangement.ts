/**
 * Arrangement helpers (ADR-0043) with structural sharing: unchanged
 * Arrangements keep object identity.
 */

import type { ArrangementId, ContainerId } from "./ids";
import type { Arrangement, LayoutSnapshot } from "./model";

export function findArrangement(
	state: LayoutSnapshot,
	arrangementId: ArrangementId,
): Arrangement | null {
	return (
		state.arrangements.find(
			(arrangement) => arrangement.id === arrangementId,
		) ?? null
	);
}

export function activeArrangement(state: LayoutSnapshot): Arrangement {
	const active = findArrangement(state, state.activeArrangementId);
	if (!active) {
		throw new Error(
			`layout snapshot has no active arrangement ${state.activeArrangementId}`,
		);
	}
	return active;
}

/** The Arrangement whose grid places the Container, if any. */
export function arrangementOf(
	state: LayoutSnapshot,
	containerId: ContainerId,
): Arrangement | null {
	return (
		state.arrangements.find((arrangement) =>
			arrangement.grid.placements.some(
				(placement) => placement.containerId === containerId,
			),
		) ?? null
	);
}

export function replaceArrangement(
	state: LayoutSnapshot,
	next: Arrangement,
): LayoutSnapshot {
	return {
		...state,
		arrangements: state.arrangements.map((arrangement) =>
			arrangement.id === next.id ? next : arrangement,
		),
	};
}

/** Clears focus memory that no longer references Content placed in that Arrangement. */
export function repairFocusMemory(state: LayoutSnapshot): LayoutSnapshot {
	let changed = false;
	const arrangements = state.arrangements.map((arrangement) => {
		if (arrangement.lastFocusedContentId === null) return arrangement;
		const placed = state.containers.some(
			(container) =>
				arrangement.grid.placements.some(
					(placement) => placement.containerId === container.id,
				) &&
				container.stack.some(
					(content) => content.id === arrangement.lastFocusedContentId,
				),
		);
		if (placed) return arrangement;
		changed = true;
		return { ...arrangement, lastFocusedContentId: null };
	});
	return changed ? { ...state, arrangements } : state;
}

/** Drops scroll anchors whose column track no longer exists. */
export function repairScrollAnchors(state: LayoutSnapshot): LayoutSnapshot {
	let changed = false;
	const arrangements = state.arrangements.map((arrangement) => {
		const anchor = arrangement.scrollAnchorColumnId;
		if (
			anchor === null ||
			arrangement.grid.columns.some((column) => column.id === anchor)
		) {
			return arrangement;
		}
		changed = true;
		return { ...arrangement, scrollAnchorColumnId: null };
	});
	return changed ? { ...state, arrangements } : state;
}

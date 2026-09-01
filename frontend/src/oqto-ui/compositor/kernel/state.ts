/**
 * Shared snapshot helpers with structural sharing: unchanged Containers
 * keep object identity so renderers can skip unrelated work.
 */

import {
	activeArrangement,
	arrangementOf,
	replaceArrangement,
} from "./arrangement";
import { containerOrder, removePlacement } from "./grid";
import type { ContainerId, ContentId } from "./ids";
import type { Container, LayoutSnapshot } from "./model";

export function findContainer(
	state: LayoutSnapshot,
	containerId: ContainerId,
): Container | null {
	return (
		state.containers.find((container) => container.id === containerId) ?? null
	);
}

export function containerOf(
	state: LayoutSnapshot,
	contentId: ContentId,
): Container | null {
	return (
		state.containers.find((container) =>
			container.stack.some((content) => content.id === contentId),
		) ?? null
	);
}

export function replaceContainer(
	state: LayoutSnapshot,
	next: Container,
): LayoutSnapshot {
	return {
		...state,
		containers: state.containers.map((container) =>
			container.id === next.id ? next : container,
		),
	};
}

/** Content ids placed anywhere in the document (all Arrangements). */
export function placedContentIds(
	state: LayoutSnapshot,
): readonly ContentId[] {
	return state.containers.flatMap((container) =>
		container.stack.map((content) => content.id),
	);
}

/**
 * Removes one Content reference from a Container stack, moving the active
 * selection to the deterministic neighbor (same index, else last, else
 * none) when the removed item was active.
 */
export function removeFromStack(
	container: Container,
	contentId: ContentId,
): Container {
	const index = container.stack.findIndex((item) => item.id === contentId);
	if (index < 0) return container;
	const stack = container.stack.filter((item) => item.id !== contentId);
	const activeContentId =
		container.activeContentId === contentId
			? (stack[Math.min(index, stack.length - 1)]?.id ?? null)
			: container.activeContentId;
	return { ...container, stack, activeContentId };
}

/**
 * Deterministic focus policy (ADR-0041/0043): focus is absent when the
 * active Arrangement holds no Content and otherwise references Content
 * placed in the active Arrangement, preferring the current focus, then the
 * Arrangement's remembered focus, then the first active Content in reading
 * order. The active Arrangement's focus memory follows the result.
 */
export function repairFocus(state: LayoutSnapshot): LayoutSnapshot {
	const active = activeArrangement(state);
	const order = containerOrder(active.grid);
	const placed = new Set<ContentId>();
	for (const containerId of order) {
		for (const content of findContainer(state, containerId)?.stack ?? []) {
			placed.add(content.id);
		}
	}
	let focus: ContentId | null = null;
	if (state.focusedContentId !== null && placed.has(state.focusedContentId)) {
		focus = state.focusedContentId;
	} else if (
		active.lastFocusedContentId !== null &&
		placed.has(active.lastFocusedContentId)
	) {
		focus = active.lastFocusedContentId;
	} else {
		for (const containerId of order) {
			const container = findContainer(state, containerId);
			const target = container?.activeContentId ?? container?.stack[0]?.id;
			if (target !== undefined && target !== null) {
				focus = target;
				break;
			}
		}
	}
	const withFocus =
		state.focusedContentId === focus ? state : { ...state, focusedContentId: focus };
	return active.lastFocusedContentId === focus
		? withFocus
		: replaceArrangement(withFocus, { ...active, lastFocusedContentId: focus });
}

/**
 * Applies a Container's declared empty behavior once its last Content
 * leaves: retain keeps it, collapse preserves it without its track, remove
 * deletes it and repairs the grid.
 */
export function settleEmptiedContainer(
	state: LayoutSnapshot,
	containerId: ContainerId,
): { readonly state: LayoutSnapshot; readonly removed: boolean } {
	const container = findContainer(state, containerId);
	if (!container || container.stack.length > 0) {
		return { state, removed: false };
	}
	if (container.emptyBehavior === "remove") {
		const owner = arrangementOf(state, containerId);
		const withoutPlacement = owner
			? replaceArrangement(state, {
					...owner,
					grid: removePlacement(owner.grid, containerId),
				})
			: state;
		return {
			state: {
				...withoutPlacement,
				containers: withoutPlacement.containers.filter(
					(candidate) => candidate.id !== containerId,
				),
			},
			removed: true,
		};
	}
	if (container.emptyBehavior === "collapse" && !container.collapsed) {
		return {
			state: replaceContainer(state, { ...container, collapsed: true }),
			removed: false,
		};
	}
	return { state, removed: false };
}

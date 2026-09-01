/**
 * open/reveal/activate/focus appliers. `open` is idempotent for an exact
 * stable Content identity: already-placed Content is revealed, never
 * duplicated. `reveal` accepts only placed Content and returns a typed
 * not-placed rejection otherwise; it may switch Arrangements and scroll
 * (ADR-0043) but never implies keyboard focus. `focus` is the only
 * focus-stealing command.
 */

import { activeArrangement, arrangementOf, findArrangement, replaceArrangement } from "./arrangement";
import type { CommandOutcome, LayoutEvent, OpenTarget } from "./commands";
import { anchorToReveal, type ViewportConstraints } from "./geometry";
import { containerOrder, placementOf } from "./grid";
import type { ContentId } from "./ids";
import { mintEdgeContainer } from "./mint";
import type { Arrangement, Container, ContentRef, LayoutSnapshot } from "./model";
import { containerOf, findContainer, repairFocus, replaceContainer } from "./state";

export interface CommandContext {
	readonly viewport?: ViewportConstraints;
}

function clampPosition(position: number | undefined, length: number): number {
	if (position === undefined || !Number.isInteger(position)) return length;
	return Math.max(0, Math.min(position, length));
}

/** Switches to `arrangement` if it is not active; returns the event, if any. */
function switchTo(
	state: LayoutSnapshot,
	arrangement: Arrangement,
): { readonly state: LayoutSnapshot; readonly events: LayoutEvent[] } {
	if (state.activeArrangementId === arrangement.id) return { state, events: [] };
	return {
		state: { ...state, activeArrangementId: arrangement.id },
		events: [
			{ type: "arrangement-switched", from: state.activeArrangementId, to: arrangement.id },
		],
	};
}

/** Scrolls the active Arrangement so `container` is visible, if needed. */
function scrollInto(
	state: LayoutSnapshot,
	container: Container,
	context: CommandContext,
): { readonly state: LayoutSnapshot; readonly events: LayoutEvent[] } {
	const arrangement = activeArrangement(state);
	const placement = placementOf(arrangement.grid, container.id);
	if (!placement) return { state, events: [] };
	const anchor = anchorToReveal(
		arrangement,
		state.containers,
		placement,
		context.viewport,
	);
	if (anchor === arrangement.scrollAnchorColumnId) return { state, events: [] };
	return {
		state: replaceArrangement(state, { ...arrangement, scrollAnchorColumnId: anchor }),
		events: [{ type: "scrolled", arrangementId: arrangement.id, anchorColumnId: anchor }],
	};
}

function revealPlaced(
	state: LayoutSnapshot,
	container: Container,
	contentId: ContentId,
	context: CommandContext,
): CommandOutcome {
	const owner = arrangementOf(state, container.id);
	if (!owner) {
		return {
			ok: false,
			rejection: { reason: "invariant-violation", detail: `container ${container.id} is unplaced` },
		};
	}
	const switched = switchTo(state, owner);
	const visible = replaceContainer(switched.state, {
		...container,
		activeContentId: contentId,
		collapsed: false,
	});
	const scrolled = scrollInto(visible, container, context);
	return {
		ok: true,
		state: repairFocus(scrolled.state),
		events: [
			...switched.events,
			...scrolled.events,
			{ type: "revealed", contentId, containerId: container.id },
		],
	};
}

export function resolveDestination(
	state: LayoutSnapshot,
	arrangement: Arrangement,
	target: OpenTarget | undefined,
): Container | null {
	const inArrangement = (predicate: (container: Container) => boolean) =>
		containerOrder(arrangement.grid)
			.map((id) => findContainer(state, id))
			.find((container) => container !== null && predicate(container)) ?? null;
	if (target?.role !== undefined) {
		const match = inArrangement((container) => container.role === target.role);
		if (match) return match;
	}
	return (
		inArrangement((container) => container.role === "primary") ??
		inArrangement(() => true)
	);
}

export function applyOpen(
	state: LayoutSnapshot,
	content: ContentRef,
	target: OpenTarget | undefined,
	context: CommandContext,
): CommandOutcome {
	const existing = containerOf(state, content.id);
	if (existing) {
		const placed = existing.stack.find((item) => item.id === content.id);
		if (placed && placed.kind !== content.kind) {
			return {
				ok: false,
				rejection: {
					reason: "invalid-command",
					detail: `content identity ${content.id} is already placed with kind ${placed.kind}`,
				},
			};
		}
		return revealPlaced(state, existing, content.id, context);
	}

	let arrangement: Arrangement | null;
	let destination: Container | null = null;
	if (target?.containerId !== undefined) {
		destination = findContainer(state, target.containerId);
		if (!destination) {
			return { ok: false, rejection: { reason: "unknown-container", containerId: target.containerId } };
		}
		arrangement = arrangementOf(state, destination.id);
	} else if (target?.arrangementId !== undefined) {
		arrangement = findArrangement(state, target.arrangementId);
		if (!arrangement) {
			return { ok: false, rejection: { reason: "unknown-arrangement", arrangementId: target.arrangementId } };
		}
	} else {
		arrangement = activeArrangement(state);
	}
	if (!arrangement) {
		return { ok: false, rejection: { reason: "invariant-violation", detail: "container without arrangement" } };
	}

	let working = state;
	destination ??= resolveDestination(working, arrangement, target);
	if (!destination) {
		const minted = mintEdgeContainer(arrangement, working.idSeed, "inline-end", [], target?.role);
		destination = minted.container;
		working = replaceArrangement(
			{ ...working, idSeed: minted.idSeed, containers: [...working.containers, minted.container] },
			{ ...arrangement, grid: minted.grid },
		);
		arrangement = findArrangement(working, arrangement.id) ?? arrangement;
	}

	const stack = [...destination.stack];
	stack.splice(clampPosition(target?.position, stack.length), 0, content);
	const placedContainer = { ...destination, stack, activeContentId: content.id, collapsed: false };
	working = replaceContainer(working, placedContainer);
	const switched = switchTo(working, arrangement);
	const scrolled = scrollInto(switched.state, placedContainer, context);
	return {
		ok: true,
		state: repairFocus(scrolled.state),
		events: [
			...switched.events,
			...scrolled.events,
			{ type: "opened", contentId: content.id, containerId: destination.id },
		],
	};
}

export function applyReveal(
	state: LayoutSnapshot,
	contentId: ContentId,
	context: CommandContext,
): CommandOutcome {
	const container = containerOf(state, contentId);
	if (!container) {
		return { ok: false, rejection: { reason: "not-placed", contentId } };
	}
	return revealPlaced(state, container, contentId, context);
}

export function applyActivate(
	state: LayoutSnapshot,
	contentId: ContentId,
): CommandOutcome {
	const container = containerOf(state, contentId);
	if (!container) {
		return { ok: false, rejection: { reason: "unknown-content", contentId } };
	}
	return {
		ok: true,
		state: repairFocus(replaceContainer(state, { ...container, activeContentId: contentId })),
		events: [{ type: "activated", contentId, containerId: container.id }],
	};
}

export function applyFocus(
	state: LayoutSnapshot,
	contentId: ContentId,
): CommandOutcome {
	const container = containerOf(state, contentId);
	const owner = container ? arrangementOf(state, container.id) : null;
	if (!container || !owner) {
		return { ok: false, rejection: { reason: "unknown-content", contentId } };
	}
	const switched = switchTo(state, owner);
	return {
		ok: true,
		state: repairFocus({ ...switched.state, focusedContentId: contentId }),
		events: [...switched.events, { type: "focused", contentId }],
	};
}

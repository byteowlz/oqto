/**
 * move/split/collapse appliers. Dropping Content onto a Container tabs it;
 * splitting beside a Container or at an Arrangement edge atomically
 * creates a Container and grid track (a full-height column or a Lane) and
 * moves the Content into it. Collapse changes visibility, never focus,
 * ownership, or content state.
 */

import { resolveDestination } from "./apply-open";
import {
	activeArrangement,
	arrangementOf,
	findArrangement,
	replaceArrangement,
} from "./arrangement";
import type {
	CommandOutcome,
	LayoutEvent,
	MoveDestination,
	SplitEdge,
} from "./commands";
import { placementOf } from "./grid";
import type { ContainerId, ContentId } from "./ids";
import { mintEdgeContainer, mintSplitContainer } from "./mint";
import type { Container, LayoutSnapshot } from "./model";
import {
	containerOf,
	findContainer,
	removeFromStack,
	repairFocus,
	replaceContainer,
	settleEmptiedContainer,
} from "./state";

function clampPosition(position: number | undefined, length: number): number {
	if (position === undefined || !Number.isInteger(position)) return length;
	return Math.max(0, Math.min(position, length));
}

function finish(
	state: LayoutSnapshot,
	sourceId: ContainerId,
	events: LayoutEvent[],
): CommandOutcome {
	const settled = settleEmptiedContainer(state, sourceId);
	if (settled.removed)
		events.push({ type: "container-removed", containerId: sourceId });
	return { ok: true, state: repairFocus(settled.state), events };
}

export function applyMove(
	state: LayoutSnapshot,
	contentId: ContentId,
	destination: MoveDestination,
): CommandOutcome {
	const source = containerOf(state, contentId);
	const ref = source?.stack.find((item) => item.id === contentId);
	if (!source || !ref) {
		return { ok: false, rejection: { reason: "unknown-content", contentId } };
	}

	let working = state;
	let target: Container | null = null;
	if (destination.containerId !== undefined) {
		target = findContainer(state, destination.containerId);
		if (!target) {
			return {
				ok: false,
				rejection: {
					reason: "unknown-container",
					containerId: destination.containerId,
				},
			};
		}
	} else {
		const arrangement =
			destination.arrangementId !== undefined
				? findArrangement(state, destination.arrangementId)
				: activeArrangement(state);
		if (!arrangement) {
			return {
				ok: false,
				rejection: {
					reason: "unknown-arrangement",
					arrangementId: destination.arrangementId as never,
				},
			};
		}
		target = resolveDestination(state, arrangement, undefined);
		if (!target || target.id === source.id) {
			const minted = mintEdgeContainer(
				arrangement,
				state.idSeed,
				"inline-end",
				[],
				undefined,
			);
			target = minted.container;
			working = replaceArrangement(
				{
					...state,
					idSeed: minted.idSeed,
					containers: [...state.containers, minted.container],
				},
				{ ...arrangement, grid: minted.grid },
			);
		}
	}

	if (source.id === target.id) {
		const without = source.stack.filter((item) => item.id !== contentId);
		const stack = [...without];
		stack.splice(clampPosition(destination.position, without.length), 0, ref);
		return {
			ok: true,
			state: replaceContainer(working, { ...source, stack }),
			events: [{ type: "moved", contentId, from: source.id, to: target.id }],
		};
	}

	working = replaceContainer(working, removeFromStack(source, contentId));
	const stack = [...target.stack];
	stack.splice(clampPosition(destination.position, stack.length), 0, ref);
	working = replaceContainer(working, {
		...target,
		stack,
		activeContentId: contentId,
		collapsed: false,
	});
	return finish(working, source.id, [
		{ type: "moved", contentId, from: source.id, to: target.id },
	]);
}

export function applySplit(
	state: LayoutSnapshot,
	contentId: ContentId,
	edge: SplitEdge,
	relativeTo: ContainerId | undefined,
): CommandOutcome {
	const source = containerOf(state, contentId);
	const ref = source?.stack.find((item) => item.id === contentId);
	if (!source || !ref) {
		return { ok: false, rejection: { reason: "unknown-content", contentId } };
	}
	const arrangement =
		relativeTo === undefined
			? activeArrangement(state)
			: arrangementOf(state, relativeTo);
	if (
		!arrangement ||
		(relativeTo !== undefined && !findContainer(state, relativeTo))
	) {
		return {
			ok: false,
			rejection: {
				reason: "unknown-container",
				containerId: relativeTo as ContainerId,
			},
		};
	}
	if (
		relativeTo !== undefined &&
		(edge === "block-start" || edge === "block-end")
	) {
		const placement = placementOf(arrangement.grid, relativeTo);
		if (placement && arrangement.grid.rows[placement.row]?.flush) {
			return {
				ok: false,
				rejection: {
					reason: "invalid-command",
					detail: "a flush lane cannot be split along the block axis",
				},
			};
		}
	}
	const minted =
		relativeTo === undefined
			? mintEdgeContainer(arrangement, state.idSeed, edge, [ref])
			: mintSplitContainer(arrangement, state.idSeed, edge, relativeTo, [ref]);
	if (!minted) {
		return {
			ok: false,
			rejection: {
				reason: "unknown-container",
				containerId: relativeTo as ContainerId,
			},
		};
	}
	let working = replaceContainer(state, removeFromStack(source, contentId));
	working = replaceArrangement(
		{
			...working,
			idSeed: minted.idSeed,
			containers: [...working.containers, minted.container],
		},
		{ ...arrangement, grid: minted.grid },
	);
	return finish(working, source.id, [
		{ type: "split", containerId: minted.container.id, edge },
		{ type: "moved", contentId, from: source.id, to: minted.container.id },
	]);
}

export function applyCollapse(
	state: LayoutSnapshot,
	containerId: ContainerId,
	collapsed: boolean,
): CommandOutcome {
	const container = findContainer(state, containerId);
	if (!container) {
		return {
			ok: false,
			rejection: { reason: "unknown-container", containerId },
		};
	}
	return {
		ok: true,
		state: replaceContainer(state, { ...container, collapsed }),
		events: [{ type: "collapsed", containerId, collapsed }],
	};
}

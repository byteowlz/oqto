/**
 * Arrangement and active-Workspace appliers (ADR-0043). Removing an
 * Arrangement closes the presentations it held — placement only; domain
 * owners are untouched. The last Arrangement can never be removed. The
 * active Workspace is opaque persisted data; the kernel never resolves
 * domain owners.
 */

import { findArrangement } from "./arrangement";
import type { CommandOutcome, LayoutEvent } from "./commands";
import { type ArrangementId, arrangementIdFrom } from "./ids";
import type { ActiveWorkspace, Arrangement, ArrangementBinding, LayoutSnapshot } from "./model";
import { repairFocus } from "./state";

export function applyArrangementCreate(
	state: LayoutSnapshot,
	label: string | undefined,
	binding: ArrangementBinding | undefined,
	activate: boolean | undefined,
): CommandOutcome {
	const created: Arrangement = {
		id: arrangementIdFrom(state.idSeed),
		...(label !== undefined ? { label } : {}),
		...(binding !== undefined ? { binding } : {}),
		grid: { rows: [], columns: [], placements: [] },
		scrollAnchorColumnId: null,
		lastFocusedContentId: null,
	};
	let working: LayoutSnapshot = {
		...state,
		idSeed: state.idSeed + 1,
		arrangements: [...state.arrangements, created],
	};
	const events: LayoutEvent[] = [{ type: "arrangement-created", arrangementId: created.id }];
	if (activate) {
		events.push({ type: "arrangement-switched", from: state.activeArrangementId, to: created.id });
		working = repairFocus({ ...working, activeArrangementId: created.id });
	}
	return { ok: true, state: working, events };
}

export function applyArrangementSwitch(
	state: LayoutSnapshot,
	arrangementId: ArrangementId,
): CommandOutcome {
	if (!findArrangement(state, arrangementId)) {
		return { ok: false, rejection: { reason: "unknown-arrangement", arrangementId } };
	}
	if (state.activeArrangementId === arrangementId) {
		return { ok: true, state, events: [] };
	}
	return {
		ok: true,
		state: repairFocus({ ...state, activeArrangementId: arrangementId }),
		events: [{ type: "arrangement-switched", from: state.activeArrangementId, to: arrangementId }],
	};
}

export function applyArrangementRemove(
	state: LayoutSnapshot,
	arrangementId: ArrangementId,
): CommandOutcome {
	const index = state.arrangements.findIndex((arrangement) => arrangement.id === arrangementId);
	if (index < 0) {
		return { ok: false, rejection: { reason: "unknown-arrangement", arrangementId } };
	}
	if (state.arrangements.length === 1) {
		return { ok: false, rejection: { reason: "last-arrangement", arrangementId } };
	}
	const removed = state.arrangements[index];
	const removedContainers = new Set(removed.grid.placements.map((placement) => placement.containerId));
	const arrangements = state.arrangements.filter((arrangement) => arrangement.id !== arrangementId);
	const events: LayoutEvent[] = [];
	for (const containerId of removedContainers) {
		const container = state.containers.find((candidate) => candidate.id === containerId);
		for (const content of container?.stack ?? []) {
			events.push({ type: "closed", contentId: content.id, containerId });
		}
		events.push({ type: "container-removed", containerId });
	}
	let working: LayoutSnapshot = {
		...state,
		arrangements,
		containers: state.containers.filter((container) => !removedContainers.has(container.id)),
	};
	if (state.activeArrangementId === arrangementId) {
		const next = arrangements[Math.max(0, index - 1)];
		events.push({ type: "arrangement-switched", from: arrangementId, to: next.id });
		working = { ...working, activeArrangementId: next.id };
	}
	events.push({ type: "arrangement-removed", arrangementId });
	return { ok: true, state: repairFocus(working), events };
}

export function applyWorkspaceSwitch(
	state: LayoutSnapshot,
	workspace: ActiveWorkspace,
): CommandOutcome {
	if (workspace.kind === "workspace" && workspace.id.length === 0) {
		return { ok: false, rejection: { reason: "invalid-command", detail: "workspace id must not be empty" } };
	}
	return {
		ok: true,
		state: { ...state, activeWorkspace: workspace },
		events: [{ type: "workspace-switched", workspace }],
	};
}

/**
 * close/undo appliers. Closing removes the placed reference only — it never
 * deletes the Content's domain owner. Undo adopts a host-captured prior
 * snapshot's topology; it is validated like any other transition and moves
 * the revision forward, never backward.
 */

import type { CommandOutcome, LayoutEvent } from "./commands";
import type { ContentId } from "./ids";
import { checkLayoutInvariants } from "./invariants";
import type { LayoutSnapshot } from "./model";
import { containerOf, removeFromStack, repairFocus, replaceContainer, settleEmptiedContainer } from "./state";

export function applyClose(state: LayoutSnapshot, contentId: ContentId): CommandOutcome {
	const container = containerOf(state, contentId);
	if (!container) {
		return { ok: false, rejection: { reason: "unknown-content", contentId } };
	}
	const working = replaceContainer(state, removeFromStack(container, contentId));
	const settled = settleEmptiedContainer(working, container.id);
	const events: LayoutEvent[] = [{ type: "closed", contentId, containerId: container.id }];
	if (settled.removed) {
		events.push({ type: "container-removed", containerId: container.id });
	}
	return { ok: true, state: repairFocus(settled.state), events };
}

export function applyUndo(state: LayoutSnapshot, snapshot: LayoutSnapshot): CommandOutcome {
	if (snapshot.schemaVersion !== state.schemaVersion) {
		return {
			ok: false,
			rejection: { reason: "invalid-command", detail: "undo target snapshot has a different schema version" },
		};
	}
	const violations = checkLayoutInvariants(snapshot);
	if (violations.length > 0) {
		return {
			ok: false,
			rejection: {
				reason: "invalid-command",
				detail: `undo target snapshot violates invariants: ${violations.map((violation) => violation.code).join(", ")}`,
			},
		};
	}
	return {
		ok: true,
		state: { ...snapshot, revision: state.revision, idSeed: Math.max(state.idSeed, snapshot.idSeed) },
		events: [{ type: "undone", restoredRevision: snapshot.revision }],
	};
}

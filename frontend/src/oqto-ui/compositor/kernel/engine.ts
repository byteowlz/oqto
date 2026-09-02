/**
 * The deterministic Layout Engine (ADR-0041): the sole mutation seam. It
 * accepts a versioned snapshot plus an atomic semantic transaction and
 * returns a new snapshot with events or a typed rejection. Multi-command
 * transactions apply completely or not at all; stale expected revisions
 * conflict instead of overwriting newer work. Previewing a transaction is
 * simply not committing the returned snapshot.
 */

import { applyCollapse, applyMove, applySplit } from "./apply-arrange";
import {
	applyArrangementCreate,
	applyArrangementRemove,
	applyArrangementSwitch,
	applyWorkspaceSwitch,
} from "./apply-arrangement";
import { applyClose, applyUndo } from "./apply-close";
import {
	type CommandContext,
	applyActivate,
	applyFocus,
	applyOpen,
	applyReveal,
} from "./apply-open";
import { applyFlush, applyResize, applyScroll } from "./apply-track";
import { repairFocusMemory, repairScrollAnchors } from "./arrangement";
import type {
	ApplyResult,
	CommandOutcome,
	LayoutCommand,
	LayoutEvent,
	LayoutTransaction,
} from "./commands";
import { checkLayoutInvariants } from "./invariants";
import type { LayoutSnapshot } from "./model";
import { repairFocus } from "./state";

function applyCommand(
	state: LayoutSnapshot,
	command: LayoutCommand,
	context: CommandContext,
): CommandOutcome {
	switch (command.type) {
		case "open":
			return applyOpen(state, command.content, command.target, context);
		case "reveal":
			return applyReveal(state, command.contentId, context);
		case "activate":
			return applyActivate(state, command.contentId);
		case "focus":
			return applyFocus(state, command.contentId);
		case "move":
			return applyMove(state, command.contentId, command.destination);
		case "split":
			return applySplit(
				state,
				command.contentId,
				command.edge,
				command.relativeTo,
			);
		case "resize":
			return applyResize(
				state,
				command.containerId,
				command.axis,
				command.size,
			);
		case "collapse":
			return applyCollapse(state, command.containerId, command.collapsed);
		case "flush":
			return applyFlush(state, command.rowId, command.flush);
		case "scroll":
			return applyScroll(state, command.anchorColumnId);
		case "close":
			return applyClose(state, command.contentId);
		case "arrangement-create":
			return applyArrangementCreate(
				state,
				command.label,
				command.binding,
				command.activate,
			);
		case "arrangement-switch":
			return applyArrangementSwitch(state, command.arrangementId);
		case "arrangement-remove":
			return applyArrangementRemove(state, command.arrangementId);
		case "workspace-switch":
			return applyWorkspaceSwitch(state, command.workspace);
		case "undo":
			return applyUndo(state, command.snapshot);
	}
}

export function applyTransaction(
	state: LayoutSnapshot,
	transaction: LayoutTransaction,
): ApplyResult {
	if (transaction.expectedRevision !== state.revision) {
		return {
			ok: false,
			rejection: {
				reason: "revision-conflict",
				expected: transaction.expectedRevision,
				actual: state.revision,
			},
		};
	}
	const context: CommandContext = { viewport: transaction.viewport };
	let working = state;
	const events: LayoutEvent[] = [];
	for (const command of transaction.commands) {
		const outcome = applyCommand(working, command, context);
		if (!outcome.ok) return { ok: false, rejection: outcome.rejection };
		working = repairFocus(
			repairFocusMemory(repairScrollAnchors(outcome.state)),
		);
		events.push(...outcome.events);
	}
	const snapshot: LayoutSnapshot = { ...working, revision: state.revision + 1 };
	const violations = checkLayoutInvariants(snapshot);
	if (violations.length > 0) {
		return {
			ok: false,
			rejection: {
				reason: "invariant-violation",
				detail: violations
					.map((violation) => `${violation.code}: ${violation.detail}`)
					.join("; "),
			},
		};
	}
	return { ok: true, snapshot, events };
}

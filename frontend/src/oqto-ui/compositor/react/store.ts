/**
 * Host-side compositor store: owns the current snapshot, funnels every
 * mutation through the kernel's transaction seam, and keeps a bounded undo
 * journal so mouse and keyboard changes alike can be undone. React
 * subscribes via useSyncExternalStore; nothing mutates Container arrays
 * directly.
 */

import {
	type ApplyResult,
	type LayoutCommand,
	type LayoutSnapshot,
	type LayoutTransaction,
	type ViewportConstraints,
	applyTransaction,
} from "../index";

export interface CompositorStore {
	getSnapshot(): LayoutSnapshot;
	subscribe(listener: () => void): () => void;
	/** Applies a full transaction; stale expected revisions conflict. */
	dispatch(transaction: LayoutTransaction): ApplyResult;
	/** Convenience: applies commands against the current revision. */
	commit(
		commands: readonly LayoutCommand[],
		viewport?: ViewportConstraints,
	): ApplyResult;
	/** Restores the snapshot before the last accepted transaction, if any. */
	undo(): ApplyResult | null;
}

const UNDO_JOURNAL_LIMIT = 50;

export function createCompositorStore(
	initial: LayoutSnapshot,
	onCommitted?: (snapshot: LayoutSnapshot) => void,
): CompositorStore {
	let snapshot = initial;
	const journal: LayoutSnapshot[] = [];
	const listeners = new Set<() => void>();
	const dispatch = (transaction: LayoutTransaction): ApplyResult => {
		const result = applyTransaction(snapshot, transaction);
		if (result.ok) {
			if (!transaction.commands.some((command) => command.type === "undo")) {
				journal.push(snapshot);
				if (journal.length > UNDO_JOURNAL_LIMIT) journal.shift();
			}
			snapshot = result.snapshot;
			onCommitted?.(snapshot);
			for (const listener of listeners) listener();
		}
		return result;
	};
	return {
		getSnapshot: () => snapshot,
		subscribe: (listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		dispatch,
		commit: (commands, viewport) =>
			dispatch({
				expectedRevision: snapshot.revision,
				commands,
				...(viewport ? { viewport } : {}),
			}),
		undo: () => {
			const previous = journal.pop();
			if (!previous) return null;
			return dispatch({
				expectedRevision: snapshot.revision,
				commands: [{ type: "undo", snapshot: previous }],
			});
		},
	};
}

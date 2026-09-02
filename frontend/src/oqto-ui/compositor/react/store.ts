/**
 * Host-side compositor store: owns the current snapshot and funnels every
 * mutation through the kernel's transaction seam. React subscribes via
 * useSyncExternalStore; nothing mutates Container arrays directly.
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
}

export function createCompositorStore(
	initial: LayoutSnapshot,
): CompositorStore {
	let snapshot = initial;
	const listeners = new Set<() => void>();
	const dispatch = (transaction: LayoutTransaction): ApplyResult => {
		const result = applyTransaction(snapshot, transaction);
		if (result.ok) {
			snapshot = result.snapshot;
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
	};
}

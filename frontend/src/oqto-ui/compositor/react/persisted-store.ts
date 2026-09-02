/**
 * A compositor store backed by a device-local layout document store:
 * recovers primary -> last-known-good -> preset on creation, promotes a
 * healthy primary to last-known-good, and writes the encoded document after
 * every accepted transaction. Storage stays disposable and never a second
 * authority (ADR-0037).
 */

import type { LayoutDocumentStore } from "../../platform/layout-storage";
import {
	type LayoutSnapshot,
	type RecoveredLayout,
	encodeLayoutDocument,
	recoverLayoutDocument,
} from "../index";
import { type CompositorStore, createCompositorStore } from "./store";

export interface PersistedStoreOptions {
	readonly storage: LayoutDocumentStore;
	readonly key: string;
	readonly fallback: LayoutSnapshot;
}

export interface PersistedCompositorStore extends CompositorStore {
	readonly recovery: RecoveredLayout;
}

export function createPersistedCompositorStore(
	options: PersistedStoreOptions,
): PersistedCompositorStore {
	const lastKnownGoodKey = `${options.key}:last-known-good`;
	const recovery = recoverLayoutDocument(
		[options.storage.read(options.key), options.storage.read(lastKnownGoodKey)],
		options.fallback,
	);
	if (recovery.sourceIndex === 0) {
		options.storage.write(
			lastKnownGoodKey,
			encodeLayoutDocument(recovery.snapshot),
		);
	}
	const inner = createCompositorStore(recovery.snapshot);
	const dispatch: CompositorStore["dispatch"] = (transaction) => {
		const result = inner.dispatch(transaction);
		if (result.ok)
			options.storage.write(options.key, encodeLayoutDocument(result.snapshot));
		return result;
	};
	return {
		getSnapshot: inner.getSnapshot,
		subscribe: inner.subscribe,
		dispatch,
		commit: (commands, viewport) =>
			dispatch({
				expectedRevision: inner.getSnapshot().revision,
				commands,
				...(viewport ? { viewport } : {}),
			}),
		recovery,
	};
}

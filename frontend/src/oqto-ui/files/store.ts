/**
 * Host-side Files store: holds the engine state, fetches listings lazily
 * (one directory level per request, at most one in flight per directory),
 * and reconciles host change events. React subscribes through
 * useSyncExternalStore; the engine itself stays pure.
 */

import type { FileHost } from "../platform/files-contract";
import { applyChange, navigate } from "./navigation";
import { type FilesState, applyListing, initialState } from "./navigator";

export interface FilesStore {
	/** The host the pane acts on, for operations and previews. */
	readonly context: {
		readonly fileHost: FileHost;
		readonly workspacePath: string;
	};
	getSnapshot(): FilesState;
	subscribe(listener: () => void): () => void;
	/** Applies a pure transition and refetches the directory when needed. */
	update(next: (state: FilesState) => FilesState): void;
	/**
	 * Fetches one directory: `force` refetches after an operation changed
	 * it, otherwise a cached listing is kept.
	 */
	load(path: string, force?: boolean): void;
	/** Enters a directory, loading it if the cache has no listing yet. */
	open(path: string): void;
	/** Releases the host watch; the store is unusable afterwards. */
	dispose(): void;
}

export function createFilesStore(
	fileHost: FileHost,
	workspacePath: string,
): FilesStore {
	let state = initialState();
	const listeners = new Set<() => void>();
	const inFlight = new Set<string>();

	const emit = (next: FilesState) => {
		if (next === state) return;
		state = next;
		for (const listener of listeners) listener();
	};

	function fetchListing(path: string): void {
		if (inFlight.has(path)) return;
		inFlight.add(path);
		emit(applyListing(state, path, { status: "loading" }));
		fileHost.list(workspacePath, path, state.showHidden).then(
			(entries) => {
				inFlight.delete(path);
				emit(applyListing(state, path, { status: "ready", entries }));
			},
			(error: Error) => {
				inFlight.delete(path);
				emit(
					applyListing(state, path, {
						status: "error",
						message: error.message,
					}),
				);
			},
		);
	}

	const unwatch = fileHost.watch(workspacePath, (change) => {
		const result = applyChange(state, change.path);
		emit(result.state);
		if (result.refetch !== null) fetchListing(result.refetch);
	});

	fetchListing("");

	return {
		context: { fileHost, workspacePath },
		getSnapshot: () => state,
		subscribe: (listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		update: (next) => {
			const before = state;
			emit(next(state));
			// Hidden entries are filtered by the host, so the flip needs a
			// refetch of everything already listed.
			if (state.showHidden !== before.showHidden) {
				for (const path of Object.keys(state.listings)) fetchListing(path);
			}
		},
		load: (path, force = false) => {
			if (force || !state.listings[path]) fetchListing(path);
		},
		open: (path) => {
			emit(navigate(state, path));
			const listing = state.listings[path];
			if (!listing || listing.status === "error") fetchListing(path);
		},
		dispose: () => {
			unwatch();
			listeners.clear();
		},
	};
}

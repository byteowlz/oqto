/**
 * Host-side Files store: holds the engine state, fetches listings lazily
 * (one directory level per request, at most one in flight per directory),
 * and reconciles host change events. React subscribes through
 * useSyncExternalStore; the engine itself stays pure.
 */

import type { FileSystem } from "../platform/files-contract";
import { applyChange, navigate } from "./navigation";
import { type FilesState, applyListing, initialState } from "./navigator";

export interface FilesStore {
	getSnapshot(): FilesState;
	subscribe(listener: () => void): () => void;
	/** Applies a pure transition and refetches the directory when needed. */
	update(next: (state: FilesState) => FilesState): void;
	/** Enters a directory, loading it if the cache has no listing yet. */
	open(path: string): void;
	/** Releases the host watch; the store is unusable afterwards. */
	dispose(): void;
}

export function createFilesStore(
	fileSystem: FileSystem,
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

	function load(path: string): void {
		if (inFlight.has(path)) return;
		inFlight.add(path);
		emit(applyListing(state, path, { status: "loading" }));
		fileSystem.list(workspacePath, path).then(
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

	const unwatch = fileSystem.watch(workspacePath, (change) => {
		const result = applyChange(state, change.path);
		emit(result.state);
		if (result.refetch !== null) load(result.refetch);
	});

	load("");

	return {
		getSnapshot: () => state,
		subscribe: (listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		update: (next) => emit(next(state)),
		open: (path) => {
			emit(navigate(state, path));
			const listing = state.listings[path];
			if (!listing || listing.status === "error") load(path);
		},
		dispose: () => {
			unwatch();
			listeners.clear();
		},
	};
}

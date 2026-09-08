/**
 * Binds a Files store to React: one store per work directory, recreated
 * when the work directory changes and disposed on unmount so the host
 * watch never outlives the pane.
 */

import { useMountEffect } from "@/hooks/use-mount-effect";
import { useRef, useSyncExternalStore } from "react";
import type { FileHost } from "../platform/files-contract";
import type { FilesState } from "./navigator";
import { type FilesStore, createFilesStore } from "./store";

export function useFilesStore(
	fileHost: FileHost,
	workspacePath: string,
): FilesStore {
	const held = useRef<{ key: string; store: FilesStore } | null>(null);
	if (!held.current || held.current.key !== workspacePath) {
		held.current?.store.dispose();
		held.current = {
			key: workspacePath,
			store: createFilesStore(fileHost, workspacePath),
		};
	}
	useMountEffect(() => () => held.current?.store.dispose());
	return held.current.store;
}

export function useFilesState(store: FilesStore): FilesState {
	return useSyncExternalStore(
		store.subscribe,
		store.getSnapshot,
		store.getSnapshot,
	);
}

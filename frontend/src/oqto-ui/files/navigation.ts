/**
 * Directory movement and host-change reconciliation: the transitions that
 * change *where* the pane is, as opposed to how the current listing reads.
 */

import { type FileEntry, parentPath } from "./entries";
import { type FilesState, placeCursor, visibleEntries } from "./navigator";

/** Navigates to a directory, keeping its cached listing when present. */
export function navigate(state: FilesState, path: string): FilesState {
	if (path === state.cwd) return state;
	return placeCursor({
		...state,
		cwd: path,
		filter: "",
		selection: [],
		cursor: null,
	});
}

/** Moves to the parent directory, placing the cursor on the directory left. */
export function goUp(state: FilesState): FilesState {
	const parent = parentPath(state.cwd);
	if (parent === null) return state;
	return placeCursor(navigate(state, parent), state.cwd);
}

/** The entry under the cursor, or null when the listing is empty. */
export function cursorEntry(state: FilesState): FileEntry | null {
	return (
		visibleEntries(state).find((entry) => entry.path === state.cursor) ?? null
	);
}

/**
 * Reconciles a host change event: marks the path as changed and invalidates
 * the affected directory's cached listing. Returns the directory to
 * refetch, or null when nothing cached covers it.
 */
export function applyChange(
	state: FilesState,
	path: string,
): { readonly state: FilesState; readonly refetch: string | null } {
	const directory = parentPath(path) ?? "";
	const changed = state.changed.includes(path)
		? state.changed
		: [...state.changed, path];
	const cached = state.listings[directory] !== undefined;
	const listings = cached
		? { ...state.listings, [directory]: { status: "loading" as const } }
		: state.listings;
	return {
		state: { ...state, changed, listings },
		refetch: cached ? directory : null,
	};
}

export function cursorToEdge(
	state: FilesState,
	edge: "first" | "last",
): FilesState {
	const visible = visibleEntries(state);
	if (visible.length === 0) return placeCursor(state);
	return placeCursor(
		state,
		visible[edge === "first" ? 0 : visible.length - 1].path,
	);
}

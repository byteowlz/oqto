/**
 * The Files engine: a pure state machine over directory listings. It owns
 * the working directory, the per-directory listing cache, cursor,
 * selection, and filter. No React, no transport, no DOM — every transition
 * returns a new value.
 */

import { type FileEntry, matchesFilter, sortEntries } from "./entries";

export type ListingState =
	| { readonly status: "loading" }
	| { readonly status: "ready"; readonly entries: readonly FileEntry[] }
	| { readonly status: "error"; readonly message: string };

export interface FilesState {
	/** Current directory, relative to the work directory root ("" is root). */
	readonly cwd: string;
	/** Listing cache keyed by directory path; survives navigation. */
	readonly listings: { readonly [path: string]: ListingState };
	/** Path of the cursor row, or null when nothing is visible. */
	readonly cursor: string | null;
	readonly selection: readonly string[];
	readonly filter: string;
	/** Paths the host reported changed since load, for live markers. */
	readonly changed: readonly string[];
}

export type SelectMode = "replace" | "toggle" | "range";

export function initialState(cwd = ""): FilesState {
	return {
		cwd,
		listings: {},
		cursor: null,
		selection: [],
		filter: "",
		changed: [],
	};
}

/** Entries of the current directory after filtering, in display order. */
export function visibleEntries(state: FilesState): readonly FileEntry[] {
	const listing = state.listings[state.cwd];
	if (!listing || listing.status !== "ready") return [];
	return listing.entries.filter((entry) =>
		matchesFilter(entry.name, state.filter),
	);
}

/**
 * Puts the cursor on `preferred` when it is visible, keeps a still-visible
 * cursor, and otherwise falls back to the first row (or none).
 */
export function placeCursor(state: FilesState, preferred?: string): FilesState {
	const visible = visibleEntries(state);
	const has = (path: string | null | undefined) =>
		path != null && visible.some((entry) => entry.path === path);
	const cursor = has(preferred)
		? (preferred as string)
		: has(state.cursor)
			? state.cursor
			: (visible[0]?.path ?? null);
	return state.cursor === cursor ? state : { ...state, cursor };
}

export function applyListing(
	state: FilesState,
	path: string,
	listing: ListingState,
): FilesState {
	const normalized =
		listing.status === "ready"
			? { status: "ready" as const, entries: sortEntries(listing.entries) }
			: listing;
	return placeCursor({
		...state,
		listings: { ...state.listings, [path]: normalized },
	});
}

export function moveCursor(state: FilesState, delta: number): FilesState {
	const visible = visibleEntries(state);
	if (visible.length === 0) return placeCursor(state);
	const index = visible.findIndex((entry) => entry.path === state.cursor);
	const next = Math.max(
		0,
		Math.min(visible.length - 1, (index < 0 ? 0 : index) + delta),
	);
	return placeCursor(state, visible[next].path);
}

export function setFilter(state: FilesState, filter: string): FilesState {
	return placeCursor({ ...state, filter });
}

export function select(
	state: FilesState,
	path: string,
	mode: SelectMode,
): FilesState {
	if (mode === "toggle") {
		const selection = state.selection.includes(path)
			? state.selection.filter((candidate) => candidate !== path)
			: [...state.selection, path];
		return { ...state, cursor: path, selection };
	}
	if (mode === "range" && state.cursor !== null) {
		const visible = visibleEntries(state);
		const from = visible.findIndex((entry) => entry.path === state.cursor);
		const to = visible.findIndex((entry) => entry.path === path);
		if (from >= 0 && to >= 0) {
			const [start, end] = from <= to ? [from, to] : [to, from];
			return {
				...state,
				cursor: path,
				selection: visible.slice(start, end + 1).map((entry) => entry.path),
			};
		}
	}
	return { ...state, cursor: path, selection: [path] };
}

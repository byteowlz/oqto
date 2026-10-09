/**
 * Tree rows: the same listings, flattened into an indented outline where
 * expanded directories show their children inline. Pure — expansion is
 * state, and the store loads whatever a newly expanded directory needs.
 */

import type { FileEntry } from "../platform/files-contract";
import { matchesFilter } from "./entries";
import type { FilesState } from "./navigator";

export interface TreeRow {
	readonly entry: FileEntry;
	readonly depth: number;
	/** Directories only: whether their children are currently shown. */
	readonly expanded: boolean;
}

function readyEntries(state: FilesState, path: string): readonly FileEntry[] {
	const listing = state.listings[path];
	return listing?.status === "ready" ? listing.entries : [];
}

/**
 * Flattens the current directory into rows, descending into expansions.
 * While filtering, a directory survives when its own name matches or when
 * something inside it does, so a deep match is never orphaned.
 */
export function treeRows(state: FilesState): readonly TreeRow[] {
	const expanded = new Set(state.expanded);
	const walk = (path: string, depth: number): TreeRow[] => {
		const rows: TreeRow[] = [];
		for (const entry of readyEntries(state, path)) {
			const open = entry.directory && expanded.has(entry.path);
			const children = open ? walk(entry.path, depth + 1) : [];
			const matches = matchesFilter(entry.name, state.filter);
			if (!matches && children.length === 0) continue;
			rows.push({ entry, depth, expanded: open });
			rows.push(...children);
		}
		return rows;
	};
	return walk(state.cwd, 0);
}

/** Expands or collapses a directory; returns the path to load, if any. */
export function toggleExpanded(
	state: FilesState,
	path: string,
): { readonly state: FilesState; readonly load: string | null } {
	if (state.expanded.includes(path)) {
		return {
			state: {
				...state,
				expanded: state.expanded.filter(
					(open) => open !== path && !open.startsWith(`${path}/`),
				),
			},
			load: null,
		};
	}
	const loaded = state.listings[path]?.status === "ready";
	return {
		state: { ...state, expanded: [...state.expanded, path] },
		load: loaded ? null : path,
	};
}

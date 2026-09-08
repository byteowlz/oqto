/**
 * Yank and paste, the way a keyboard file manager does it: mark what to
 * copy or move, then paste into the directory you navigate to.
 */

import type { FilesState } from "./navigator";

/** Yanks the current selection (or the cursor) for a later paste. */
export function yank(state: FilesState, mode: "copy" | "move"): FilesState {
	const paths =
		state.selection.length > 0
			? state.selection
			: state.cursor === null
				? []
				: [state.cursor];
	return paths.length === 0 ? state : { ...state, clipboard: { paths, mode } };
}

export function clearClipboard(state: FilesState): FilesState {
	return state.clipboard === null ? state : { ...state, clipboard: null };
}

/** The paths an operation should act on: the selection, else the cursor. */
export function actionTargets(state: FilesState): readonly string[] {
	if (state.selection.length > 0) return state.selection;
	return state.cursor === null ? [] : [state.cursor];
}

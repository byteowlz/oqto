/**
 * The pane's action line: one input that filters, renames, or names a new
 * folder, plus the last operation's outcome and its undo. Modelled as
 * explicit modes so the pane never guesses what Enter means.
 */

import { useCallback, useState } from "react";
import { cursorEntry } from "./navigation";
import { setFilter } from "./navigator";
import {
	type OperationResult,
	createFolder,
	removeEntry,
	renameEntry,
} from "./operations";
import type { FilesStore } from "./store";

export type ActionMode = "filter" | "rename" | "create" | "confirmDelete";

export interface ActionState {
	readonly mode: ActionMode | null;
	readonly draft: string;
	/** Message key under oqtoUi.files plus its name value, or null. */
	readonly outcome: { readonly key: string; readonly name: string } | null;
	readonly canUndo: boolean;
}

export interface FileActions extends ActionState {
	begin(mode: ActionMode): void;
	change(draft: string): void;
	submit(): void;
	cancel(): void;
	undo(): void;
}

const IDLE: ActionState = {
	mode: null,
	draft: "",
	outcome: null,
	canUndo: false,
};

export function useFileActions(store: FilesStore): FileActions {
	const [state, setState] = useState<ActionState>(IDLE);
	const [undoStep, setUndoStep] = useState<(() => Promise<void>) | null>(null);

	const settle = useCallback(
		(promise: Promise<OperationResult>, directory: string) => {
			promise.then(
				(result) => {
					setState({
						...IDLE,
						outcome: result.message,
						canUndo: result.undo !== null,
					});
					setUndoStep(() => result.undo);
					store.load(directory, true);
				},
				(error: Error) => {
					setState({
						...IDLE,
						outcome: { key: "opFailed", name: error.message },
					});
					setUndoStep(null);
				},
			);
		},
		[store],
	);

	const begin = useCallback(
		(mode: ActionMode) => {
			const entry = cursorEntry(store.getSnapshot());
			const draft =
				mode === "rename"
					? (entry?.name ?? "")
					: mode === "filter"
						? store.getSnapshot().filter
						: "";
			if ((mode === "rename" || mode === "confirmDelete") && !entry) return;
			setState({ ...IDLE, mode, draft });
		},
		[store],
	);

	const cancel = useCallback(() => {
		setState(IDLE);
		store.update((current) => setFilter(current, ""));
	}, [store]);

	const submit = useCallback(() => {
		const snapshot = store.getSnapshot();
		const entry = cursorEntry(snapshot);
		const context = store.context;
		if (state.mode === "filter") {
			setState((current) => ({ ...current, mode: null }));
			return;
		}
		if (state.mode === "rename" && entry && state.draft.trim() !== "") {
			settle(
				renameEntry(context, entry.path, state.draft.trim()),
				snapshot.cwd,
			);
			return;
		}
		if (state.mode === "create" && state.draft.trim() !== "") {
			settle(
				createFolder(context, snapshot.cwd, state.draft.trim()),
				snapshot.cwd,
			);
			return;
		}
		if (state.mode === "confirmDelete" && entry) {
			settle(
				removeEntry(context, entry.path, entry.name, entry.directory),
				snapshot.cwd,
			);
			return;
		}
		setState(IDLE);
	}, [state.mode, state.draft, store, settle]);

	const change = useCallback(
		(draft: string) => {
			setState((current) => ({ ...current, draft }));
			if (state.mode === "filter")
				store.update((current) => setFilter(current, draft));
		},
		[state.mode, store],
	);

	const undo = useCallback(() => {
		if (!undoStep) return;
		const directory = store.getSnapshot().cwd;
		setUndoStep(null);
		undoStep().then(
			() => {
				setState({ ...IDLE, outcome: { key: "undone", name: "" } });
				store.load(directory, true);
			},
			(error: Error) =>
				setState({
					...IDLE,
					outcome: { key: "opFailed", name: error.message },
				}),
		);
	}, [undoStep, store]);

	return { ...state, begin, change, submit, cancel, undo };
}

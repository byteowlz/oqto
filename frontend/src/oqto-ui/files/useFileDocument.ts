/**
 * One file's contents as placeable Content: loaded once on mount, edited in
 * a draft that never fights the loaded text, saved back through the host.
 * Loading is an interaction (mount, reload, save), never a render effect.
 */

import { useMountEffect } from "@/hooks/use-mount-effect";
import { useCallback, useRef, useState } from "react";
import type { FileHost } from "../platform/files-contract";
import { viewerFor } from "./viewers/registry";

export type DocumentStatus = "loading" | "ready" | "saving" | "failed";

export interface FileDocument {
	readonly status: DocumentStatus;
	/** The loaded text, or the draft once editing has begun. */
	readonly text: string;
	readonly editing: boolean;
	readonly dirty: boolean;
	edit(text: string): void;
	/** Enters or leaves editing; leaving discards an unsaved draft. */
	setEditing(editing: boolean): void;
	save(): void;
}

interface DocumentState {
	readonly status: DocumentStatus;
	readonly loaded: string;
	readonly draft: string | null;
	readonly editing: boolean;
}

const START: DocumentState = {
	status: "loading",
	loaded: "",
	draft: null,
	editing: false,
};

export function useFileDocument(
	fileHost: FileHost,
	workspacePath: string,
	path: string,
	name: string,
): FileDocument {
	const [state, setState] = useState<DocumentState>(START);
	const live = useRef(true);

	const load = useCallback(() => {
		if (viewerFor(name, false) !== "text") {
			setState({ ...START, status: "ready" });
			return;
		}
		fileHost.read(workspacePath, path).then(
			(text) => {
				if (live.current)
					setState({
						status: "ready",
						loaded: text,
						draft: null,
						editing: false,
					});
			},
			() => {
				if (live.current) setState({ ...START, status: "failed" });
			},
		);
	}, [fileHost, workspacePath, path, name]);

	useMountEffect(() => {
		// Strict mode mounts, cleans up, and mounts again; the guard has to be
		// re-armed here or the second load resolves into a dead component.
		live.current = true;
		load();
		return () => {
			live.current = false;
		};
	});

	const save = useCallback(() => {
		setState((current) => {
			if (current.draft === null) return current;
			const draft = current.draft;
			fileHost.write(workspacePath, path, draft).then(
				() => {
					if (live.current)
						setState({
							status: "ready",
							loaded: draft,
							draft: null,
							editing: true,
						});
				},
				() => {
					if (live.current)
						setState((failed) => ({ ...failed, status: "failed" }));
				},
			);
			return { ...current, status: "saving" };
		});
	}, [fileHost, workspacePath, path]);

	return {
		status: state.status,
		text: state.draft ?? state.loaded,
		editing: state.editing,
		dirty: state.draft !== null && state.draft !== state.loaded,
		edit: (text: string) =>
			setState((current) => ({ ...current, draft: text })),
		setEditing: (editing: boolean) =>
			setState((current) => ({ ...current, editing, draft: null })),
		save,
	};
}

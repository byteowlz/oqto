/**
 * Quick Look: the cursor entry's contents. Fetching is driven by the
 * interaction that changes what is previewed, never by rendering, so the
 * pane has no render-phase side effects and no effect hooks.
 */

import { useCallback, useRef, useState } from "react";
import type { FileHost } from "../platform/files-contract";
import { viewerFor } from "./viewers/registry";

export type PreviewState =
	| { readonly status: "idle" }
	| { readonly status: "loading" }
	| { readonly status: "text"; readonly text: string }
	| { readonly status: "unavailable" };

export interface PreviewTarget {
	readonly path: string;
	readonly name: string;
	readonly directory: boolean;
}

export interface Preview {
	readonly state: PreviewState;
	/** Shows an entry, or closes the preview when given null. */
	show(target: PreviewTarget | null): void;
}

const MAX_PREVIEW_CHARACTERS = 20_000;

export function usePreview(fileHost: FileHost, workspacePath: string): Preview {
	const [state, setState] = useState<PreviewState>({ status: "idle" });
	const showing = useRef<string | null>(null);

	const show = useCallback(
		(target: PreviewTarget | null) => {
			if (!target) {
				showing.current = null;
				setState({ status: "idle" });
				return;
			}
			const key = `${workspacePath}:${target.path}`;
			if (showing.current === key) return;
			showing.current = key;
			// Image and media viewers stream from a URL; only text is fetched.
			if (target.directory || viewerFor(target.name, false) !== "text") {
				setState({ status: "unavailable" });
				return;
			}
			setState({ status: "loading" });
			fileHost.read(workspacePath, target.path).then(
				(text) => {
					if (showing.current !== key) return;
					setState({
						status: "text",
						text: text.slice(0, MAX_PREVIEW_CHARACTERS),
					});
				},
				() => {
					if (showing.current === key) setState({ status: "unavailable" });
				},
			);
		},
		[fileHost, workspacePath],
	);

	return { state, show };
}

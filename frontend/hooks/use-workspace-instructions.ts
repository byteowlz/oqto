import { useEffect, useState } from "react";

import { readFileMux, watchFilesMux } from "@/lib/mux-files";
import { getWsManager } from "@/lib/ws-manager";
import type { FilesWsEvent } from "@/lib/ws-mux-types";

const INSTRUCTIONS_FILE = "INSTRUCTIONS.md";
const MAX_BYTES = 256 * 1024;

const decoder = new TextDecoder("utf-8", { fatal: false });

async function readInstructions(workspacePath: string): Promise<string | null> {
	try {
		const { data } = await readFileMux(workspacePath, INSTRUCTIONS_FILE);
		if (data.byteLength === 0) return null;
		const trimmed =
			data.byteLength > MAX_BYTES ? data.slice(0, MAX_BYTES) : data;
		const text = decoder.decode(trimmed).trim();
		return text.length > 0 ? text : null;
	} catch {
		return null;
	}
}

/**
 * Reads INSTRUCTIONS.md from the workspace root. Returns its trimmed contents
 * when present, or null when missing/empty/unreadable.
 *
 * Starts a backend file watcher for the workspace and refreshes on every
 * change to INSTRUCTIONS.md. The watcher is shared per workspace (keyed in
 * WsConnectionState), so other components (e.g. FileTreeView) can use the same
 * watcher. We never call `unwatchFilesMux` on cleanup -- the watcher dies with
 * the WS connection -- and we re-arm it if another component unwatches it.
 */
export function useWorkspaceInstructions(
	workspacePath: string | null | undefined,
): string | null {
	const [content, setContent] = useState<string | null>(null);

	useEffect(() => {
		if (!workspacePath) {
			setContent(null);
			return;
		}

		let cancelled = false;

		const load = () => {
			void readInstructions(workspacePath).then((next) => {
				if (!cancelled) setContent(next);
			});
		};

		const ensureWatcher = () => {
			void watchFilesMux(workspacePath).catch(() => {
				/* ignore: best-effort */
			});
		};

		ensureWatcher();
		load();

		const ws = getWsManager();
		const unsubscribe = ws.subscribe("files", (event: FilesWsEvent) => {
			if (event.type === "file_changed") {
				if (
					event.workspace_path === workspacePath &&
					event.path === INSTRUCTIONS_FILE
				) {
					load();
				}
				return;
			}
			if (event.type === "unwatch_files_result") {
				if (event.workspace_path === workspacePath) {
					ensureWatcher();
				}
			}
		});

		return () => {
			cancelled = true;
			unsubscribe();
		};
	}, [workspacePath]);

	return content;
}

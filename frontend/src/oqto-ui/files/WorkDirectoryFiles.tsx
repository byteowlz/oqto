/**
 * Files for one work directory: owns the store lifetime and renders the
 * pane. Shells pass the platform file system and the work directory; the
 * pane itself stays transport-free.
 */

import type { ReactNode } from "react";
import { useState } from "react";
import {
	type ActionHost,
	createEmptyActionHost,
} from "../platform/actions-contract";
import type { FileHost } from "../platform/files-contract";
import { FilesPane } from "./FilesPane";
import { useFilesStore } from "./useFilesStore";

interface WorkDirectoryFilesProps {
	readonly fileHost: FileHost;
	/** Host path of the work directory; the listing root. */
	readonly workspacePath: string;
	/** Opening a file; without one, files only preview inside the pane. */
	readonly onOpenFile?: (path: string) => void;
	/** The Container's own controls, placed in this pane's toolbar. */
	readonly chrome?: ReactNode;
	/**
	 * Projects eligible Actions into the resource menu (ADR-0045). Without
	 * one the menu still opens, offering only the pane's own commands.
	 */
	readonly actionHost?: ActionHost;
}

export function WorkDirectoryFiles({
	fileHost,
	workspacePath,
	onOpenFile,
	chrome,
	actionHost,
}: WorkDirectoryFilesProps) {
	const store = useFilesStore(fileHost, workspacePath);
	const [offered] = useState(() => actionHost ?? createEmptyActionHost());
	return (
		<FilesPane
			store={store}
			chrome={chrome}
			actionHost={offered}
			onOpenFile={onOpenFile ? (entry) => onOpenFile(entry.path) : undefined}
		/>
	);
}

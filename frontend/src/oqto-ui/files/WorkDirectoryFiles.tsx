/**
 * Files for one work directory: owns the store lifetime and renders the
 * pane. Shells pass the platform file system and the work directory; the
 * pane itself stays transport-free.
 */

import type { FileHost } from "../platform/files-contract";
import { FilesPane } from "./FilesPane";
import { useFilesStore } from "./useFilesStore";

interface WorkDirectoryFilesProps {
	readonly fileHost: FileHost;
	/** Host path of the work directory; the listing root. */
	readonly workspacePath: string;
	/** Opening a file; without one, files only preview inside the pane. */
	readonly onOpenFile?: (path: string) => void;
}

export function WorkDirectoryFiles({
	fileHost,
	workspacePath,
	onOpenFile,
}: WorkDirectoryFilesProps) {
	const store = useFilesStore(fileHost, workspacePath);
	return (
		<FilesPane
			store={store}
			onOpenFile={onOpenFile ? (entry) => onOpenFile(entry.path) : undefined}
		/>
	);
}

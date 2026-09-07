/**
 * Files for one work directory: owns the store lifetime and renders the
 * pane. Shells pass the platform file system and the work directory; the
 * pane itself stays transport-free.
 */

import type { FileSystem } from "../platform/files-contract";
import { FilesPane } from "./FilesPane";
import { useFilesStore } from "./useFilesStore";

interface WorkDirectoryFilesProps {
	readonly fileSystem: FileSystem;
	/** Host path of the work directory; the listing root. */
	readonly workspacePath: string;
}

export function WorkDirectoryFiles({
	fileSystem,
	workspacePath,
}: WorkDirectoryFilesProps) {
	const store = useFilesStore(fileSystem, workspacePath);
	return <FilesPane store={store} />;
}

/**
 * The file-system port OqtoUI's Files pane talks to. Listing is single
 * level and lazy by contract: a pane never asks for a whole tree, so a
 * directory with a hundred thousand entries costs one request.
 */

export interface FileEntry {
	/** Path relative to the work directory root; "" is the root itself. */
	readonly path: string;
	readonly name: string;
	readonly directory: boolean;
	readonly symlink: boolean;
	readonly size: number;
	/** Unix milliseconds; 0 when the host did not report one. */
	readonly modifiedAt: number;
}

export interface FileChange {
	/** Path relative to the work directory root. */
	readonly path: string;
	readonly kind: "created" | "modified" | "deleted";
	readonly directory: boolean;
}

export interface FileSystem {
	/** One directory level; `path` is "" for the work directory root. */
	list(workspacePath: string, path: string): Promise<readonly FileEntry[]>;
	/** Streams host change events; returns an unsubscribe function. */
	watch(
		workspacePath: string,
		onChange: (change: FileChange) => void,
	): () => void;
}

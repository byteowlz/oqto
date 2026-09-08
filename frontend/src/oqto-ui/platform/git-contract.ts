/**
 * The version-control port OqtoUI's Git pane talks to. Semantic operations
 * only: the pane asks for a status, a diff, a log, or a change to the index
 * — never for a command line. The host runs git where the work directory
 * lives, and the pane never learns where that is.
 */

/** Git's two status codes, kept distinct: index side and working-tree side. */
export interface GitEntry {
	/** Path relative to the repository root. */
	readonly path: string;
	readonly index: string;
	readonly worktree: string;
	readonly renamedFrom: string | null;
}

export interface GitStatus {
	readonly branch: string;
	readonly upstream: string | null;
	readonly ahead: number;
	readonly behind: number;
	readonly entries: readonly GitEntry[];
	/** True when the host capped the listing. */
	readonly truncated: boolean;
}

export interface GitDiff {
	readonly path: string;
	readonly staged: boolean;
	readonly patch: string;
	readonly truncated: boolean;
}

export interface GitCommit {
	readonly id: string;
	readonly shortId: string;
	readonly summary: string;
	readonly author: string;
	/** Unix milliseconds. */
	readonly timestamp: number;
}

export interface GitHost {
	status(workspacePath: string): Promise<GitStatus>;
	diff(workspacePath: string, path: string, staged: boolean): Promise<GitDiff>;
	log(workspacePath: string, limit?: number): Promise<readonly GitCommit[]>;
	/** Moves paths into or out of the index. */
	stage(
		workspacePath: string,
		paths: readonly string[],
		staged: boolean,
	): Promise<void>;
	/** Commits what is staged; never stages on the caller's behalf. */
	commit(workspacePath: string, message: string): Promise<string>;
}

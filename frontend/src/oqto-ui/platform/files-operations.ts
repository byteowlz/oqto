/**
 * Mutating file operations. Split from the reading contract so each stays
 * narrow, and so a read-only host can exist without stubbing writes.
 */

export interface FileOperations {
	/** Replaces a file's contents, creating it when it does not exist. */
	write(workspacePath: string, path: string, text: string): Promise<void>;
	rename(workspacePath: string, from: string, to: string): Promise<void>;
	createDirectory(workspacePath: string, path: string): Promise<void>;
	remove(
		workspacePath: string,
		path: string,
		recursive: boolean,
	): Promise<void>;
	copy(workspacePath: string, from: string, to: string): Promise<void>;
	move(workspacePath: string, from: string, to: string): Promise<void>;
}

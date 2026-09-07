/**
 * File operations with a single-step undo. Every operation reports what it
 * did and how to reverse it, so the status line can offer "undo" the way a
 * fast file manager does. Destructive removal is deliberately not
 * reversible and says so.
 */

import type { FileSystem } from "../platform/files-contract";
import { childPath, parentPath } from "./entries";

export interface OperationResult {
	/** Message key and values for the status line. */
	readonly message: { readonly key: string; readonly name: string };
	/** Reverses the operation, or null when it cannot be reversed. */
	readonly undo: (() => Promise<void>) | null;
}

export interface OperationContext {
	readonly fileSystem: FileSystem;
	readonly workspacePath: string;
}

export async function renameEntry(
	context: OperationContext,
	path: string,
	name: string,
): Promise<OperationResult> {
	const target = childPath(parentPath(path) ?? "", name);
	await context.fileSystem.rename(context.workspacePath, path, target);
	return {
		message: { key: "renamed", name },
		undo: () => context.fileSystem.rename(context.workspacePath, target, path),
	};
}

export async function createFolder(
	context: OperationContext,
	directory: string,
	name: string,
): Promise<OperationResult> {
	const target = childPath(directory, name);
	await context.fileSystem.createDirectory(context.workspacePath, target);
	return {
		message: { key: "created", name },
		undo: () => context.fileSystem.remove(context.workspacePath, target, true),
	};
}

export async function removeEntry(
	context: OperationContext,
	path: string,
	name: string,
	recursive: boolean,
): Promise<OperationResult> {
	await context.fileSystem.remove(context.workspacePath, path, recursive);
	// Deletion is not reversible through the host contract; say so plainly
	// rather than offering an undo that would silently do nothing.
	return { message: { key: "deleted", name }, undo: null };
}

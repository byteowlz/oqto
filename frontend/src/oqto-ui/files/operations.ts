/**
 * File operations with a single-step undo. Every operation reports what it
 * did and how to reverse it, so the status line can offer "undo" the way a
 * fast file manager does. Destructive removal is deliberately not
 * reversible and says so.
 */

import type { FileHost } from "../platform/files-contract";
import { childPath, parentPath } from "./entries";

/** What the status line says: a message key plus its interpolations. */
export interface OperationMessage {
	readonly key: string;
	readonly name?: string;
	readonly count?: number;
}

export interface OperationResult {
	readonly message: OperationMessage;
	/** Reverses the operation, or null when it cannot be reversed. */
	readonly undo: (() => Promise<void>) | null;
}

export interface OperationContext {
	readonly fileHost: FileHost;
	readonly workspacePath: string;
}

export async function renameEntry(
	context: OperationContext,
	path: string,
	name: string,
): Promise<OperationResult> {
	const target = childPath(parentPath(path) ?? "", name);
	await context.fileHost.rename(context.workspacePath, path, target);
	return {
		message: { key: "renamed", name },
		undo: () => context.fileHost.rename(context.workspacePath, target, path),
	};
}

export async function createFolder(
	context: OperationContext,
	directory: string,
	name: string,
): Promise<OperationResult> {
	const target = childPath(directory, name);
	await context.fileHost.createDirectory(context.workspacePath, target);
	return {
		message: { key: "created", name },
		undo: () => context.fileHost.remove(context.workspacePath, target, true),
	};
}

/**
 * Pastes yanked paths into a directory. A copy is reversed by removing
 * what it created; a move is reversed by moving each entry back.
 */
export async function pasteEntries(
	context: OperationContext,
	paths: readonly string[],
	directory: string,
	mode: "copy" | "move",
): Promise<OperationResult> {
	const pairs = paths.map((path) => ({
		from: path,
		to: childPath(directory, path.split("/").pop() ?? path),
	}));
	const host = context.fileHost;
	for (const pair of pairs) {
		if (mode === "copy")
			await host.copy(context.workspacePath, pair.from, pair.to);
		else await host.move(context.workspacePath, pair.from, pair.to);
	}
	return {
		message: { key: "pasted", name: directory === "" ? "/" : directory },
		undo: async () => {
			for (const pair of pairs) {
				if (mode === "copy")
					await host.remove(context.workspacePath, pair.to, true);
				else await host.move(context.workspacePath, pair.to, pair.from);
			}
		},
	};
}

/** Removes several entries; like a single removal, this cannot be undone. */
export async function removeEntries(
	context: OperationContext,
	paths: readonly string[],
): Promise<OperationResult> {
	for (const path of paths) {
		await context.fileHost.remove(context.workspacePath, path, true);
	}
	return {
		message: { key: "deletedCount", count: paths.length },
		undo: null,
	};
}

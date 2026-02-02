import { getWsManager } from "@/lib/ws-manager";
import type { FileTreeNode, FilesWsEvent } from "@/lib/ws-mux-types";

function arrayBufferToBase64(buffer: ArrayBuffer): string {
	let binary = "";
	const bytes = new Uint8Array(buffer);
	const chunkSize = 0x8000;
	for (let i = 0; i < bytes.length; i += chunkSize) {
		const chunk = bytes.subarray(i, i + chunkSize);
		binary += String.fromCharCode(...chunk);
	}
	return btoa(binary);
}

function base64ToArrayBuffer(base64: string): ArrayBuffer {
	const binary = atob(base64);
	const length = binary.length;
	const bytes = new Uint8Array(length);
	for (let i = 0; i < length; i += 1) {
		bytes[i] = binary.charCodeAt(i);
	}
	return bytes.buffer;
}

export async function fetchFileTreeMux(
	workspacePath: string,
	path = ".",
	depth = 6,
	includeHidden = false,
): Promise<FileTreeNode[]> {
	const manager = getWsManager();
	const response = (await manager.sendAndWait({
		channel: "files",
		type: "tree",
		path,
		depth,
		include_hidden: includeHidden,
		workspace_path: workspacePath,
	})) as FilesWsEvent;

	if (response.type !== "tree_result") {
		throw new Error("Unexpected file tree response");
	}
	return response.entries;
}

export async function readFileMux(
	workspacePath: string,
	path: string,
): Promise<{ data: ArrayBuffer; size?: number; truncated?: boolean }> {
	const manager = getWsManager();
	const response = (await manager.sendAndWait({
		channel: "files",
		type: "read",
		path,
		workspace_path: workspacePath,
	})) as FilesWsEvent;

	if (response.type !== "read_result") {
		throw new Error("Unexpected file read response");
	}

	return {
		data: base64ToArrayBuffer(response.content),
		size: response.size,
		truncated: response.truncated,
	};
}

export async function writeFileMux(
	workspacePath: string,
	path: string,
	content: ArrayBuffer,
	createParents = false,
): Promise<void> {
	const manager = getWsManager();
	const response = (await manager.sendAndWait({
		channel: "files",
		type: "write",
		path,
		content: arrayBufferToBase64(content),
		create_parents: createParents,
		workspace_path: workspacePath,
	})) as FilesWsEvent;

	if (response.type === "write_result") {
		return;
	}
	throw new Error("Failed to write file");
}

export async function deletePathMux(
	workspacePath: string,
	path: string,
	recursive = false,
): Promise<void> {
	const manager = getWsManager();
	const response = (await manager.sendAndWait({
		channel: "files",
		type: "delete",
		path,
		recursive,
		workspace_path: workspacePath,
	})) as FilesWsEvent;

	if (response.type === "delete_result") {
		return;
	}
	throw new Error("Failed to delete path");
}

export async function statPathMux(
	workspacePath: string,
	path: string,
): Promise<unknown> {
	const manager = getWsManager();
	const response = (await manager.sendAndWait({
		channel: "files",
		type: "stat",
		path,
		workspace_path: workspacePath,
	})) as FilesWsEvent;

	if (response.type === "stat_result") {
		return response.stat;
	}
	throw new Error("Failed to stat path");
}

export async function createDirectoryMux(
	workspacePath: string,
	path: string,
	createParents = false,
): Promise<void> {
	const manager = getWsManager();
	const response = (await manager.sendAndWait({
		channel: "files",
		type: "create_directory",
		path,
		create_parents: createParents,
		workspace_path: workspacePath,
	})) as FilesWsEvent;

	if (response.type === "create_directory_result") {
		return;
	}
	throw new Error("Failed to create directory");
}

export async function renamePathMux(
	workspacePath: string,
	from: string,
	to: string,
): Promise<void> {
	const manager = getWsManager();
	const response = (await manager.sendAndWait({
		channel: "files",
		type: "rename",
		from,
		to,
		workspace_path: workspacePath,
	})) as FilesWsEvent;

	if (response.type === "rename_result") {
		return;
	}
	throw new Error("Failed to rename path");
}

export async function copyPathMux(
	workspacePath: string,
	from: string,
	to: string,
	overwrite = false,
): Promise<void> {
	const manager = getWsManager();
	const response = (await manager.sendAndWait({
		channel: "files",
		type: "copy",
		from,
		to,
		overwrite,
		workspace_path: workspacePath,
	})) as FilesWsEvent;

	if (response.type === "copy_result") {
		return;
	}
	throw new Error("Failed to copy path");
}

export async function movePathMux(
	workspacePath: string,
	from: string,
	to: string,
	overwrite = false,
): Promise<void> {
	const manager = getWsManager();
	const response = (await manager.sendAndWait({
		channel: "files",
		type: "move",
		from,
		to,
		overwrite,
		workspace_path: workspacePath,
	})) as FilesWsEvent;

	if (response.type === "move_result") {
		return;
	}
	throw new Error("Failed to move path");
}

export async function downloadFileMux(
	workspacePath: string,
	path: string,
	filename?: string,
): Promise<void> {
	const result = await readFileMux(workspacePath, path);
	const blob = new Blob([result.data]);
	const url = URL.createObjectURL(blob);
	const link = document.createElement("a");
	link.href = url;
	link.download = filename ?? path.split("/").pop() ?? "download";
	link.click();
	URL.revokeObjectURL(url);
}

export async function uploadFileMux(
	workspacePath: string,
	destPath: string,
	file: File,
): Promise<void> {
	const buffer = await file.arrayBuffer();
	await writeFileMux(workspacePath, destPath, buffer, true);
}

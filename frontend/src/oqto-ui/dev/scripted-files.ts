/**
 * Scripted file system for /dev/oqto-ui: a small in-memory tree served
 * with the same single-level contract as the live adapter, so the pane
 * exercises one code path in both routes.
 */

import type { FileEntry, FileHost } from "../platform/files-contract";

const TREE: { readonly [directory: string]: readonly [string, boolean][] } = {
	"": [
		["docs", true],
		["src", true],
		["AGENTS.md", false],
		["package.json", false],
		["README.md", false],
	],
	src: [
		["oqto-ui", true],
		["App.tsx", false],
		["main.tsx", false],
	],
	"src/oqto-ui": [
		["files", true],
		["compositor", true],
		["OqtoUiShell.tsx", false],
	],
	docs: [
		["adr", true],
		["frontend", true],
	],
};

function entriesOf(directory: string): FileEntry[] {
	return (TREE[directory] ?? []).map(([name, isDirectory]) => ({
		path: directory === "" ? name : `${directory}/${name}`,
		name,
		directory: isDirectory,
		symlink: false,
		size: isDirectory ? 0 : 1024,
		modifiedAt: 1_767_225_600_000,
	}));
}

export function createScriptedFileHost(): FileHost {
	const written = new Map<string, string>();
	return {
		async list(_workspacePath, path) {
			return entriesOf(path);
		},
		watch() {
			return () => {};
		},
		async read(_workspacePath, path) {
			return written.get(path) ?? `scripted contents of ${path}\n`;
		},
		async write(_workspacePath, path, text) {
			written.set(path, text);
		},
		async copyToWorkspace() {
			return 1;
		},
		async rename() {},
		async createDirectory() {},
		async remove() {},
		async copy() {},
		async move() {},
	};
}

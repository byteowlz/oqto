/**
 * Scripted file system for /dev/oqto-ui: a small in-memory tree served
 * with the same single-level contract as the live adapter, so the pane
 * exercises one code path in both routes.
 */

import type { FileEntry, FileSystem } from "../platform/files-contract";

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

export const scriptedFileSystem: FileSystem = {
	async list(_workspacePath, path) {
		return entriesOf(path);
	},
	watch() {
		return () => {};
	},
};

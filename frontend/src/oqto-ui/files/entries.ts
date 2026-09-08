/**
 * Ordering and filtering of directory entries. Pure: no React, no
 * transport. The entry shape itself is a platform contract.
 *
 * Ordering follows the convention a file manager is judged by — directories
 * first, then natural name ordering: `file_2` before `file_10`, case
 * ignored, leading zeros worthless.
 */

import type { FileEntry } from "../platform/files-contract";

const NUMBER_RUN = /(\d+)|(\D+)/g;

/** Splits a name into alternating text and numeric runs for natural order. */
function chunks(name: string): (string | number)[] {
	const parts: (string | number)[] = [];
	for (const [, digits, text] of name.toLowerCase().matchAll(NUMBER_RUN)) {
		parts.push(
			digits === undefined ? (text ?? "") : Number.parseInt(digits, 10),
		);
	}
	return parts;
}

export function compareNames(left: string, right: string): number {
	const a = chunks(left);
	const b = chunks(right);
	for (let index = 0; index < Math.min(a.length, b.length); index += 1) {
		const x = a[index];
		const y = b[index];
		if (typeof x === "number" && typeof y === "number") {
			if (x !== y) return x - y;
			continue;
		}
		const text = String(x).localeCompare(String(y));
		if (text !== 0) return text;
	}
	if (a.length !== b.length) return a.length - b.length;
	// Same folded name: fall back to the raw name so ordering stays total.
	return left.localeCompare(right);
}

/** Case-insensitive subsequence match, the filter every fast pane uses. */
export function matchesFilter(name: string, filter: string): boolean {
	if (filter === "") return true;
	const haystack = name.toLowerCase();
	const needle = filter.toLowerCase();
	let index = 0;
	for (const character of needle) {
		index = haystack.indexOf(character, index);
		if (index < 0) return false;
		index += 1;
	}
	return true;
}

/** Joins a directory path with a child name, keeping the root path empty. */
export function childPath(directory: string, name: string): string {
	return directory === "" ? name : `${directory}/${name}`;
}

/** The parent of a relative path; null at the root. */
export function parentPath(path: string): string | null {
	if (path === "") return null;
	const cut = path.lastIndexOf("/");
	return cut < 0 ? "" : path.slice(0, cut);
}

/** Root-to-path segments with their paths, for a breadcrumb. */
export function breadcrumb(path: string): { name: string; path: string }[] {
	if (path === "") return [];
	const names = path.split("/");
	return names.map((name, index) => ({
		name,
		path: names.slice(0, index + 1).join("/"),
	}));
}

export type { FileEntry };

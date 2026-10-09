/**
 * Entry ordering: directories always group ahead of files, then the chosen
 * key decides with the natural name as a stable tiebreak.
 */

import { type FileEntry, compareNames } from "./entries";

export type SortKey = "name" | "size" | "modified";

export interface SortOrder {
	readonly key: SortKey;
	readonly descending: boolean;
}

export const DEFAULT_SORT: SortOrder = { key: "name", descending: false };

/**
 * Directories always group ahead of files; within a group the chosen key
 * decides, with the name as a stable tiebreak so ordering stays total.
 */
export function sortEntries(
	entries: readonly FileEntry[],
	order: SortOrder = DEFAULT_SORT,
): FileEntry[] {
	const direction = order.descending ? -1 : 1;
	return [...entries].sort((left, right) => {
		if (left.directory !== right.directory) return left.directory ? -1 : 1;
		if (order.key === "size" && left.size !== right.size) {
			return (left.size - right.size) * direction;
		}
		if (order.key === "modified" && left.modifiedAt !== right.modifiedAt) {
			return (left.modifiedAt - right.modifiedAt) * direction;
		}
		return compareNames(left.name, right.name) * direction;
	});
}

import {
	breadcrumb,
	compareNames,
	matchesFilter,
	parentPath,
	sortEntries,
} from "@/src/oqto-ui/files/entries";
import { formatModified, formatSize } from "@/src/oqto-ui/files/format";
import {
	applyChange,
	cursorEntry,
	cursorToEdge,
	goUp,
	navigate,
} from "@/src/oqto-ui/files/navigation";
import {
	applyListing,
	initialState,
	moveCursor,
	select,
	setFilter,
	visibleEntries,
} from "@/src/oqto-ui/files/navigator";
import type { FileEntry } from "@/src/oqto-ui/platform/files-contract";
import { describe, expect, it } from "vitest";

function entry(path: string, directory = false): FileEntry {
	const name = path.split("/").pop() ?? path;
	return { path, name, directory, symlink: false, size: 0, modifiedAt: 0 };
}

const ROOT: FileEntry[] = [
	entry("readme.md"),
	entry("src", true),
	entry("File_10.txt"),
	entry("file_2.txt"),
	entry("docs", true),
];

function loaded() {
	return applyListing(initialState(), "", { status: "ready", entries: ROOT });
}

describe("entry ordering", () => {
	it("sorts directories first, then natural name order", () => {
		const names = sortEntries(ROOT).map((item) => item.name);
		expect(names).toEqual([
			"docs",
			"src",
			"file_2.txt",
			"File_10.txt",
			"readme.md",
		]);
	});

	it("orders numbers by value, ignores case, and treats leading zeros as nothing", () => {
		expect(Math.sign(compareNames("file_2", "file_10"))).toBe(-1);
		expect(Math.sign(compareNames("a007", "a8"))).toBe(-1);
		expect(Math.sign(compareNames("a07", "a10"))).toBe(-1);
		expect(Math.sign(compareNames("b", "a"))).toBe(1);
		// Case never changes relative order; it only breaks exact ties.
		expect(
			sortEntries([entry("B.txt"), entry("a.txt")]).map((i) => i.name),
		).toEqual(["a.txt", "B.txt"]);
		expect(compareNames("readme.md", "readme.md")).toBe(0);
	});

	it("matches a filter as a case-insensitive subsequence", () => {
		expect(matchesFilter("OqtoUiShell.tsx", "ous")).toBe(true);
		expect(matchesFilter("OqtoUiShell.tsx", "shell")).toBe(true);
		expect(matchesFilter("readme.md", "zz")).toBe(false);
		expect(matchesFilter("anything", "")).toBe(true);
	});

	it("derives parents and breadcrumbs from relative paths", () => {
		expect(parentPath("")).toBeNull();
		expect(parentPath("src")).toBe("");
		expect(parentPath("src/oqto-ui/files")).toBe("src/oqto-ui");
		expect(breadcrumb("src/oqto-ui")).toEqual([
			{ name: "src", path: "src" },
			{ name: "oqto-ui", path: "src/oqto-ui" },
		]);
		expect(breadcrumb("")).toEqual([]);
	});
});

describe("navigator", () => {
	it("places the cursor on the first row once a listing arrives", () => {
		const state = loaded();
		expect(state.cursor).toBe("docs");
		expect(visibleEntries(state)).toHaveLength(5);
	});

	it("moves the cursor and clamps at both ends", () => {
		let state = loaded();
		state = moveCursor(state, 2);
		expect(state.cursor).toBe("file_2.txt");
		state = moveCursor(state, -50);
		expect(state.cursor).toBe("docs");
		state = cursorToEdge(state, "last");
		expect(state.cursor).toBe("readme.md");
		expect(moveCursor(state, 50).cursor).toBe("readme.md");
	});

	it("filters in place and keeps the cursor on a visible row", () => {
		let state = moveCursor(loaded(), 4);
		expect(state.cursor).toBe("readme.md");
		state = setFilter(state, "file");
		expect(visibleEntries(state).map((item) => item.name)).toEqual([
			"file_2.txt",
			"File_10.txt",
		]);
		expect(state.cursor).toBe("file_2.txt");
		state = setFilter(state, "");
		expect(state.cursor).toBe("file_2.txt");
	});

	it("selects like a file manager: replace, toggle, and range", () => {
		let state = loaded();
		state = select(state, "src", "replace");
		expect(state.selection).toEqual(["src"]);
		state = select(state, "readme.md", "toggle");
		expect(state.selection).toEqual(["src", "readme.md"]);
		state = select(state, "readme.md", "toggle");
		expect(state.selection).toEqual(["src"]);
		state = select(state, "docs", "replace");
		state = select(state, "file_2.txt", "range");
		expect(state.selection).toEqual(["docs", "src", "file_2.txt"]);
	});

	it("keeps an empty listing cursorless and reports loading and error states", () => {
		const empty = applyListing(initialState(), "", {
			status: "ready",
			entries: [],
		});
		expect(empty.cursor).toBeNull();
		expect(visibleEntries(empty)).toEqual([]);
		const failed = applyListing(initialState(), "", {
			status: "error",
			message: "denied",
		});
		expect(failed.listings[""]).toEqual({ status: "error", message: "denied" });
		expect(visibleEntries(failed)).toEqual([]);
	});
});

describe("navigation", () => {
	it("enters a directory and returns with the cursor on it", () => {
		let state = navigate(loaded(), "src");
		expect(state.cwd).toBe("src");
		expect(state.cursor).toBeNull();
		state = applyListing(state, "src", {
			status: "ready",
			entries: [entry("src/app.ts"), entry("src/lib", true)],
		});
		expect(cursorEntry(state)?.name).toBe("lib");
		state = goUp(state);
		expect(state.cwd).toBe("");
		expect(state.cursor).toBe("src");
		// The cached root listing is reused, not refetched.
		expect(state.listings[""]).toEqual({
			status: "ready",
			entries: sortEntries(ROOT),
		});
	});

	it("clears the filter and selection when the directory changes", () => {
		let state = select(setFilter(loaded(), "file"), "file_2.txt", "replace");
		state = navigate(state, "src");
		expect(state.filter).toBe("");
		expect(state.selection).toEqual([]);
	});

	it("stays put at the root", () => {
		const state = loaded();
		expect(goUp(state)).toBe(state);
	});

	it("invalidates the affected directory on a host change and marks the path", () => {
		const state = loaded();
		const changed = applyChange(state, "readme.md");
		expect(changed.refetch).toBe("");
		expect(changed.state.listings[""]).toEqual({ status: "loading" });
		expect(changed.state.changed).toEqual(["readme.md"]);
		// A change in a directory nobody listed costs no refetch.
		const elsewhere = applyChange(state, "src/deep/file.ts");
		expect(elsewhere.refetch).toBeNull();
		expect(elsewhere.state.listings).toBe(state.listings);
	});
});

describe("entry facts", () => {
	const labels = {
		bytes: (count: number) => `${count} B`,
		kilo: (value: string) => `${value} KB`,
		mega: (value: string) => `${value} MB`,
		giga: (value: string) => `${value} GB`,
	};

	it("scales sizes to the unit a human reads", () => {
		expect(formatSize(0, labels)).toBe("0 B");
		expect(formatSize(999, labels)).toBe("999 B");
		expect(formatSize(2048, labels)).toBe("2.0 KB");
		expect(formatSize(20_480, labels)).toBe("20 KB");
		expect(formatSize(5_242_880, labels)).toBe("5.0 MB");
		expect(formatSize(3 * 1024 ** 3, labels)).toBe("3.0 GB");
	});

	it("shortens timestamps by distance and hides unknown ones", () => {
		// Local-time constructors keep the expectations timezone-independent.
		const now = new Date(2026, 8, 7, 22, 30).getTime();
		expect(formatModified(0, "en-GB", now)).toBe("");
		expect(
			formatModified(new Date(2026, 8, 7, 9, 15).getTime(), "en-GB", now),
		).toMatch(/\d{2}:\d{2}/);
		expect(
			formatModified(new Date(2026, 2, 2, 9, 15).getTime(), "en-GB", now),
		).toMatch(/Mar/);
		expect(
			formatModified(new Date(2024, 2, 2, 9, 15).getTime(), "en-GB", now),
		).toMatch(/2024/);
	});
});

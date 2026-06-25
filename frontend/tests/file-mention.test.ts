import { describe, expect, it } from "vitest";

import { insertFileMention } from "@/lib/file-mention";

describe("insertFileMention (oqto-drwc)", () => {
	it("inserts @<path> with a trailing space at the cursor", () => {
		const { value, cursor } = insertFileMention("see ", 4, "src/main.rs");
		expect(value).toBe("see @src/main.rs ");
		// cursor advances past "@src/main.rs " = path.length + 2
		expect(cursor).toBe(4 + "src/main.rs".length + 2);
		expect(value.slice(0, cursor)).toBe("see @src/main.rs ");
	});

	it("inserts in the middle, preserving the tail", () => {
		const { value, cursor } = insertFileMention("ab cd", 3, "x.ts");
		expect(value).toBe("ab @x.ts cd");
		expect(value[cursor]).toBe("c");
	});

	it("appends at end of an empty composer", () => {
		const { value, cursor } = insertFileMention("", 0, "README.md");
		expect(value).toBe("@README.md ");
		expect(cursor).toBe(value.length);
	});

	it("clamps an out-of-range cursor to the value length", () => {
		const { value, cursor } = insertFileMention("hi", 999, "a.txt");
		expect(value).toBe("hi@a.txt ");
		expect(cursor).toBe(value.length);
	});

	it("clamps a negative cursor to 0", () => {
		const { value } = insertFileMention("hi", -5, "a.txt");
		expect(value).toBe("@a.txt hi");
	});
});

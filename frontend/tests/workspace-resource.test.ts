import { describe, expect, it } from "vitest";
import { normalizeWorkspaceFileReference } from "../lib/workspace-resource";

describe("normalizeWorkspaceFileReference", () => {
	it.each([
		["@src/main.ts", "src/main.ts"],
		["./src/main.ts", "src/main.ts"],
		["/workspace/project/src/main.ts", "src/main.ts"],
		["file:///workspace/project/src/main.ts", "src/main.ts"],
	])("normalizes %s within a Work Directory", (reference, expected) => {
		expect(
			normalizeWorkspaceFileReference(reference, "/workspace/project"),
		).toBe(expected);
	});
});

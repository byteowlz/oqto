import { workspaceFileUrl } from "@/lib/control-plane-client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type LocalStorageMock = {
	getItem: ReturnType<typeof vi.fn>;
};

describe("workspaceFileUrl", () => {
	const storage = window.localStorage as unknown as LocalStorageMock;

	beforeEach(() => {
		storage.getItem.mockReturnValue("https://example.com/api");
	});

	afterEach(() => {
		storage.getItem.mockReturnValue(null);
	});

	it("builds a workspace file URL with encoded params", () => {
		const url = workspaceFileUrl("my workspace", "path/to file.png");
		const parsed = new URL(url);

		expect(parsed.origin).toBe("https://example.com");
		expect(parsed.pathname).toBe("/api/workspace/files/file");
		expect(parsed.searchParams.get("path")).toBe("path/to file.png");
		expect(parsed.searchParams.get("workspace_path")).toBe("my workspace");
	});
});

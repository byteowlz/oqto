import { beforeEach, describe, expect, it, vi } from "vitest";

const sendAndWaitMock = vi.fn();

vi.mock("@/lib/ws-manager", () => ({
	getWsManager: () => ({
		sendAndWait: sendAndWaitMock,
	}),
}));

import { clearTreeCache, fetchFileTreeMux, readFileMux } from "@/lib/mux-files";

describe("mux-files traversal budget handling", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		clearTreeCache();
	});

	it("does not cache truncated tree responses", async () => {
		sendAndWaitMock.mockResolvedValue({
			channel: "files",
			type: "tree_result",
			path: ".",
			entries: [{ name: "src", path: "src", type: "directory" }],
			truncated: true,
		});

		await fetchFileTreeMux("/tmp/ws", ".", 2, false);
		await fetchFileTreeMux("/tmp/ws", ".", 2, false);

		expect(sendAndWaitMock).toHaveBeenCalledTimes(2);
	});

	it("caches full tree responses", async () => {
		sendAndWaitMock.mockResolvedValue({
			channel: "files",
			type: "tree_result",
			path: ".",
			entries: [{ name: "src", path: "src", type: "directory" }],
			truncated: false,
		});

		await fetchFileTreeMux("/tmp/ws", ".", 2, false);
		await fetchFileTreeMux("/tmp/ws", ".", 2, false);

		expect(sendAndWaitMock).toHaveBeenCalledTimes(1);
	});

	it("deduplicates concurrent reads of the same workspace file", async () => {
		let resolveResponse: (value: object) => void = () => {};
		sendAndWaitMock.mockReturnValue(
			new Promise((resolve) => {
				resolveResponse = resolve;
			}),
		);
		const first = readFileMux("/tmp/ws", "src/main.ts");
		const second = readFileMux("/tmp/ws", "src/main.ts");
		expect(sendAndWaitMock).toHaveBeenCalledTimes(1);
		resolveResponse({
			channel: "files",
			type: "read_result",
			path: "src/main.ts",
			content: "aGk=",
		});
		const [left, right] = await Promise.all([first, second]);
		expect(new TextDecoder().decode(left.data)).toBe("hi");
		expect(new TextDecoder().decode(right.data)).toBe("hi");
	});

	it("surfaces file read errors instead of hiding the response", async () => {
		sendAndWaitMock.mockResolvedValue({
			channel: "files",
			type: "error",
			error: "file not found",
		});
		await expect(readFileMux("/tmp/ws", "missing.ts")).rejects.toThrow(
			"file not found",
		);
	});

	it("merges paged tree_result responses using next_offset", async () => {
		sendAndWaitMock
			.mockResolvedValueOnce({
				channel: "files",
				type: "tree_result",
				path: ".",
				entries: [{ name: "a", path: "a", type: "file" }],
				truncated: true,
				next_offset: 1,
			})
			.mockResolvedValueOnce({
				channel: "files",
				type: "tree_result",
				path: ".",
				entries: [{ name: "b", path: "b", type: "file" }],
				truncated: false,
			});

		const entries = await fetchFileTreeMux("/tmp/ws", ".", 2, false);
		expect(entries.map((e) => e.name)).toEqual(["a", "b"]);
		expect(sendAndWaitMock).toHaveBeenCalledTimes(2);
	});
});

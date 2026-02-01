import { FileReferenceCard } from "@/features/chat/components/ChatView";
import { render, screen } from "@testing-library/react";
import { type Mock, beforeEach, describe, expect, it, vi } from "vitest";

type LocalStorageMock = {
	getItem: ReturnType<typeof vi.fn>;
};

describe("FileReferenceCard", () => {
	const storage = window.localStorage as unknown as LocalStorageMock;

	beforeEach(() => {
		storage.getItem.mockReturnValue("https://example.com/api");
		(globalThis.fetch as Mock | undefined) = vi.fn(() =>
			Promise.resolve({ ok: true } as Response),
		);
	});

	it("renders image previews with workspace file endpoint", async () => {
		render(
			<FileReferenceCard
				filePath="images/panda.png"
				workspacePath="my workspace"
			/>,
		);

		const image = await screen.findByAltText("panda.png");
		expect(image.getAttribute("src")).toBe(
			"https://example.com/api/workspace/files/file?path=images%2Fpanda.png&workspace_path=my+workspace",
		);
	});
});

import { FileReferenceCard } from "@/features/main-chat/components/MainChatPiView";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, type vi } from "vitest";

type LocalStorageMock = {
	getItem: ReturnType<typeof vi.fn>;
};

describe("FileReferenceCard", () => {
	const storage = window.localStorage as unknown as LocalStorageMock;

	beforeEach(() => {
		storage.getItem.mockReturnValue("https://example.com/api");
	});

	it("renders image previews with workspace file endpoint", () => {
		render(
			<FileReferenceCard
				filePath="images/panda.png"
				workspacePath="my workspace"
			/>,
		);

		const image = screen.getByRole("img", { name: "images/panda.png" });
		expect(image.getAttribute("src")).toBe(
			"https://example.com/api/workspace/files/file?path=images%2Fpanda.png&workspace_path=my+workspace",
		);
	});
});

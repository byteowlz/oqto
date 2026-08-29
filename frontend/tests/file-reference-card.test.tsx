import { FileReferenceCard } from "@/lib/chat-rendering/CanonicalMessageRenderer";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock the mux-files module
vi.mock("@/lib/mux-files", () => ({
	statPathMux: vi.fn(() => Promise.resolve({ size: 1024 })),
	readFileMux: vi.fn(() =>
		Promise.resolve({ data: new Uint8Array([137, 80, 78, 71]) }),
	),
}));

// Mock URL.createObjectURL
globalThis.URL.createObjectURL = vi.fn(() => "blob:mock-url");
globalThis.URL.revokeObjectURL = vi.fn();

describe("FileReferenceCard", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders image previews for image files", async () => {
		render(
			<FileReferenceCard
				filePath="images/panda.png"
				workspacePath="my workspace"
				fileAdapter={{ fileUrl: () => "blob:mock-url" }}
			/>,
		);

		const image = await screen.findByAltText("panda.png");
		expect(image).toBeDefined();
		expect(image.tagName).toBe("IMG");
	});

	it("stacks and truncates long file paths instead of splitting columns", async () => {
		render(
			<FileReferenceCard
				filePath="/workspace/wiki/projects/pi-new-harness-impact-on-oqto.md"
				workspacePath="/workspace"
				onOpenFileReference={vi.fn()}
			/>,
		);

		await waitFor(() => {
			expect(screen.getByTestId("file-reference-card")).toBeInTheDocument();
		});
		const fileName = screen.getByText("pi-new-harness-impact-on-oqto.md");
		const path = screen.getByText(
			"wiki/projects/pi-new-harness-impact-on-oqto.md",
		);

		expect(fileName).toHaveClass("block", "truncate");
		expect(path).toHaveClass("block", "truncate");
		expect(fileName.parentElement).toHaveClass("min-w-0", "flex-1");
	});
});

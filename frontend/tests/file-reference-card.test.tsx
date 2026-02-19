import { FileReferenceCard } from "@/features/chat/components/ChatView";
import { render, screen } from "@testing-library/react";
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
			/>,
		);

		const image = await screen.findByAltText("panda.png");
		expect(image).toBeDefined();
		expect(image.tagName).toBe("IMG");
	});
});

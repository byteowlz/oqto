import { MarkdownRenderer } from "@/components/data-display/markdown-renderer";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

describe("MarkdownRenderer", () => {
	it("renders file paths as compact inline links without a code background", () => {
		const onOpen = vi.fn();
		render(
			<MarkdownRenderer
				content="Changed: `frontend/lib/chat-rendering/CanonicalMessageRenderer.tsx`"
				onFileReferenceOpen={onOpen}
			/>,
		);

		const link = screen.getByRole("button", {
			name: "frontend/lib/chat-rendering/CanonicalMessageRenderer.tsx",
		});
		expect(link).toHaveClass(
			"markdown-file-reference-inline",
			"inline",
			"bg-transparent",
			"p-0",
		);
		expect(link).not.toHaveStyle({
			backgroundColor: "var(--code-inline-bg)",
		});

		fireEvent.click(link);
		expect(onOpen).toHaveBeenCalledWith(
			expect.objectContaining({
				filePath: "frontend/lib/chat-rendering/CanonicalMessageRenderer.tsx",
			}),
		);
	});

	it("uses square markers for unordered lists", () => {
		const { container } = render(
			<MarkdownRenderer content="- First\n- Second" />,
		);
		expect(container.querySelector("ul")).toHaveClass(
			"[list-style-type:square]",
		);
	});
});

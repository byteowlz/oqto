import { MarkdownRenderer } from "@/components/data-display/markdown-renderer";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { initI18n } from "../../lib/i18n";

initI18n();

/**
 * An image in an answer is a thing to look at, so it has to be openable — and
 * openable as part of the set it belongs to, not as one orphaned picture.
 */

const TWO_IMAGES = [
	"Here is the plan:",
	"",
	"![Verteilerkasten](/api/workspace/files/file?path=uploads%2Fkasten.jpg&workspace_path=%2Fhaus)",
	"",
	"![Stromlaufplan](/api/workspace/files/file?path=uploads%2Fplan.png&workspace_path=%2Fhaus)",
].join("\n");

describe("an image in a message", () => {
	it("reports the one clicked and the set it came from", () => {
		const onImageOpen = vi.fn();
		render(<MarkdownRenderer content={TWO_IMAGES} onImageOpen={onImageOpen} />);

		fireEvent.click(screen.getByRole("button", { name: /Stromlaufplan/ }));

		expect(onImageOpen).toHaveBeenCalledTimes(1);
		const [clicked, siblings] = onImageOpen.mock.calls[0];
		expect(clicked.alt).toBe("Stromlaufplan");
		expect(siblings.map((image: { alt: string }) => image.alt)).toEqual([
			"Verteilerkasten",
			"Stromlaufplan",
		]);
	});

	it("keeps the alt text as the picture's own name", () => {
		render(<MarkdownRenderer content={TWO_IMAGES} onImageOpen={vi.fn()} />);
		expect(screen.getByAltText("Verteilerkasten")).toBeInTheDocument();
	});

	it("stays an inert image where the host offers nowhere to open it", () => {
		render(<MarkdownRenderer content={TWO_IMAGES} />);
		expect(screen.queryByRole("button", { name: /Stromlaufplan/ })).toBeNull();
		expect(screen.getByAltText("Stromlaufplan")).toBeInTheDocument();
	});

	it("ignores an image inside a fenced block", () => {
		const onImageOpen = vi.fn();
		render(
			<MarkdownRenderer
				content={"```md\n![not real](x.png)\n```\n\n![real](y.png)"}
				onImageOpen={onImageOpen}
			/>,
		);
		fireEvent.click(screen.getByRole("button", { name: /real/ }));
		const [, siblings] = onImageOpen.mock.calls[0];
		expect(siblings.map((image: { alt: string }) => image.alt)).toEqual([
			"real",
		]);
	});
});

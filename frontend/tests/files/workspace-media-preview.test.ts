import { describe, expect, it } from "vitest";
import {
	workspaceFilePathFromUrl,
	workspaceFilePreviewUrl,
	workspaceImagePreviewUrl,
} from "../../src/oqto-ui/platform/workspace-file-url";

describe("workspace raster previews", () => {
	it("uses compressed thumbnails without changing original URLs or path identity", () => {
		const original = new URL(
			workspaceFilePreviewUrl("/work/a b", "images/a #.PNG"),
		);
		const preview = new URL(
			workspaceImagePreviewUrl("/work/a b", "images/a #.PNG"),
		);
		expect(original.pathname).toBe("/api/workspace/files/file");
		expect(preview.pathname).toBe("/api/workspace/files/thumbnail");
		expect(preview.searchParams.get("size")).toBe("768");
		expect(preview.searchParams.get("workspace_path")).toBe("/work/a b");
		expect(workspaceFilePathFromUrl(preview.toString())).toBe("images/a #.PNG");
	});
	it("does not rasterize SVG or route non-images into the image decoder", () => {
		for (const path of ["diagram.svg", "document.pdf", "clip.mp4"])
			expect(workspaceImagePreviewUrl("/work", path)).toBe(
				workspaceFilePreviewUrl("/work", path),
			);
	});
});

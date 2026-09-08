import { FileViewer } from "@/src/oqto-ui/files/viewers/FileViewer";
import { mediaElement, viewerFor } from "@/src/oqto-ui/files/viewers/registry";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { initI18n } from "../../lib/i18n";

initI18n();

describe("viewer registry", () => {
	it("routes a format to exactly one viewer", () => {
		expect(viewerFor("main.rs", false)).toBe("text");
		expect(viewerFor("notes.md", false)).toBe("text");
		expect(viewerFor("config.toml", false)).toBe("text");
		expect(viewerFor("logo.png", false)).toBe("image");
		expect(viewerFor("clip.mp4", false)).toBe("media");
		expect(viewerFor("paper.pdf", false)).toBe("pdf");
		expect(viewerFor(".gitignore", false)).toBe("text");
		expect(viewerFor("bundle.tar.gz", false)).toBe("unsupported");
		expect(viewerFor("app.wasm", false)).toBe("unsupported");
		expect(viewerFor("src", true)).toBe("unsupported");
	});

	it("plays audio in an audio element and everything else in video", () => {
		expect(mediaElement("song.flac")).toBe("audio");
		expect(mediaElement("voice.mp3")).toBe("audio");
		expect(mediaElement("clip.webm")).toBe("video");
	});
});

describe("file viewer", () => {
	const at = { path: "docs/logo.png", workspacePath: "/w" };

	it("streams an image from the workspace URL instead of the host contract", () => {
		render(
			<FileViewer
				name="logo.png"
				path={at.path}
				workspacePath={at.workspacePath}
				text={{ status: "idle" }}
			/>,
		);
		const image = screen.getByAltText("logo.png") as HTMLImageElement;
		expect(image.src).toContain("/api/workspace/files/file");
		expect(image.src).toContain("path=docs%2Flogo.png");
		expect(image.src).toContain("workspace_path=%2Fw");
	});

	it("renders fetched text for textual formats", () => {
		render(
			<FileViewer
				name="notes.md"
				path="notes.md"
				workspacePath="/w"
				text={{ status: "text", text: "# title" }}
			/>,
		);
		expect(screen.getByText("# title")).toBeInTheDocument();
	});

	it("says a format has no viewer rather than showing an empty pane", () => {
		render(
			<FileViewer
				name="bundle.tar.gz"
				path="bundle.tar.gz"
				workspacePath="/w"
				text={{ status: "idle" }}
			/>,
		);
		expect(screen.getByText("No preview for this kind")).toBeInTheDocument();
	});
});

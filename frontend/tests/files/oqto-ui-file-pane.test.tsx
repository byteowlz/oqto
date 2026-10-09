import { FilePane } from "@/src/oqto-ui/files/FilePane";
import type { FileHost } from "@/src/oqto-ui/platform/files-contract";
import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { initI18n } from "../../lib/i18n";

initI18n();

function host(overrides: Partial<FileHost> = {}): FileHost {
	return {
		async list() {
			return [];
		},
		watch() {
			return () => {};
		},
		async read() {
			return "first line\nsecond line\n";
		},
		async write() {},
		async rename() {},
		async createDirectory() {},
		async remove() {},
		async copy() {},
		async move() {},
		...overrides,
	};
}

describe("file Content", () => {
	it("loads a text file once and shows it", async () => {
		const read = vi.fn(async () => "first line\nsecond line\n");
		render(
			<FilePane
				fileHost={host({ read })}
				workspacePath="/work"
				path="docs/notes.md"
			/>,
		);
		expect(await screen.findByText(/first line/)).toBeInTheDocument();
		expect(read).toHaveBeenCalledTimes(1);
		expect(read).toHaveBeenCalledWith("/work", "docs/notes.md");
	});

	it("edits and writes the draft back through the host", async () => {
		const write = vi.fn(async () => {});
		render(
			<FilePane
				fileHost={host({ write })}
				workspacePath="/work"
				path="notes.md"
			/>,
		);
		await screen.findByText(/first line/);
		fireEvent.click(screen.getByRole("button", { name: "Edit file" }));
		const editor = screen.getByRole("textbox", { name: "Edit file" });
		expect(editor).toHaveValue("first line\nsecond line\n");
		// Saving is refused until the draft actually differs.
		expect(screen.getByRole("button", { name: "Save file" })).toBeDisabled();
		act(() => {
			fireEvent.change(editor, { target: { value: "edited\n" } });
		});
		expect(screen.getByText("Unsaved")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "Save file" }));
		await waitFor(() =>
			expect(write).toHaveBeenCalledWith("/work", "notes.md", "edited\n"),
		);
		await waitFor(() =>
			expect(screen.queryByText("Unsaved")).not.toBeInTheDocument(),
		);
	});

	it("discards a draft without writing", async () => {
		const write = vi.fn(async () => {});
		render(
			<FilePane
				fileHost={host({ write })}
				workspacePath="/work"
				path="notes.md"
			/>,
		);
		await screen.findByText(/first line/);
		fireEvent.click(screen.getByRole("button", { name: "Edit file" }));
		act(() => {
			fireEvent.change(screen.getByRole("textbox", { name: "Edit file" }), {
				target: { value: "throwaway" },
			});
		});
		fireEvent.click(screen.getByRole("button", { name: "Discard changes" }));
		expect(write).not.toHaveBeenCalled();
		expect(await screen.findByText(/first line/)).toBeInTheDocument();
	});

	it("never reads a binary format and offers no editor for it", async () => {
		const read = vi.fn(async () => "");
		render(
			<FilePane
				fileHost={host({ read })}
				workspacePath="/work"
				path="logo.png"
			/>,
		);
		const image = (await screen.findByAltText("logo.png")) as HTMLImageElement;
		expect(image.src).toContain("path=logo.png");
		expect(read).not.toHaveBeenCalled();
		expect(screen.queryByRole("button", { name: "Edit file" })).toBeNull();
	});
});

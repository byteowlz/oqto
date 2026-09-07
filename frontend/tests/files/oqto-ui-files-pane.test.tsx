import { WorkDirectoryFiles } from "@/src/oqto-ui/files/WorkDirectoryFiles";
import type { FileSystem } from "@/src/oqto-ui/platform/files-contract";
import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { i18n, initI18n } from "../../lib/i18n";

initI18n();

/**
 * jsdom has no layout engine, so a virtualized list measures zero and
 * renders nothing. Give the scroll container a box, as the suite already
 * does for ResizeObserver.
 */
beforeAll(() => {
	for (const [property, value] of [
		["offsetWidth", 320],
		["offsetHeight", 640],
		["clientWidth", 320],
		["clientHeight", 640],
	] as const) {
		Object.defineProperty(HTMLElement.prototype, property, {
			configurable: true,
			get() {
				return this.classList?.contains("wb-files-rows") ? value : 0;
			},
		});
	}
	const original = HTMLElement.prototype.getBoundingClientRect;
	HTMLElement.prototype.getBoundingClientRect = function boundingRect() {
		if (this.classList?.contains("wb-files-rows")) {
			return {
				x: 0,
				y: 0,
				top: 0,
				left: 0,
				right: 320,
				bottom: 640,
				width: 320,
				height: 640,
				toJSON: () => ({}),
			} as DOMRect;
		}
		return original.call(this);
	};
});

function fileSystem(overrides: Partial<FileSystem> = {}): FileSystem & {
	calls: string[];
	emit: (path: string) => void;
} {
	const calls: string[] = [];
	let notify:
		| ((change: { path: string; kind: "modified"; directory: boolean }) => void)
		| null = null;
	return {
		calls,
		emit: (path) => notify?.({ path, kind: "modified", directory: false }),
		async list(_workspace, path) {
			calls.push(path);
			if (path === "") {
				return [
					{
						path: "src",
						name: "src",
						directory: true,
						symlink: false,
						size: 0,
						modifiedAt: 0,
					},
					{
						path: "readme.md",
						name: "readme.md",
						directory: false,
						symlink: false,
						size: 10,
						modifiedAt: 0,
					},
					{
						path: "package.json",
						name: "package.json",
						directory: false,
						symlink: false,
						size: 20,
						modifiedAt: 0,
					},
				];
			}
			if (path === "src") {
				return [
					{
						path: "src/app.ts",
						name: "app.ts",
						directory: false,
						symlink: false,
						size: 5,
						modifiedAt: 0,
					},
				];
			}
			return [];
		},
		watch(_workspace, onChange) {
			notify = onChange;
			return () => {
				notify = null;
			};
		},
		...overrides,
	};
}

async function renderPane(fs = fileSystem()) {
	const view = render(
		<WorkDirectoryFiles fileSystem={fs} workspacePath="/work/repo" />,
	);
	await screen.findByText("readme.md");
	const rows = view.container.querySelector(".wb-files-rows") as HTMLElement;
	return { view, rows, fs };
}

describe("Files pane", () => {
	it("lists one directory level and orders directories first", async () => {
		const { view, fs } = await renderPane();
		expect(fs.calls).toEqual([""]);
		const names = [...view.container.querySelectorAll(".wb-tree__name")].map(
			(n) => n.textContent,
		);
		expect(names).toEqual(["src", "package.json", "readme.md"]);
		expect(
			view.container.querySelector("[data-cursor]")?.textContent,
		).toContain("src");
	});

	it("navigates with the keyboard and fetches the entered directory once", async () => {
		const { view, rows, fs } = await renderPane();
		fireEvent.keyDown(rows, { key: "j" });
		expect(
			view.container.querySelector("[data-cursor]")?.textContent,
		).toContain("package.json");
		fireEvent.keyDown(rows, { key: "k" });
		fireEvent.keyDown(rows, { key: "Enter" });
		await screen.findByText("app.ts");
		expect(fs.calls).toEqual(["", "src"]);
		fireEvent.keyDown(rows, { key: "h" });
		await screen.findByText("readme.md");
		// Going back uses the cached listing and restores the cursor.
		expect(fs.calls).toEqual(["", "src"]);
		expect(
			view.container.querySelector("[data-cursor]")?.textContent,
		).toContain("src");
	});

	it("filters in place with / and clears with Escape", async () => {
		const { view, rows } = await renderPane();
		fireEvent.keyDown(rows, { key: "/" });
		const filter = screen.getByLabelText("Filter this folder");
		fireEvent.change(filter, { target: { value: "read" } });
		await waitFor(() =>
			expect(
				[...view.container.querySelectorAll(".wb-tree__name")].map(
					(n) => n.textContent,
				),
			).toEqual(["readme.md"]),
		);
		fireEvent.keyDown(filter, { key: "Escape" });
		await waitFor(() =>
			expect(view.container.querySelectorAll(".wb-tree__name")).toHaveLength(3),
		);
	});

	it("selects with the mouse: click replaces, ctrl-click toggles", async () => {
		const { view } = await renderPane();
		const rows = [
			...view.container.querySelectorAll(".wb-tree__row"),
		] as HTMLElement[];
		fireEvent.click(rows[1]);
		expect(view.container.querySelectorAll("[data-selected]")).toHaveLength(1);
		fireEvent.click(rows[2], { ctrlKey: true });
		expect(view.container.querySelectorAll("[data-selected]")).toHaveLength(2);
		expect(screen.getByText("2 selected")).toBeInTheDocument();
	});

	it("refetches only the directory a host change touched", async () => {
		const { fs } = await renderPane();
		act(() => fs.emit("readme.md"));
		await waitFor(() => expect(fs.calls).toEqual(["", ""]));
		act(() => fs.emit("elsewhere/deep/file.ts"));
		expect(fs.calls).toEqual(["", ""]);
	});

	it("renders only the rows that fit, not the whole directory", async () => {
		const many = Array.from({ length: 5000 }, (_, index) => ({
			path: `file-${index}.txt`,
			name: `file-${index}.txt`,
			directory: false,
			symlink: false,
			size: 0,
			modifiedAt: 0,
		}));
		const fs: FileSystem = {
			async list() {
				return many;
			},
			watch: () => () => {},
		};
		const view = render(
			<WorkDirectoryFiles fileSystem={fs} workspacePath="/work/repo" />,
		);
		await screen.findByText("file-0.txt");
		const rendered = view.container.querySelectorAll(".wb-tree__row").length;
		expect(rendered).toBeGreaterThan(0);
		expect(rendered).toBeLessThan(80);
		expect(screen.getByText("5000 entries")).toBeInTheDocument();
	});

	it("reports a failed listing instead of rendering an empty folder", async () => {
		const fs: FileSystem = {
			async list() {
				throw new Error("permission denied");
			},
			watch: () => () => {},
		};
		render(<WorkDirectoryFiles fileSystem={fs} workspacePath="/work/repo" />);
		expect(
			await screen.findByText("Could not read this folder"),
		).toBeInTheDocument();
	});
});

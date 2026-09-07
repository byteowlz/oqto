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
				return this.classList?.contains("wb-files-rows") ||
					this.classList?.contains("wb-files")
					? value
					: 0;
			},
		});
	}
	const original = HTMLElement.prototype.getBoundingClientRect;
	HTMLElement.prototype.getBoundingClientRect = function boundingRect() {
		if (
			this.classList?.contains("wb-files-rows") ||
			this.classList?.contains("wb-files")
		) {
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

const NO_OPS = {
	async read() {
		return "";
	},
	async rename() {},
	async createDirectory() {},
	async remove() {},
};

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
		...NO_OPS,
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
	const rows = view.container.querySelector(".wb-files-body") as HTMLElement;
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
			...NO_OPS,
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

describe("Quick Look and operations", () => {
	function actionFs() {
		const ops: string[] = [];
		const fs: FileSystem = {
			async list(_workspace, path) {
				if (path !== "") return [];
				return [
					{
						path: "notes.md",
						name: "notes.md",
						directory: false,
						symlink: false,
						size: 12,
						modifiedAt: 0,
					},
					{
						path: "pics",
						name: "pics",
						directory: true,
						symlink: false,
						size: 0,
						modifiedAt: 0,
					},
				];
			},
			watch: () => () => {},
			async read(_workspace, path) {
				ops.push(`read:${path}`);
				return "# notes\nbody";
			},
			async rename(_workspace, from, to) {
				ops.push(`rename:${from}->${to}`);
			},
			async createDirectory(_workspace, path) {
				ops.push(`mkdir:${path}`);
			},
			async remove(_workspace, path, recursive) {
				ops.push(`remove:${path}:${recursive}`);
			},
		};
		return { fs, ops };
	}

	it("previews a text file on Space and closes it again", async () => {
		const { fs, ops } = actionFs();
		const view = render(
			<WorkDirectoryFiles fileSystem={fs} workspacePath="/w" />,
		);
		await screen.findByText("notes.md");
		const rows = view.container.querySelector(".wb-files-body") as HTMLElement;
		// Directories sort first, so step onto the file before previewing.
		fireEvent.keyDown(rows, { key: "j" });
		fireEvent.keyDown(rows, { key: " " });
		await screen.findByText(/# notes/);
		expect(ops).toContain("read:notes.md");
		fireEvent.keyDown(rows, { key: " " });
		expect(screen.queryByText(/# notes/)).toBeNull();
	});

	it("says so instead of previewing a directory", async () => {
		const { fs } = actionFs();
		const view = render(
			<WorkDirectoryFiles fileSystem={fs} workspacePath="/w" />,
		);
		await screen.findByText("pics");
		const rows = view.container.querySelector(".wb-files-body") as HTMLElement;
		fireEvent.keyDown(rows, { key: " " });
		expect(
			await screen.findByText("No preview for this kind"),
		).toBeInTheDocument();
	});

	it("renames from the action line and offers an undo that reverses it", async () => {
		const { fs, ops } = actionFs();
		const view = render(
			<WorkDirectoryFiles fileSystem={fs} workspacePath="/w" />,
		);
		await screen.findByText("notes.md");
		const rows = view.container.querySelector(".wb-files-body") as HTMLElement;
		fireEvent.keyDown(rows, { key: "j" });
		fireEvent.keyDown(rows, { key: "r" });
		const input = screen.getByLabelText("Rename");
		expect((input as HTMLInputElement).value).toBe("notes.md");
		fireEvent.change(input, { target: { value: "todo.md" } });
		fireEvent.keyDown(input, { key: "Enter" });
		await waitFor(() => expect(ops).toContain("rename:notes.md->todo.md"));
		const undo = await screen.findByRole("button", { name: "Undo" });
		fireEvent.click(undo);
		await waitFor(() => expect(ops).toContain("rename:todo.md->notes.md"));
	});

	it("creates a folder in the current directory", async () => {
		const { fs, ops } = actionFs();
		const view = render(
			<WorkDirectoryFiles fileSystem={fs} workspacePath="/w" />,
		);
		await screen.findByText("notes.md");
		const rows = view.container.querySelector(".wb-files-body") as HTMLElement;
		fireEvent.keyDown(rows, { key: "n", ctrlKey: true });
		const input = screen.getByLabelText("Folder name");
		fireEvent.change(input, { target: { value: "drafts" } });
		fireEvent.keyDown(input, { key: "Enter" });
		await waitFor(() => expect(ops).toContain("mkdir:drafts"));
	});

	it("asks before deleting and does nothing when cancelled", async () => {
		const { fs, ops } = actionFs();
		const view = render(
			<WorkDirectoryFiles fileSystem={fs} workspacePath="/w" />,
		);
		await screen.findByText("notes.md");
		const rows = view.container.querySelector(".wb-files-body") as HTMLElement;
		fireEvent.keyDown(rows, { key: "j" });
		fireEvent.keyDown(rows, { key: "Delete" });
		expect(await screen.findByText("Delete notes.md?")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
		expect(ops.some((op) => op.startsWith("remove:"))).toBe(false);
		fireEvent.keyDown(rows, { key: "Delete" });
		fireEvent.click(screen.getByRole("button", { name: "Delete" }));
		await waitFor(() => expect(ops).toContain("remove:notes.md:false"));
		// Deletion is not reversible through the host contract.
		expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
	});

	it("reports a failed operation instead of pretending it worked", async () => {
		const { fs } = actionFs();
		const failing: FileSystem = {
			...fs,
			async rename() {
				throw new Error("read-only file system");
			},
		};
		const view = render(
			<WorkDirectoryFiles fileSystem={failing} workspacePath="/w" />,
		);
		await screen.findByText("notes.md");
		const rows = view.container.querySelector(".wb-files-body") as HTMLElement;
		fireEvent.keyDown(rows, { key: "j" });
		fireEvent.keyDown(rows, { key: "r" });
		fireEvent.keyDown(screen.getByLabelText("Rename"), { key: "Enter" });
		expect(
			await screen.findByText("read-only file system"),
		).toBeInTheDocument();
	});
});

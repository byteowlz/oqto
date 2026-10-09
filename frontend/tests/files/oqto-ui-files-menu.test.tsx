import { WorkDirectoryFiles } from "@/src/oqto-ui/files/WorkDirectoryFiles";
import { fileMenu, mediaTypeOf, menuSubjects } from "@/src/oqto-ui/files/menu";
import { initialState } from "@/src/oqto-ui/files/navigator";
import type {
	ActionHost,
	ResourceSubject,
} from "@/src/oqto-ui/platform/actions-contract";
import type { FileHost } from "@/src/oqto-ui/platform/files-contract";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { initI18n } from "../../lib/i18n";

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
	async copy() {},
	async move() {},
};

function entry(name: string, directory = false) {
	return {
		path: name,
		name,
		directory,
		symlink: false,
		size: 10,
		modifiedAt: 0,
	};
}

const ENTRIES = [entry("src", true), entry("notes.md"), entry("logo.png")];

function state() {
	return {
		...initialState(),
		listings: { "": { status: "ready" as const, entries: ENTRIES } },
	};
}

function fileHost(): FileHost {
	return {
		async list() {
			return ENTRIES;
		},
		watch() {
			return () => {};
		},
		async read() {
			return "";
		},
		async write() {},
		async rename() {},
		async createDirectory() {},
		async remove() {},
		async copy() {},
		async move() {},
	};
}

describe("resource menu model", () => {
	it("offers what applies to an entry, and only directory commands on empty space", () => {
		const onEntry = fileMenu(state(), "notes.md", []).flatMap((group) =>
			group.items.map((item) => item.command),
		);
		expect(onEntry).toEqual([
			"open",
			"rename",
			"copy",
			"cut",
			"createFolder",
			"delete",
		]);
		const onEmpty = fileMenu(state(), null, []).flatMap((group) =>
			group.items.map((item) => item.command),
		);
		expect(onEmpty).toEqual(["createFolder"]);
	});

	it("offers paste only when something is yanked", () => {
		const yanked = {
			...state(),
			clipboard: { paths: ["notes.md"], mode: "copy" as const },
		};
		const commands = fileMenu(yanked, null, []).flatMap((group) =>
			group.items.map((item) => item.command),
		);
		expect(commands).toContain("paste");
	});

	it("offers a copy into each other work directory, on an entry only", () => {
		const destinations = [
			{ path: "/work/other", name: "other" },
			{ path: "/work/third", name: "third" },
		];
		// One opener, not one item per work directory.
		const onEntry = fileMenu(state(), "notes.md", [], destinations);
		const copyTo = onEntry.find((group) => group.id === "copyTo");
		expect(copyTo?.items).toHaveLength(1);
		expect(copyTo?.items[0].destination).toBeUndefined();
		const picking = fileMenu(
			state(),
			"notes.md",
			[],
			destinations,
			"destinations",
		);
		expect(picking[0].items.map((item) => item.destination?.name)).toEqual([
			"other",
			"third",
		]);
		// Empty space has nothing selected to copy.
		expect(
			fileMenu(state(), null, [], destinations).some(
				(group) => group.id === "copyTo",
			),
		).toBe(false);
	});

	it("keeps contributed Actions in their own group, after the host's", () => {
		const groups = fileMenu(state(), "notes.md", [
			{ id: "canvas.image.annotate", title: "Annotate in Canvas" },
		]);
		const contributed = groups.find((group) => group.id === "contributed");
		expect(contributed?.items[0]).toMatchObject({
			actionId: "canvas.image.annotate",
			title: "Annotate in Canvas",
			command: null,
		});
		// Contributed Actions never sit above the pane's own commands.
		expect(
			groups.indexOf(contributed as (typeof groups)[number]),
		).toBeGreaterThan(0);
	});

	it("describes subjects by type, and takes the selection when the target is in it", () => {
		const selected = { ...state(), selection: ["notes.md", "logo.png"] };
		const subjects = menuSubjects(selected, "/work", "notes.md");
		expect(
			subjects.map((subject: ResourceSubject) => subject.reference),
		).toEqual(["notes.md", "logo.png"]);
		expect(subjects[1]).toMatchObject({
			kind: "workspace-file",
			workspacePath: "/work",
			mediaType: "image/*",
			label: "logo.png",
		});
		// A target outside the selection acts on itself alone.
		expect(menuSubjects(selected, "/work", "src")).toHaveLength(1);
		expect(mediaTypeOf("src", true)).toBe("inode/directory");
	});
});

describe("resource menu surface", () => {
	function actionHost(offers: { id: string; title: string }[]): ActionHost {
		return {
			async offers() {
				return offers;
			},
			async invoke() {},
		};
	}

	it("opens on right-click and shows contributed Actions with the host's own", async () => {
		render(
			<WorkDirectoryFiles
				fileHost={fileHost()}
				workspacePath="/work"
				actionHost={actionHost([
					{ id: "canvas.image.annotate", title: "Annotate in Canvas" },
				])}
			/>,
		);
		const row = await screen.findByText("notes.md");
		fireEvent.contextMenu(row);
		expect(screen.getByRole("menu")).toBeInTheDocument();
		expect(
			screen.getByRole("menuitem", { name: "Rename" }),
		).toBeInTheDocument();
		expect(
			await screen.findByRole("menuitem", { name: "Annotate in Canvas" }),
		).toBeInTheDocument();
	});

	it("asks the Broker to run a contributed Action on the subjects it opened for", async () => {
		const invoke = vi.fn(async () => {});
		render(
			<WorkDirectoryFiles
				fileHost={fileHost()}
				workspacePath="/work"
				actionHost={{
					async offers() {
						return [{ id: "canvas.image.annotate", title: "Annotate" }];
					},
					invoke,
				}}
			/>,
		);
		fireEvent.contextMenu(await screen.findByText("logo.png"));
		fireEvent.click(await screen.findByRole("menuitem", { name: "Annotate" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("canvas.image.annotate", [
				expect.objectContaining({
					reference: "logo.png",
					mediaType: "image/*",
					kind: "workspace-file",
				}),
			]),
		);
		expect(screen.queryByRole("menu")).toBeNull();
	});

	it("opens from the keyboard on the cursor row and closes on Escape", async () => {
		const view = render(
			<WorkDirectoryFiles fileHost={fileHost()} workspacePath="/work" />,
		);
		const body = view.container.querySelector(".wb-files-body") as HTMLElement;
		await screen.findByText("notes.md");
		fireEvent.click(screen.getByText("notes.md"));
		fireEvent.keyDown(body, { key: "m" });
		expect(screen.getByRole("menu")).toBeInTheDocument();
		fireEvent.keyDown(body, { key: "Escape" });
		expect(screen.queryByRole("menu")).toBeNull();
	});

	it("copies the selection into another work directory and reports it", async () => {
		const copyToWorkspace = vi.fn(async () => 3);
		render(
			<WorkDirectoryFiles
				fileHost={{ ...fileHost(), copyToWorkspace }}
				workspacePath="/work"
				destinations={[{ path: "/work/other", name: "other" }]}
			/>,
		);
		fireEvent.contextMenu(await screen.findByText("notes.md"));
		fireEvent.click(screen.getByRole("menuitem", { name: "Copy to…" }));
		fireEvent.click(await screen.findByRole("menuitem", { name: "other" }));
		await waitFor(() =>
			expect(copyToWorkspace).toHaveBeenCalledWith(
				"/work",
				"notes.md",
				"/work/other",
				"notes.md",
			),
		);
		expect(
			await screen.findByText("Copied 3 files to other"),
		).toBeInTheDocument();
	});

	it("runs a host command through the pane's own engine", async () => {
		render(<WorkDirectoryFiles fileHost={fileHost()} workspacePath="/work" />);
		fireEvent.contextMenu(await screen.findByText("notes.md"));
		fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
		expect(screen.queryByRole("menu")).toBeNull();
		// The rename line opens on the entry the menu pointed at.
		expect(await screen.findByLabelText("Rename")).toHaveValue("notes.md");
	});
});

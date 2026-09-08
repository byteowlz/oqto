import { WorkDirectoryFiles } from "@/src/oqto-ui/files/WorkDirectoryFiles";
import {
	paneFidelity,
	showsColumns,
	showsFacts,
} from "@/src/oqto-ui/files/fidelity";
import type { FileHost } from "@/src/oqto-ui/platform/files-contract";
import { render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";
import { initI18n } from "../../lib/i18n";

initI18n();

/** jsdom has no layout; give the pane and its columns a width. */
let paneWidth = 320;
beforeAll(() => {
	Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
		configurable: true,
		get() {
			return this.classList?.contains("wb-files") ? paneWidth : 320;
		},
	});
	Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
		configurable: true,
		get() {
			return 640;
		},
	});
	const original = HTMLElement.prototype.getBoundingClientRect;
	HTMLElement.prototype.getBoundingClientRect = function rect() {
		if (this.classList?.contains("wb-files")) {
			return {
				width: paneWidth,
				height: 640,
				x: 0,
				y: 0,
				top: 0,
				left: 0,
				right: paneWidth,
				bottom: 640,
				toJSON: () => ({}),
			} as DOMRect;
		}
		if (this.classList?.contains("wb-files-rows")) {
			return {
				width: 320,
				height: 640,
				x: 0,
				y: 0,
				top: 0,
				left: 0,
				right: 320,
				bottom: 640,
				toJSON: () => ({}),
			} as DOMRect;
		}
		return original.call(this);
	};
});

const fs: FileHost = {
	async list(_workspace, path) {
		if (path === "") {
			return [
				{
					path: "src",
					name: "src",
					directory: true,
					symlink: false,
					size: 0,
					modifiedAt: 1_700_000_000_000,
				},
				{
					path: "readme.md",
					name: "readme.md",
					directory: false,
					symlink: false,
					size: 2048,
					modifiedAt: 1_700_000_000_000,
				},
			];
		}
		return [
			{
				path: "src/app.ts",
				name: "app.ts",
				directory: false,
				symlink: false,
				size: 10,
				modifiedAt: 0,
			},
		];
	},
	watch: () => () => {},
	async read() {
		return "contents";
	},
	async rename() {},
	async createDirectory() {},
	async remove() {},
	async copy() {},
	async move() {},
};

describe("files fidelity", () => {
	it("derives the mode from the allocated width", () => {
		expect(paneFidelity(300)).toBe("compact");
		expect(paneFidelity(500)).toBe("standard");
		expect(paneFidelity(900)).toBe("expanded");
		expect(showsFacts("compact")).toBe(false);
		expect(showsFacts("standard")).toBe(true);
		expect(showsColumns("standard")).toBe(false);
		expect(showsColumns("expanded")).toBe(true);
	});

	it("renders one column in a narrow Container", async () => {
		paneWidth = 320;
		const view = render(
			<WorkDirectoryFiles fileHost={fs} workspacePath="/w" />,
		);
		await screen.findByText("readme.md");
		expect(view.container.querySelector(".wb-files")).toHaveAttribute(
			"data-fidelity",
			"compact",
		);
		expect(view.container.querySelector(".wb-files-columns")).toBeNull();
	});

	it("earns Miller columns in a wide Container, showing parent, current, and preview", async () => {
		paneWidth = 900;
		const view = render(
			<WorkDirectoryFiles fileHost={fs} workspacePath="/w" />,
		);
		await screen.findByText("readme.md");
		expect(view.container.querySelector(".wb-files")).toHaveAttribute(
			"data-fidelity",
			"expanded",
		);
		const columns = view.container.querySelectorAll(".wb-files-column");
		// At the root there is no parent column: current plus preview.
		expect(columns).toHaveLength(2);
		expect(
			view.container.querySelector('[data-role="preview"]'),
		).not.toBeNull();
	});
});

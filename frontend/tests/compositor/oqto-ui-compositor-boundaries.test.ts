import {
	type LayoutSnapshot,
	createClassicPresetLayout,
} from "@/src/oqto-ui/compositor/index";
import { resizeBoundaries } from "@/src/oqto-ui/compositor/queries";
import { describe, expect, it } from "vitest";
import {
	activeArrangementOf,
	chatContent,
	filesContent,
	sessionsContent,
	terminalContent,
} from "./fixtures";

function withStatus(): LayoutSnapshot {
	return createClassicPresetLayout({
		navigation: [sessionsContent],
		primary: [chatContent],
		auxiliary: [filesContent],
		status: [terminalContent],
	});
}

function boundaries(snapshot: LayoutSnapshot, axis: "inline" | "block") {
	return resizeBoundaries(
		activeArrangementOf(snapshot),
		snapshot.containers,
		axis,
	);
}

describe("resize boundaries", () => {
	it("finds no row boundary when the second row is only the status Lane", () => {
		// The sidebar spans both rows and the status Lane spans the rest, so the
		// row boundary separates nothing that can be dragged.
		expect(boundaries(withStatus(), "block")).toEqual([]);
	});

	it("stops a column boundary above the status Lane", () => {
		// Row 0 only: at row 1 the sidebar meets chrome, and the status Lane
		// spans the primary and auxiliary columns as one Container.
		expect(boundaries(withStatus(), "inline")).toEqual([
			{ index: 0, start: 0, end: 0 },
			{ index: 1, start: 0, end: 0 },
		]);
	});

	it("spans the whole column when every row really is divided", () => {
		const snapshot = createClassicPresetLayout({
			navigation: [sessionsContent],
			primary: [chatContent],
			auxiliary: [filesContent],
		});
		expect(boundaries(snapshot, "inline")).toEqual([
			{ index: 0, start: 0, end: 0 },
			{ index: 1, start: 0, end: 0 },
		]);
		expect(boundaries(snapshot, "block")).toEqual([]);
	});

	it("reports each contiguous run of a partially divided boundary", () => {
		const snapshot = withStatus();
		const arrangement = activeArrangementOf(snapshot);
		// Three rows: the sidebar spans rows 0-1 but not the new row 2, so the
		// first column boundary divides row 2 as its own run.
		const grid = {
			rows: [...arrangement.grid.rows, { id: arrangement.grid.rows[0].id }],
			columns: arrangement.grid.columns,
			placements: [
				...arrangement.grid.placements,
				{
					containerId: arrangement.grid.placements[1].containerId,
					row: 2,
					column: 0,
					rowSpan: 1,
					colSpan: 1,
				},
				{
					containerId: arrangement.grid.placements[2].containerId,
					row: 2,
					column: 1,
					rowSpan: 1,
					colSpan: 2,
				},
			],
		};
		expect(
			resizeBoundaries({ ...arrangement, grid }, snapshot.containers, "inline"),
		).toEqual([
			{ index: 0, start: 0, end: 0 },
			{ index: 0, start: 2, end: 2 },
			{ index: 1, start: 0, end: 0 },
		]);
	});
});

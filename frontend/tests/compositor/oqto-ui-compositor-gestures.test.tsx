import type { ContentRef } from "@/src/oqto-ui/compositor/index";
import { solveLayoutGeometry } from "@/src/oqto-ui/compositor/index";
import { CompositorHost } from "@/src/oqto-ui/compositor/react/CompositorHost";
import {
	CONTENT_DRAG_TYPE,
	dropCommands,
	dropZoneAt,
	edgeDropCommands,
	resizeCommands,
	scrollByColumns,
} from "@/src/oqto-ui/compositor/react/gestures";
import { createCompositorStore } from "@/src/oqto-ui/compositor/react/store";
import { act, createEvent, fireEvent, render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
	LABELS,
	VIEWPORT,
	activeArrangementOf,
	chatContent,
	classicLayout,
	containerByRole,
	filesContent,
	gitContent,
} from "./fixtures";

const BOX = { x: 100, y: 50, width: 400, height: 200 };
const DESKTOP = { ...VIEWPORT, responsive: "scroll" as const };

function dataTransfer(contentId: string) {
	return {
		types: [CONTENT_DRAG_TYPE],
		getData: (type: string) => (type === CONTENT_DRAG_TYPE ? contentId : ""),
		setData: () => {},
		effectAllowed: "move",
	};
}

function mount(viewport = DESKTOP) {
	const store = createCompositorStore(classicLayout());
	const renderContent = (content: ContentRef) => (
		<output data-testid={`content-${content.id}`}>{content.id}</output>
	);
	const view = render(
		<CompositorHost
			store={store}
			viewport={viewport}
			renderContent={renderContent}
			contentLabel={(content) => content.id}
			labels={LABELS}
		/>,
	);
	return { store, view };
}

function section(
	view: ReturnType<typeof render>,
	containerId: string,
): HTMLElement {
	const element = view.container.querySelector(
		`[data-container-id="${containerId}"]`,
	) as HTMLElement;
	element.getBoundingClientRect = () => ({
		...BOX,
		top: BOX.y,
		left: BOX.x,
		right: BOX.x + BOX.width,
		bottom: BOX.y + BOX.height,
		toJSON: () => ({}),
	});
	return element;
}

describe("pure gesture helpers", () => {
	it("hit-tests drop zones: center tabs, outer quarters split", () => {
		expect(dropZoneAt(BOX, 300, 150)).toBe("center");
		expect(dropZoneAt(BOX, 110, 150)).toBe("inline-start");
		expect(dropZoneAt(BOX, 490, 150)).toBe("inline-end");
		expect(dropZoneAt(BOX, 300, 55)).toBe("block-start");
		expect(dropZoneAt(BOX, 300, 245)).toBe("block-end");
		expect(dropZoneAt({ ...BOX, width: 0 }, 300, 150)).toBe("center");
	});

	it("maps zones to move or split commands", () => {
		const layout = classicLayout();
		const primaryId = containerByRole(layout, "primary").id;
		expect(dropCommands(filesContent.id, primaryId, "center")).toEqual([
			{
				type: "move",
				contentId: filesContent.id,
				destination: { containerId: primaryId },
			},
		]);
		expect(dropCommands(filesContent.id, primaryId, "block-end")).toEqual([
			{
				type: "split",
				contentId: filesContent.id,
				relativeTo: primaryId,
				edge: "block-end",
			},
		]);
		expect(edgeDropCommands(filesContent.id, "block-start")).toEqual([
			{ type: "split", contentId: filesContent.id, edge: "block-start" },
		]);
	});

	it("resizes adjacent fraction tracks by re-weighting and honors minima", () => {
		const layout = classicLayout();
		const arrangement = activeArrangementOf(layout);
		const geometry = solveLayoutGeometry(layout, VIEWPORT);
		const commands = resizeCommands(arrangement, geometry, "inline", 1, 100);
		expect(commands).toHaveLength(2);
		const [a, b] = commands;
		if (a.type !== "resize" || b.type !== "resize")
			throw new Error("expected resize commands");
		expect(a.size.unit).toBe("fraction");
		expect(a.size.value + b.size.value).toBeCloseTo(3, 6);
		expect(a.size.value).toBeGreaterThan(2);
		expect(a.size.min).toBe(360);
		// Pushing far past the neighbor's minimum clamps at that minimum.
		const clamped = resizeCommands(arrangement, geometry, "inline", 1, 5000);
		const [, right] = clamped;
		if (right.type !== "resize") throw new Error("expected resize");
		expect(right.size.value).toBeCloseTo((280 * 3) / (1600 - 320), 6);
		expect(resizeCommands(arrangement, geometry, "inline", 1, 0)).toEqual([]);
	});

	it("resizes a fixed track by its new logical length only", () => {
		const layout = classicLayout();
		const commands = resizeCommands(
			activeArrangementOf(layout),
			solveLayoutGeometry(layout, VIEWPORT),
			"inline",
			0,
			50,
		);
		expect(commands).toEqual([
			{
				type: "resize",
				containerId: containerByRole(layout, "navigation").id,
				axis: "inline",
				size: { unit: "fixed", value: 370, min: 240 },
			},
		]);
	});

	it("scrolls by whole columns within bounds", () => {
		const arrangement = activeArrangementOf(classicLayout());
		expect(scrollByColumns(arrangement, -1)).toBeNull();
		expect(scrollByColumns(arrangement, 1)).toEqual({
			type: "scroll",
			anchorColumnId: arrangement.grid.columns[1].id,
		});
		expect(
			scrollByColumns(
				{
					...arrangement,
					scrollAnchorColumnId: arrangement.grid.columns[1].id,
				},
				-1,
			),
		).toEqual({ type: "scroll", anchorColumnId: null });
	});
});

describe("CompositorHost gestures", () => {
	it("dropping a tab on a Container's center tabs it there", () => {
		const { store, view } = mount();
		act(() => {
			store.commit([
				{ type: "open", content: gitContent, target: { role: "auxiliary" } },
			]);
		});
		const primary = section(
			view,
			containerByRole(store.getSnapshot(), "primary").id,
		);
		fireEvent.dragOver(primary, {
			clientX: 300,
			clientY: 150,
			dataTransfer: dataTransfer(gitContent.id),
		});
		expect(primary).toHaveAttribute("data-drop", "center");
		fireEvent.drop(primary, {
			clientX: 300,
			clientY: 150,
			dataTransfer: dataTransfer(gitContent.id),
		});
		expect(
			containerByRole(store.getSnapshot(), "primary").stack.map((c) => c.id),
		).toEqual([chatContent.id, gitContent.id]);
		expect(primary).not.toHaveAttribute("data-drop");
	});

	it("dropping near a Container edge splits beside it", () => {
		const { store, view } = mount();
		const primary = section(
			view,
			containerByRole(store.getSnapshot(), "primary").id,
		);
		// jsdom has no DragEvent, so pointer coordinates must be attached by hand.
		const drop = createEvent.drop(primary, {
			dataTransfer: dataTransfer(filesContent.id),
		});
		Object.defineProperty(drop, "clientX", { value: 300 });
		Object.defineProperty(drop, "clientY", { value: 245 });
		fireEvent(primary, drop);
		const grid = activeArrangementOf(store.getSnapshot()).grid;
		expect(grid.rows).toHaveLength(2);
		expect(
			grid.placements.find(
				(p) =>
					p.containerId ===
					containerByRole(store.getSnapshot(), "navigation").id,
			)?.rowSpan,
		).toBe(2);
	});

	it("dropping on the viewport bottom edge creates a full-width Lane", () => {
		const { store, view } = mount();
		const root = view.container.querySelector(
			".oqto-compositor",
		) as HTMLElement;
		fireEvent.dragOver(root, { dataTransfer: dataTransfer(filesContent.id) });
		expect(root).toHaveAttribute("data-dragging");
		const edge = view.getByLabelText(LABELS.dropBottom);
		fireEvent.drop(edge, { dataTransfer: dataTransfer(filesContent.id) });
		const grid = activeArrangementOf(store.getSnapshot()).grid;
		expect(grid.rows).toHaveLength(2);
		expect(grid.placements.find((p) => p.row === 1)?.colSpan).toBe(
			grid.columns.length,
		);
		expect(root).not.toHaveAttribute("data-dragging");
	});

	it("gutter drags commit semantic resizes on release", () => {
		const { store, view } = mount();
		const gutters = view.getAllByLabelText(LABELS.resizeColumns);
		expect(gutters).toHaveLength(2);
		const gutter = gutters[1];
		gutter.setPointerCapture = () => {};
		fireEvent.pointerDown(gutter, { clientX: 1000, pointerId: 1 });
		fireEvent.pointerUp(gutter, { clientX: 1100, pointerId: 1 });
		const columns = activeArrangementOf(store.getSnapshot()).grid.columns;
		expect(columns[1].size.value).toBeGreaterThan(2);
		expect(columns[2].size.value).toBeLessThan(1);
		expect(store.getSnapshot().revision).toBe(1);
	});

	it("horizontal wheel scrolls by one column, debounced", () => {
		const { store, view } = mount();
		const body = view.container.querySelector(
			".oqto-compositor-body",
		) as HTMLElement;
		fireEvent.wheel(body, { deltaX: 120, deltaY: 0, timeStamp: 1000 });
		expect(activeArrangementOf(store.getSnapshot()).scrollAnchorColumnId).toBe(
			activeArrangementOf(store.getSnapshot()).grid.columns[1].id,
		);
		fireEvent.wheel(body, { deltaX: 120, deltaY: 0, timeStamp: 1050 });
		expect(activeArrangementOf(store.getSnapshot()).scrollAnchorColumnId).toBe(
			activeArrangementOf(store.getSnapshot()).grid.columns[1].id,
		);
		fireEvent.wheel(body, { deltaX: 10, deltaY: 300, timeStamp: 2000 });
		expect(store.getSnapshot().revision).toBe(1);
	});

	it("hides gutters while the projection has merged columns away", () => {
		const { view } = mount({
			inlineSize: 600,
			blockSize: 800,
			responsive: "merge",
		});
		expect(view.queryAllByLabelText(LABELS.resizeColumns)).toHaveLength(0);
	});
});

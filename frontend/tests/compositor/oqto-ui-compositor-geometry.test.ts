import {
	type LayoutCommand,
	type LayoutSnapshot,
	type SolvedRect,
	applyTransaction,
	contentIdFrom,
	solveLayoutGeometry,
} from "@/src/oqto-ui/compositor/index";
import { describe, expect, it } from "vitest";
import { VIEWPORT, activeArrangementOf, chatContent, classicLayout, containerByRole, filesContent, terminalContent } from "./fixtures";

function rectOf(rects: readonly SolvedRect[], containerId: string): SolvedRect {
	const rect = rects.find((candidate) => candidate.containerId === containerId);
	if (!rect) throw new Error(`no rect for ${containerId}`);
	return rect;
}

function assertSane(rects: readonly SolvedRect[]) {
	for (const rect of rects) {
		expect(rect.inlineSize).toBeGreaterThanOrEqual(0);
		expect(rect.blockSize).toBeGreaterThanOrEqual(0);
		expect(rect.y).toBeGreaterThanOrEqual(0);
	}
	const visible = rects.filter((rect) => !rect.collapsed);
	for (const a of visible) {
		for (const b of visible) {
			if (a === b) continue;
			const inlineOverlap = a.x < b.x + b.inlineSize && b.x < a.x + a.inlineSize;
			const blockOverlap = a.y < b.y + b.blockSize && b.y < a.y + a.blockSize;
			expect(inlineOverlap && blockOverlap, `${a.containerId} overlaps ${b.containerId}`).toBe(false);
		}
	}
}

function step(snapshot: LayoutSnapshot, ...commands: LayoutCommand[]): LayoutSnapshot {
	const result = applyTransaction(snapshot, { expectedRevision: snapshot.revision, commands, viewport: VIEWPORT });
	if (!result.ok) throw new Error(JSON.stringify(result.rejection));
	return result.snapshot;
}

describe("semantic grid geometry (v2)", () => {
	it("solves the classic preset: fixed nav, 2:1 primary/auxiliary, full height", () => {
		const layout = classicLayout();
		const { rects, degradations, columnSizes } = solveLayoutGeometry(layout, VIEWPORT);
		expect(degradations).toEqual([]);
		expect(columnSizes[0]).toBe(320);
		expect(columnSizes[1]).toBeCloseTo((1600 - 320) * (2 / 3), 3);
		expect(columnSizes[2]).toBeCloseTo((1600 - 320) * (1 / 3), 3);
		for (const role of ["navigation", "primary", "auxiliary"]) {
			const rect = rectOf(rects, containerByRole(layout, role).id);
			expect(rect.y).toBe(0);
			expect(rect.blockSize).toBe(900);
		}
		assertSane(rects);
	});

	it("respects host safe areas", () => {
		const layout = classicLayout();
		const { rects } = solveLayoutGeometry(layout, { ...VIEWPORT, safeArea: { top: 24, right: 8, bottom: 16, left: 8 } });
		const nav = rectOf(rects, containerByRole(layout, "navigation").id);
		expect(nav.x).toBe(8);
		expect(nav.y).toBe(24);
		expect(nav.blockSize).toBe(900 - 24 - 16);
	});

	it("collapse preserves the Container without consuming its track", () => {
		const layout = classicLayout();
		const navId = containerByRole(layout, "navigation").id;
		const collapsed = step(layout, { type: "collapse", containerId: navId, collapsed: true });
		const { rects, columnSizes } = solveLayoutGeometry(collapsed, VIEWPORT);
		expect(rectOf(rects, navId)).toMatchObject({ collapsed: true, inlineSize: 0 });
		expect(columnSizes[1]).toBeCloseTo(1600 * (2 / 3), 3);
	});

	it("spans: a block split shares the column and the nav keeps full height", () => {
		const layout = classicLayout();
		const primaryId = containerByRole(layout, "primary").id;
		const split = step(layout, { type: "split", contentId: filesContent.id, relativeTo: primaryId, edge: "block-end" });
		const { rects } = solveLayoutGeometry(split, VIEWPORT);
		const primary = rectOf(rects, primaryId);
		const nav = rectOf(rects, containerByRole(layout, "navigation").id);
		expect(nav.blockSize).toBe(900);
		const created = rects.find((r) => r.x === primary.x && r.y > primary.y);
		expect(primary.blockSize + (created?.blockSize ?? 0)).toBeCloseTo(900, 3);
		assertSane(rects);
	});

	it("a flush bottom Lane spans the screen width, is pinned, and ignores scrolling", () => {
		let layout = step(classicLayout(), { type: "open", content: terminalContent, target: { role: "auxiliary" } });
		layout = step(layout, { type: "split", contentId: terminalContent.id, edge: "block-end" });
		const lane = activeArrangementOf(layout).grid.rows[1];
		layout = step(layout, { type: "flush", rowId: lane.id, flush: true });
		const laneContainer = activeArrangementOf(layout).grid.placements.find((p) => p.row === 1)?.containerId as string;
		layout = step(layout, { type: "resize", containerId: containerByRole(layout, "primary").id, axis: "inline", size: { unit: "fixed", value: 2000 } });
		layout = step(layout, { type: "scroll", anchorColumnId: activeArrangementOf(layout).grid.columns[2].id });
		const solved = solveLayoutGeometry(layout, VIEWPORT);
		const laneRect = rectOf(solved.rects, laneContainer);
		expect(laneRect.flush).toBe(true);
		expect(laneRect.x).toBe(0);
		expect(laneRect.inlineSize).toBe(1600);
		expect(laneRect.y + laneRect.blockSize).toBe(900);
		expect(solved.scrollOffset).toBeGreaterThan(0);
		expect(rectOf(solved.rects, containerByRole(layout, "navigation").id).x).toBeLessThan(0);
	});

	it("scroll policy honors minima and reports overflow instead of scaling", () => {
		const layout = classicLayout();
		const scroll = solveLayoutGeometry(layout, { inlineSize: 500, blockSize: 400 });
		expect(scroll.degradations).toContainEqual({ kind: "overflow", axis: "inline", extent: expect.any(Number) });
		expect(scroll.inlineExtent).toBeGreaterThan(500);
		expect(scroll.columnSizes[0]).toBe(320);
		const fit = solveLayoutGeometry(layout, { inlineSize: 500, blockSize: 400, overflow: "fit" });
		expect(fit.degradations).toContainEqual({ kind: "minima-unsatisfiable", axis: "inline", deficit: expect.any(Number) });
		expect(fit.columnSizes.reduce((a, b) => a + b, 0)).toBeLessThanOrEqual(500 + 0.001);
		assertSane(fit.rects);
	});

	it("derives geometry purely and independently of focus", () => {
		const layout = classicLayout();
		const focused = step(layout, { type: "focus", contentId: chatContent.id });
		expect(solveLayoutGeometry(focused, VIEWPORT)).toEqual(solveLayoutGeometry(layout, VIEWPORT));
		expect(layout).toEqual(classicLayout());
	});

	it("scroll anchors beyond the extent clamp to the last screenful", () => {
		let layout = classicLayout();
		layout = step(layout, { type: "open", content: { id: contentIdFrom("x"), kind: "chat" } });
		layout = step(layout, { type: "split", contentId: contentIdFrom("x"), edge: "inline-end" });
		const last = activeArrangementOf(layout).grid.columns.at(-1)?.id as never;
		layout = step(layout, { type: "scroll", anchorColumnId: last });
		const solved = solveLayoutGeometry(layout, VIEWPORT);
		expect(solved.scrollOffset).toBe(Math.max(0, solved.inlineExtent - 1600));
	});
});

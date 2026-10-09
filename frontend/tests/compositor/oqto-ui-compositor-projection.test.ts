import {
	type LayoutCommand,
	type LayoutSnapshot,
	applyTransaction,
	projectLayout,
} from "@/src/oqto-ui/compositor/index";
import { checkLayoutInvariants } from "@/src/oqto-ui/compositor/kernel/invariants";
import { describe, expect, it } from "vitest";
import {
	VIEWPORT,
	chatContent,
	classicLayout,
	containerByRole,
	filesContent,
	gitContent,
	terminalContent,
} from "./fixtures";
import { deepFreeze } from "./harness";

function step(
	snapshot: LayoutSnapshot,
	...commands: LayoutCommand[]
): LayoutSnapshot {
	const result = applyTransaction(snapshot, {
		expectedRevision: snapshot.revision,
		commands,
		viewport: VIEWPORT,
	});
	if (!result.ok) throw new Error(JSON.stringify(result.rejection));
	return result.snapshot;
}

describe("responsive projection ladder (ADR-0042)", () => {
	it("wide viewports project the canonical layout unchanged", () => {
		const layout = classicLayout();
		const projected = projectLayout(layout, {
			...VIEWPORT,
			responsive: "merge",
		});
		expect(projected.snapshot).toBe(layout);
		expect(projected.merges).toEqual([]);
		expect(projected.collapsedForSpace).toEqual([]);
	});

	it("collapses navigation first, then merges auxiliary into primary tabs", () => {
		const layout = deepFreeze(
			step(classicLayout(), {
				type: "open",
				content: gitContent,
				target: { role: "auxiliary" },
			}),
		);
		const navId = containerByRole(layout, "navigation").id;
		// 320 nav + 360 primary + 280 auxiliary = 960 minima.
		const stillFits = projectLayout(layout, {
			inlineSize: 700,
			blockSize: 800,
			responsive: "merge",
		});
		expect(stillFits.collapsedForSpace).toEqual([navId]);
		expect(stillFits.merges).toEqual([]);
		const merged = projectLayout(layout, {
			inlineSize: 600,
			blockSize: 800,
			responsive: "merge",
		});
		expect(merged.merges).toEqual([
			{
				containerId: containerByRole(layout, "auxiliary").id,
				into: containerByRole(layout, "primary").id,
				contentIds: [filesContent.id, gitContent.id],
			},
		]);
		const primary = containerByRole(merged.snapshot, "primary");
		expect(primary.stack.map((content) => content.id)).toEqual([
			chatContent.id,
			filesContent.id,
			gitContent.id,
		]);
		expect(primary.activeContentId).toBe(chatContent.id);
		expect(merged.snapshot.containers.some((c) => c.role === "auxiliary")).toBe(
			false,
		);
		expect(checkLayoutInvariants(merged.snapshot)).toEqual([]);
		expect(
			merged.geometry.degradations.some((d) => d.kind === "overflow"),
		).toBe(false);
		// Non-destructive: the canonical snapshot still has three Containers.
		expect(layout.containers).toHaveLength(3);
		expect(containerByRole(layout, "navigation").collapsed).toBe(false);
	});

	it("keeps the focused Content active when its Container merges", () => {
		const layout = step(classicLayout(), {
			type: "focus",
			contentId: filesContent.id,
		});
		const merged = projectLayout(layout, {
			inlineSize: 600,
			blockSize: 800,
			responsive: "merge",
		});
		expect(containerByRole(merged.snapshot, "primary").activeContentId).toBe(
			filesContent.id,
		);
	});

	it("lanes persist longer than side-by-side columns", () => {
		let layout = step(classicLayout(), {
			type: "open",
			content: terminalContent,
			target: { role: "auxiliary" },
		});
		layout = step(layout, {
			type: "split",
			contentId: terminalContent.id,
			edge: "block-end",
		});
		const merged = projectLayout(layout, {
			inlineSize: 600,
			blockSize: 800,
			responsive: "merge",
		});
		const laneContainer = layout.arrangements[0].grid.placements.find(
			(p) => p.row === 1,
		)?.containerId;
		expect(merged.snapshot.containers.some((c) => c.id === laneContainer)).toBe(
			true,
		);
		expect(merged.snapshot.arrangements[0].grid.rows).toHaveLength(2);
	});

	it("falls back to scaling only when nothing is left to merge", () => {
		const layout = classicLayout();
		const tiny = projectLayout(layout, {
			inlineSize: 200,
			blockSize: 400,
			responsive: "merge",
		});
		expect(tiny.merges).toHaveLength(1);
		expect(tiny.geometry.degradations).toContainEqual({
			kind: "minima-unsatisfiable",
			axis: "inline",
			deficit: expect.any(Number),
		});
		expect(
			tiny.geometry.columnSizes.reduce((a, b) => a + b, 0),
		).toBeLessThanOrEqual(200 + 0.001);
	});

	it("scroll policy never merges", () => {
		const layout = classicLayout();
		const projected = projectLayout(layout, {
			inlineSize: 600,
			blockSize: 800,
			responsive: "scroll",
		});
		expect(projected.merges).toEqual([]);
		expect(
			projected.geometry.degradations.some((d) => d.kind === "overflow"),
		).toBe(true);
	});

	it("is deterministic and reversible across a width sweep", () => {
		const layout = deepFreeze(
			step(classicLayout(), {
				type: "open",
				content: gitContent,
				target: { role: "auxiliary" },
			}),
		);
		const encoded = JSON.stringify(layout);
		for (const inlineSize of [1600, 900, 600, 300, 600, 900, 1600]) {
			const a = projectLayout(layout, {
				inlineSize,
				blockSize: 800,
				responsive: "merge",
			});
			const b = projectLayout(layout, {
				inlineSize,
				blockSize: 800,
				responsive: "merge",
			});
			expect(a).toEqual(b);
		}
		expect(
			projectLayout(layout, { ...VIEWPORT, responsive: "merge" }).merges,
		).toEqual([]);
		expect(JSON.stringify(layout)).toBe(encoded);
	});
});

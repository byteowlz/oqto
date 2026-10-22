import { describe, expect, it } from "vitest";
import { resourceLayout } from "../../src/oqto-ui/dev/policy-editor/layout";
import { resources } from "../../src/oqto-ui/dev/policy-editor/resources";

describe("resource layout geometry", () => {
	it("spans ownership bars across their workdirs, including expanded rows", () => {
		const { boxes } = resourceLayout(new Set(["oqto"]), "horizontal");
		const first = boxes.get("oqto");
		const personal = boxes.get("ws-personal");
		const you = boxes.get("account");
		expect(personal?.y).toBe(first?.y);
		expect(you?.y).toBe(first?.y);
		const workdirs = resources.filter((item) => item.kind === "workdir");
		for (const item of workdirs) {
			const child = boxes.get(item.id);
			const owner = boxes.get(item.parent ?? "");
			if (!child || !owner || !you)
				throw new Error("Missing ownership geometry");
			expect(owner.y).toBeLessThanOrEqual(child.y);
			expect(owner.y + owner.h).toBeGreaterThanOrEqual(child.y + child.h);
			expect(you.y + you.h).toBeGreaterThanOrEqual(child.y + child.h);
		}
	});
	for (const direction of ["horizontal", "vertical"] as const) {
		it(`keeps ${direction} nodes separate and inside the canvas`, () => {
			const layout = resourceLayout(
				new Set(["oqto", "svc-kyz", "wiki-eng"]),
				direction,
			);
			const boxes = [...layout.boxes.values()];
			for (const box of boxes) {
				expect(box.x).toBeGreaterThanOrEqual(0);
				expect(box.y).toBeGreaterThanOrEqual(0);
				expect(box.x + box.w).toBeLessThanOrEqual(layout.width);
				expect(box.y + box.h).toBeLessThanOrEqual(layout.height);
			}
			for (let i = 0; i < boxes.length; i++) {
				for (let j = i + 1; j < boxes.length; j++) {
					const a = boxes[i];
					const b = boxes[j];
					expect(
						a.x + a.w <= b.x ||
							b.x + b.w <= a.x ||
							a.y + a.h <= b.y ||
							b.y + b.h <= a.y,
					).toBe(true);
				}
			}
		});
	}
});

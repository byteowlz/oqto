import {
	type ArrangementId,
	type ContainerId,
	type ContentId,
	type LayoutCommand,
	type LayoutSnapshot,
	applyTransaction,
	contentIdFrom,
	projectLayout,
} from "@/src/oqto-ui/compositor/index";
import { checkLayoutInvariants } from "@/src/oqto-ui/compositor/kernel/invariants";
import {
	decodeLayoutDocument,
	encodeLayoutDocument,
} from "@/src/oqto-ui/compositor/kernel/persistence/codec";
import { describe, expect, it } from "vitest";
import { VIEWPORT, classicLayout } from "./fixtures";
import { type SeededRng, deepFreeze, runGeneratedSequences } from "./harness";

interface Model {
	snapshot: LayoutSnapshot;
	history: LayoutSnapshot[];
	minted: number;
}

interface Step {
	command:
		| LayoutCommand
		| { type: "serialize-restore" }
		| { type: "stale" }
		| { type: "project" };
	expectation: "accept" | "reject" | "either";
}

const KINDS = ["chat", "files", "git", "terminal", "app", "hologram"];
const EDGES = [
	"inline-start",
	"inline-end",
	"block-start",
	"block-end",
] as const;

function placedIds(snapshot: LayoutSnapshot): ContentId[] {
	return snapshot.containers.flatMap((container) =>
		container.stack.map((content) => content.id),
	);
}

function containerIds(snapshot: LayoutSnapshot): ContainerId[] {
	return snapshot.containers.map((container) => container.id);
}

function activeGrid(snapshot: LayoutSnapshot) {
	return snapshot.arrangements.find(
		(a) => a.id === snapshot.activeArrangementId,
	)?.grid;
}

function generateStep(rng: SeededRng, model: Model): Step {
	const s = model.snapshot;
	const placed = placedIds(s);
	const containers = containerIds(s);
	const arrangements = s.arrangements.map((a) => a.id);
	const roll = rng.int(100);

	if (roll < 12) {
		const content = {
			id: contentIdFrom(`generated-${model.minted}`),
			kind: rng.pick(KINDS),
		};
		const r = rng.int(4);
		const target =
			r === 0 && containers.length > 0
				? { containerId: rng.pick(containers), position: rng.int(4) }
				: r === 1
					? { role: rng.pick(["primary", "auxiliary", "navigation", "lane"]) }
					: r === 2
						? { arrangementId: rng.pick(arrangements) }
						: undefined;
		return {
			command: { type: "open", content, ...(target ? { target } : {}) },
			expectation: "accept",
		};
	}
	if (roll < 17 && placed.length > 0) {
		const container = rng.pick(s.containers.filter((c) => c.stack.length > 0));
		const existing = rng.pick(container.stack);
		return {
			command: {
				type: "open",
				content: { id: existing.id, kind: existing.kind },
			},
			expectation: "accept",
		};
	}
	if (roll < 22) {
		const known = placed.length > 0 && rng.bool(0.7);
		return {
			command: {
				type: "reveal",
				contentId: known ? rng.pick(placed) : contentIdFrom(`ghost-${roll}`),
			},
			expectation: known ? "accept" : "reject",
		};
	}
	if (roll < 27 && placed.length > 0) {
		return {
			command: {
				type: rng.bool() ? "activate" : "focus",
				contentId: rng.pick(placed),
			},
			expectation: "accept",
		};
	}
	if (roll < 35 && placed.length > 0) {
		const destination = rng.bool(0.7)
			? { containerId: rng.pick(containers), position: rng.int(5) }
			: { arrangementId: rng.pick(arrangements) };
		return {
			command: { type: "move", contentId: rng.pick(placed), destination },
			expectation: "accept",
		};
	}
	if (roll < 43 && placed.length > 0) {
		const edge = rng.pick(EDGES);
		const relative = rng.bool(0.6) && containers.length > 0;
		return {
			command: {
				type: "split",
				contentId: rng.pick(placed),
				edge,
				...(relative ? { relativeTo: rng.pick(containers) } : {}),
			},
			expectation: "either",
		};
	}
	if (roll < 49 && containers.length > 0) {
		const valid = rng.bool(0.8);
		return {
			command: {
				type: "resize",
				containerId: rng.pick(containers),
				axis: rng.bool() ? "inline" : "block",
				size: valid
					? { unit: rng.bool() ? "fraction" : "fixed", value: 1 + rng.int(500) }
					: { unit: "fraction", value: -rng.int(3) },
			},
			expectation: valid ? "either" : "reject",
		};
	}
	if (roll < 54 && containers.length > 0) {
		return {
			command: {
				type: "collapse",
				containerId: rng.pick(containers),
				collapsed: rng.bool(),
			},
			expectation: "accept",
		};
	}
	if (roll < 58) {
		const grid = activeGrid(s);
		if (grid && grid.rows.length > 0) {
			return {
				command: {
					type: "flush",
					rowId: rng.pick(grid.rows).id,
					flush: rng.bool(0.7),
				},
				expectation: "either",
			};
		}
	}
	if (roll < 62) {
		const grid = activeGrid(s);
		const anchor =
			grid && grid.columns.length > 0 && rng.bool(0.8)
				? rng.pick(grid.columns).id
				: null;
		return {
			command: { type: "scroll", anchorColumnId: anchor },
			expectation: "accept",
		};
	}
	if (roll < 72 && placed.length > 0) {
		return {
			command: { type: "close", contentId: rng.pick(placed) },
			expectation: "accept",
		};
	}
	if (roll < 75) {
		return {
			command: {
				type: "arrangement-create",
				...(rng.bool()
					? { binding: { kind: "work-directory", id: `wd-${roll}` } }
					: {}),
				activate: rng.bool(),
			},
			expectation: "accept",
		};
	}
	if (roll < 79) {
		return {
			command: {
				type: "arrangement-switch",
				arrangementId: rng.pick(arrangements),
			},
			expectation: "accept",
		};
	}
	if (roll < 82) {
		return {
			command: {
				type: "arrangement-remove",
				arrangementId: rng.pick(arrangements),
			},
			expectation: arrangements.length > 1 ? "accept" : "reject",
		};
	}
	if (roll < 84) {
		return {
			command: {
				type: "workspace-switch",
				workspace: rng.bool()
					? { kind: "all" }
					: { kind: "workspace", id: `ws-${roll}` },
			},
			expectation: "accept",
		};
	}
	if (roll < 88 && model.history.length > 0) {
		return {
			command: { type: "undo", snapshot: rng.pick(model.history) },
			expectation: "accept",
		};
	}
	if (roll < 93)
		return { command: { type: "serialize-restore" }, expectation: "accept" };
	if (roll < 97) return { command: { type: "project" }, expectation: "accept" };
	return { command: { type: "stale" }, expectation: "reject" };
}

function applyStep(model: Model, step: Step): Model {
	const before = model.snapshot;

	if (step.command.type === "serialize-restore") {
		const decoded = decodeLayoutDocument(encodeLayoutDocument(before));
		if (!decoded.ok) throw new Error(`round trip failed: ${decoded.detail}`);
		expect(encodeLayoutDocument(decoded.snapshot)).toBe(
			encodeLayoutDocument(before),
		);
		expect(decoded.snapshot).toEqual(before);
		return model;
	}
	if (step.command.type === "project") {
		// Responsive projection is pure and non-destructive: the canonical
		// snapshot is untouched (frozen), placed Content is preserved, and the
		// projection is deterministic across widths.
		const encodedBefore = encodeLayoutDocument(before);
		for (const inlineSize of [1600, 1100, 700, 420]) {
			const projected = projectLayout(before, {
				inlineSize,
				blockSize: 800,
				responsive: "merge",
			});
			expect(checkLayoutInvariants(projected.snapshot)).toEqual([]);
			expect([...placedIds(projected.snapshot)].sort()).toEqual(
				[...placedIds(before)].sort(),
			);
			expect(
				projected.geometry.degradations.some((d) => d.kind === "overflow"),
			).toBe(false);
			expect(
				projectLayout(before, {
					inlineSize,
					blockSize: 800,
					responsive: "merge",
				}),
			).toEqual(projected);
		}
		expect(encodeLayoutDocument(before)).toBe(encodedBefore);
		return model;
	}
	if (step.command.type === "stale") {
		const result = applyTransaction(before, {
			expectedRevision: before.revision + 1 + (before.revision % 3),
			commands: [],
		});
		if (result.ok) throw new Error("stale transaction must conflict");
		expect(result.rejection.reason).toBe("revision-conflict");
		return model;
	}

	const command = step.command;
	const placedBefore = placedIds(before);
	const result = applyTransaction(before, {
		expectedRevision: before.revision,
		commands: [command],
		viewport: VIEWPORT,
	});

	if (!result.ok) {
		if (step.expectation === "accept")
			throw new Error(
				`expected acceptance, got ${JSON.stringify(result.rejection)}`,
			);
		expect(result.rejection.reason).toMatch(
			/^(revision-conflict|not-placed|unknown-content|unknown-container|unknown-arrangement|last-arrangement|invalid-command|invariant-violation)$/,
		);
		expect(JSON.parse(JSON.stringify(result.rejection))).toEqual(
			result.rejection,
		);
		return model;
	}
	if (step.expectation === "reject")
		throw new Error(`expected rejection for ${JSON.stringify(command)}`);

	const after = result.snapshot;
	const violations = checkLayoutInvariants(after);
	if (violations.length > 0)
		throw new Error(`invariants violated: ${JSON.stringify(violations)}`);
	expect(after.revision).toBe(before.revision + 1);
	const wire = { command, events: result.events, snapshot: after };
	expect(JSON.parse(JSON.stringify(wire))).toEqual(wire);

	const placedAfter = placedIds(after);
	if (command.type === "open") {
		const wasPlaced = placedBefore.includes(command.content.id);
		expect(placedAfter.length).toBe(placedBefore.length + (wasPlaced ? 0 : 1));
		expect(placedAfter).toContain(command.content.id);
	}
	if (command.type === "close") {
		expect(placedAfter.length).toBe(placedBefore.length - 1);
		expect(placedAfter).not.toContain(command.contentId);
	}
	if (
		[
			"move",
			"split",
			"reveal",
			"activate",
			"focus",
			"collapse",
			"resize",
			"flush",
			"scroll",
			"arrangement-create",
			"arrangement-switch",
			"workspace-switch",
		].includes(command.type)
	) {
		expect([...placedAfter].sort()).toEqual([...placedBefore].sort());
	}
	if (command.type === "undo") {
		expect(after.containers).toEqual(command.snapshot.containers);
		expect(after.arrangements).toEqual(command.snapshot.arrangements);
	}
	if (command.type === "arrangement-remove") {
		expect(after.arrangements.some((a) => a.id === command.arrangementId)).toBe(
			false,
		);
	}
	if (command.type === "reveal") {
		const container = after.containers.find((c) =>
			c.stack.some((content) => content.id === command.contentId),
		);
		expect(container?.activeContentId).toBe(command.contentId);
		expect(container?.collapsed).toBe(false);
		const owner = after.arrangements.find((a) =>
			a.grid.placements.some((p) => p.containerId === container?.id),
		);
		expect(owner?.id as ArrangementId).toBe(after.activeArrangementId);
	}

	deepFreeze(after);
	const minted =
		command.type === "open" && command.content.id.startsWith("generated-")
			? model.minted + 1
			: model.minted;
	return {
		snapshot: after,
		history: [...model.history.slice(-9), before],
		minted,
	};
}

describe("compositor model properties", () => {
	it("preserves every invariant across generated command sequences", () => {
		runGeneratedSequences<Model, Step>({
			seeds: Array.from({ length: 30 }, (_, index) => index + 1),
			stepsPerSeed: 80,
			initialModel: () => ({
				snapshot: deepFreeze(classicLayout()),
				history: [],
				minted: 0,
			}),
			generateStep,
			applyStep: (model, step) => applyStep(model, step),
		});
	});

	it("meets the representative performance budget fixed before implementation", () => {
		// Budget (oqto-a9j4.9): 400 generated transactions on a growing
		// many-Container, multi-Arrangement layout apply, validate invariants,
		// project, and round-trip in under 2000 ms.
		const start = performance.now();
		runGeneratedSequences<Model, Step>({
			seeds: [42],
			stepsPerSeed: 400,
			initialModel: () => ({
				snapshot: deepFreeze(classicLayout()),
				history: [],
				minted: 0,
			}),
			generateStep,
			applyStep: (model, step) => applyStep(model, step),
		});
		expect(performance.now() - start).toBeLessThan(2000);
	});
});

/**
 * Deterministic conformance-trace builder for the OqtoUI compositor
 * (ADR-0037 `contracts/oqto-ui/traces/`, ADR-0041/0042/0043 kernel, schema
 * v2). The TypeScript kernel is the reference implementation: this builder
 * replays curated and seeded command plans through the public seam and
 * records the typed outcomes. The committed JSON corpus — not this script —
 * is the contract a second host (e.g. GPUI/Rust) must reproduce.
 *
 * Regenerate with: bun scripts/generate-compositor-traces.ts
 */

import {
	type ApplyResult,
	type ContentRef,
	type LayoutCommand,
	type LayoutSnapshot,
	type LayoutTransaction,
	type ProjectedLayout,
	type SolvedLayout,
	type ViewportClass,
	type ViewportConstraints,
	applyTransaction,
	contentIdFrom,
	createClassicPresetLayout,
	encodeLayoutDocument,
	projectLayout,
	recoverLayoutDocument,
	solveLayoutGeometry,
} from "../../src/oqto-ui/compositor/index";
import { SeededRng } from "./seeded-rng";

export const TRACE_FORMAT_VERSION = 2;

export interface TransitionStep {
	readonly transaction: LayoutTransaction;
	readonly expected: ApplyResult;
}

interface TraceHeader {
	readonly formatVersion: number;
	readonly schemaVersion: number;
	readonly name: string;
	readonly description: string;
}

export interface TransitionTrace extends TraceHeader {
	readonly format: "oqto-compositor-transitions";
	readonly initial: LayoutSnapshot;
	readonly steps: readonly TransitionStep[];
}

export interface GeometryScenario {
	readonly name: string;
	readonly snapshot: LayoutSnapshot;
	readonly cases: readonly {
		readonly name: string;
		readonly viewport: ViewportConstraints;
		readonly expected: SolvedLayout;
	}[];
}

export interface GeometryTrace extends TraceHeader {
	readonly format: "oqto-compositor-geometry";
	readonly scenarios: readonly GeometryScenario[];
}

export interface ProjectionScenario {
	readonly name: string;
	readonly snapshot: LayoutSnapshot;
	readonly cases: readonly {
		readonly name: string;
		readonly viewport: ViewportClass;
		readonly expected: ProjectedLayout;
	}[];
}

export interface ProjectionTrace extends TraceHeader {
	readonly format: "oqto-compositor-projection";
	readonly scenarios: readonly ProjectionScenario[];
}

export interface PersistenceDocument {
	readonly name: string;
	readonly raw: string;
	readonly expected:
		| { readonly ok: true; readonly snapshot: LayoutSnapshot }
		| { readonly ok: false; readonly reason: string };
}

export interface PersistenceTrace extends TraceHeader {
	readonly format: "oqto-compositor-persistence";
	readonly documents: readonly PersistenceDocument[];
}

export type CompositorTrace =
	| TransitionTrace
	| GeometryTrace
	| ProjectionTrace
	| PersistenceTrace;

export interface TraceFile {
	readonly path: string;
	readonly data: CompositorTrace;
}

const VIEWPORT: ViewportConstraints = { inlineSize: 1600, blockSize: 900 };
const sessionsContent: ContentRef = {
	id: contentIdFrom("sessions:catalog"),
	kind: "sessions",
};
const chatContent: ContentRef = {
	id: contentIdFrom("chat:session-1"),
	kind: "chat",
};
const filesContent: ContentRef = {
	id: contentIdFrom("files:workdir-1"),
	kind: "files",
};
const gitContent: ContentRef = {
	id: contentIdFrom("git:workdir-1"),
	kind: "git",
};
const terminalContent: ContentRef = {
	id: contentIdFrom("terminal:workdir-1"),
	kind: "terminal",
};

function classic(): LayoutSnapshot {
	return createClassicPresetLayout({
		navigation: [sessionsContent],
		primary: [chatContent],
		auxiliary: [filesContent],
	});
}

function header(
	name: string,
	description: string,
	schemaVersion: number,
): TraceHeader {
	return {
		formatVersion: TRACE_FORMAT_VERSION,
		schemaVersion,
		name,
		description,
	};
}

type PlanEntry = (state: LayoutSnapshot) => {
	readonly commands: readonly LayoutCommand[];
	readonly expectedRevision?: number;
	readonly viewport?: ViewportConstraints;
};

function record(
	name: string,
	description: string,
	initial: LayoutSnapshot,
	plan: readonly PlanEntry[],
): TransitionTrace {
	let state = initial;
	const steps: TransitionStep[] = [];
	for (const entry of plan) {
		const { commands, expectedRevision, viewport } = entry(state);
		const transaction: LayoutTransaction = {
			expectedRevision: expectedRevision ?? state.revision,
			commands,
			...(viewport ? { viewport } : {}),
		};
		const expected = applyTransaction(state, transaction);
		steps.push({ transaction, expected });
		if (expected.ok) state = expected.snapshot;
	}
	return {
		format: "oqto-compositor-transitions",
		...header(name, description, initial.schemaVersion),
		initial,
		steps,
	};
}

function byRole(state: LayoutSnapshot, role: string) {
	const container = state.containers.find(
		(candidate) => candidate.role === role,
	);
	if (!container) throw new Error(`trace builder: no ${role} container`);
	return container;
}

function activeGrid(state: LayoutSnapshot) {
	const arrangement = state.arrangements.find(
		(a) => a.id === state.activeArrangementId,
	);
	if (!arrangement) throw new Error("trace builder: no active arrangement");
	return arrangement.grid;
}

function mustApply(
	state: LayoutSnapshot,
	commands: readonly LayoutCommand[],
	viewport = VIEWPORT,
): LayoutSnapshot {
	const result = applyTransaction(state, {
		expectedRevision: state.revision,
		commands,
		viewport,
	});
	if (!result.ok)
		throw new Error(
			`trace builder setup failed: ${JSON.stringify(result.rejection)}`,
		);
	return result.snapshot;
}

function classicLifecycleTrace(): TransitionTrace {
	return record(
		"classic-lifecycle",
		"Open/tab/reveal/activate/focus/collapse/close over the classic preset, including idempotent open, typed not-placed reveal, empty retain/collapse behavior, and deterministic focus handoff.",
		classic(),
		[
			() => ({
				commands: [
					{ type: "open", content: gitContent, target: { role: "auxiliary" } },
				],
			}),
			() => ({ commands: [{ type: "open", content: chatContent }] }),
			() => ({
				commands: [
					{ type: "reveal", contentId: contentIdFrom("chat:never-opened") },
				],
			}),
			() => ({ commands: [{ type: "activate", contentId: filesContent.id }] }),
			() => ({ commands: [{ type: "focus", contentId: filesContent.id }] }),
			(s) => ({
				commands: [
					{
						type: "collapse",
						containerId: byRole(s, "navigation").id,
						collapsed: true,
					},
				],
			}),
			() => ({ commands: [{ type: "reveal", contentId: sessionsContent.id }] }),
			() => ({ commands: [{ type: "close", contentId: gitContent.id }] }),
			() => ({ commands: [{ type: "close", contentId: sessionsContent.id }] }),
			() => ({ commands: [{ type: "close", contentId: chatContent.id }] }),
			(s) => ({
				commands: [
					{
						type: "open",
						content: chatContent,
						target: { containerId: byRole(s, "primary").id },
					},
				],
			}),
		],
	);
}

function splitsMovesTrace(): TransitionTrace {
	return record(
		"splits-and-moves",
		"Container-relative splits growing spanning neighbors, cross-Container moves, in-Container reordering, semantic resize, remove empty behavior with grid repair, and typed unknown-target rejections.",
		classic(),
		[
			() => ({
				commands: [
					{ type: "open", content: gitContent, target: { role: "auxiliary" } },
				],
			}),
			(s) => ({
				commands: [
					{
						type: "split",
						contentId: filesContent.id,
						relativeTo: byRole(s, "primary").id,
						edge: "inline-end",
					},
				],
			}),
			(s) => ({
				commands: [
					{
						type: "split",
						contentId: gitContent.id,
						relativeTo: byRole(s, "auxiliary").id,
						edge: "block-end",
					},
				],
			}),
			(s) => ({
				commands: [
					{
						type: "move",
						contentId: chatContent.id,
						destination: { containerId: byRole(s, "auxiliary").id },
					},
				],
			}),
			(s) => ({
				commands: [
					{
						type: "move",
						contentId: chatContent.id,
						destination: {
							containerId: byRole(s, "auxiliary").id,
							position: 0,
						},
					},
				],
			}),
			(s) => ({
				commands: [
					{
						type: "resize",
						containerId: byRole(s, "auxiliary").id,
						axis: "inline",
						size: { unit: "fraction", value: 3, min: 300 },
					},
				],
			}),
			() => ({ commands: [{ type: "close", contentId: filesContent.id }] }),
			(s) => ({
				commands: [
					{
						type: "resize",
						containerId: byRole(s, "primary").id,
						axis: "inline",
						size: { unit: "fraction", value: 0 },
					},
				],
			}),
			() => ({
				commands: [
					{
						type: "move",
						contentId: chatContent.id,
						destination: { containerId: "container-9999" as never },
					},
				],
			}),
		],
	);
}

function conflictsUndoTrace(): TransitionTrace {
	const initial = classic();
	return record(
		"conflicts-and-undo",
		"Stale-revision conflicts (newer state wins), undo to a captured prior snapshot with forward-moving revision, undo schema-version rejection, identity-reuse rejection, and unknown-content close.",
		initial,
		[
			() => ({
				commands: [
					{ type: "open", content: gitContent, target: { role: "auxiliary" } },
				],
			}),
			() => ({
				expectedRevision: 0,
				commands: [{ type: "close", contentId: chatContent.id }],
			}),
			() => ({ commands: [{ type: "undo", snapshot: initial }] }),
			() => ({
				commands: [
					{ type: "undo", snapshot: { ...initial, schemaVersion: 99 } },
				],
			}),
			() => ({
				commands: [
					{ type: "open", content: { id: chatContent.id, kind: "files" } },
				],
			}),
			() => ({
				commands: [
					{ type: "close", contentId: contentIdFrom("chat:never-opened") },
				],
			}),
		],
	);
}

function lanesScrollingTrace(): TransitionTrace {
	return record(
		"lanes-and-scrolling",
		"Edge splits creating full-width Lanes at the bottom and top, flush lanes (lane-level, edge-only, fixed size), wide fixed columns overflowing the screen, discrete scroll commands, and reveal settling the scroll anchor with and without a viewport.",
		classic(),
		[
			() => ({
				commands: [
					{
						type: "open",
						content: terminalContent,
						target: { role: "auxiliary" },
					},
				],
			}),
			() => ({
				commands: [
					{ type: "split", contentId: terminalContent.id, edge: "block-end" },
				],
			}),
			(s) => ({
				commands: [
					{ type: "flush", rowId: activeGrid(s).rows[1].id, flush: true },
				],
			}),
			(s) => ({
				commands: [
					{ type: "flush", rowId: activeGrid(s).rows[0].id, flush: true },
				],
			}),
			() => ({
				commands: [
					{ type: "split", contentId: gitContent.id, edge: "block-start" },
				],
			}),
			() => ({
				commands: [
					{ type: "open", content: gitContent, target: { role: "auxiliary" } },
				],
			}),
			() => ({
				commands: [
					{ type: "split", contentId: gitContent.id, edge: "block-start" },
				],
			}),
			(s) => ({
				commands: [
					{ type: "flush", rowId: activeGrid(s).rows[1].id, flush: true },
				],
			}),
			(s) => ({
				commands: [
					{
						type: "resize",
						containerId: byRole(s, "primary").id,
						axis: "inline",
						size: { unit: "fixed", value: 1400 },
					},
				],
			}),
			(s) => ({
				commands: [
					{ type: "scroll", anchorColumnId: activeGrid(s).columns[2].id },
				],
			}),
			() => ({
				commands: [{ type: "reveal", contentId: sessionsContent.id }],
				viewport: VIEWPORT,
			}),
			() => ({
				commands: [{ type: "reveal", contentId: filesContent.id }],
				viewport: VIEWPORT,
			}),
			() => ({ commands: [{ type: "reveal", contentId: sessionsContent.id }] }),
			() => ({
				commands: [{ type: "scroll", anchorColumnId: "track-404" as never }],
			}),
			(s) => ({
				commands: [
					{
						type: "resize",
						containerId: byRole(s, "primary").id,
						axis: "inline",
						size: { unit: "fraction", value: 2, min: 360 },
					},
				],
			}),
		],
	);
}

function arrangementsTrace(): TransitionTrace {
	return record(
		"arrangements-and-workspaces",
		"Arrangement create/switch/remove with per-Arrangement focus memory, work-directory binding as data, moving Content across Arrangements without switching, reveal teleporting across Arrangements, last-Arrangement refusal, and active-Workspace switching as opaque data.",
		classic(),
		[
			() => ({
				commands: [
					{
						type: "arrangement-create",
						label: "second",
						binding: { kind: "work-directory", id: "wd-2" },
						activate: true,
					},
				],
			}),
			() => ({ commands: [{ type: "open", content: gitContent }] }),
			(s) => ({
				commands: [
					{ type: "arrangement-switch", arrangementId: s.arrangements[0].id },
				],
			}),
			(s) => ({
				commands: [
					{
						type: "move",
						contentId: filesContent.id,
						destination: { arrangementId: s.arrangements[1].id },
					},
				],
			}),
			() => ({
				commands: [{ type: "reveal", contentId: gitContent.id }],
				viewport: VIEWPORT,
			}),
			() => ({
				commands: [
					{
						type: "workspace-switch",
						workspace: { kind: "workspace", id: "ws-1" },
					},
				],
			}),
			(s) => ({
				commands: [
					{ type: "arrangement-switch", arrangementId: s.arrangements[0].id },
				],
			}),
			(s) => ({
				commands: [
					{ type: "arrangement-remove", arrangementId: s.arrangements[1].id },
				],
			}),
			(s) => ({
				commands: [
					{ type: "arrangement-remove", arrangementId: s.arrangements[0].id },
				],
			}),
			() => ({
				commands: [
					{
						type: "arrangement-switch",
						arrangementId: "arrangement-404" as never,
					},
				],
			}),
			() => ({
				commands: [{ type: "workspace-switch", workspace: { kind: "all" } }],
			}),
		],
	);
}

const GENERATED_KINDS = ["chat", "files", "git", "terminal", "app"];
const EDGES = [
	"inline-start",
	"inline-end",
	"block-start",
	"block-end",
] as const;

function generatedTrace(seed: number, stepCount: number): TransitionTrace {
	const rng = new SeededRng(seed);
	const initial = classic();
	let state = initial;
	const history: LayoutSnapshot[] = [];
	let minted = 0;
	const steps: TransitionStep[] = [];

	for (let index = 0; index < stepCount; index += 1) {
		const placed = state.containers.flatMap((container) =>
			container.stack.map((content) => content.id),
		);
		const containers = state.containers.map((container) => container.id);
		const arrangements = state.arrangements.map(
			(arrangement) => arrangement.id,
		);
		const grid = activeGrid(state);
		const roll = rng.int(100);
		let command: LayoutCommand;
		let expectedRevision = state.revision;

		if (roll < 14 || placed.length === 0) {
			minted += 1;
			command = {
				type: "open",
				content: {
					id: contentIdFrom(`generated-${seed}-${minted}`),
					kind: rng.pick(GENERATED_KINDS),
				},
				...(containers.length > 0 && rng.bool(0.4)
					? { target: { containerId: rng.pick(containers) } }
					: {}),
			};
		} else if (roll < 20) {
			command = {
				type: "reveal",
				contentId: rng.bool(0.7)
					? rng.pick(placed)
					: contentIdFrom(`ghost-${index}`),
			};
		} else if (roll < 27) {
			command = {
				type: rng.bool() ? "activate" : "focus",
				contentId: rng.pick(placed),
			};
		} else if (roll < 37) {
			command = {
				type: "move",
				contentId: rng.pick(placed),
				destination: rng.bool(0.7)
					? { containerId: rng.pick(containers), position: rng.int(4) }
					: { arrangementId: rng.pick(arrangements) },
			};
		} else if (roll < 47) {
			command = {
				type: "split",
				contentId: rng.pick(placed),
				edge: rng.pick(EDGES),
				...(rng.bool(0.6) ? { relativeTo: rng.pick(containers) } : {}),
			};
		} else if (roll < 54) {
			command = {
				type: "resize",
				containerId: rng.pick(containers),
				axis: rng.bool() ? "inline" : "block",
				size: rng.bool(0.8)
					? { unit: rng.bool() ? "fraction" : "fixed", value: 1 + rng.int(300) }
					: { unit: "fraction", value: 0 },
			};
		} else if (roll < 60) {
			command = {
				type: "collapse",
				containerId: rng.pick(containers),
				collapsed: rng.bool(),
			};
		} else if (roll < 64 && grid.rows.length > 0) {
			command = {
				type: "flush",
				rowId: rng.pick(grid.rows).id,
				flush: rng.bool(0.7),
			};
		} else if (roll < 68) {
			command = {
				type: "scroll",
				anchorColumnId:
					grid.columns.length > 0 && rng.bool(0.8)
						? rng.pick(grid.columns).id
						: null,
			};
		} else if (roll < 78) {
			command = { type: "close", contentId: rng.pick(placed) };
		} else if (roll < 82) {
			command = {
				type: "arrangement-create",
				activate: rng.bool(),
				...(rng.bool()
					? { binding: { kind: "work-directory", id: `wd-${index}` } }
					: {}),
			};
		} else if (roll < 86) {
			command = {
				type: "arrangement-switch",
				arrangementId: rng.pick(arrangements),
			};
		} else if (roll < 89) {
			command = {
				type: "arrangement-remove",
				arrangementId: rng.pick(arrangements),
			};
		} else if (roll < 91) {
			command = {
				type: "workspace-switch",
				workspace: rng.bool()
					? { kind: "all" }
					: { kind: "workspace", id: `ws-${index}` },
			};
		} else if (roll < 96 && history.length > 0) {
			command = { type: "undo", snapshot: rng.pick(history) };
		} else {
			command = { type: "focus", contentId: rng.pick(placed) };
			expectedRevision = state.revision + 1;
		}

		const transaction: LayoutTransaction = {
			expectedRevision,
			commands: [command],
			viewport: VIEWPORT,
		};
		const expected = applyTransaction(state, transaction);
		steps.push({ transaction, expected });
		if (expected.ok) {
			history.push(state);
			if (history.length > 5) history.shift();
			state = expected.snapshot;
		}
	}

	return {
		format: "oqto-compositor-transitions",
		...header(
			`generated-seed-${seed}`,
			`Seeded (${seed}) generated command sequence over the classic preset; every step records the transaction and the reference kernel's typed outcome.`,
			initial.schemaVersion,
		),
		initial,
		steps,
	};
}

function representativeSnapshots(): readonly {
	name: string;
	snapshot: LayoutSnapshot;
}[] {
	const base = classic();
	const collapsedNav = mustApply(base, [
		{
			type: "collapse",
			containerId: byRole(base, "navigation").id,
			collapsed: true,
		},
	]);
	const blockSplit = mustApply(base, [
		{
			type: "split",
			contentId: filesContent.id,
			relativeTo: byRole(base, "primary").id,
			edge: "block-end",
		},
	]);
	let flushLane = mustApply(base, [
		{ type: "open", content: terminalContent, target: { role: "auxiliary" } },
		{ type: "split", contentId: terminalContent.id, edge: "block-end" },
	]);
	flushLane = mustApply(flushLane, [
		{ type: "flush", rowId: activeGrid(flushLane).rows[1].id, flush: true },
	]);
	flushLane = mustApply(flushLane, [
		{
			type: "resize",
			containerId: byRole(flushLane, "primary").id,
			axis: "inline",
			size: { unit: "fixed", value: 1400 },
		},
	]);
	flushLane = mustApply(flushLane, [
		{ type: "scroll", anchorColumnId: activeGrid(flushLane).columns[2].id },
	]);
	const tabbed = mustApply(base, [
		{ type: "open", content: gitContent, target: { role: "auxiliary" } },
		{ type: "focus", contentId: filesContent.id },
	]);
	return [
		{ name: "classic-preset", snapshot: base },
		{ name: "collapsed-navigation", snapshot: collapsedNav },
		{ name: "block-split-primary", snapshot: blockSplit },
		{ name: "flush-lane-scrolled", snapshot: flushLane },
		{ name: "auxiliary-tabs-focused", snapshot: tabbed },
	];
}

function geometryTrace(): GeometryTrace {
	const viewports: readonly { name: string; viewport: ViewportConstraints }[] =
		[
			{ name: "desktop", viewport: VIEWPORT },
			{
				name: "desktop-safe-area",
				viewport: {
					...VIEWPORT,
					safeArea: { top: 24, right: 8, bottom: 16, left: 8 },
				},
			},
			{ name: "narrow-scroll", viewport: { inlineSize: 500, blockSize: 400 } },
			{
				name: "narrow-fit",
				viewport: { inlineSize: 500, blockSize: 400, overflow: "fit" },
			},
			{ name: "tall", viewport: { inlineSize: 900, blockSize: 1400 } },
		];
	return {
		format: "oqto-compositor-geometry",
		...header(
			"geometry-grid",
			"Solved logical geometry (IEEE-754 doubles, solver operation order fixed by the corpus) for representative topologies and viewports: safe areas, spans, flush lanes, scroll offsets, overflow and fit policies.",
			2,
		),
		scenarios: representativeSnapshots().map(({ name, snapshot }) => ({
			name,
			snapshot,
			cases: viewports.map((entry) => ({
				...entry,
				expected: solveLayoutGeometry(snapshot, entry.viewport),
			})),
		})),
	};
}

function projectionTrace(): ProjectionTrace {
	const widths = [1600, 1100, 900, 700, 600, 420, 200];
	return {
		format: "oqto-compositor-projection",
		...header(
			"projection-ladder",
			"Non-destructive responsive projection (ADR-0042 ladder): per width, the projected snapshot, merge provenance, containers collapsed for space, and solved geometry; the canonical snapshot in each scenario is the input and is never mutated.",
			2,
		),
		scenarios: representativeSnapshots().map(({ name, snapshot }) => ({
			name,
			snapshot,
			cases: widths.flatMap((inlineSize) =>
				(["merge", "scroll"] as const).map((responsive) => {
					const viewport: ViewportClass = {
						inlineSize,
						blockSize: 800,
						responsive,
					};
					return {
						name: `${responsive}-${inlineSize}`,
						viewport,
						expected: projectLayout(snapshot, viewport),
					};
				}),
			),
		})),
	};
}

function persistenceTrace(): PersistenceTrace {
	const layout = classic();
	const valid = encodeLayoutDocument(layout);
	const mutate = (edit: (doc: ReturnType<typeof JSON.parse>) => void) => {
		const doc = JSON.parse(valid);
		edit(doc);
		return JSON.stringify(doc);
	};
	const v1 = JSON.stringify({
		schemaVersion: 1,
		revision: 3,
		idSeed: 8,
		strategy: "grid",
		focusedContentId: chatContent.id,
		grid: {
			columns: [
				{
					id: "column-1",
					size: { unit: "fixed", value: 320, min: 240 },
					cells: [
						{
							containerId: "container-0",
							size: { unit: "fraction", value: 1 },
						},
					],
				},
				{
					id: "column-3",
					size: { unit: "fraction", value: 2, min: 360 },
					cells: [
						{
							containerId: "container-2",
							size: { unit: "fraction", value: 1 },
						},
						{
							containerId: "container-6",
							size: { unit: "fraction", value: 1 },
						},
					],
				},
				{
					id: "column-5",
					size: { unit: "fraction", value: 1, min: 280 },
					cells: [
						{
							containerId: "container-4",
							size: { unit: "fraction", value: 1 },
						},
					],
				},
			],
		},
		containers: [
			{
				id: "container-0",
				role: "navigation",
				emptyBehavior: "collapse",
				collapsed: false,
				activeContentId: sessionsContent.id,
				stack: [sessionsContent],
			},
			{
				id: "container-2",
				role: "primary",
				emptyBehavior: "retain",
				collapsed: false,
				activeContentId: chatContent.id,
				stack: [chatContent],
			},
			{
				id: "container-6",
				emptyBehavior: "remove",
				collapsed: false,
				activeContentId: "terminal:x",
				stack: [{ id: "terminal:x", kind: "terminal" }],
			},
			{
				id: "container-4",
				role: "auxiliary",
				emptyBehavior: "retain",
				collapsed: false,
				activeContentId: filesContent.id,
				stack: [filesContent],
				futureField: 1,
			},
		],
		futureTopLevel: true,
	});
	const raws: readonly { name: string; raw: string }[] = [
		{ name: "classic-preset-valid", raw: valid },
		{
			name: "unknown-variants-preserved",
			raw: mutate((doc) => {
				doc.futureTopLevel = { anything: [1, 2, 3] };
				doc.arrangements[0].futureArrangementField = "kept";
				doc.containers[0].futureContainerField = "kept";
				doc.containers[1].stack[0].futureContentField = true;
				doc.containers[2].stack[0].kind = "kind-from-the-future";
			}),
		},
		{ name: "schema-v1-migrated", raw: v1 },
		{ name: "truncated-json", raw: '{"schemaVersion": 2, "rev' },
		{ name: "non-object-root", raw: "[1,2,3]" },
		{
			name: "missing-schema-version",
			raw: mutate((doc) => {
				doc.schemaVersion = undefined;
			}),
		},
		{
			name: "newer-schema-version",
			raw: mutate((doc) => {
				doc.schemaVersion = 99;
			}),
		},
		{
			name: "unknown-empty-behavior",
			raw: mutate((doc) => {
				doc.containers[0].emptyBehavior = "detonate";
			}),
		},
		{
			name: "malformed-active-workspace",
			raw: mutate((doc) => {
				doc.activeWorkspace = { kind: "galaxy" };
			}),
		},
		{
			name: "negative-track-size",
			raw: mutate((doc) => {
				doc.arrangements[0].grid.columns[0].size.value = -5;
			}),
		},
		{
			name: "overlapping-placements",
			raw: mutate((doc) => {
				doc.arrangements[0].grid.placements[1].column = 0;
			}),
		},
		{
			name: "active-content-not-in-stack",
			raw: mutate((doc) => {
				doc.containers[0].activeContentId = "nonexistent";
			}),
		},
		{
			name: "focus-on-unplaced-content",
			raw: mutate((doc) => {
				doc.focusedContentId = "nonexistent";
			}),
		},
		{
			name: "unknown-active-arrangement",
			raw: mutate((doc) => {
				doc.activeArrangementId = "arrangement-404";
			}),
		},
	];
	return {
		format: "oqto-compositor-persistence",
		...header(
			"persistence-documents",
			"Raw layout documents with the typed decode outcome a conforming host must produce: valid round trips (unknown variants preserved), a schema v1 document migrated to v2, and corrupted documents with their failure reasons.",
			2,
		),
		documents: raws.map(({ name, raw }) => {
			const recovered = recoverLayoutDocument([raw], layout);
			return {
				name,
				raw,
				expected:
					recovered.sourceIndex === 0
						? { ok: true, snapshot: recovered.snapshot }
						: { ok: false, reason: recovered.failures[0].reason },
			};
		}),
	};
}

export function buildCompositorTraceCorpus(): readonly TraceFile[] {
	return [
		{
			path: "transitions-classic-lifecycle.json",
			data: classicLifecycleTrace(),
		},
		{ path: "transitions-splits-moves.json", data: splitsMovesTrace() },
		{ path: "transitions-conflicts-undo.json", data: conflictsUndoTrace() },
		{ path: "transitions-lanes-scrolling.json", data: lanesScrollingTrace() },
		{
			path: "transitions-arrangements-workspaces.json",
			data: arrangementsTrace(),
		},
		{
			path: "transitions-generated-seed-11.json",
			data: generatedTrace(11, 30),
		},
		{
			path: "transitions-generated-seed-23.json",
			data: generatedTrace(23, 30),
		},
		{
			path: "transitions-generated-seed-37.json",
			data: generatedTrace(37, 30),
		},
		{ path: "geometry-grid.json", data: geometryTrace() },
		{ path: "projection-ladder.json", data: projectionTrace() },
		{ path: "persistence-documents.json", data: persistenceTrace() },
	];
}

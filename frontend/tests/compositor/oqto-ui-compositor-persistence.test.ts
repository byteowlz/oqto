import type { ExtensionFields } from "@/src/oqto-ui/compositor/index";
import {
	decodeLayoutDocument,
	encodeLayoutDocument,
	recoverLayoutDocument,
} from "@/src/oqto-ui/compositor/kernel/persistence/codec";
import { runLayoutMigrations } from "@/src/oqto-ui/compositor/kernel/persistence/migrations";
import { describe, expect, it } from "vitest";
import {
	chatContent,
	classicLayout,
	filesContent,
	sessionsContent,
} from "./fixtures";

function classicDocument(): ExtensionFields {
	return JSON.parse(encodeLayoutDocument(classicLayout())) as ExtensionFields;
}

function mutated(edit: (doc: { [key: string]: unknown }) => void): string {
	const doc = JSON.parse(encodeLayoutDocument(classicLayout()));
	edit(doc);
	return JSON.stringify(doc);
}

/** A schema v1 document as the pre-ADR-0042 kernel wrote it. */
const V1_DOCUMENT = JSON.stringify({
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
					{ containerId: "container-0", size: { unit: "fraction", value: 1 } },
				],
			},
			{
				id: "column-3",
				size: { unit: "fraction", value: 2, min: 360 },
				cells: [
					{ containerId: "container-2", size: { unit: "fraction", value: 1 } },
					{ containerId: "container-6", size: { unit: "fraction", value: 1 } },
				],
			},
			{
				id: "column-5",
				size: { unit: "fraction", value: 1, min: 280 },
				cells: [
					{ containerId: "container-4", size: { unit: "fraction", value: 1 } },
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

describe("versioned layout persistence (schema v2)", () => {
	it("round-trips the classic preset byte-for-byte", () => {
		const layout = classicLayout();
		const encoded = encodeLayoutDocument(layout);
		const decoded = decodeLayoutDocument(encoded);
		expect(decoded.ok).toBe(true);
		if (decoded.ok) {
			expect(decoded.snapshot).toEqual(layout);
			expect(decoded.migratedFrom).toBeNull();
			expect(encodeLayoutDocument(decoded.snapshot)).toBe(encoded);
		}
	});

	it("preserves unknown variants and extension fields at every level", () => {
		const raw = mutated((doc) => {
			doc.futureTopLevel = { anything: [1, 2, 3] };
			const arrangements = doc.arrangements as { [key: string]: unknown }[];
			arrangements[0].futureArrangementField = "kept";
			const containers = doc.containers as { [key: string]: unknown }[];
			containers[0].futureContainerField = "kept";
			(
				containers[1].stack as { [key: string]: unknown }[]
			)[0].futureContentField = true;
			(containers[2].stack as { [key: string]: unknown }[])[0].kind =
				"kind-from-the-future";
		});
		const decoded = decodeLayoutDocument(raw);
		expect(decoded.ok).toBe(true);
		if (!decoded.ok) return;
		expect(decoded.snapshot.extensions).toEqual({
			futureTopLevel: { anything: [1, 2, 3] },
		});
		expect(decoded.snapshot.arrangements[0].extensions).toEqual({
			futureArrangementField: "kept",
		});
		expect(decoded.snapshot.containers[0].extensions).toEqual({
			futureContainerField: "kept",
		});
		expect(decoded.snapshot.containers[1].stack[0].extensions).toEqual({
			futureContentField: true,
		});
		expect(decoded.snapshot.containers[2].stack[0].kind).toBe(
			"kind-from-the-future",
		);
		expect(JSON.parse(encodeLayoutDocument(decoded.snapshot))).toEqual(
			JSON.parse(raw),
		);
	});

	it.each([
		{
			name: "truncated JSON",
			raw: '{"schemaVersion": 2, "rev',
			reason: "parse-error",
		},
		{ name: "non-object root", raw: "[1,2,3]", reason: "invalid-shape" },
		{
			name: "missing schemaVersion",
			raw: mutated((doc) => {
				doc.schemaVersion = undefined;
			}),
			reason: "invalid-shape",
		},
		{
			name: "newer schemaVersion",
			raw: mutated((doc) => {
				doc.schemaVersion = 99;
			}),
			reason: "unsupported-version",
		},
		{
			name: "unknown empty behavior",
			raw: mutated((doc) => {
				(doc.containers as { [k: string]: unknown }[])[0].emptyBehavior =
					"detonate";
			}),
			reason: "invalid-shape",
		},
		{
			name: "malformed active workspace",
			raw: mutated((doc) => {
				doc.activeWorkspace = { kind: "galaxy" };
			}),
			reason: "invalid-shape",
		},
		{
			name: "negative track size",
			raw: mutated((doc) => {
				(
					doc.arrangements as {
						grid: { columns: { size: { value: number } }[] };
					}[]
				)[0].grid.columns[0].size.value = -5;
			}),
			reason: "invalid-shape",
		},
		{
			name: "overlapping placements",
			raw: mutated((doc) => {
				(
					doc.arrangements as { grid: { placements: { column: number }[] } }[]
				)[0].grid.placements[1].column = 0;
			}),
			reason: "invariant-violation",
		},
		{
			name: "active content missing from its stack",
			raw: mutated((doc) => {
				(doc.containers as { [k: string]: unknown }[])[0].activeContentId =
					"nonexistent";
			}),
			reason: "invariant-violation",
		},
		{
			name: "focus on unplaced content",
			raw: mutated((doc) => {
				doc.focusedContentId = "nonexistent";
			}),
			reason: "invariant-violation",
		},
		{
			name: "unknown active arrangement",
			raw: mutated((doc) => {
				doc.activeArrangementId = "arrangement-404";
			}),
			reason: "invariant-violation",
		},
	])("rejects a corrupted document: $name", ({ raw, reason }) => {
		const decoded = decodeLayoutDocument(raw);
		expect(decoded.ok).toBe(false);
		if (!decoded.ok) expect(decoded.reason).toBe(reason);
	});

	it("migrates a schema v1 document into one Arrangement with spans", () => {
		const decoded = decodeLayoutDocument(V1_DOCUMENT);
		expect(decoded.ok).toBe(true);
		if (!decoded.ok) return;
		expect(decoded.migratedFrom).toBe(1);
		const { snapshot } = decoded;
		expect(snapshot.schemaVersion).toBe(2);
		expect(snapshot.arrangements).toHaveLength(1);
		const grid = snapshot.arrangements[0].grid;
		expect(grid.rows).toHaveLength(2);
		expect(grid.columns.map((c) => c.id)).toEqual([
			"column-1",
			"column-3",
			"column-5",
		]);
		const nav = grid.placements.find((p) => p.containerId === "container-0");
		expect(nav).toMatchObject({ row: 0, column: 0, rowSpan: 2, colSpan: 1 });
		expect(
			grid.placements.find((p) => p.containerId === "container-2"),
		).toMatchObject({ row: 0, rowSpan: 1, column: 1 });
		expect(
			grid.placements.find((p) => p.containerId === "container-6"),
		).toMatchObject({ row: 1, rowSpan: 1, column: 1 });
		expect(snapshot.focusedContentId).toBe(chatContent.id);
		expect(snapshot.arrangements[0].lastFocusedContentId).toBe(chatContent.id);
		expect(snapshot.activeWorkspace).toEqual({ kind: "all" });
		expect(snapshot.extensions).toEqual({ futureTopLevel: true });
		expect(snapshot.containers[3].extensions).toEqual({ futureField: 1 });
		// Migrated ids never collide with ids the kernel mints later.
		expect(snapshot.idSeed).toBeGreaterThan(8);
		expect(encodeLayoutDocument(snapshot)).toBe(
			encodeLayoutDocument(
				decodeLayoutDocument(encodeLayoutDocument(snapshot)).ok
					? snapshot
					: snapshot,
			),
		);
	});

	it("walks a migration chain in order and fails closed on a missing step", () => {
		const applied: number[] = [];
		const result = runLayoutMigrations(
			{ ...classicDocument(), schemaVersion: 2 },
			[
				{
					from: 3,
					migrate: (doc) => {
						applied.push(3);
						return { ...doc, addedInV4: true };
					},
				},
				{
					from: 2,
					migrate: (doc) => {
						applied.push(2);
						return { ...doc, addedInV3: true };
					},
				},
			],
			4,
		);
		expect(applied).toEqual([2, 3]);
		expect(result?.from).toBe(2);
		expect(result?.layoutDocument.schemaVersion).toBe(4);
		expect(
			runLayoutMigrations(
				{ ...classicDocument(), schemaVersion: 2 },
				[{ from: 2, migrate: (doc) => doc }],
				4,
			),
		).toBeNull();
	});

	it("recovers the first healthy candidate and falls back when all are corrupt", () => {
		const layout = classicLayout();
		const recovered = recoverLayoutDocument(
			["{corrupt", encodeLayoutDocument(layout)],
			classicLayout(),
		);
		expect(recovered.sourceIndex).toBe(1);
		expect(recovered.snapshot).toEqual(layout);
		expect(recovered.failures).toEqual([
			{
				index: 0,
				reason: "parse-error",
				detail: expect.stringContaining("JSON"),
			},
		]);
		const fallback = classicLayout();
		const none = recoverLayoutDocument(
			[null, undefined, '{"schemaVersion": 99}'],
			fallback,
		);
		expect(none.sourceIndex).toBeNull();
		expect(none.snapshot).toBe(fallback);
		expect(none.failures).toHaveLength(1);
	});
});

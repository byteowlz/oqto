import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import {
	type LayoutSnapshot,
	type LayoutTransaction,
	type ViewportConstraints,
	applyTransaction,
	recoverLayoutDocument,
	solveLayoutGeometry,
} from "@/src/oqto-ui/compositor/index";
import { describe, expect, it } from "vitest";
import {
	type GeometryTrace,
	type PersistenceTrace,
	TRACE_FORMAT_VERSION,
	type TransitionTrace,
	buildCompositorTraceCorpus,
} from "../../scripts/compositor-traces/build";
import { classicLayout } from "./fixtures";

const CORPUS_DIR = path.resolve(
	process.cwd(),
	"../contracts/oqto-ui/traces/compositor",
);

/** What a non-TypeScript host sees: pure JSON, no branded types. */
function roundTrip<T>(value: T): T {
	return JSON.parse(JSON.stringify(value)) as T;
}

function readTrace(fileName: string): unknown {
	return JSON.parse(readFileSync(path.join(CORPUS_DIR, fileName), "utf8"));
}

const committedFiles = readdirSync(CORPUS_DIR).sort();

describe("conformance corpus freshness", () => {
	it("the committed corpus is exactly the deterministic regeneration", () => {
		const built = buildCompositorTraceCorpus();
		expect(committedFiles).toEqual(built.map((file) => file.path).sort());
		for (const file of built) {
			// Structural comparison: byte-stable encoding is not part of the
			// contract; a Rust host may serialize keys in any order.
			expect(readTrace(file.path)).toEqual(roundTrip(file.data));
		}
	});

	it("every trace declares its format and schema versions", () => {
		for (const fileName of committedFiles) {
			const trace = readTrace(fileName) as {
				format: string;
				formatVersion: number;
				schemaVersion: number;
			};
			expect(trace.format).toMatch(/^oqto-compositor-/);
			expect(trace.formatVersion).toBe(TRACE_FORMAT_VERSION);
			expect(trace.schemaVersion).toBe(1);
		}
	});
});

describe("conformance corpus replay (public seam only, from JSON alone)", () => {
	const transitionFiles = committedFiles.filter((name) =>
		name.startsWith("transitions-"),
	);

	it.each(transitionFiles)(
		"replays %s transition by transition",
		(fileName) => {
			const trace = readTrace(fileName) as TransitionTrace;
			let state = trace.initial as LayoutSnapshot;
			for (const [index, step] of trace.steps.entries()) {
				const actual = applyTransaction(
					state,
					step.transaction as LayoutTransaction,
				);
				expect(roundTrip(actual), `${fileName} step ${index}`).toEqual(
					step.expected,
				);
				if (actual.ok) state = actual.snapshot;
			}
		},
	);

	it("reproduces every geometry fixture", () => {
		const trace = readTrace("geometry-grid.json") as GeometryTrace;
		for (const scenario of trace.scenarios) {
			for (const geometryCase of scenario.cases) {
				const actual = solveLayoutGeometry(
					scenario.snapshot as LayoutSnapshot,
					geometryCase.viewport as ViewportConstraints,
				);
				expect(
					roundTrip(actual),
					`${scenario.name}/${geometryCase.name}`,
				).toEqual(geometryCase.expected);
			}
		}
	});

	it("reproduces every persistence decode outcome", () => {
		const trace = readTrace("persistence-documents.json") as PersistenceTrace;
		for (const layoutDocument of trace.documents) {
			const recovered = recoverLayoutDocument(
				[layoutDocument.raw],
				classicLayout(),
			);
			if (layoutDocument.expected.ok) {
				expect(recovered.sourceIndex, layoutDocument.name).toBe(0);
				expect(roundTrip(recovered.snapshot), layoutDocument.name).toEqual(
					layoutDocument.expected.snapshot,
				);
			} else {
				expect(recovered.sourceIndex, layoutDocument.name).toBeNull();
				expect(recovered.failures[0]?.reason, layoutDocument.name).toBe(
					layoutDocument.expected.reason,
				);
			}
		}
	});
});

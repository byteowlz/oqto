/**
 * Deterministic, non-destructive responsive projection (ADR-0042). Given
 * the canonical snapshot and a viewport class it returns a projected
 * snapshot for rendering plus merge provenance; the canonical document is
 * never touched, so narrow-then-widen restores it exactly. Ladder: collapse
 * navigation -> merge inline neighbors into their target's tabs per row by
 * role priority -> scale as the terminal fallback. With the "scroll" policy
 * nothing merges: overflowing columns scroll instead.
 */

import { activeArrangement, repairScrollAnchors, replaceArrangement } from "./arrangement";
import { solveLayoutGeometry, type SolvedLayout, type ViewportConstraints } from "./geometry";
import { removePlacement } from "./grid";
import type { ContainerId, ContentId } from "./ids";
import type { Container, GridPlacement, LayoutSnapshot } from "./model";
import { findContainer, replaceContainer } from "./state";

export interface ViewportClass extends ViewportConstraints {
	/** How inline overflow resolves: scroll (desktop) or merge (midsize). */
	readonly responsive?: "scroll" | "merge";
}

export interface MergeProvenance {
	readonly containerId: ContainerId;
	readonly into: ContainerId;
	readonly contentIds: readonly ContentId[];
}

export interface ProjectedLayout {
	readonly snapshot: LayoutSnapshot;
	readonly merges: readonly MergeProvenance[];
	readonly collapsedForSpace: readonly ContainerId[];
	readonly geometry: SolvedLayout;
}

function overflows(state: LayoutSnapshot, viewport: ViewportConstraints): boolean {
	return solveLayoutGeometry(state, { ...viewport, overflow: "scroll" }).degradations.some(
		(degradation) => degradation.kind === "overflow",
	);
}

interface MergeCandidate {
	readonly source: Container;
	readonly target: Container;
}

const NEVER_MERGED = new Set(["navigation", "primary"]);

function mergeCandidate(state: LayoutSnapshot): MergeCandidate | null {
	const arrangement = activeArrangement(state);
	const rows = new Map<number, GridPlacement[]>();
	for (const placement of arrangement.grid.placements) {
		rows.set(placement.row, [...(rows.get(placement.row) ?? []), placement]);
	}
	for (const row of [...rows.keys()].sort((a, b) => a - b)) {
		const inRow = (rows.get(row) ?? [])
			.map((placement) => ({ placement, container: findContainer(state, placement.containerId) }))
			.filter((item): item is { placement: GridPlacement; container: Container } => item.container !== null);
		const sources = inRow
			.filter(({ container }) => !NEVER_MERGED.has(container.role ?? "") && !container.collapsed)
			.sort(
				(a, b) =>
					Number(b.container.role === "auxiliary") - Number(a.container.role === "auxiliary") ||
					b.placement.column - a.placement.column,
			);
		if (sources.length === 0) continue;
		const source = sources[0].container;
		const target =
			inRow.find(({ container }) => container.role === "primary")?.container ??
			inRow
				.filter(({ container }) => container.id !== source.id && container.role !== "navigation")
				.sort((a, b) => a.placement.column - b.placement.column)[0]?.container;
		if (target) return { source, target };
	}
	return null;
}

function merge(state: LayoutSnapshot, candidate: MergeCandidate): LayoutSnapshot {
	const arrangement = activeArrangement(state);
	const stack = [...candidate.target.stack, ...candidate.source.stack];
	const focused = state.focusedContentId;
	const activeContentId =
		focused !== null && stack.some((content) => content.id === focused)
			? focused
			: (candidate.target.activeContentId ?? candidate.source.activeContentId);
	const merged = replaceContainer(state, { ...candidate.target, stack, activeContentId, collapsed: false });
	return repairScrollAnchors(
		replaceArrangement(
			{ ...merged, containers: merged.containers.filter((container) => container.id !== candidate.source.id) },
			{ ...arrangement, grid: removePlacement(arrangement.grid, candidate.source.id) },
		),
	);
}

export function projectLayout(state: LayoutSnapshot, viewport: ViewportClass): ProjectedLayout {
	if ((viewport.responsive ?? "scroll") !== "merge") {
		return {
			snapshot: state,
			merges: [],
			collapsedForSpace: [],
			geometry: solveLayoutGeometry(state, { ...viewport, overflow: "scroll" }),
		};
	}
	let working = state;
	const collapsedForSpace: ContainerId[] = [];
	const merges: MergeProvenance[] = [];
	if (overflows(working, viewport)) {
		for (const containerId of activeArrangement(working).grid.placements.map((p) => p.containerId)) {
			const container = findContainer(working, containerId);
			if (container && container.role === "navigation" && !container.collapsed) {
				working = replaceContainer(working, { ...container, collapsed: true });
				collapsedForSpace.push(containerId);
			}
		}
	}
	while (overflows(working, viewport)) {
		const candidate = mergeCandidate(working);
		if (!candidate) break;
		merges.push({
			containerId: candidate.source.id,
			into: candidate.target.id,
			contentIds: candidate.source.stack.map((content) => content.id),
		});
		working = merge(working, candidate);
	}
	return {
		snapshot: working,
		merges,
		collapsedForSpace,
		geometry: solveLayoutGeometry(working, { ...viewport, overflow: "fit" }),
	};
}

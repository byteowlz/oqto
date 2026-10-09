/**
 * Directional navigation over the semantic grid (ADR-0041: an internal
 * part of the Layout Engine). Resolves the neighboring Container in a
 * direction by track adjacency and cross-axis overlap; collapsed
 * Containers are skipped because they are not reachable targets.
 */

import type { SplitEdge } from "./commands";
import { placementOf } from "./grid";
import type { ContainerId } from "./ids";
import type { Arrangement, Container, GridPlacement } from "./model";

export type NavigationDirection = SplitEdge;

function overlap(
	a: GridPlacement,
	b: GridPlacement,
	axis: "row" | "column",
): number {
	const span = axis === "row" ? "rowSpan" : "colSpan";
	const start = Math.max(a[axis], b[axis]);
	const end = Math.min(a[axis] + a[span], b[axis] + b[span]);
	return end - start;
}

/**
 * The Container adjacent to `containerId` in `direction`: first among
 * placements touching its edge, preferring the largest cross-axis overlap,
 * then reading order; otherwise the nearest further placement that still
 * overlaps on the cross axis. Null at the edge of the Arrangement.
 */
export function neighborContainer(
	arrangement: Arrangement,
	containers: readonly Container[],
	containerId: ContainerId,
	direction: NavigationDirection,
): ContainerId | null {
	const origin = placementOf(arrangement.grid, containerId);
	if (!origin) return null;
	const collapsed = new Set(
		containers
			.filter((container) => container.collapsed)
			.map((container) => container.id),
	);
	const axis =
		direction === "inline-start" || direction === "inline-end"
			? "column"
			: "row";
	const cross = axis === "column" ? "row" : "column";
	const span = axis === "column" ? "colSpan" : "rowSpan";
	const forward = direction === "inline-end" || direction === "block-end";
	const originEnd = origin[axis] + origin[span];
	const candidates = arrangement.grid.placements
		.filter(
			(placement) =>
				placement.containerId !== containerId &&
				!collapsed.has(placement.containerId),
		)
		.filter((placement) =>
			forward
				? placement[axis] >= originEnd
				: placement[axis] + placement[span] <= origin[axis],
		)
		.filter((placement) => overlap(origin, placement, cross) > 0)
		.map((placement) => ({
			placement,
			distance: forward
				? placement[axis] - originEnd
				: origin[axis] - (placement[axis] + placement[span]),
			overlap: overlap(origin, placement, cross),
		}))
		.sort(
			(a, b) =>
				a.distance - b.distance ||
				b.overlap - a.overlap ||
				a.placement.row - b.placement.row ||
				a.placement.column - b.placement.column,
		);
	return candidates[0]?.placement.containerId ?? null;
}

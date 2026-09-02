/**
 * Preset layout factories. The classic desktop Preset is data, not
 * permanent feature regions: one Arrangement, one row, a full-height
 * navigation Container, a primary Container (Chat by default), and an
 * auxiliary Container. Preset Containers retain or collapse when emptied;
 * only dynamically created split Containers remove themselves.
 */

import {
	type ContentId,
	arrangementIdFrom,
	containerIdFrom,
	trackIdFrom,
} from "./ids";
import type {
	Container,
	ContentRef,
	GridPlacement,
	GridTrack,
	LayoutSnapshot,
	TrackSize,
} from "./model";
import { LAYOUT_SCHEMA_VERSION } from "./model";

export interface ClassicPresetContent {
	readonly navigation: readonly ContentRef[];
	readonly primary: readonly ContentRef[];
	readonly auxiliary: readonly ContentRef[];
}

interface PresetRegion {
	readonly role: "navigation" | "primary" | "auxiliary";
	readonly emptyBehavior: "retain" | "collapse";
	readonly size: TrackSize;
	readonly stack: readonly ContentRef[];
}

export function createEmptyLayout(): LayoutSnapshot {
	return {
		schemaVersion: LAYOUT_SCHEMA_VERSION,
		revision: 0,
		idSeed: 1,
		strategy: "grid",
		activeArrangementId: arrangementIdFrom(0),
		activeWorkspace: { kind: "all" },
		arrangements: [
			{
				id: arrangementIdFrom(0),
				grid: { rows: [], columns: [], placements: [] },
				scrollAnchorColumnId: null,
				lastFocusedContentId: null,
			},
		],
		containers: [],
		focusedContentId: null,
	};
}

/** arrangement-0, row track-1, columns track-2..4, containers 5..7. */
export function createClassicPresetLayout(
	content: ClassicPresetContent,
): LayoutSnapshot {
	const regions: readonly PresetRegion[] = [
		{
			role: "navigation",
			emptyBehavior: "collapse",
			size: { unit: "fixed", value: 320, min: 240 },
			stack: content.navigation,
		},
		{
			role: "primary",
			emptyBehavior: "retain",
			size: { unit: "fraction", value: 2, min: 360 },
			stack: content.primary,
		},
		{
			role: "auxiliary",
			emptyBehavior: "retain",
			size: { unit: "fraction", value: 1, min: 280 },
			stack: content.auxiliary,
		},
	];
	const row: GridTrack = {
		id: trackIdFrom(1),
		size: { unit: "fraction", value: 1 },
	};
	const columns: GridTrack[] = [];
	const placements: GridPlacement[] = [];
	const containers: Container[] = [];
	regions.forEach((region, index) => {
		const containerId = containerIdFrom(5 + index);
		columns.push({ id: trackIdFrom(2 + index), size: region.size });
		placements.push({
			containerId,
			row: 0,
			column: index,
			rowSpan: 1,
			colSpan: 1,
		});
		containers.push({
			id: containerId,
			role: region.role,
			emptyBehavior: region.emptyBehavior,
			stack: region.stack,
			activeContentId: region.stack[0]?.id ?? null,
			collapsed: false,
		});
	});
	const focusedContentId: ContentId | null =
		containers.find((container) => container.role === "primary")
			?.activeContentId ??
		containers.find((container) => container.activeContentId !== null)
			?.activeContentId ??
		null;
	return {
		schemaVersion: LAYOUT_SCHEMA_VERSION,
		revision: 0,
		idSeed: 8,
		strategy: "grid",
		activeArrangementId: arrangementIdFrom(0),
		activeWorkspace: { kind: "all" },
		arrangements: [
			{
				id: arrangementIdFrom(0),
				grid: { rows: [row], columns, placements },
				scrollAnchorColumnId: null,
				lastFocusedContentId: focusedContentId,
			},
		],
		containers,
		focusedContentId,
	};
}

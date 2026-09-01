/**
 * Thin CSS Grid projection of the active Arrangement (ADR-0041/0042). The
 * kernel's responsive projection and solved tracks become grid-template
 * custom properties; flush Lanes sit outside the horizontally scrolled
 * body; DOM geometry is never read back into layout state. All
 * interaction dispatches semantic commands through the store.
 */

import { type CSSProperties, useCallback, useSyncExternalStore } from "react";
import {
	type Container,
	type GridPlacement,
	type LayoutCommand,
	projectLayout,
	type ViewportClass,
} from "../index";
import { ContainerView } from "./ContainerView";
import type { CompositorChromeLabels, ContentLabel, RenderContent } from "./contracts";
import { gridLines, gridTemplateFromSizes } from "./grid-template";
import type { CompositorStore } from "./store";
import "./compositor.css";

interface CompositorHostProps {
	readonly store: CompositorStore;
	readonly viewport: ViewportClass;
	readonly renderContent: RenderContent;
	readonly contentLabel: ContentLabel;
	readonly labels: CompositorChromeLabels;
}

interface CellProps {
	readonly container: Container;
	readonly placement: GridPlacement;
	readonly rowOffset: number;
	readonly focusedContentId: Container["activeContentId"];
	readonly renderContent: RenderContent;
	readonly contentLabel: ContentLabel;
	readonly labels: CompositorChromeLabels;
	readonly commit: (commands: readonly LayoutCommand[]) => void;
}

function Cell({ container, placement, rowOffset, ...view }: CellProps) {
	const lines = gridLines(placement, rowOffset);
	return (
		<div
			className="oqto-compositor-cell"
			style={{ "--oqto-cell-row": lines.row, "--oqto-cell-column": lines.column } as CSSProperties}
		>
			<ContainerView container={container} {...view} />
		</div>
	);
}

export function CompositorHost({ store, viewport, renderContent, contentLabel, labels }: CompositorHostProps) {
	const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
	const commit = useCallback(
		(commands: readonly LayoutCommand[]) => {
			store.commit(commands, viewport);
		},
		[store, viewport],
	);
	const projected = projectLayout(snapshot, viewport);
	const arrangement =
		projected.snapshot.arrangements.find((candidate) => candidate.id === projected.snapshot.activeArrangementId) ??
		projected.snapshot.arrangements[0];
	if (!arrangement) return null;
	const containersById = new Map(projected.snapshot.containers.map((container) => [container.id, container]));
	const rows = arrangement.grid.rows;
	const flushTop = rows[0]?.flush === true ? 0 : null;
	const flushBottom = rows.length > 1 && rows[rows.length - 1]?.flush === true ? rows.length - 1 : null;
	const bodyRowSizes = projected.geometry.rowSizes.filter(
		(_, index) => index !== flushTop && index !== flushBottom,
	);
	const focusOf = (container: Container) =>
		projected.snapshot.focusedContentId !== null &&
		container.stack.some((content) => content.id === projected.snapshot.focusedContentId)
			? projected.snapshot.focusedContentId
			: null;
	const cell = (placement: GridPlacement, rowOffset: number) => {
		const container = containersById.get(placement.containerId);
		return container ? (
			<Cell
				key={container.id}
				container={container}
				placement={placement}
				rowOffset={rowOffset}
				focusedContentId={focusOf(container)}
				renderContent={renderContent}
				contentLabel={contentLabel}
				labels={labels}
				commit={commit}
			/>
		) : null;
	};
	const lane = (rowIndex: number | null) =>
		rowIndex === null ? null : (
			<div
				className="oqto-compositor-lane"
				style={{ "--oqto-lane-size": `${projected.geometry.rowSizes[rowIndex]}px` } as CSSProperties}
			>
				{arrangement.grid.placements.filter((p) => p.row === rowIndex).map((p) => cell(p, rowIndex))}
			</div>
		);
	return (
		<div className="oqto-compositor" data-merges={projected.merges.length || undefined}>
			{lane(flushTop)}
			<div className="oqto-compositor-body">
				<div
					className="oqto-compositor-grid"
					style={
						{
							"--oqto-compositor-columns": gridTemplateFromSizes(projected.geometry.columnSizes),
							"--oqto-compositor-rows": gridTemplateFromSizes(bodyRowSizes),
							"--oqto-compositor-scroll": `${projected.geometry.scrollOffset}px`,
						} as CSSProperties
					}
				>
					{arrangement.grid.placements
						.filter((p) => p.row !== flushTop && p.row !== flushBottom)
						.map((p) => cell(p, flushTop === null ? 0 : 1))}
				</div>
			</div>
			{lane(flushBottom)}
		</div>
	);
}

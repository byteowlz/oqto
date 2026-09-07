/**
 * Compositor chrome pieces used by CompositorHost: grid cells, resize
 * gutters with pointer capture (previewing while dragging, committing on
 * release), and the viewport-edge drop zones.
 */

import { type CSSProperties, type PointerEvent, useRef } from "react";
import type {
	Container,
	GridPlacement,
	LayoutCommand,
	SplitEdge,
} from "../index";
import { ContainerView } from "./ContainerView";
import type {
	CompositorChromeLabels,
	ContentLabel,
	RenderContent,
} from "./contracts";
import { gridLines } from "./grid-template";

export interface CellGridContext {
	readonly rowOffset: number;
	readonly rows: number;
	readonly columns: number;
}

export interface CellProps {
	readonly container: Container;
	readonly placement: GridPlacement;
	readonly grid: CellGridContext;
	readonly focusedContentId: Container["activeContentId"];
	readonly renderContent: RenderContent;
	readonly contentLabel: ContentLabel;
	readonly labels: CompositorChromeLabels;
	readonly commit: (commands: readonly LayoutCommand[]) => void;
}

/** Which Arrangement edges a placement touches, as a space-separated token list. */
function edgesOf(placement: GridPlacement, grid: CellGridContext): string {
	const edges: string[] = [];
	if (placement.column === 0) edges.push("inline-start");
	if (placement.column + placement.colSpan >= grid.columns)
		edges.push("inline-end");
	if (placement.row === 0) edges.push("block-start");
	if (placement.row + placement.rowSpan >= grid.rows) edges.push("block-end");
	return edges.join(" ");
}

export function Cell({ container, placement, grid, ...view }: CellProps) {
	const lines = gridLines(placement, grid.rowOffset);
	return (
		<div
			className="oqto-compositor-cell"
			data-edges={edgesOf(placement, grid)}
			data-full-height={placement.rowSpan >= grid.rows || undefined}
			style={
				{
					"--oqto-cell-row": lines.row,
					"--oqto-cell-column": lines.column,
				} as CSSProperties
			}
		>
			<ContainerView container={container} {...view} />
		</div>
	);
}

export type ResizeAxisName = "inline" | "block";

export interface GutterProps {
	readonly axis: ResizeAxisName;
	readonly index: number;
	readonly label: string;
	/** Provisional delta while dragging; null when the drag ends. */
	readonly onPreview: (
		axis: ResizeAxisName,
		index: number,
		deltaPx: number | null,
	) => void;
	readonly onResize: (
		axis: ResizeAxisName,
		index: number,
		deltaPx: number,
	) => void;
}

/** Pointer-captured drag between two adjacent tracks. */
export function Gutter({
	axis,
	index,
	label,
	onPreview,
	onResize,
}: GutterProps) {
	const origin = useRef<number | null>(null);
	const read = (event: PointerEvent<HTMLElement>) =>
		axis === "inline" ? event.clientX : event.clientY;
	return (
		<div
			role="separator"
			aria-orientation={axis === "inline" ? "vertical" : "horizontal"}
			aria-label={label}
			className="oqto-compositor-gutter"
			data-axis={axis}
			style={{ "--oqto-gutter-line": `${index + 2}` } as CSSProperties}
			onPointerDown={(event) => {
				origin.current = read(event);
				event.currentTarget.setPointerCapture(event.pointerId);
			}}
			onPointerMove={(event) => {
				if (origin.current === null) return;
				onPreview(axis, index, read(event) - origin.current);
			}}
			onPointerUp={(event) => {
				if (origin.current === null) return;
				const delta = read(event) - origin.current;
				origin.current = null;
				onPreview(axis, index, null);
				if (delta !== 0) onResize(axis, index, delta);
			}}
			onPointerCancel={() => {
				origin.current = null;
				onPreview(axis, index, null);
			}}
		/>
	);
}

export const EDGE_ZONES: readonly {
	edge: SplitEdge;
	labelKey: "dropTop" | "dropBottom" | "dropStart" | "dropEnd";
}[] = [
	{ edge: "block-start", labelKey: "dropTop" },
	{ edge: "block-end", labelKey: "dropBottom" },
	{ edge: "inline-start", labelKey: "dropStart" },
	{ edge: "inline-end", labelKey: "dropEnd" },
];

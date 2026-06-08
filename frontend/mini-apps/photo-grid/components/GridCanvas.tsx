import { cn } from "@/lib/utils";
import type { CSSProperties, RefObject } from "react";
import { useRef, useState } from "react";
import {
	type GridState,
	MIN_FRACTION,
	MIN_GRID_SPAN,
	type Margins,
	type PoolImage,
} from "../types";
import { GridTile } from "./GridTile";

type Axis = "col" | "row";
type Side = "top" | "right" | "bottom" | "left";

type DragState =
	| {
			kind: "track";
			axis: Axis;
			index: number;
			start: number;
			extent: number;
			sizes: number[];
	  }
	| {
			kind: "edge";
			side: Side;
			start: number;
			extent: number;
			margins: Margins;
	  };

function clamp(v: number, min: number, max: number): number {
	return Math.max(min, Math.min(max, v));
}

function cumulative(sizes: number[]): number[] {
	const out: number[] = [];
	let acc = 0;
	for (let i = 0; i < sizes.length - 1; i++) {
		acc += sizes[i];
		out.push(acc);
	}
	return out;
}

/** Resize two adjacent tracks by a fraction delta, keeping the sum constant. */
function resizePair(sizes: number[], index: number, delta: number): number[] {
	const next = [...sizes];
	let a = sizes[index] + delta;
	let b = sizes[index + 1] - delta;
	if (a < MIN_FRACTION) {
		b -= MIN_FRACTION - a;
		a = MIN_FRACTION;
	}
	if (b < MIN_FRACTION) {
		a -= MIN_FRACTION - b;
		b = MIN_FRACTION;
	}
	next[index] = a;
	next[index + 1] = b;
	return next;
}

/** Apply an edge drag delta to one side, keeping the grid above its min span. */
function resizeMargin(margins: Margins, side: Side, delta: number): Margins {
	const next = { ...margins };
	if (side === "left") {
		next.left = clamp(
			margins.left + delta,
			0,
			1 - margins.right - MIN_GRID_SPAN,
		);
	} else if (side === "right") {
		next.right = clamp(
			margins.right - delta,
			0,
			1 - margins.left - MIN_GRID_SPAN,
		);
	} else if (side === "top") {
		next.top = clamp(
			margins.top + delta,
			0,
			1 - margins.bottom - MIN_GRID_SPAN,
		);
	} else {
		next.bottom = clamp(
			margins.bottom - delta,
			0,
			1 - margins.top - MIN_GRID_SPAN,
		);
	}
	return next;
}

interface HandleProps {
	id: string;
	vertical: boolean;
	style: CSSProperties;
	active: boolean;
	className: string;
	onPointerDown: (e: React.PointerEvent<HTMLSpanElement>) => void;
	onPointerMove: (e: React.PointerEvent<HTMLSpanElement>) => void;
	onPointerUp: (e: React.PointerEvent<HTMLSpanElement>) => void;
}

function Handle({
	vertical,
	style,
	active,
	className,
	onPointerDown,
	onPointerMove,
	onPointerUp,
}: HandleProps) {
	const idleLine = vertical
		? "w-px group-hover:w-1 group-hover:bg-primary"
		: "h-px group-hover:h-1 group-hover:bg-primary";
	const activeLine = vertical ? "w-1 bg-primary" : "h-1 bg-primary";

	return (
		<span
			className={cn(
				"group absolute z-10 flex touch-none items-stretch justify-center",
				vertical ? "cursor-col-resize" : "cursor-row-resize flex-col",
				active && "bg-primary/5",
				className,
			)}
			style={style}
			onPointerDown={onPointerDown}
			onPointerMove={onPointerMove}
			onPointerUp={onPointerUp}
		>
			<span
				className={cn(
					"transition-all",
					idleLine,
					active ? activeLine : "bg-border",
				)}
			/>
		</span>
	);
}

export interface GridCanvasProps {
	state: GridState;
	imagesById: Map<string, PoolImage>;
	selectedIndex: number | null;
	gridRef: RefObject<HTMLDivElement | null>;
	onSelectTile: (index: number) => void;
	onAssignDrop: (index: number, imageId: string) => void;
	onPanChange: (index: number, posX: number, posY: number) => void;
	onClear: (index: number) => void;
	onColSizes: (sizes: number[]) => void;
	onRowSizes: (sizes: number[]) => void;
	onMargins: (margins: Margins) => void;
}

export function GridCanvas({
	state,
	imagesById,
	selectedIndex,
	gridRef,
	onSelectTile,
	onAssignDrop,
	onPanChange,
	onClear,
	onColSizes,
	onRowSizes,
	onMargins,
}: GridCanvasProps) {
	const frameRef = useRef<HTMLDivElement | null>(null);
	const dragRef = useRef<DragState | null>(null);
	const [activeHandle, setActiveHandle] = useState<string | null>(null);

	const beginTrack =
		(axis: Axis, index: number, id: string) =>
		(e: React.PointerEvent<HTMLSpanElement>) => {
			const grid = gridRef.current;
			if (!grid) return;
			const rect = grid.getBoundingClientRect();
			dragRef.current = {
				kind: "track",
				axis,
				index,
				start: axis === "col" ? e.clientX : e.clientY,
				extent: axis === "col" ? rect.width : rect.height,
				sizes: axis === "col" ? [...state.colSizes] : [...state.rowSizes],
			};
			setActiveHandle(id);
			e.currentTarget.setPointerCapture(e.pointerId);
		};

	const beginEdge =
		(side: Side, id: string) => (e: React.PointerEvent<HTMLSpanElement>) => {
			const frame = frameRef.current;
			if (!frame) return;
			const rect = frame.getBoundingClientRect();
			const horizontal = side === "left" || side === "right";
			dragRef.current = {
				kind: "edge",
				side,
				start: horizontal ? e.clientX : e.clientY,
				extent: horizontal ? rect.width : rect.height,
				margins: { ...state.margins },
			};
			setActiveHandle(id);
			e.currentTarget.setPointerCapture(e.pointerId);
		};

	const moveDrag = (e: React.PointerEvent<HTMLSpanElement>) => {
		const drag = dragRef.current;
		if (!drag) return;
		if (drag.kind === "track") {
			const current = drag.axis === "col" ? e.clientX : e.clientY;
			const delta = (current - drag.start) / drag.extent;
			const next = resizePair(drag.sizes, drag.index, delta);
			if (drag.axis === "col") onColSizes(next);
			else onRowSizes(next);
		} else {
			const horizontal = drag.side === "left" || drag.side === "right";
			const current = horizontal ? e.clientX : e.clientY;
			const delta = (current - drag.start) / drag.extent;
			onMargins(resizeMargin(drag.margins, drag.side, delta));
		}
	};

	const endDrag = (e: React.PointerEvent<HTMLSpanElement>) => {
		dragRef.current = null;
		setActiveHandle(null);
		e.currentTarget.releasePointerCapture(e.pointerId);
	};

	const colBoundaries = cumulative(state.colSizes);
	const rowBoundaries = cumulative(state.rowSizes);
	const m = state.margins;
	const vSpan = { top: `${m.top * 100}%`, bottom: `${m.bottom * 100}%` };
	const hSpan = { left: `${m.left * 100}%`, right: `${m.right * 100}%` };

	return (
		<div className="relative h-full w-full p-4">
			<div ref={frameRef} className="relative h-full w-full">
				<div
					ref={gridRef}
					className="absolute"
					style={{
						left: `${m.left * 100}%`,
						top: `${m.top * 100}%`,
						right: `${m.right * 100}%`,
						bottom: `${m.bottom * 100}%`,
						display: "grid",
						gridTemplateColumns: state.colSizes.map((f) => `${f}fr`).join(" "),
						gridTemplateRows: state.rowSizes.map((f) => `${f}fr`).join(" "),
						gap: `${state.gap}px`,
					}}
				>
					{state.tiles.map((tile, i) => (
						<GridTile
							// biome-ignore lint/suspicious/noArrayIndexKey: tiles are positional by design
							key={i}
							index={i}
							tile={tile}
							image={
								tile.imageId ? (imagesById.get(tile.imageId) ?? null) : null
							}
							selected={selectedIndex === i}
							onSelect={onSelectTile}
							onAssignDrop={onAssignDrop}
							onPanChange={onPanChange}
							onClear={onClear}
						/>
					))}

					{colBoundaries.map((pos, i) => (
						<Handle
							// biome-ignore lint/suspicious/noArrayIndexKey: boundaries are positional
							key={`col-${i}`}
							id={`col-${i}`}
							vertical
							active={activeHandle === `col-${i}`}
							className="bottom-0 top-0 w-4 -translate-x-1/2"
							style={{ left: `${pos * 100}%` }}
							onPointerDown={beginTrack("col", i, `col-${i}`)}
							onPointerMove={moveDrag}
							onPointerUp={endDrag}
						/>
					))}

					{rowBoundaries.map((pos, i) => (
						<Handle
							// biome-ignore lint/suspicious/noArrayIndexKey: boundaries are positional
							key={`row-${i}`}
							id={`row-${i}`}
							vertical={false}
							active={activeHandle === `row-${i}`}
							className="inset-x-0 h-4 -translate-y-1/2"
							style={{ top: `${pos * 100}%` }}
							onPointerDown={beginTrack("row", i, `row-${i}`)}
							onPointerMove={moveDrag}
							onPointerUp={endDrag}
						/>
					))}
				</div>

				<Handle
					id="edge-left"
					vertical
					active={activeHandle === "edge-left"}
					className="w-5 -translate-x-1/2"
					style={{ left: hSpan.left, top: vSpan.top, bottom: vSpan.bottom }}
					onPointerDown={beginEdge("left", "edge-left")}
					onPointerMove={moveDrag}
					onPointerUp={endDrag}
				/>
				<Handle
					id="edge-right"
					vertical
					active={activeHandle === "edge-right"}
					className="w-5 translate-x-1/2"
					style={{ right: hSpan.right, top: vSpan.top, bottom: vSpan.bottom }}
					onPointerDown={beginEdge("right", "edge-right")}
					onPointerMove={moveDrag}
					onPointerUp={endDrag}
				/>
				<Handle
					id="edge-top"
					vertical={false}
					active={activeHandle === "edge-top"}
					className="h-5 -translate-y-1/2"
					style={{ top: vSpan.top, left: hSpan.left, right: hSpan.right }}
					onPointerDown={beginEdge("top", "edge-top")}
					onPointerMove={moveDrag}
					onPointerUp={endDrag}
				/>
				<Handle
					id="edge-bottom"
					vertical={false}
					active={activeHandle === "edge-bottom"}
					className="h-5 translate-y-1/2"
					style={{ bottom: vSpan.bottom, left: hSpan.left, right: hSpan.right }}
					onPointerDown={beginEdge("bottom", "edge-bottom")}
					onPointerMove={moveDrag}
					onPointerUp={endDrag}
				/>
			</div>
		</div>
	);
}

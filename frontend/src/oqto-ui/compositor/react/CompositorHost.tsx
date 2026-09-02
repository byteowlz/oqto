/**
 * Thin CSS Grid projection of the active Arrangement (ADR-0041/0042). The
 * kernel's responsive projection and solved tracks become grid-template
 * custom properties; flush Lanes sit outside the horizontally scrolled
 * body; DOM geometry is never read back into layout state. All
 * interaction — tabs, drops, edge drops, resize gutters, wheel scrolling —
 * dispatches semantic commands through the store.
 */

import {
	type CSSProperties,
	type DragEvent,
	type PointerEvent,
	useCallback,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";
import {
	type Arrangement,
	type Container,
	type GridPlacement,
	type LayoutCommand,
	type SolvedLayout,
	type SplitEdge,
	type ViewportClass,
	contentIdFrom,
	projectLayout,
} from "../index";
import { ContainerView } from "./ContainerView";
import type {
	CompositorChromeLabels,
	ContentLabel,
	RenderContent,
} from "./contracts";
import {
	CONTENT_DRAG_TYPE,
	edgeDropCommands,
	resizeCommands,
	scrollByColumns,
} from "./gestures";
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

interface GutterProps {
	readonly axis: "inline" | "block";
	readonly index: number;
	readonly label: string;
	readonly onResize: (
		axis: "inline" | "block",
		index: number,
		deltaPx: number,
	) => void;
}

/** Pointer-captured drag between two adjacent tracks; commits on release. */
function Gutter({ axis, index, label, onResize }: GutterProps) {
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
			onPointerUp={(event) => {
				if (origin.current === null) return;
				const delta = read(event) - origin.current;
				origin.current = null;
				if (delta !== 0) onResize(axis, index, delta);
			}}
		/>
	);
}

const EDGES: readonly {
	edge: SplitEdge;
	labelKey: keyof CompositorChromeLabels;
}[] = [
	{ edge: "block-start", labelKey: "dropTop" },
	{ edge: "block-end", labelKey: "dropBottom" },
	{ edge: "inline-start", labelKey: "dropStart" },
	{ edge: "inline-end", labelKey: "dropEnd" },
];

const WHEEL_STEP_PX = 40;
const WHEEL_DEBOUNCE_MS = 250;

export function CompositorHost({
	store,
	viewport,
	renderContent,
	contentLabel,
	labels,
}: CompositorHostProps) {
	const snapshot = useSyncExternalStore(
		store.subscribe,
		store.getSnapshot,
		store.getSnapshot,
	);
	const [dragging, setDragging] = useState(false);
	const lastWheel = useRef(0);
	const commit = useCallback(
		(commands: readonly LayoutCommand[]) => {
			store.commit(commands, viewport);
		},
		[store, viewport],
	);
	const projected = projectLayout(snapshot, viewport);
	const arrangement: Arrangement | undefined =
		projected.snapshot.arrangements.find(
			(candidate) => candidate.id === projected.snapshot.activeArrangementId,
		) ?? projected.snapshot.arrangements[0];
	const canonicalArrangement =
		snapshot.arrangements.find(
			(candidate) => candidate.id === snapshot.activeArrangementId,
		) ?? snapshot.arrangements[0];
	if (!arrangement || !canonicalArrangement) return null;
	const geometry: SolvedLayout = projected.geometry;
	const containersById = new Map(
		projected.snapshot.containers.map((container) => [container.id, container]),
	);
	const rows = arrangement.grid.rows;
	const flushTop = rows[0]?.flush === true ? 0 : null;
	const flushBottom =
		rows.length > 1 && rows[rows.length - 1]?.flush === true
			? rows.length - 1
			: null;
	const bodyRowSizes = geometry.rowSizes.filter(
		(_, index) => index !== flushTop && index !== flushBottom,
	);
	const rowOffset = flushTop === null ? 0 : 1;
	const focusOf = (container: Container) =>
		projected.snapshot.focusedContentId !== null &&
		container.stack.some(
			(content) => content.id === projected.snapshot.focusedContentId,
		)
			? projected.snapshot.focusedContentId
			: null;
	const onResize = (
		axis: "inline" | "block",
		index: number,
		deltaPx: number,
	) => {
		// Gutters address canonical tracks; resizing while merged is disabled by
		// the projection not rendering gutters for merged-away columns.
		const commands = resizeCommands(
			canonicalArrangement,
			geometry,
			axis,
			index,
			deltaPx,
		);
		if (commands.length > 0) commit(commands);
	};
	const cell = (placement: GridPlacement, offset: number) => {
		const container = containersById.get(placement.containerId);
		return container ? (
			<Cell
				key={container.id}
				container={container}
				placement={placement}
				rowOffset={offset}
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
				style={
					{
						"--oqto-lane-size": `${geometry.rowSizes[rowIndex]}px`,
					} as CSSProperties
				}
			>
				{arrangement.grid.placements
					.filter((p) => p.row === rowIndex)
					.map((p) => cell(p, rowIndex))}
			</div>
		);
	const onEdgeDrop = (edge: SplitEdge) => (event: DragEvent<HTMLElement>) => {
		const id = event.dataTransfer?.getData(CONTENT_DRAG_TYPE);
		setDragging(false);
		if (!id) return;
		event.preventDefault();
		commit(edgeDropCommands(contentIdFrom(id), edge));
	};
	const gutters =
		projected.merges.length > 0 ? null : (
			<>
				{arrangement.grid.columns.slice(0, -1).map((column, index) => (
					<Gutter
						key={column.id}
						axis="inline"
						index={index}
						label={labels.resizeColumns}
						onResize={onResize}
					/>
				))}
				{rows
					.slice(rowOffset, flushBottom ?? rows.length)
					.slice(0, -1)
					.map((row, index) => (
						<Gutter
							key={row.id}
							axis="block"
							index={index}
							label={labels.resizeRows}
							onResize={onResize}
						/>
					))}
			</>
		);
	return (
		<div
			className="oqto-compositor"
			data-merges={projected.merges.length || undefined}
			data-dragging={dragging || undefined}
			onDragOver={() => {
				if (!dragging) setDragging(true);
			}}
			onDragEnd={() => setDragging(false)}
			onDrop={() => setDragging(false)}
		>
			{EDGES.map(({ edge, labelKey }) => (
				<div
					key={edge}
					className="oqto-compositor-edge"
					data-edge={edge}
					aria-label={labels[labelKey]}
					onDragOver={(event) => event.preventDefault()}
					onDrop={onEdgeDrop(edge)}
				/>
			))}
			{lane(flushTop)}
			<div
				className="oqto-compositor-body"
				onWheel={(event) => {
					if (
						Math.abs(event.deltaX) < WHEEL_STEP_PX ||
						Math.abs(event.deltaX) < Math.abs(event.deltaY)
					)
						return;
					const now = event.timeStamp;
					if (now - lastWheel.current < WHEEL_DEBOUNCE_MS) return;
					const command = scrollByColumns(
						canonicalArrangement,
						event.deltaX > 0 ? 1 : -1,
					);
					if (!command) return;
					lastWheel.current = now;
					commit([command]);
				}}
			>
				<div
					className="oqto-compositor-grid"
					style={
						{
							"--oqto-compositor-columns": gridTemplateFromSizes(
								geometry.columnSizes,
							),
							"--oqto-compositor-rows": gridTemplateFromSizes(bodyRowSizes),
							"--oqto-compositor-scroll": `${geometry.scrollOffset}px`,
						} as CSSProperties
					}
				>
					{arrangement.grid.placements
						.filter((p) => p.row !== flushTop && p.row !== flushBottom)
						.map((p) => cell(p, rowOffset))}
					{gutters}
				</div>
			</div>
			{lane(flushBottom)}
		</div>
	);
}

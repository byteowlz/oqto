/**
 * Thin CSS Grid projection of the active Arrangement (ADR-0041/0042). The
 * kernel's responsive projection and solved tracks become grid-template
 * custom properties; flush Lanes sit outside the horizontally scrolled
 * body; DOM geometry is never read back into layout state. All
 * interaction — tabs, drops, edge drops, resize gutters, wheel scrolling,
 * keyboard chords, the command palette — dispatches semantic commands
 * through the store. A gutter drag previews by applying the would-be
 * transaction without committing it (ADR-0041 preview).
 */

import {
	type CSSProperties,
	type DragEvent,
	type KeyboardEvent,
	useCallback,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";
import {
	type Container,
	type GridPlacement,
	type LayoutCommand,
	type LayoutSnapshot,
	type SplitEdge,
	type ViewportClass,
	applyTransaction,
	contentIdFrom,
	projectLayout,
} from "../index";
import { CommandPalette } from "./CommandPalette";
import { Cell, EDGE_ZONES, Gutter, type ResizeAxisName } from "./chrome";
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
import { gridTemplateFromSizes } from "./grid-template";
import {
	type CompositorAction,
	DEFAULT_KEY_BINDINGS,
	type KeyBinding,
	matchBinding,
	resolveAction,
} from "./keybindings";
import type { CompositorStore } from "./store";
import { useMeasuredViewport } from "./useMeasuredViewport";
import "./compositor.css";

interface CompositorHostProps {
	readonly store: CompositorStore;
	readonly viewport: ViewportClass;
	readonly renderContent: RenderContent;
	readonly contentLabel: ContentLabel;
	readonly labels: CompositorChromeLabels;
	/** ADR-0040-shaped Binding -> Action data; defaults to the Alt chords. */
	readonly keyBindings?: readonly KeyBinding[];
}

interface ResizePreview {
	readonly axis: ResizeAxisName;
	readonly index: number;
	readonly deltaPx: number;
}

const WHEEL_STEP_PX = 40;
const WHEEL_DEBOUNCE_MS = 250;

function activeOf(snapshot: LayoutSnapshot) {
	return (
		snapshot.arrangements.find(
			(candidate) => candidate.id === snapshot.activeArrangementId,
		) ?? snapshot.arrangements[0]
	);
}

/** The snapshot a pending gutter drag would produce — applied, never committed. */
function previewed(
	snapshot: LayoutSnapshot,
	viewport: ViewportClass,
	preview: ResizePreview | null,
): LayoutSnapshot {
	const arrangement = activeOf(snapshot);
	if (!preview || !arrangement) return snapshot;
	const geometry = projectLayout(snapshot, viewport).geometry;
	const commands = resizeCommands(
		arrangement,
		geometry,
		preview.axis,
		preview.index,
		preview.deltaPx,
	);
	if (commands.length === 0) return snapshot;
	const result = applyTransaction(snapshot, {
		expectedRevision: snapshot.revision,
		commands,
	});
	return result.ok ? result.snapshot : snapshot;
}

export function CompositorHost({
	store,
	viewport,
	renderContent,
	contentLabel,
	labels,
	keyBindings = DEFAULT_KEY_BINDINGS,
}: CompositorHostProps) {
	const snapshot = useSyncExternalStore(
		store.subscribe,
		store.getSnapshot,
		store.getSnapshot,
	);
	const [dragging, setDragging] = useState(false);
	const [preview, setPreview] = useState<ResizePreview | null>(null);
	const [paletteOpen, setPaletteOpen] = useState(false);
	const [body, setBody] = useState<HTMLDivElement | null>(null);
	const solveViewport = useMeasuredViewport(body, viewport);
	const lastWheel = useRef(0);
	const commit = useCallback(
		(commands: readonly LayoutCommand[]) => {
			store.commit(commands, solveViewport);
		},
		[store, solveViewport],
	);
	const shown = previewed(snapshot, solveViewport, preview);
	const projected = projectLayout(shown, solveViewport);
	const arrangement = activeOf(projected.snapshot);
	const canonicalArrangement = activeOf(snapshot);
	if (!arrangement || !canonicalArrangement) return null;
	const geometry = projected.geometry;
	const canonicalGeometry = preview
		? projectLayout(snapshot, solveViewport).geometry
		: geometry;
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
	const runAction = (action: CompositorAction) => {
		if (action.type === "undo") {
			store.undo();
			return;
		}
		if (action.type === "open-palette") {
			setPaletteOpen(true);
			return;
		}
		const commands = resolveAction(snapshot, canonicalGeometry, action);
		if (commands.length > 0) commit(commands);
	};
	const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
		const action = matchBinding(event, keyBindings);
		if (!action) return;
		event.preventDefault();
		runAction(action);
	};
	const onPreview = (
		axis: ResizeAxisName,
		index: number,
		deltaPx: number | null,
	) => setPreview(deltaPx === null ? null : { axis, index, deltaPx });
	const onResize = (axis: ResizeAxisName, index: number, deltaPx: number) => {
		const commands = resizeCommands(
			canonicalArrangement,
			canonicalGeometry,
			axis,
			index,
			deltaPx,
		);
		if (commands.length > 0) commit(commands);
	};
	const cell = (placement: GridPlacement, offset: number) => {
		const gridContext = {
			rowOffset: offset,
			rows: rows.length,
			columns: arrangement.grid.columns.length,
		};
		const container = containersById.get(placement.containerId);
		return container ? (
			<Cell
				key={container.id}
				container={container}
				placement={placement}
				grid={gridContext}
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
	const collapsedEdge = canonicalArrangement.grid.placements.find(
		(placement) => {
			const container =
				containersById.get(placement.containerId) ??
				snapshot.containers.find((c) => c.id === placement.containerId);
			return (
				placement.column === 0 &&
				placement.rowSpan >= canonicalArrangement.grid.rows.length &&
				container?.collapsed === true
			);
		},
	);
	const gutters =
		projected.merges.length > 0 ? null : (
			<>
				{arrangement.grid.columns.slice(0, -1).map((column, index) => (
					<Gutter
						key={column.id}
						axis="inline"
						index={index}
						label={labels.resizeColumns}
						onPreview={onPreview}
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
							onPreview={onPreview}
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
			data-previewing={preview ? true : undefined}
			onKeyDown={onKeyDown}
			onDragOver={() => {
				if (!dragging) setDragging(true);
			}}
			onDragEnd={() => setDragging(false)}
			onDrop={() => setDragging(false)}
		>
			{EDGE_ZONES.map(({ edge, labelKey }) => (
				<div
					key={edge}
					className="oqto-compositor-edge"
					data-edge={edge}
					aria-label={labels[labelKey]}
					onDragOver={(event) => event.preventDefault()}
					onDrop={onEdgeDrop(edge)}
				/>
			))}
			{collapsedEdge ? (
				<button
					type="button"
					className="oqto-compositor-expand"
					aria-label={labels.expandStart}
					onClick={() =>
						commit([
							{
								type: "collapse",
								containerId: collapsedEdge.containerId,
								collapsed: false,
							},
						])
					}
				/>
			) : null}
			{lane(flushTop)}
			<div
				className="oqto-compositor-body"
				ref={setBody}
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
							"--oqto-compositor-column-gap": `${solveViewport.gaps?.inline ?? 0}px`,
							"--oqto-compositor-row-gap": `${solveViewport.gaps?.block ?? 0}px`,
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
			{paletteOpen ? (
				<CommandPalette
					bindings={keyBindings}
					labels={labels.palette}
					onRun={runAction}
					onClose={() => setPaletteOpen(false)}
				/>
			) : null}
		</div>
	);
}

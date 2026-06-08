import { Button } from "@/components/ui/button";
import { useCallback, useRef, useState } from "react";
import type { SelectionRect } from "../types";

interface DragState {
	startX: number;
	startY: number;
}

export interface CropOverlayProps {
	onApply: (selection: SelectionRect) => void;
	onCancel: () => void;
}

const MIN_SIZE = 8;

/**
 * Draws a selection rectangle over the canvas using pointer capture (no global
 * listeners). Coordinates are in canvas CSS px, matching the editor viewport, so
 * the parent can map the selection straight onto source pixels.
 */
export function CropOverlay({ onApply, onCancel }: CropOverlayProps) {
	const [rect, setRect] = useState<SelectionRect | null>(null);
	const dragRef = useRef<DragState | null>(null);

	const localPoint = useCallback((e: React.PointerEvent) => {
		const bounds = e.currentTarget.getBoundingClientRect();
		return { x: e.clientX - bounds.left, y: e.clientY - bounds.top };
	}, []);

	const handlePointerDown = useCallback(
		(e: React.PointerEvent) => {
			const point = localPoint(e);
			dragRef.current = { startX: point.x, startY: point.y };
			setRect({ x: point.x, y: point.y, width: 0, height: 0 });
			e.currentTarget.setPointerCapture(e.pointerId);
		},
		[localPoint],
	);

	const handlePointerMove = useCallback(
		(e: React.PointerEvent) => {
			const drag = dragRef.current;
			if (!drag) return;
			const point = localPoint(e);
			setRect({
				x: Math.min(drag.startX, point.x),
				y: Math.min(drag.startY, point.y),
				width: Math.abs(point.x - drag.startX),
				height: Math.abs(point.y - drag.startY),
			});
		},
		[localPoint],
	);

	const handlePointerUp = useCallback((e: React.PointerEvent) => {
		dragRef.current = null;
		e.currentTarget.releasePointerCapture(e.pointerId);
	}, []);

	const valid =
		rect !== null && rect.width >= MIN_SIZE && rect.height >= MIN_SIZE;

	return (
		<div className="absolute inset-0">
			<div
				className="absolute inset-0 cursor-crosshair touch-none"
				onPointerDown={handlePointerDown}
				onPointerMove={handlePointerMove}
				onPointerUp={handlePointerUp}
			>
				{rect ? (
					<div
						className="absolute border border-primary bg-primary/10"
						style={{
							left: rect.x,
							top: rect.y,
							width: rect.width,
							height: rect.height,
						}}
					/>
				) : null}
			</div>
			<div className="pointer-events-none absolute inset-x-0 bottom-0 flex justify-center p-3">
				<div className="pointer-events-auto flex items-center gap-2 border border-border bg-popover px-2 py-1.5 text-popover-foreground shadow-lg">
					<span className="px-1 text-xs text-muted-foreground">
						{valid
							? `${Math.round(rect.width)} x ${Math.round(rect.height)}`
							: "Drag to select"}
					</span>
					<Button variant="ghost" size="sm" onClick={onCancel}>
						Cancel
					</Button>
					<Button
						size="sm"
						disabled={!valid}
						onClick={() => {
							if (rect && valid) onApply(rect);
						}}
					>
						Apply crop
					</Button>
				</div>
			</div>
		</div>
	);
}

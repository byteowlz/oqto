import { cn } from "@/lib/utils";
import { XIcon } from "lucide-react";
import { useRef } from "react";
import type { PoolImage, Tile } from "../types";
import { DRAG_MIME } from "./drag";

interface PanDrag {
	startX: number;
	startY: number;
	startPosX: number;
	startPosY: number;
	w: number;
	h: number;
}

function clamp(v: number, min: number, max: number): number {
	return Math.max(min, Math.min(max, v));
}

export interface GridTileProps {
	index: number;
	tile: Tile;
	image: PoolImage | null;
	selected: boolean;
	onSelect: (index: number) => void;
	onAssignDrop: (index: number, imageId: string) => void;
	onPanChange: (index: number, posX: number, posY: number) => void;
	onClear: (index: number) => void;
}

export function GridTile({
	index,
	tile,
	image,
	selected,
	onSelect,
	onAssignDrop,
	onPanChange,
	onClear,
}: GridTileProps) {
	const dragRef = useRef<PanDrag | null>(null);

	const handlePointerDown = (e: React.PointerEvent<HTMLImageElement>) => {
		if (!image) return;
		const rect = e.currentTarget.getBoundingClientRect();
		dragRef.current = {
			startX: e.clientX,
			startY: e.clientY,
			startPosX: tile.posX,
			startPosY: tile.posY,
			w: rect.width,
			h: rect.height,
		};
		e.currentTarget.setPointerCapture(e.pointerId);
	};

	const handlePointerMove = (e: React.PointerEvent<HTMLImageElement>) => {
		const drag = dragRef.current;
		if (!drag) return;
		const dxPct = ((e.clientX - drag.startX) / drag.w) * 100;
		const dyPct = ((e.clientY - drag.startY) / drag.h) * 100;
		onPanChange(
			index,
			clamp(drag.startPosX - dxPct, 0, 100),
			clamp(drag.startPosY - dyPct, 0, 100),
		);
	};

	const handlePointerUp = (e: React.PointerEvent<HTMLImageElement>) => {
		dragRef.current = null;
		e.currentTarget.releasePointerCapture(e.pointerId);
	};

	return (
		<div
			className={cn(
				"group relative overflow-hidden bg-muted/40",
				selected ? "ring-2 ring-primary ring-inset" : "",
			)}
		>
			<button
				type="button"
				className="absolute inset-0 h-full w-full text-left outline-none"
				aria-label={`Tile ${index + 1}`}
				onClick={() => onSelect(index)}
				onDragOver={(e) => {
					e.preventDefault();
					e.dataTransfer.dropEffect = "copy";
				}}
				onDrop={(e) => {
					e.preventDefault();
					const id = e.dataTransfer.getData(DRAG_MIME);
					if (id) onAssignDrop(index, id);
				}}
			>
				{image ? (
					<img
						src={image.url}
						alt=""
						draggable={false}
						className="absolute inset-0 h-full w-full cursor-grab touch-none select-none object-cover active:cursor-grabbing"
						style={{ objectPosition: `${tile.posX}% ${tile.posY}%` }}
						onPointerDown={handlePointerDown}
						onPointerMove={handlePointerMove}
						onPointerUp={handlePointerUp}
					/>
				) : (
					<span className="flex h-full w-full items-center justify-center text-xs text-muted-foreground">
						Empty
					</span>
				)}
			</button>
			{image ? (
				<button
					type="button"
					className="absolute right-1 top-1 z-10 hidden border border-border bg-popover p-0.5 text-popover-foreground group-hover:block"
					aria-label="Clear tile"
					onClick={() => onClear(index)}
				>
					<XIcon className="size-3" />
				</button>
			) : null}
		</div>
	);
}

"use client";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
	Download,
	Maximize2,
	Minimize2,
	RotateCw,
	ZoomIn,
	ZoomOut,
} from "lucide-react";
import { type WheelEvent, useCallback, useRef, useState } from "react";

interface ImageViewerProps {
	src: string;
	alt?: string;
	filename?: string;
	className?: string;
}

export function ImageViewer({
	src,
	alt,
	filename,
	className,
}: ImageViewerProps) {
	const [scale, setScale] = useState(1);
	const [rotation, setRotation] = useState(0);
	const [position, setPosition] = useState({ x: 0, y: 0 });
	const [isDragging, setIsDragging] = useState(false);
	const [isFitToScreen, setIsFitToScreen] = useState(true);
	const dragStart = useRef({ x: 0, y: 0 });
	const containerRef = useRef<HTMLDivElement>(null);

	const handleZoomIn = useCallback(() => {
		setScale((s) => Math.min(s * 1.25, 10));
		setIsFitToScreen(false);
	}, []);

	const handleZoomOut = useCallback(() => {
		setScale((s) => Math.max(s / 1.25, 0.1));
		setIsFitToScreen(false);
	}, []);

	const handleRotate = useCallback(() => {
		setRotation((r) => (r + 90) % 360);
	}, []);

	const handleReset = useCallback(() => {
		setScale(1);
		setRotation(0);
		setPosition({ x: 0, y: 0 });
		setIsFitToScreen(true);
	}, []);

	const handleWheel = useCallback((e: WheelEvent) => {
		e.preventDefault();
		const delta = e.deltaY > 0 ? 0.9 : 1.1;
		setScale((s) => Math.max(0.1, Math.min(10, s * delta)));
		setIsFitToScreen(false);
	}, []);

	const handleMouseDown = useCallback(
		(e: React.MouseEvent) => {
			if (scale > 1 || !isFitToScreen) {
				setIsDragging(true);
				dragStart.current = {
					x: e.clientX - position.x,
					y: e.clientY - position.y,
				};
			}
		},
		[scale, position, isFitToScreen],
	);

	const handleMouseMove = useCallback(
		(e: React.MouseEvent) => {
			if (isDragging) {
				setPosition({
					x: e.clientX - dragStart.current.x,
					y: e.clientY - dragStart.current.y,
				});
			}
		},
		[isDragging],
	);

	const handleMouseUp = useCallback(() => {
		setIsDragging(false);
	}, []);

	const handleDownload = useCallback(() => {
		const link = document.createElement("a");
		link.href = src;
		link.download = filename || "image";
		link.click();
	}, [src, filename]);

	return (
		<div className={cn("flex flex-col h-full", className)}>
			{/* Toolbar */}
			<div className="flex items-center justify-between px-3 py-2 bg-muted border-b border-border shrink-0">
				<div className="flex items-center gap-2">
					{filename && (
						<span className="text-sm font-medium truncate max-w-[200px]">
							{filename}
						</span>
					)}
					<span className="text-xs text-muted-foreground">
						{Math.round(scale * 100)}%
					</span>
				</div>
				<div className="flex items-center gap-1">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={handleZoomOut}
						title="Zoom out"
					>
						<ZoomOut className="w-4 h-4" />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={handleZoomIn}
						title="Zoom in"
					>
						<ZoomIn className="w-4 h-4" />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={handleRotate}
						title="Rotate"
					>
						<RotateCw className="w-4 h-4" />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={handleReset}
						title={isFitToScreen ? "Actual size" : "Fit to screen"}
					>
						{isFitToScreen ? (
							<Maximize2 className="w-4 h-4" />
						) : (
							<Minimize2 className="w-4 h-4" />
						)}
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={handleDownload}
						title="Download"
					>
						<Download className="w-4 h-4" />
					</Button>
				</div>
			</div>

			{/* Image container */}
			<div
				ref={containerRef}
				className="flex-1 overflow-hidden bg-[repeating-conic-gradient(var(--muted)_0%_25%,transparent_0%_50%)] bg-[length:20px_20px] flex items-center justify-center"
				onWheel={handleWheel}
				onMouseDown={handleMouseDown}
				onMouseMove={handleMouseMove}
				onMouseUp={handleMouseUp}
				onMouseLeave={handleMouseUp}
				style={{
					cursor: isDragging
						? "grabbing"
						: scale > 1 || !isFitToScreen
							? "grab"
							: "default",
				}}
			>
				<img
					src={src}
					alt={alt || filename || "Image preview"}
					className={cn(
						"transition-transform",
						isFitToScreen && "max-w-full max-h-full object-contain",
					)}
					style={{
						transform: `translate(${position.x}px, ${position.y}px) scale(${scale}) rotate(${rotation}deg)`,
						transformOrigin: "center center",
					}}
					draggable={false}
				/>
			</div>
		</div>
	);
}

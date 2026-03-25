"use client";

import { useDocumentEvent } from "@/hooks/use-document-event";
import { useResetOnOpen } from "@/hooks/use-reset-on-open";
import { readFileMux } from "@/lib/mux-files";
import { cn } from "@/lib/utils";
import {
	ChevronLeft,
	ChevronRight,
	Download,
	Loader2,
	Maximize2,
	RotateCw,
	X,
	ZoomIn,
	ZoomOut,
} from "lucide-react";
import { memo, useCallback, useEffect, useRef, useState } from "react";

export interface LightboxItem {
	type: "image" | "video";
	/** For videos: duration in seconds */
	duration?: number;
	/** File path relative to workspace */
	path: string;
	/** File name */
	filename: string;
}

export interface LightboxGalleryProps {
	/** Whether lightbox is open */
	open: boolean;
	/** Items to display */
	items: LightboxItem[];
	/** Initial index */
	initialIndex?: number;
	/** Callback when closed */
	onClose: () => void;
	/** Workspace path for building URLs */
	workspacePath?: string | null;
}

export const LightboxGallery = memo(function LightboxGallery({
	open,
	items,
	initialIndex = 0,
	onClose,
	workspacePath,
}: LightboxGalleryProps) {
	const [currentIndex, setCurrentIndex] = useState(initialIndex);
	const [zoom, setZoom] = useState(1);
	const [rotation, setRotation] = useState(0);
	const [isFullscreen, setIsFullscreen] = useState(false);
	const [mediaBlobUrl, setMediaBlobUrl] = useState<string | null>(null);
	const [mediaLoading, setMediaLoading] = useState(false);

	const containerRef = useRef<HTMLDivElement>(null);
	const imageRef = useRef<HTMLImageElement>(null);
	const blobCacheRef = useRef<Map<string, string>>(new Map());

	// Touch swipe state
	const touchStartRef = useRef<{ x: number; y: number; t: number } | null>(
		null,
	);
	const [swipeOffset, setSwipeOffset] = useState(0);

	// Reset state when lightbox opens, cleanup blobs when it closes
	useResetOnOpen(
		open,
		() => {
			// Revoke old blobs before resetting
			for (const url of blobCacheRef.current.values()) {
				URL.revokeObjectURL(url);
			}
			blobCacheRef.current.clear();
			setMediaBlobUrl(null);
			// Reset view state
			setCurrentIndex(initialIndex);
			setZoom(1);
			setRotation(0);
		},
		[initialIndex],
	);

	const currentItem = items[currentIndex];
	const isVideo = currentItem?.type === "video";
	const isImage = currentItem?.type === "image";

	// Fetch current media via mux
	// biome-ignore lint/correctness/useExhaustiveDependencies: currentIndex drives currentItem, readFileMux is module-level
	useEffect(() => {
		if (!open || !currentItem || !workspacePath) return;

		const cached = blobCacheRef.current.get(currentItem.path);
		if (cached) {
			setMediaBlobUrl(cached);
			return;
		}

		let cancelled = false;
		setMediaLoading(true);
		setMediaBlobUrl(null);

		readFileMux(workspacePath, currentItem.path)
			.then(({ data }) => {
				if (cancelled) return;
				const ext = currentItem.filename
					.substring(currentItem.filename.lastIndexOf("."))
					.toLowerCase();
				const mimeMap: Record<string, string> = {
					".png": "image/png",
					".jpg": "image/jpeg",
					".jpeg": "image/jpeg",
					".gif": "image/gif",
					".webp": "image/webp",
					".svg": "image/svg+xml",
					".bmp": "image/bmp",
					".mp4": "video/mp4",
					".webm": "video/webm",
					".ogg": "video/ogg",
					".mov": "video/quicktime",
					".mkv": "video/x-matroska",
				};
				const blob = new Blob([data], {
					type: mimeMap[ext] ?? "application/octet-stream",
				});
				const url = URL.createObjectURL(blob);
				blobCacheRef.current.set(currentItem.path, url);
				setMediaBlobUrl(url);
			})
			.catch(() => {
				if (!cancelled) setMediaBlobUrl(null);
			})
			.finally(() => {
				if (!cancelled) setMediaLoading(false);
			});

		return () => {
			cancelled = true;
		};
	}, [open, currentIndex, currentItem, workspacePath]);

	// Keyboard shortcuts
	const handleKeyDown = useCallback(
		(e: KeyboardEvent) => {
			if (!open) return;

			switch (e.key) {
				case "Escape":
					onClose();
					break;
				case "ArrowLeft":
					e.preventDefault();
					setCurrentIndex((prev) => (prev - 1 + items.length) % items.length);
					break;
				case "ArrowRight":
					e.preventDefault();
					setCurrentIndex((prev) => (prev + 1) % items.length);
					break;
				case " ":
				case "k": {
					e.preventDefault();
					// Handle video play/pause
					const video = document.querySelector("video[controls]");
					if (video instanceof HTMLVideoElement) {
						if (video.paused) video.play();
						else video.pause();
					}
					break;
				}
				case "+":
				case "=":
					e.preventDefault();
					setZoom((prev) => Math.min(5, prev + 0.5));
					break;
				case "-":
				case "_":
					e.preventDefault();
					setZoom((prev) => Math.max(0.5, prev - 0.5));
					break;
				case "r":
					e.preventDefault();
					setRotation((prev) => (prev + 90) % 360);
					break;
				case "0":
					e.preventDefault();
					setZoom(1);
					setRotation(0);
					break;
				case "f":
					e.preventDefault();
					if (document.fullscreenElement) {
						document.exitFullscreen();
					} else if (containerRef.current) {
						containerRef.current.requestFullscreen();
					}
					break;
			}
		},
		[open, items.length, onClose],
	);

	useDocumentEvent("keydown", handleKeyDown, open);

	useDocumentEvent(
		"fullscreenchange",
		useCallback(() => {
			setIsFullscreen(!!document.fullscreenElement);
		}, []),
	);

	const handleZoomIn = () => {
		setZoom((prev) => Math.min(5, prev + 0.5));
	};

	const handleZoomOut = () => {
		setZoom((prev) => Math.max(0.5, prev - 0.5));
	};

	const handleRotate = () => {
		setRotation((prev) => (prev + 90) % 360);
	};

	const handleReset = () => {
		setZoom(1);
		setRotation(0);
	};

	const handlePrevious = () => {
		setCurrentIndex((prev) => (prev - 1 + items.length) % items.length);
	};

	const handleNext = () => {
		setCurrentIndex((prev) => (prev + 1) % items.length);
	};

	const handleDownload = () => {
		if (!currentItem || !mediaBlobUrl) return;

		const a = document.createElement("a");
		a.href = mediaBlobUrl;
		a.download = currentItem.filename;
		a.click();
	};

	const getTransformStyle = () => {
		if (!isImage) return {};

		return {
			transform: `scale(${zoom}) rotate(${rotation}deg)`,
			transition: "transform 0.2s ease-out",
		};
	};

	// Touch swipe handlers
	const handleTouchStart = useCallback(
		(e: React.TouchEvent) => {
			if (zoom > 1) return; // Don't swipe when zoomed in
			const touch = e.touches[0];
			touchStartRef.current = {
				x: touch.clientX,
				y: touch.clientY,
				t: Date.now(),
			};
			setSwipeOffset(0);
		},
		[zoom],
	);

	const handleTouchMove = useCallback(
		(e: React.TouchEvent) => {
			if (!touchStartRef.current || zoom > 1) return;
			const touch = e.touches[0];
			const dx = touch.clientX - touchStartRef.current.x;
			const dy = touch.clientY - touchStartRef.current.y;

			// Only track horizontal swipes (ignore vertical scroll)
			if (Math.abs(dx) > Math.abs(dy) * 1.5) {
				e.preventDefault();
				setSwipeOffset(dx);
			}
		},
		[zoom],
	);

	const handleTouchEnd = useCallback(() => {
		if (!touchStartRef.current || zoom > 1) {
			touchStartRef.current = null;
			return;
		}

		const elapsed = Date.now() - touchStartRef.current.t;
		const velocity = Math.abs(swipeOffset) / Math.max(elapsed, 1);
		const threshold = 50; // px
		const velocityThreshold = 0.3; // px/ms

		if (
			swipeOffset < -threshold ||
			(swipeOffset < 0 && velocity > velocityThreshold)
		) {
			// Swipe left → next
			setCurrentIndex((prev) => (prev + 1) % items.length);
			setZoom(1);
			setRotation(0);
		} else if (
			swipeOffset > threshold ||
			(swipeOffset > 0 && velocity > velocityThreshold)
		) {
			// Swipe right → previous
			setCurrentIndex((prev) => (prev - 1 + items.length) % items.length);
			setZoom(1);
			setRotation(0);
		}

		touchStartRef.current = null;
		setSwipeOffset(0);
	}, [swipeOffset, zoom, items.length]);

	if (!open || !currentItem) return null;

	return (
		// biome-ignore lint/a11y/useKeyWithClickEvents: keyboard handled via document listener
		<div
			className="fixed inset-0 z-[100] bg-black flex flex-col"
			onClick={onClose}
		>
			{/* Header */}
			<div className="flex-shrink-0 flex items-center justify-between p-4 bg-black/50 backdrop-blur-sm">
				<div className="flex items-center gap-4 text-white">
					{/* Counter */}
					<span className="text-sm font-medium">
						{currentIndex + 1} / {items.length}
					</span>

					{/* Filename */}
					<span
						className="text-sm text-muted-foreground truncate max-w-md"
						title={currentItem.filename}
					>
						{currentItem.filename}
					</span>

					{/* File size (if available) */}
					{currentItem.duration && (
						<span className="text-sm text-muted-foreground">
							{Math.floor(currentItem.duration / 60)}:
							{Math.floor(currentItem.duration % 60)
								.toString()
								.padStart(2, "0")}
						</span>
					)}
				</div>

				<div className="flex items-center gap-2">
					{/* Zoom controls */}
					<div className="flex items-center gap-1">
						<button
							type="button"
							onClick={handleZoomOut}
							className="p-2 rounded hover:bg-white/10 transition-colors text-white"
							title="Zoom Out (-)"
						>
							<ZoomOut className="w-5 h-5" />
						</button>
						<span className="text-sm text-white font-mono w-12 text-center">
							{Math.round(zoom * 100)}%
						</span>
						<button
							type="button"
							onClick={handleZoomIn}
							className="p-2 rounded hover:bg-white/10 transition-colors text-white"
							title="Zoom In (+)"
						>
							<ZoomIn className="w-5 h-5" />
						</button>
					</div>

					{/* Rotate (images only) */}
					{isImage && (
						<button
							type="button"
							onClick={handleRotate}
							className="p-2 rounded hover:bg-white/10 transition-colors text-white"
							title="Rotate (R)"
						>
							<RotateCw className="w-5 h-5" />
						</button>
					)}

					{/* Fullscreen */}
					<button
						type="button"
						onClick={() => {
							if (document.fullscreenElement) {
								document.exitFullscreen();
							} else if (containerRef.current) {
								containerRef.current.requestFullscreen();
							}
						}}
						className="p-2 rounded hover:bg-white/10 transition-colors text-white"
						title="Fullscreen (F)"
					>
						{isFullscreen ? (
							<Maximize2 className="w-5 h-5" />
						) : (
							<Maximize2 className="w-5 h-5" />
						)}
					</button>

					{/* Download */}
					<button
						type="button"
						onClick={handleDownload}
						className="p-2 rounded hover:bg-white/10 transition-colors text-white"
						title="Download"
					>
						<Download className="w-5 h-5" />
					</button>

					{/* Reset */}
					<button
						type="button"
						onClick={handleReset}
						className="p-2 rounded hover:bg-white/10 transition-colors text-white"
						title="Reset (0)"
					>
						0
					</button>

					{/* Close */}
					<button
						type="button"
						onClick={onClose}
						className="p-2 rounded hover:bg-white/10 transition-colors text-white"
						title="Close (Esc)"
					>
						<X className="w-5 h-5" />
					</button>
				</div>
			</div>

			{/* Main content */}
			<div
				className="flex-1 flex items-center justify-center overflow-hidden touch-none"
				onTouchStart={handleTouchStart}
				onTouchMove={handleTouchMove}
				onTouchEnd={handleTouchEnd}
			>
				<div
					ref={containerRef}
					className="relative w-full h-full flex items-center justify-center"
					style={{
						transform:
							swipeOffset !== 0 ? `translateX(${swipeOffset}px)` : undefined,
						transition: swipeOffset !== 0 ? "none" : "transform 0.2s ease-out",
					}}
				>
					{/* Navigation buttons */}
					<button
						type="button"
						onClick={handlePrevious}
						className="absolute left-4 top-1/2 -translate-y-1/2 p-2 rounded-full bg-black/50 backdrop-blur-sm hover:bg-black/70 transition-colors text-white"
						title="Previous (←)"
					>
						<ChevronLeft className="w-6 h-6" />
					</button>

					<button
						type="button"
						onClick={handleNext}
						className="absolute right-4 top-1/2 -translate-y-1/2 p-2 rounded-full bg-black/50 backdrop-blur-sm hover:bg-black/70 transition-colors text-white"
						title="Next (→)"
					>
						<ChevronRight className="w-6 h-6" />
					</button>

					{/* Media */}
					{mediaLoading && (
						<Loader2 className="w-8 h-8 text-white/50 animate-spin" />
					)}

					{!mediaLoading && mediaBlobUrl && isImage && (
						<img
							ref={imageRef}
							src={mediaBlobUrl}
							alt={currentItem.filename}
							className="max-w-full max-h-full object-contain"
							style={getTransformStyle()}
						/>
					)}

					{!mediaLoading && mediaBlobUrl && isVideo && (
						// biome-ignore lint/a11y/useMediaCaption: lightbox video doesn't need captions
						<video
							src={mediaBlobUrl}
							className="max-w-full max-h-full object-contain"
							controls
							preload="metadata"
							autoPlay
						/>
					)}
				</div>
			</div>

			{/* Filmstrip */}
			{items.length > 1 && (
				// biome-ignore lint/a11y/useKeyWithClickEvents: filmstrip is supplementary to keyboard nav
				<div
					className="flex-shrink-0 bg-black/90 border-t border-white/10 px-4 py-2 overflow-x-auto scrollbar-none [scrollbar-width:none] [-ms-overflow-style:none] [&::-webkit-scrollbar]:hidden"
					onClick={(e) => e.stopPropagation()}
				>
					<div className="flex gap-1.5 justify-center min-w-min">
						{items.map((item, index) => (
							<button
								key={item.path}
								type="button"
								onClick={() => {
									setCurrentIndex(index);
									setZoom(1);
									setRotation(0);
								}}
								className={cn(
									"flex-shrink-0 w-16 h-12 rounded overflow-hidden border-2 transition-all",
									index === currentIndex
										? "border-primary opacity-100 scale-105"
										: "border-transparent opacity-50 hover:opacity-80",
								)}
								title={item.filename}
							>
								{blobCacheRef.current.has(item.path) ? (
									item.type === "image" ? (
										<img
											src={blobCacheRef.current.get(item.path)}
											alt={item.filename}
											className="w-full h-full object-cover"
										/>
									) : (
										<div className="w-full h-full bg-muted/30 flex items-center justify-center text-white/60 text-[10px]">
											▶
										</div>
									)
								) : (
									<div
										className={cn(
											"w-full h-full flex items-center justify-center text-[8px]",
											item.type === "video" ? "text-white/40" : "text-white/30",
										)}
									>
										{item.type === "video" ? "▶" : "◻"}
									</div>
								)}
							</button>
						))}
					</div>
				</div>
			)}

			{/* Keyboard shortcuts footer */}
			<div className="flex-shrink-0 bg-black/80 backdrop-blur-sm px-6 py-1.5">
				<div className="flex items-center justify-center gap-4 text-[10px] text-white/40">
					<span>←/→ Navigate</span>
					<span>Space Play/Pause</span>
					<span>+/- Zoom</span>
					<span>R Rotate</span>
					<span>0 Reset</span>
					<span>F Fullscreen</span>
					<span>Esc Close</span>
				</div>
			</div>
		</div>
	);
});

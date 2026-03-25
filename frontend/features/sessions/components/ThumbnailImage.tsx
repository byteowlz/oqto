"use client";

import { fetchThumbnailUrl } from "@/lib/thumbnail-utils";
import { cn } from "@/lib/utils";
import { Film, Image as ImageIcon, Loader2 } from "lucide-react";
import { memo, useCallback, useEffect, useRef, useState } from "react";

export interface ThumbnailImageProps {
	/** Workspace path for fetching via mux */
	workspacePath: string;
	/** File path relative to workspace */
	filePath: string;
	/** File name */
	filename: string;
	/** Whether this is a video file */
	isVideo?: boolean;
	/** Video source URL for hover preview (blob URL) */
	videoSrc?: string;
	/** Optional duration badge for videos */
	duration?: string;
	/** Thumbnail size for layout */
	size?: number;
	/** On click handler */
	onClick?: () => void;
	/** Whether item is selected */
	selected?: boolean;
	/** Additional class names */
	className?: string;
}

export const ThumbnailImage = memo(function ThumbnailImage({
	workspacePath,
	filePath,
	filename,
	isVideo = false,
	videoSrc,
	duration,
	size = 128,
	onClick,
	selected = false,
	className,
}: ThumbnailImageProps) {
	const [blobUrl, setBlobUrl] = useState<string | null>(null);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState(false);
	const [showVideoPreview, setShowVideoPreview] = useState(false);

	const containerRef = useRef<HTMLDivElement>(null);
	const videoPreviewRef = useRef<HTMLVideoElement>(null);
	const hoverTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const mountedRef = useRef(true);

	// Fetch thumbnail via mux when visible
	// useeffect-guardrail: allow
	useEffect(() => {
		mountedRef.current = true;
		let cancelled = false;

		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0]?.isIntersecting && !blobUrl && !error) {
					fetchThumbnailUrl({ workspacePath, filePath })
						.then((url) => {
							if (cancelled || !mountedRef.current) return;
							if (url) {
								setBlobUrl(url);
								setLoading(false);
							} else {
								setError(true);
								setLoading(false);
							}
						})
						.catch(() => {
							if (!cancelled && mountedRef.current) {
								setError(true);
								setLoading(false);
							}
						});
					observer.disconnect();
				}
			},
			{ rootMargin: "100px" },
		);

		if (containerRef.current) {
			observer.observe(containerRef.current);
		}

		return () => {
			cancelled = true;
			mountedRef.current = false;
			observer.disconnect();
		};
	}, [workspacePath, filePath, blobUrl, error]);

	// Hover video preview handlers
	const handleMouseEnter = useCallback(() => {
		if (!isVideo || !videoSrc) return;
		hoverTimerRef.current = setTimeout(() => {
			setShowVideoPreview(true);
			// Start playback directly when showing preview
			requestAnimationFrame(() => {
				videoPreviewRef.current?.play().catch(() => {});
			});
		}, 800);
	}, [isVideo, videoSrc]);

	const handleMouseLeave = useCallback(() => {
		setShowVideoPreview(false);
		if (hoverTimerRef.current) {
			clearTimeout(hoverTimerRef.current);
			hoverTimerRef.current = null;
		}
		if (videoPreviewRef.current) {
			videoPreviewRef.current.pause();
			videoPreviewRef.current.currentTime = 0;
		}
	}, []);

	// useeffect-guardrail: allow
	useEffect(() => {
		return () => {
			if (hoverTimerRef.current) clearTimeout(hoverTimerRef.current);
		};
	}, []);

	const showThumbnail = blobUrl && !error;

	return (
		// biome-ignore lint/a11y/useKeyWithClickEvents: thumbnail click triggers gallery, keyboard nav handled at gallery level
		<div
			ref={containerRef}
			className={cn(
				"relative flex-shrink-0 rounded-md overflow-hidden cursor-pointer",
				"bg-muted/50 hover:bg-muted/80",
				"transition-colors duration-150",
				selected && "ring-2 ring-primary ring-offset-2 ring-offset-background",
				className,
			)}
			style={{ width: size, height: size }}
			onClick={onClick}
			onMouseEnter={handleMouseEnter}
			onMouseLeave={handleMouseLeave}
		>
			{/* Thumbnail image */}
			{showThumbnail && (
				<img
					src={blobUrl}
					alt={filename}
					className="w-full h-full object-cover"
				/>
			)}

			{/* Placeholder */}
			{!showThumbnail && (
				<div className="w-full h-full flex items-center justify-center">
					{loading ? (
						<Loader2 className="w-6 h-6 text-muted-foreground animate-spin" />
					) : isVideo ? (
						<Film className="w-6 h-6 text-muted-foreground" />
					) : (
						<ImageIcon className="w-6 h-6 text-muted-foreground" />
					)}
				</div>
			)}

			{/* Video hover preview */}
			{showVideoPreview && videoSrc && (
				<video
					ref={videoPreviewRef}
					src={videoSrc}
					className="absolute inset-0 w-full h-full object-cover"
					muted
					loop
					playsInline
				/>
			)}

			{/* Video indicator / duration badge */}
			{isVideo && (
				<div className="absolute bottom-1 right-1 flex items-center gap-1">
					{showVideoPreview && (
						<span className="bg-primary/80 text-primary-foreground text-[8px] px-1 rounded">
							▶
						</span>
					)}
					{duration && (
						<span className="bg-black/70 text-white text-[8px] px-1 rounded">
							{duration}
						</span>
					)}
				</div>
			)}
		</div>
	);
});

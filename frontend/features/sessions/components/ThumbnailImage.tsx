"use client";

import { useIntersectionOnce } from "@/hooks/use-intersection-observer";
import { fetchThumbnailUrl } from "@/lib/thumbnail-utils";
import { cn } from "@/lib/utils";
import { Film, Image as ImageIcon, Loader2 } from "lucide-react";
import { memo, useCallback, useRef, useState } from "react";

export interface ThumbnailImageProps {
	workspacePath: string;
	filePath: string;
	filename: string;
	isVideo?: boolean;
	videoSrc?: string;
	duration?: string;
	size?: number;
	onClick?: () => void;
	selected?: boolean;
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

	const videoPreviewRef = useRef<HTMLVideoElement>(null);
	const hoverTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

	// Fetch thumbnail when element enters viewport
	const intersectionRef = useIntersectionOnce(
		() => {
			fetchThumbnailUrl({ workspacePath, filePath })
				.then((url) => {
					if (url) {
						setBlobUrl(url);
					} else {
						setError(true);
					}
				})
				.catch(() => setError(true))
				.finally(() => setLoading(false));
		},
		{ enabled: !blobUrl && !error },
	);

	const handleMouseEnter = useCallback(() => {
		if (!isVideo || !videoSrc) return;
		hoverTimerRef.current = setTimeout(() => {
			setShowVideoPreview(true);
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

	const showThumbnail = blobUrl && !error;

	return (
		// biome-ignore lint/a11y/useKeyWithClickEvents: thumbnail click triggers gallery, keyboard nav handled at gallery level
		<div
			ref={intersectionRef}
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
			{showThumbnail && (
				<img
					src={blobUrl}
					alt={filename}
					className="w-full h-full object-cover"
				/>
			)}

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

"use client";

import { cn } from "@/lib/utils";
import { Image as ImageIcon, Film, Loader2 } from "lucide-react";
import { memo, useCallback, useState, useRef, useEffect } from "react";

export interface ThumbnailImageProps {
	src: string | null;
	alt: string;
	/** File name to display if thumbnail fails or loading */
	filename: string;
	/** File extension for icon fallback */
	extension?: string;
	/** Whether this is a video file */
	isVideo?: boolean;
	/** Optional duration badge for videos */
	duration?: string;
	/** Thumbnail size for layout */
	size?: number;
	/** Whether image is currently loading */
	loading?: boolean;
	/** Error state */
	error?: boolean;
	/** On click handler */
	onClick?: () => void;
	/** On double click handler */
	onDoubleClick?: () => void;
	/** Whether item is selected */
	selected?: boolean;
	/** Additional class names */
	className?: string;
	/** Image priority for loading */
	priority?: "high" | "low" | "auto";
}

export const ThumbnailImage = memo(function ThumbnailImage({
	src,
	alt,
	filename,
	extension,
	isVideo = false,
	duration,
	size = 128,
	loading: externalLoading = false,
	error: externalError = false,
	onClick,
	onDoubleClick,
	selected = false,
	className,
	priority = "auto",
}: ThumbnailImageProps) {
	const [internalLoading, setInternalLoading] = useState(true);
	const [internalError, setInternalError] = useState(false);
	const [imageLoaded, setImageLoaded] = useState(false);
	const imgRef = useRef<HTMLImageElement>(null);
	const observerRef = useRef<IntersectionObserver | null>(null);

	const isLoading = externalLoading || internalLoading;
	const hasError = externalError || internalError;
	const showThumbnail = src && !hasError && imageLoaded;
	const showPlaceholder = !showThumbnail;

	// Handle image load
	const handleLoad = useCallback(() => {
		setInternalLoading(false);
		setInternalError(false);
		setImageLoaded(true);
	}, []);

	// Handle image error
	const handleError = useCallback(() => {
		setInternalLoading(false);
		setInternalError(true);
		setImageLoaded(false);
	}, []);

	// Intersection observer for lazy loading
	useEffect(() => {
		const img = imgRef.current;
		if (!img || !src || priority === "high") return;

		const observer = new IntersectionObserver(
			(entries) => {
				for (const entry of entries) {
					if (entry.isIntersecting) {
						setInternalLoading(true);
					}
				}
			},
			{ rootMargin: "50px" },
		);

		observer.observe(img);
		observerRef.current = observer;

		return () => {
			observer.disconnect();
			observerRef.current = null;
		};
	}, [src, priority]);

	// Cleanup observer on unmount
	useEffect(() => {
		return () => {
			if (observerRef.current) {
				observerRef.current.disconnect();
			}
		};
	}, []);

	// Reset state when src changes
	useEffect(() => {
		if (src) {
			setInternalLoading(true);
			setInternalError(false);
			setImageLoaded(false);
		}
	}, [src]);

	// Calculate aspect-ratio styles
	const style = {
		"--thumbnail-size": `${size}px`,
	} as React.CSSProperties;

	return (
		<div
			className={cn(
				"relative flex-shrink-0 rounded-md overflow-hidden",
				"bg-muted/50",
				"hover:bg-muted/80",
				"transition-colors duration-150",
				selected && "ring-2 ring-primary ring-offset-2 ring-offset-background",
				className,
			)}
			style={style}
			onClick={onClick}
			onDoubleClick={onDoubleClick}
		>
			{showThumbnail && src ? (
				<img
					ref={imgRef}
					src={src}
					alt={alt}
					width={size}
					height={size}
					loading="lazy"
					className="w-full h-full object-cover"
					onLoad={handleLoad}
					onError={handleError}
					style={{
						aspectRatio: "1/1",
					}}
				/>
			) : null}

			{showPlaceholder && (
				<div className="w-full h-full flex items-center justify-center">
					{isLoading ? (
						<Loader2 className="w-6 h-6 text-muted-foreground animate-spin" />
					) : (
						<>
							{isVideo ? (
								<Film className="w-6 h-6 text-muted-foreground" />
							) : (
								<ImageIcon className="w-6 h-6 text-muted-foreground" />
							)}
							{duration && !isLoading && (
								<div className="absolute bottom-1 right-1 bg-black/70 text-white text-[8px] px-1 rounded">
									{duration}
								</div>
							)}
						</>
					)}
				</div>
			)}
		</div>
	);
});

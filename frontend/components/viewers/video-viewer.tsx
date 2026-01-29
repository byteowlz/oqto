"use client";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Download, FileVideo, Maximize, Volume2, VolumeX } from "lucide-react";
import { useCallback, useRef, useState } from "react";

interface VideoViewerProps {
	src: string;
	filename?: string;
	className?: string;
}

export function VideoViewer({ src, filename, className }: VideoViewerProps) {
	const videoRef = useRef<HTMLVideoElement>(null);
	const [isMuted, setIsMuted] = useState(false);
	const [isFullscreen, setIsFullscreen] = useState(false);
	const containerRef = useRef<HTMLDivElement>(null);

	const handleDownload = useCallback(() => {
		const link = document.createElement("a");
		link.href = src;
		link.download = filename || "video";
		link.click();
	}, [src, filename]);

	const handleToggleMute = useCallback(() => {
		if (videoRef.current) {
			videoRef.current.muted = !videoRef.current.muted;
			setIsMuted(videoRef.current.muted);
		}
	}, []);

	const handleFullscreen = useCallback(() => {
		if (!containerRef.current) return;

		if (!document.fullscreenElement) {
			containerRef.current.requestFullscreen().then(() => {
				setIsFullscreen(true);
			});
		} else {
			document.exitFullscreen().then(() => {
				setIsFullscreen(false);
			});
		}
	}, []);

	return (
		<div ref={containerRef} className={cn("flex flex-col h-full", className)}>
			{/* Toolbar */}
			<div className="flex items-center justify-between px-3 py-2 bg-muted border-b border-border shrink-0">
				<div className="flex items-center gap-2">
					<FileVideo className="w-4 h-4 text-muted-foreground" />
					{filename && (
						<span className="text-sm font-medium truncate max-w-[200px]">
							{filename}
						</span>
					)}
				</div>
				<div className="flex items-center gap-1">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={handleToggleMute}
						title={isMuted ? "Unmute" : "Mute"}
					>
						{isMuted ? (
							<VolumeX className="w-4 h-4" />
						) : (
							<Volume2 className="w-4 h-4" />
						)}
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={handleFullscreen}
						title="Fullscreen"
					>
						<Maximize className="w-4 h-4" />
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

			{/* Video container */}
			<div className="flex-1 overflow-hidden bg-black flex items-center justify-center">
				<video
					ref={videoRef}
					src={src}
					controls
					playsInline
					className="max-w-full max-h-full"
				>
					<track kind="captions" />
					Your browser does not support the video tag.
				</video>
			</div>
		</div>
	);
}

"use client";

import { cn } from "@/lib/utils";
import { Download, Maximize2, Volume2, VolumeX, X } from "lucide-react";
import { memo, useCallback, useEffect, useRef, useState } from "react";

export interface VideoPreviewModalProps {
	/** Whether modal is open */
	open: boolean;
	/** Video URL to play */
	src: string;
	/** File name */
	filename: string;
	/** File path for download action */
	filePath?: string;
	/** Callback when modal is closed */
	onClose: () => void;
	/** Callback to enter fullscreen */
	onFullscreen?: () => void;
}

export const VideoPreviewModal = memo(function VideoPreviewModal({
	open,
	src,
	filename,
	filePath,
	onClose,
	onFullscreen,
}: VideoPreviewModalProps) {
	const [isPlaying, setIsPlaying] = useState(false);
	const [currentTime, setCurrentTime] = useState(0);
	const [duration, setDuration] = useState(0);
	const [volume, setVolume] = useState(1);
	const [isMuted, setIsMuted] = useState(false);
	const [isFullscreen, setIsFullscreen] = useState(false);

	const videoRef = useRef<HTMLVideoElement>(null);
	const containerRef = useRef<HTMLDivElement>(null);

	// Handle keyboard shortcuts
	const handleKeyDown = useCallback(
		(e: KeyboardEvent) => {
			if (!open) return;

			switch (e.key) {
				case "Escape":
					onClose();
					break;
				case " ":
				case "k":
					if (isPlaying) setIsPlaying(false);
					break;
				case "ArrowRight":
					e.preventDefault();
					if (videoRef.current) videoRef.current.currentTime += 5;
					break;
				case "ArrowLeft":
					e.preventDefault();
					if (videoRef.current)
						videoRef.current.currentTime = Math.max(
							0,
							videoRef.current.currentTime - 5,
						);
					break;
				case "m":
					setIsMuted((prev) => !prev);
					break;
				case "ArrowUp":
					e.preventDefault();
					setVolume((prev) => Math.min(1, prev + 0.1));
					break;
				case "ArrowDown":
					e.preventDefault();
					setVolume((prev) => Math.max(0, prev - 0.1));
					break;
				case "f":
					e.preventDefault();
					onFullscreen?.();
					break;
			}
		},
		[open, isPlaying, onFullscreen, onClose],
	);

	// Setup keyboard listeners
	// useeffect-guardrail: allow
	useEffect(() => {
		if (!open) return;

		const handler = (e: KeyboardEvent) => handleKeyDown(e);
		document.addEventListener("keydown", handler);
		return () => {
			document.removeEventListener("keydown", handler);
		};
	}, [open, handleKeyDown]);

	// Reset state when modal opens/closes
	// useeffect-guardrail: allow
	useEffect(() => {
		if (!open) {
			setIsPlaying(false);
			setCurrentTime(0);
			setDuration(0);
		}
	}, [open]);

	const handleTimeUpdate = () => {
		const video = videoRef.current;
		if (video) {
			setCurrentTime(video.currentTime);
			if (duration === 0 && video.duration) {
				setDuration(video.duration);
			}
		}
	};

	const handleVolumeChange = (e: React.ChangeEvent<HTMLInputElement>) => {
		setVolume(Number.parseFloat(e.target.value));
	};

	const togglePlayPause = () => {
		setIsPlaying((prev) => !prev);
		if (videoRef.current) {
			if (isPlaying) {
				videoRef.current.pause();
			} else {
				videoRef.current.play();
			}
		}
	};

	const toggleMute = () => {
		setIsMuted((prev) => !prev);
	};

	const formatTime = (seconds: number): string => {
		const mins = Math.floor(seconds / 60);
		const secs = Math.floor(seconds % 60);
		return `${mins}:${secs.toString().padStart(2, "0")}`;
	};

	const progress = duration > 0 ? (currentTime / duration) * 100 : 0;

	if (!open) return null;

	return (
		// biome-ignore lint/a11y/useKeyWithClickEvents: keyboard handled via document listener
		<div
			className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm"
			onClick={onClose}
		>
			{/* biome-ignore lint/a11y/useKeyWithClickEvents: keyboard handled via document listener */}
			<div
				ref={containerRef}
				className={cn(
					"relative bg-card rounded-lg overflow-hidden shadow-2xl",
					isFullscreen
						? "fixed inset-0 rounded-none"
						: "max-w-4xl w-full max-h-[90vh]",
				)}
				onClick={(e) => e.stopPropagation()}
			>
				{/* Header */}
				<div className="flex items-center justify-between p-4 border-b border-border bg-muted/30">
					<div className="flex-1 min-w-0">
						<h3
							className="text-sm font-medium text-foreground truncate"
							title={filename}
						>
							{filename}
						</h3>
					</div>
					<div className="flex items-center gap-2">
						<button
							type="button"
							onClick={onFullscreen}
							className="p-2 rounded-md hover:bg-muted transition-colors"
							title="Fullscreen"
						>
							<Maximize2 className="w-4 h-4 text-foreground" />
						</button>
						<button
							type="button"
							onClick={onClose}
							className="p-2 rounded-md hover:bg-muted transition-colors"
							title="Close (Esc)"
						>
							<X className="w-4 h-4 text-foreground" />
						</button>
					</div>
				</div>

				{/* Video player */}
				<div className="relative flex-1 bg-black flex items-center justify-center min-h-[300px]">
					{/* biome-ignore lint/a11y/useMediaCaption: video preview doesn't need captions */}
					{/* biome-ignore lint/a11y/useKeyWithClickEvents: keyboard handled via document listener */}
					<video
						ref={videoRef}
						src={src}
						className="max-w-full max-h-full object-contain"
						onClick={togglePlayPause}
						onTimeUpdate={handleTimeUpdate}
						onPlay={() => setIsPlaying(true)}
						onPause={() => setIsPlaying(false)}
						onVolumeChange={(e) => {
							if (videoRef.current) {
								setVolume(videoRef.current.volume);
								setIsMuted(videoRef.current.muted);
							}
						}}
						onLoadedMetadata={() => {
							if (videoRef.current) {
								setDuration(videoRef.current.duration);
							}
						}}
						preload="metadata"
					/>

					{/* Play/Pause overlay */}
					{!isPlaying && (
						<div className="absolute inset-0 flex items-center justify-center pointer-events-none">
							<div className="w-16 h-16 rounded-full bg-white/20 backdrop-blur-sm flex items-center justify-center">
								<button
									type="button"
									onClick={togglePlayPause}
									className="w-12 h-12 rounded-full bg-primary text-primary-foreground hover:bg-primary/90 transition-colors pointer-events-auto"
								>
									▶
								</button>
							</div>
						</div>
					)}
				</div>

				{/* Controls bar */}
				<div className="flex items-center gap-3 p-4 border-t border-border bg-muted/30">
					{/* Play/Pause button */}
					<button
						type="button"
						onClick={togglePlayPause}
						className="p-2 rounded-md hover:bg-muted transition-colors"
						title={isPlaying ? "Pause (Space)" : "Play (Space)"}
					>
						{isPlaying ? (
							<span className="text-foreground font-medium">⏸</span>
						) : (
							<span className="text-foreground font-medium">▶</span>
						)}
					</button>

					{/* Time display */}
					<div className="flex-1 text-sm font-mono text-muted-foreground">
						{formatTime(currentTime)} / {formatTime(duration)}
					</div>

					{/* Progress bar */}
					<div className="flex-1 h-1 bg-border rounded-full overflow-hidden">
						{/* biome-ignore lint/a11y/useKeyWithClickEvents: seek handled via keyboard shortcuts */}
						<div
							className="h-full bg-primary transition-all duration-100 ease-out"
							style={{ width: `${progress}%` }}
							onClick={(e) => {
								const rect = (e.target as HTMLElement).getBoundingClientRect();
								const video = videoRef.current;
								if (video && rect) {
									const time =
										((e.clientX - rect.left) / rect.width) * video.duration;
									video.currentTime = time;
								}
							}}
						/>
					</div>

					{/* Volume control */}
					<div className="flex items-center gap-2">
						<button
							type="button"
							onClick={toggleMute}
							className="p-2 rounded-md hover:bg-muted transition-colors"
							title={isMuted ? "Unmute (M)" : "Mute (M)"}
						>
							{isMuted ? (
								<VolumeX className="w-4 h-4 text-foreground" />
							) : (
								<Volume2 className="w-4 h-4 text-foreground" />
							)}
						</button>
						<input
							type="range"
							min="0"
							max="1"
							step="0.01"
							value={volume}
							onChange={handleVolumeChange}
							className="w-20 h-1 accent-primary"
							title={`Volume: ${Math.round(volume * 100)}%`}
						/>
					</div>

					{/* Download button */}
					{filePath && (
						<button
							type="button"
							onClick={() => {
								// Trigger download
								const a = document.createElement("a");
								a.href = src;
								a.download = filename;
								a.click();
							}}
							className="p-2 rounded-md hover:bg-muted transition-colors"
							title="Download"
						>
							<Download className="w-4 h-4 text-foreground" />
						</button>
					)}
				</div>

				{/* Keyboard shortcuts hint */}
				<div className="absolute bottom-4 left-1/2 -translate-x-1/2 bg-black/80 backdrop-blur-sm px-3 py-1.5 rounded-md text-xs text-white">
					<div className="text-muted-foreground flex items-center gap-3">
						<span>Space: Play/Pause</span>
						<span>←/→: Seek ±5s</span>
						<span>M: Mute</span>
						<span>↑/↓: Volume</span>
						<span>F: Fullscreen</span>
						<span>Esc: Close</span>
					</div>
				</div>
			</div>
		</div>
	);
});

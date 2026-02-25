/**
 * Read Aloud Button component for TTS playback of text content.
 * Supports paragraph navigation (prev/next).
 * Voice settings (voice, speed) are configured in the Chat Settings sidebar.
 * Responsive: compact (icon-only) on mobile, full controls on desktop.
 */

import { useTTSWithParagraphs } from "@/hooks/use-tts";
import { cn } from "@/lib/utils";
import {
	ChevronLeft,
	ChevronRight,
	Loader2,
	Square,
	Volume2,
} from "lucide-react";
import { useCallback } from "react";

interface ReadAloudButtonProps {
	/** Text content to read aloud */
	text: string;
	/** Additional CSS classes */
	className?: string;
	/** Force compact mode (icon only, no navigation) */
	compact?: boolean;
}

/**
 * Button to read text aloud using TTS with paragraph navigation.
 * Voice/speed settings are managed in the Chat Settings sidebar.
 *
 * By default renders responsively: icon-only on mobile (< sm),
 * full controls with navigation on desktop (>= sm).
 * Pass compact={true} to force icon-only mode at all sizes.
 */
export function ReadAloudButton({
	text,
	className,
	compact = false,
}: ReadAloudButtonProps) {
	const {
		state,
		isSpeaking,
		play,
		stop,
		previousParagraph,
		nextParagraph,
		currentParagraph,
		totalParagraphs,
		hasPrevious,
		hasNext,
		isReading,
	} = useTTSWithParagraphs(text);

	const handleClick = useCallback(() => {
		if (isSpeaking) {
			stop();
		} else {
			play();
		}
	}, [isSpeaking, stop, play]);

	const isConnecting = state === "connecting";
	const isActive = isReading || isSpeaking;
	const isDisabled = !text.trim() || isConnecting;
	const showNavigation = totalParagraphs > 1;

	const icon = isConnecting ? (
		<Loader2 className="w-3.5 h-3.5 animate-spin" />
	) : isActive ? (
		<Square className="w-3.5 h-3.5 fill-current" />
	) : (
		<Volume2 className="w-3.5 h-3.5" />
	);

	if (compact) {
		return (
			<button
				type="button"
				onClick={handleClick}
				disabled={isDisabled}
				className={cn(
					"inline-flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50 disabled:cursor-not-allowed p-1",
					className,
				)}
				title={isActive ? "Stop reading" : "Read aloud"}
			>
				{icon}
			</button>
		);
	}

	// Responsive: compact on mobile, full on desktop
	return (
		<div className={cn("inline-flex items-center gap-0.5", className)}>
			{/* Mobile: icon-only button */}
			<button
				type="button"
				onClick={handleClick}
				disabled={isDisabled}
				className="sm:hidden inline-flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50 disabled:cursor-not-allowed p-1"
				title={isActive ? "Stop reading" : "Read aloud"}
			>
				{icon}
			</button>

			{/* Desktop: full controls */}
			{showNavigation && (
				<button
					type="button"
					onClick={previousParagraph}
					disabled={!hasPrevious || isConnecting}
					className="hidden sm:inline-flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors disabled:opacity-30 disabled:cursor-not-allowed p-1"
					title="Previous paragraph"
				>
					<ChevronLeft className="w-3.5 h-3.5" />
				</button>
			)}

			<button
				type="button"
				onClick={handleClick}
				disabled={isDisabled}
				className="hidden sm:inline-flex items-center justify-center gap-1.5 text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50 disabled:cursor-not-allowed px-2 py-1 text-xs"
				title={isActive ? "Stop reading" : "Read aloud"}
			>
				{icon}
				<span>
					{isActive
						? showNavigation
							? `${currentParagraph + 1}/${totalParagraphs}`
							: "Stop"
						: isConnecting
							? "..."
							: "Read"}
				</span>
			</button>

			{showNavigation && (
				<button
					type="button"
					onClick={nextParagraph}
					disabled={!hasNext || isConnecting}
					className="hidden sm:inline-flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors disabled:opacity-30 disabled:cursor-not-allowed p-1"
					title="Next paragraph"
				>
					<ChevronRight className="w-3.5 h-3.5" />
				</button>
			)}
		</div>
	);
}

/**
 * Compact version of ReadAloudButton (icon only, no navigation).
 */
export function CompactReadAloudButton({
	text,
	className,
}: Omit<ReadAloudButtonProps, "compact">) {
	return <ReadAloudButton text={text} className={className} compact />;
}

/**
 * Live transcript display component.
 *
 * Shows the current transcript being accumulated from STT,
 * with a typing cursor effect.
 */

import { cn } from "@/lib/utils";
import * as React from "react";

export interface LiveTranscriptProps {
	/** Current transcript text */
	text: string;
	/** Whether currently listening */
	isListening: boolean;
	/** Optional class name */
	className?: string;
}

/**
 * Displays live transcript with cursor animation.
 */
export function LiveTranscript({
	text,
	isListening,
	className,
}: LiveTranscriptProps) {
	if (!text && !isListening) {
		return null;
	}

	return (
		<div
			className={cn(
				"px-3 py-2 text-sm text-muted-foreground bg-muted/50 rounded-md",
				"min-h-[2.5rem] flex items-center",
				className,
			)}
		>
			<span className="flex-1">
				{text || (isListening ? "Listening..." : "")}
				{isListening && (
					<span className="inline-block w-0.5 h-4 ml-0.5 bg-foreground animate-pulse" />
				)}
			</span>
		</div>
	);
}

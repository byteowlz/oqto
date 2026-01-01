/**
 * Voice mode toggle button for the chat input area.
 *
 * Shows a microphone icon that toggles voice input mode.
 * Visual feedback indicates current state (idle, listening, processing, speaking).
 */

import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { VoiceState } from "@/lib/voice/types";
import { Loader2, Mic, MicOff } from "lucide-react";
import * as React from "react";

export interface VoiceModeButtonProps {
	/** Whether voice mode is active */
	isActive: boolean;
	/** Current voice state */
	voiceState: VoiceState;
	/** Toggle callback */
	onToggle: () => void;
	/** Whether the button should be disabled */
	disabled?: boolean;
	/** Optional class name */
	className?: string;
}

/**
 * Voice mode toggle button with state indication.
 */
export function VoiceModeButton({
	isActive,
	voiceState,
	onToggle,
	disabled,
	className,
}: VoiceModeButtonProps) {
	const getTooltip = () => {
		if (disabled) return "Voice mode unavailable";
		if (!isActive) return "Start voice mode";
		switch (voiceState) {
			case "listening":
				return "Listening... (click to stop)";
			case "processing":
				return "Processing...";
			case "speaking":
				return "Speaking... (click to stop)";
			default:
				return "Stop voice mode";
		}
	};

	const getIcon = () => {
		if (!isActive) {
			return <Mic className="size-4" />;
		}

		switch (voiceState) {
			case "processing":
				return <Loader2 className="size-4 animate-spin" />;
			case "listening":
			case "speaking":
				return <Mic className="size-4" />;
			default:
				return <MicOff className="size-4" />;
		}
	};

	// State-based styling
	const stateStyles = {
		idle: "",
		listening:
			"bg-blue-500/20 text-blue-500 hover:bg-blue-500/30 border-blue-500/50",
		processing:
			"bg-purple-500/20 text-purple-500 hover:bg-purple-500/30 border-purple-500/50",
		speaking:
			"bg-green-500/20 text-green-500 hover:bg-green-500/30 border-green-500/50",
	};

	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					onClick={onToggle}
					disabled={disabled}
					className={cn(
						"relative transition-colors",
						isActive && stateStyles[voiceState],
						className,
					)}
					aria-label={getTooltip()}
				>
					{getIcon()}

					{/* Pulse animation when listening */}
					{isActive && voiceState === "listening" && (
						<span className="absolute inset-0 rounded-md animate-ping bg-blue-500/20" />
					)}
				</Button>
			</TooltipTrigger>
			<TooltipContent side="top">
				<p>{getTooltip()}</p>
			</TooltipContent>
		</Tooltip>
	);
}

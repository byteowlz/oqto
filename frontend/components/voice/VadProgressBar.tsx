/**
 * VAD (Voice Activity Detection) progress bar.
 *
 * Shows the countdown until silence triggers auto-send.
 * Progress drains from 100% to 0% during silence.
 */

import { cn } from "@/lib/utils";
import * as React from "react";

export interface VadProgressBarProps {
	/** Progress value (0-1), where 0 = full, 1 = empty */
	progress: number;
	/** Whether to show the bar */
	visible?: boolean;
	/** Optional class name */
	className?: string;
}

/**
 * VAD progress bar that drains during silence.
 */
export function VadProgressBar({
	progress,
	visible = true,
	className,
}: VadProgressBarProps) {
	if (!visible || progress === 0) {
		return null;
	}

	// Invert progress so bar drains (100% → 0%)
	const remaining = 1 - progress;

	// Color transitions from blue to orange to red as time runs out
	const getColor = () => {
		if (remaining > 0.5) return "bg-blue-500";
		if (remaining > 0.25) return "bg-orange-500";
		return "bg-red-500";
	};

	return (
		<div
			className={cn(
				"h-1 w-full bg-muted rounded-full overflow-hidden",
				className,
			)}
		>
			<div
				className={cn(
					"h-full transition-all duration-100 ease-linear",
					getColor(),
				)}
				style={{ width: `${remaining * 100}%` }}
			/>
		</div>
	);
}

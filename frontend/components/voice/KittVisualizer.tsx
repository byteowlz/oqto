/**
 * K.I.T.T. Voice Visualizer - Knight Rider style LED bar visualization.
 *
 * Features:
 * - 3-column LED segment display
 * - Scanning animation when idle
 * - VAD progress bar that drains during silence
 * - State-based badge display
 */

import { cn } from "@/lib/utils";
import type { VoiceState } from "@/lib/voice/types";
import { useEffect, useMemo, useState } from "react";

const NUM_SEGMENTS = 12;
const CENTER_INDEX_LOW = Math.floor((NUM_SEGMENTS - 1) / 2);
const CENTER_INDEX_HIGH = Math.ceil((NUM_SEGMENTS - 1) / 2);
const SEGMENT_KEYS = Array.from(
	{ length: NUM_SEGMENTS },
	(_, index) => `segment-${index}`,
);

type BarMode = "scanning" | "draining";

export interface KittVisualizerProps {
	/** Current voice state */
	state: VoiceState;
	/** VAD progress (0-1), where 1 means silence timeout reached */
	vadProgress: number;
	/** Input volume (0-1) for LED visualization */
	inputVolume: number;
	/** Output volume (0-1) for LED visualization when speaking */
	outputVolume: number;
	/** Whether audio is enabled */
	audioEnabled: boolean;
	/** Optional class name */
	className?: string;
}

/**
 * K.I.T.T. style LED bar visualizer.
 */
export function KittVisualizer({
	state,
	vadProgress,
	inputVolume,
	outputVolume,
	audioEnabled,
	className,
}: KittVisualizerProps) {
	const [barMode, setBarMode] = useState<BarMode>("scanning");
	const [animationPlayState, setAnimationPlayState] = useState<
		"running" | "paused"
	>("running");

	// Determine which volume to use based on state
	const activeVolume = state === "speaking" ? outputVolume : inputVolume;
	const hasSpeechActivity = audioEnabled && activeVolume > 0.05;
	const isCountdownActive = audioEnabled && vadProgress > 0.05;

	// Calculate LED segments based on volume
	const litSegments = useMemo(() => {
		const base = Math.floor(activeVolume * NUM_SEGMENTS);
		// Add some variance across columns
		return [Math.max(0, base - 1), base, Math.max(0, base - 1)];
	}, [activeVolume]);

	useEffect(() => {
		const isListening = state === "listening";
		const isSpeaking = state === "speaking";
		const isProcessing = state === "processing";
		const shouldShowDrain = hasSpeechActivity || isCountdownActive;

		if (
			audioEnabled &&
			(isListening || isSpeaking || isProcessing) &&
			shouldShowDrain
		) {
			setBarMode("draining");
			setAnimationPlayState("paused");
		} else {
			setBarMode("scanning");
			setAnimationPlayState("running");
		}
	}, [state, hasSpeechActivity, isCountdownActive, audioEnabled]);

	const renderColumn = (colIndex: number) => {
		const totalLit = Math.round(litSegments[colIndex]);

		if (totalLit === 0) {
			return (
				<div className="flex flex-col justify-center items-center h-full w-[44px] gap-[3px]">
					{SEGMENT_KEYS.map((segmentKey) => (
						<div
							key={segmentKey}
							className="h-[14px] w-full rounded-[2px] flex-shrink-0 transition-all duration-100 ease-in-out bg-red-950 border-red-900/50 opacity-70 border"
						/>
					))}
				</div>
			);
		}

		const segmentsToLightEachSide = Math.max(0, totalLit / 2 - 1);
		const startIndex = CENTER_INDEX_LOW - segmentsToLightEachSide;
		const endIndex = CENTER_INDEX_HIGH + segmentsToLightEachSide;

		return (
			<div className="flex flex-col justify-center items-center h-full w-[44px] gap-[3px]">
				{SEGMENT_KEYS.map((segmentKey, segIndex) => {
					const isActive = segIndex >= startIndex && segIndex <= endIndex;
					return (
						<div
							key={segmentKey}
							className={cn(
								"h-[14px] w-full rounded-[2px] flex-shrink-0 transition-all duration-100 ease-in-out border",
								isActive
									? "bg-red-500 border-red-500 opacity-100 shadow-[0_0_8px_rgba(239,68,68,0.7),0_0_16px_rgba(239,68,68,0.5)]"
									: "bg-red-950 border-red-900/50 opacity-70",
							)}
						/>
					);
				})}
			</div>
		);
	};

	// Bar drains from 100% to 0% as vadProgress goes from 0 to 1
	const barWidth = Math.max(0, (1 - vadProgress) * 100);
	const isBarNearEmpty = barWidth < 5;

	const getStateBadge = () => {
		if (!audioEnabled) return "MIC OFF";
		switch (state) {
			case "idle":
				return "STANDBY";
			case "listening":
				return "LISTENING";
			case "speaking":
				return "SPEAKING";
			case "processing":
				return "PROCESSING";
			default:
				return "STANDBY";
		}
	};

	return (
		<div className={cn("flex flex-col items-center", className)}>
			<style>{`
        @keyframes kitt-swoosh {
          0% { left: 0%; }
          50% { left: 80%; }
          100% { left: 0%; }
        }
        @keyframes kitt-ghost-1 {
          0%, 100% { left: 1.5%; opacity: 0.6; }
          50% { left: 78.5%; opacity: 0.6; }
        }
        @keyframes kitt-ghost-2 {
          0%, 100% { left: 3%; opacity: 0.4; }
          50% { left: 77%; opacity: 0.4; }
        }
        @keyframes kitt-ghost-3 {
          0%, 100% { left: 4.5%; opacity: 0.25; }
          50% { left: 75.5%; opacity: 0.25; }
        }
        .kitt-swoosh-bar {
          animation: kitt-swoosh 1.2s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite;
          width: 20%;
          z-index: 6;
        }
        .kitt-ghost-1 {
          animation: kitt-ghost-1 1.2s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite;
          width: 20%;
          z-index: 5;
        }
        .kitt-ghost-2 {
          animation: kitt-ghost-2 1.2s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite;
          width: 20%;
          z-index: 4;
        }
        .kitt-ghost-3 {
          animation: kitt-ghost-3 1.2s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite;
          width: 20%;
          z-index: 3;
        }
      `}</style>

			{/* LED Display */}
			<div className="flex items-center justify-center bg-black p-4 border border-zinc-800 rounded shadow-[inset_0_0_12px_rgba(0,0,0,0.8)] gap-[10px] mb-4 h-[220px] overflow-hidden">
				{renderColumn(0)}
				{renderColumn(1)}
				{renderColumn(2)}
			</div>

			{/* Scanner / VAD Progress Bar */}
			<div
				className="h-[8px] bg-red-950 rounded-full overflow-hidden relative mb-4"
				style={{ width: "170px" }}
			>
				{barMode === "scanning" && (
					<div className="absolute inset-0">
						<div
							className="kitt-ghost-3 h-full rounded-full absolute top-0"
							style={{
								background:
									"linear-gradient(to right, rgba(239,68,68,0), rgba(239,68,68,0.6), rgba(239,68,68,0))",
								boxShadow: "0 0 5px rgba(239,68,68,0.4)",
								filter: "blur(1.5px)",
								animationPlayState,
							}}
						/>
						<div
							className="kitt-ghost-2 h-full rounded-full absolute top-0"
							style={{
								background:
									"linear-gradient(to right, rgba(239,68,68,0), rgba(239,68,68,0.7), rgba(239,68,68,0))",
								boxShadow: "0 0 6px rgba(239,68,68,0.5)",
								filter: "blur(1.2px)",
								animationPlayState,
							}}
						/>
						<div
							className="kitt-ghost-1 h-full rounded-full absolute top-0"
							style={{
								background:
									"linear-gradient(to right, rgba(239,68,68,0), rgba(239,68,68,0.85), rgba(239,68,68,0))",
								boxShadow: "0 0 7px rgba(239,68,68,0.7)",
								filter: "blur(0.9px)",
								animationPlayState,
							}}
						/>
						<div
							className="kitt-swoosh-bar h-full bg-red-500 rounded-full absolute top-0"
							style={{
								boxShadow:
									"0 0 10px rgba(239,68,68,1), 0 0 20px rgba(239,68,68,0.8)",
								filter: "blur(0.5px)",
								animationPlayState,
							}}
						/>
					</div>
				)}

				{/* VAD draining bar */}
				{barMode === "draining" && !isBarNearEmpty && (
					<div
						className="h-full bg-red-500 rounded-full absolute top-0 transition-all duration-[16ms] linear"
						style={{
							width: `${barWidth}%`,
							left: `${(100 - barWidth) / 2}%`,
							boxShadow:
								"0 0 6px rgba(239,68,68,0.9), 0 0 12px rgba(239,68,68,0.6)",
						}}
					/>
				)}
			</div>

			{/* State Badge */}
			<div
				className={cn(
					"bg-amber-500 text-black text-[13px] font-bold py-1.5 px-3 rounded",
					"tracking-wider uppercase transition-opacity",
					"font-mono",
					!audioEnabled && "opacity-60",
				)}
				style={{ width: "170px", textAlign: "center" }}
			>
				{getStateBadge()}
			</div>
		</div>
	);
}

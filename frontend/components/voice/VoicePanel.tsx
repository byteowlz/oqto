/**
 * Voice Panel - Sidebar voice mode UI for desktop.
 *
 * Displays in the right sidebar when voice mode is active, showing:
 * - Selected visualizer (Orb or K.I.T.T.)
 * - Live transcript
 * - VAD progress
 * - Controls (stop, mute, settings)
 */

"use client";

import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Slider } from "@/components/ui/slider";
import { cn } from "@/lib/utils";
import type {
	VisualizerType,
	VoiceSettings,
	VoiceState,
} from "@/lib/voice/types";
import {
	ChevronDown,
	ChevronUp,
	MicOff,
	Settings2,
	Volume2,
	VolumeX,
	X,
} from "lucide-react";
import { useState } from "react";
import { LiveTranscript } from "./LiveTranscript";
import { VadProgressBar } from "./VadProgressBar";
import { DynamicVisualizer, getAvailableVisualizers } from "./visualizers";

export interface VoicePanelProps {
	/** Current voice state */
	voiceState: VoiceState;
	/** Live transcript text */
	liveTranscript: string;
	/** VAD progress (0-1) */
	vadProgress: number;
	/** Input volume (0-1) */
	inputVolume: number;
	/** Output volume (0-1) */
	outputVolume: number;
	/** Current settings */
	settings: VoiceSettings;
	/** Available TTS voices */
	availableVoices: string[];
	/** Close/stop voice mode */
	onClose: () => void;
	/** Interrupt current TTS */
	onInterrupt: () => void;
	/** Update settings */
	onSettingsChange: {
		setVisualizer: (type: VisualizerType) => void;
		setMuted: (muted: boolean) => void;
		setContinuous: (continuous: boolean) => void;
		setVoice: (voice: string) => void;
		setSpeed: (speed: number) => void;
		setVadTimeout: (ms: number) => void;
		setInterruptWordCount: (count: number) => void;
	};
}

/**
 * Sidebar voice panel for desktop view.
 */
export function VoicePanel({
	voiceState,
	liveTranscript,
	vadProgress,
	inputVolume,
	outputVolume,
	settings,
	availableVoices,
	onClose,
	onInterrupt,
	onSettingsChange,
}: VoicePanelProps) {
	const [showSettings, setShowSettings] = useState(false);
	const visualizers = getAvailableVisualizers();

	const handleVisualizerChange = (id: string) => {
		onSettingsChange.setVisualizer(id as VisualizerType);
	};

	return (
		<div className="flex flex-col h-full">
			{/* Visualizer */}
			<div className="flex-shrink-0 p-4">
				<div className="w-full aspect-square max-w-[280px] mx-auto">
					<DynamicVisualizer
						type={settings.visualizer as VisualizerType}
						state={voiceState}
						vadProgress={vadProgress}
						inputVolume={inputVolume}
						outputVolume={outputVolume}
						audioEnabled={true}
					/>
				</div>
			</div>

			{/* Status indicator */}
			<div className="flex items-center justify-center gap-2 px-4 pb-2">
				<div
					className={cn(
						"w-2 h-2 rounded-full",
						voiceState === "listening" && "bg-blue-500 animate-pulse",
						voiceState === "processing" && "bg-purple-500 animate-pulse",
						voiceState === "speaking" && "bg-green-500",
						voiceState === "idle" && "bg-gray-500",
					)}
				/>
				<span className="text-xs text-muted-foreground">
					{voiceState === "listening" && "Listening..."}
					{voiceState === "processing" && "Processing..."}
					{voiceState === "speaking" && "Speaking..."}
					{voiceState === "idle" && "Ready"}
				</span>
			</div>

			{/* VAD Progress */}
			<div className="px-4">
				<VadProgressBar
					progress={vadProgress}
					visible={voiceState === "listening" && vadProgress > 0}
				/>
			</div>

			{/* Live transcript */}
			<div className="flex-1 min-h-0 overflow-y-auto px-4 py-2">
				<LiveTranscript
					text={liveTranscript}
					isListening={voiceState === "listening"}
				/>
			</div>

			{/* Controls */}
			<div className="flex-shrink-0 border-t p-3 space-y-3">
				{/* Quick controls row */}
				<div className="flex items-center justify-between">
					<div className="flex items-center gap-1">
						{/* Visualizer selector */}
						<DropdownMenu>
							<DropdownMenuTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="sm"
									className="gap-1 h-8 px-2 text-xs"
								>
									{visualizers.find((v) => v.id === settings.visualizer)
										?.name || "Viz"}
									<ChevronDown className="size-3" />
								</Button>
							</DropdownMenuTrigger>
							<DropdownMenuContent align="start">
								<DropdownMenuLabel>Visualizer</DropdownMenuLabel>
								<DropdownMenuSeparator />
								{visualizers.map((v) => (
									<DropdownMenuItem
										key={v.id}
										onClick={() => handleVisualizerChange(v.id)}
										className={cn(settings.visualizer === v.id && "bg-accent")}
									>
										{v.name}
									</DropdownMenuItem>
								))}
							</DropdownMenuContent>
						</DropdownMenu>

						{/* Mute toggle */}
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => onSettingsChange.setMuted(!settings.muted)}
							className={cn(
								"h-8 w-8 p-0",
								settings.muted && "text-muted-foreground",
							)}
						>
							{settings.muted ? (
								<VolumeX className="size-4" />
							) : (
								<Volume2 className="size-4" />
							)}
						</Button>

						{/* Settings toggle */}
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => setShowSettings(!showSettings)}
							className="h-8 w-8 p-0"
						>
							{showSettings ? (
								<ChevronUp className="size-4" />
							) : (
								<Settings2 className="size-4" />
							)}
						</Button>
					</div>

					{/* Action button */}
					{voiceState === "speaking" ? (
						<Button
							type="button"
							variant="destructive"
							size="sm"
							onClick={onInterrupt}
							className="gap-1 h-8"
						>
							<MicOff className="size-3" />
							Interrupt
						</Button>
					) : (
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={onClose}
							className="gap-1 h-8"
						>
							<X className="size-3" />
							Stop
						</Button>
					)}
				</div>

				{/* Expandable settings */}
				{showSettings && (
					<div className="space-y-3 pt-2 border-t">
						{/* Voice selection */}
						<div className="space-y-1">
							<span className="text-xs font-medium text-muted-foreground">
								Voice
							</span>
							<DropdownMenu>
								<DropdownMenuTrigger asChild>
									<Button
										type="button"
										variant="outline"
										size="sm"
										className="w-full justify-between h-8 text-xs"
									>
										{settings.voice}
										<ChevronDown className="size-3" />
									</Button>
								</DropdownMenuTrigger>
								<DropdownMenuContent className="max-h-48 overflow-y-auto">
									{availableVoices.map((voice) => (
										<DropdownMenuItem
											key={voice}
											onClick={() => onSettingsChange.setVoice(voice)}
											className="text-xs"
										>
											{voice}
										</DropdownMenuItem>
									))}
								</DropdownMenuContent>
							</DropdownMenu>
						</div>

						{/* Speed */}
						<div className="space-y-1">
							<span className="text-xs font-medium text-muted-foreground">
								Speed: {settings.speed.toFixed(1)}x
							</span>
							<Slider
								value={[settings.speed]}
								onValueChange={([v]) => onSettingsChange.setSpeed(v)}
								min={0.5}
								max={2.0}
								step={0.1}
								className="py-1"
							/>
						</div>

						{/* VAD Timeout */}
						<div className="space-y-1">
							<span className="text-xs font-medium text-muted-foreground">
								Silence: {(settings.vadTimeoutMs / 1000).toFixed(1)}s
							</span>
							<Slider
								value={[settings.vadTimeoutMs]}
								onValueChange={([v]) => onSettingsChange.setVadTimeout(v)}
								min={500}
								max={3000}
								step={100}
								className="py-1"
							/>
						</div>

						{/* Continuous mode */}
						<label className="flex items-center gap-2 cursor-pointer">
							<input
								type="checkbox"
								checked={settings.continuous}
								onChange={(e) =>
									onSettingsChange.setContinuous(e.target.checked)
								}
								className="rounded"
							/>
							<span className="text-xs">Continuous mode</span>
						</label>

						{/* Interrupt word count */}
						<div className="space-y-1">
							<span className="text-xs font-medium text-muted-foreground">
								Interrupt:{" "}
								{settings.interruptWordCount === 0
									? "off"
									: `${settings.interruptWordCount} word${settings.interruptWordCount !== 1 ? "s" : ""}`}
							</span>
							<Slider
								value={[settings.interruptWordCount]}
								onValueChange={([v]) =>
									onSettingsChange.setInterruptWordCount(v)
								}
								min={0}
								max={5}
								step={1}
								className="py-1"
							/>
						</div>
					</div>
				)}
			</div>
		</div>
	);
}

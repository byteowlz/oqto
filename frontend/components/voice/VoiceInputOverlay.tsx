/**
 * Voice Input Overlay - Full voice mode UI.
 *
 * Displayed when voice mode is active, showing:
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
	Mic,
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

export interface VoiceInputOverlayProps {
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
		setMicMuted: (muted: boolean) => void;
		setContinuous: (continuous: boolean) => void;
		setVoice: (voice: string) => void;
		setSpeed: (speed: number) => void;
		setVadTimeout: (ms: number) => void;
		setInterruptWordCount: (count: number) => void;
	};
	/** Optional class name */
	className?: string;
}

/**
 * Full-screen voice input overlay.
 */
export function VoiceInputOverlay({
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
	className,
}: VoiceInputOverlayProps) {
	const [showSettings, setShowSettings] = useState(false);
	const visualizers = getAvailableVisualizers();

	const handleVisualizerChange = (id: string) => {
		onSettingsChange.setVisualizer(id as VisualizerType);
	};

	return (
		<div
			className={cn(
				"fixed inset-0 z-50 flex flex-col",
				"bg-background/95 backdrop-blur-sm",
				className,
			)}
		>
			{/* Header */}
			<div className="flex items-center justify-between p-4 border-b">
				<div className="flex items-center gap-2">
					<div
						className={cn(
							"w-2 h-2 rounded-full",
							voiceState === "listening" && "bg-blue-500 animate-pulse",
							voiceState === "processing" && "bg-purple-500 animate-pulse",
							voiceState === "speaking" && "bg-green-500",
							voiceState === "idle" && "bg-gray-500",
						)}
					/>
					<span className="text-sm font-medium">Voice Mode</span>
				</div>

				<div className="flex items-center gap-2">
					{/* Visualizer selector */}
					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button type="button" variant="ghost" size="sm" className="gap-1">
								{visualizers.find((v) => v.id === settings.visualizer)?.name ||
									"Visualizer"}
								<ChevronDown className="size-3" />
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end">
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
						size="icon-sm"
						onClick={() => onSettingsChange.setMuted(!settings.muted)}
						className={cn(settings.muted && "text-muted-foreground")}
					>
						{settings.muted ? (
							<VolumeX className="size-4" />
						) : (
							<Volume2 className="size-4" />
						)}
					</Button>

					{/* Mic mute toggle */}
					<Button
						type="button"
						variant="ghost"
						size="icon-sm"
						onClick={() => onSettingsChange.setMicMuted(!settings.micMuted)}
						className={cn(settings.micMuted && "text-muted-foreground")}
					>
						{settings.micMuted ? (
							<MicOff className="size-4" />
						) : (
							<Mic className="size-4" />
						)}
					</Button>

					{/* Settings */}
					<Button
						type="button"
						variant="ghost"
						size="icon-sm"
						onClick={() => setShowSettings(!showSettings)}
					>
						<Settings2 className="size-4" />
					</Button>

					{/* Close */}
					<Button
						type="button"
						variant="ghost"
						size="icon-sm"
						onClick={onClose}
					>
						<X className="size-4" />
					</Button>
				</div>
			</div>

			{/* Settings Panel (collapsible) */}
			{showSettings && (
				<div className="p-4 border-b bg-muted/30 space-y-4">
					<div className="grid grid-cols-1 md:grid-cols-3 gap-4">
						{/* Voice selection */}
						<div className="space-y-2">
							<span className="text-xs font-medium text-muted-foreground">
								Voice
							</span>
							<DropdownMenu>
								<DropdownMenuTrigger asChild>
									<Button
										type="button"
										variant="outline"
										size="sm"
										className="w-full justify-between"
									>
										{settings.voice}
										<ChevronDown className="size-3" />
									</Button>
								</DropdownMenuTrigger>
								<DropdownMenuContent className="max-h-60 overflow-y-auto">
									{availableVoices.map((voice) => (
										<DropdownMenuItem
											key={voice}
											onClick={() => onSettingsChange.setVoice(voice)}
										>
											{voice}
										</DropdownMenuItem>
									))}
								</DropdownMenuContent>
							</DropdownMenu>
						</div>

						{/* Speed */}
						<div className="space-y-2">
							<span className="text-xs font-medium text-muted-foreground">
								Speed: {settings.speed.toFixed(1)}x
							</span>
							<Slider
								value={[settings.speed]}
								onValueChange={([v]) => onSettingsChange.setSpeed(v)}
								min={0.5}
								max={2.0}
								step={0.1}
							/>
						</div>

						{/* VAD Timeout */}
						<div className="space-y-2">
							<span className="text-xs font-medium text-muted-foreground">
								Silence timeout: {(settings.vadTimeoutMs / 1000).toFixed(1)}s
							</span>
							<Slider
								value={[settings.vadTimeoutMs]}
								onValueChange={([v]) => onSettingsChange.setVadTimeout(v)}
								min={500}
								max={3000}
								step={100}
							/>
						</div>
					</div>

					{/* Continuous mode toggle */}
					<div className="flex items-center gap-2">
						<input
							type="checkbox"
							id="continuous"
							checked={settings.continuous}
							onChange={(e) => onSettingsChange.setContinuous(e.target.checked)}
							className="rounded"
						/>
						<label htmlFor="continuous" className="text-sm">
							Continuous conversation (auto-listen after response)
						</label>
					</div>

					{/* Interrupt word count */}
					<div className="space-y-2">
						<span className="text-xs font-medium text-muted-foreground">
							Interrupt after {settings.interruptWordCount} word
							{settings.interruptWordCount !== 1 ? "s" : ""}{" "}
							{settings.interruptWordCount === 0 ? "(disabled)" : ""}
						</span>
						<Slider
							value={[settings.interruptWordCount]}
							onValueChange={([v]) => onSettingsChange.setInterruptWordCount(v)}
							min={0}
							max={5}
							step={1}
						/>
					</div>
				</div>
			)}

			{/* Main content - Visualizer */}
			<div className="flex-1 flex flex-col items-center justify-center p-4 min-h-0">
				<div className="w-full max-w-md aspect-square">
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

			{/* Footer - Transcript and controls */}
			<div className="p-4 border-t space-y-3">
				{/* VAD Progress */}
				<VadProgressBar
					progress={vadProgress}
					visible={voiceState === "listening" && vadProgress > 0}
				/>

				{/* Live transcript */}
				<LiveTranscript
					text={liveTranscript}
					isListening={voiceState === "listening"}
				/>

				{/* Action buttons */}
				<div className="flex justify-center gap-3">
					{voiceState === "speaking" ? (
						<Button
							type="button"
							variant="destructive"
							onClick={onInterrupt}
							className="gap-2"
						>
							<MicOff className="size-4" />
							Interrupt
						</Button>
					) : (
						<Button
							type="button"
							variant="outline"
							onClick={onClose}
							className="gap-2"
						>
							<X className="size-4" />
							Stop Voice Mode
						</Button>
					)}
				</div>

				{/* Hint text */}
				<p className="text-center text-xs text-muted-foreground">
					{voiceState === "listening" &&
						"Speak now... (silence will auto-send)"}
					{voiceState === "processing" && "Processing your message..."}
					{voiceState === "speaking" && "Agent is responding..."}
					{voiceState === "idle" &&
						(settings.micMuted ? "Mic muted" : "Voice mode ready")}
				</p>
			</div>
		</div>
	);
}

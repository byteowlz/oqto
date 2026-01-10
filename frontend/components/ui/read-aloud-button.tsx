/**
 * Read Aloud Button component for TTS playback of text content.
 * Supports paragraph navigation (prev/next) and voice settings.
 */

import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Slider } from "@/components/ui/slider";
import { useTTS, useTTSWithParagraphs } from "@/hooks/use-tts";
import { cn } from "@/lib/utils";
import {
	ChevronDown,
	ChevronLeft,
	ChevronRight,
	Loader2,
	Settings2,
	Square,
	Volume2,
} from "lucide-react";
import { useCallback, useState } from "react";

interface ReadAloudButtonProps {
	/** Text content to read aloud */
	text: string;
	/** Additional CSS classes */
	className?: string;
	/** Compact mode (icon only, no settings or navigation) */
	compact?: boolean;
}

/**
 * Button to read text aloud using TTS with paragraph navigation and settings.
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
		settings,
		availableVoices,
		setVoice,
		setSpeed,
	} = useTTSWithParagraphs(text);
	const [showSettings, setShowSettings] = useState(false);

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
				{isConnecting ? (
					<Loader2 className="w-3.5 h-3.5 animate-spin" />
				) : isActive ? (
					<Square className="w-3.5 h-3.5 fill-current" />
				) : (
					<Volume2 className="w-3.5 h-3.5" />
				)}
			</button>
		);
	}

	return (
		<div className={cn("inline-flex items-center gap-0.5", className)}>
			{/* Previous paragraph button */}
			{showNavigation && (
				<button
					type="button"
					onClick={previousParagraph}
					disabled={!hasPrevious || isConnecting}
					className="inline-flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors disabled:opacity-30 disabled:cursor-not-allowed p-1"
					title="Previous paragraph"
				>
					<ChevronLeft className="w-3.5 h-3.5" />
				</button>
			)}

			{/* Main play/stop button */}
			<button
				type="button"
				onClick={handleClick}
				disabled={isDisabled}
				className="inline-flex items-center justify-center gap-1.5 text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50 disabled:cursor-not-allowed px-2 py-1 text-xs"
				title={isActive ? "Stop reading" : "Read aloud"}
			>
				{isConnecting ? (
					<Loader2 className="w-3.5 h-3.5 animate-spin" />
				) : isActive ? (
					<Square className="w-3.5 h-3.5 fill-current" />
				) : (
					<Volume2 className="w-3.5 h-3.5" />
				)}
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

			{/* Next paragraph button */}
			{showNavigation && (
				<button
					type="button"
					onClick={nextParagraph}
					disabled={!hasNext || isConnecting}
					className="inline-flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors disabled:opacity-30 disabled:cursor-not-allowed p-1"
					title="Next paragraph"
				>
					<ChevronRight className="w-3.5 h-3.5" />
				</button>
			)}

			{/* Settings dropdown */}
			<DropdownMenu open={showSettings} onOpenChange={setShowSettings}>
				<DropdownMenuTrigger asChild>
					<button
						type="button"
						className="inline-flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors p-1"
						title="Voice settings"
					>
						<Settings2 className="w-3 h-3" />
					</button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end" className="w-56">
					<DropdownMenuLabel>Voice Settings</DropdownMenuLabel>
					<DropdownMenuSeparator />

					{/* Voice selection */}
					<div className="px-2 py-1.5">
						<span className="text-xs font-medium text-muted-foreground">
							Voice
						</span>
						<DropdownMenu>
							<DropdownMenuTrigger asChild>
								<button
									type="button"
									className="w-full flex items-center justify-between mt-1 px-2 py-1.5 text-xs border rounded hover:bg-accent"
								>
									<span className="truncate">{settings.voice}</span>
									<ChevronDown className="w-3 h-3 ml-1 flex-shrink-0" />
								</button>
							</DropdownMenuTrigger>
							<DropdownMenuContent className="max-h-48 overflow-y-auto">
								{availableVoices.length > 0 ? (
									availableVoices.map((voice) => (
										<DropdownMenuItem
											key={voice}
											onClick={() => setVoice(voice)}
											className={cn(
												"text-xs",
												settings.voice === voice && "bg-accent",
											)}
										>
											{voice}
										</DropdownMenuItem>
									))
								) : (
									<DropdownMenuItem disabled className="text-xs">
										Connect to load voices...
									</DropdownMenuItem>
								)}
							</DropdownMenuContent>
						</DropdownMenu>
					</div>

					{/* Speed slider */}
					<div className="px-2 py-1.5">
						<div className="flex items-center justify-between">
							<span className="text-xs font-medium text-muted-foreground">
								Speed
							</span>
							<span className="text-xs text-muted-foreground">
								{settings.speed.toFixed(1)}x
							</span>
						</div>
						<Slider
							value={[settings.speed]}
							onValueChange={([v]) => setSpeed(v)}
							min={0.5}
							max={2.0}
							step={0.1}
							className="mt-2"
						/>
					</div>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}

/**
 * Compact version of ReadAloudButton (icon only, no settings or navigation).
 */
export function CompactReadAloudButton({
	text,
	className,
}: Omit<ReadAloudButtonProps, "compact">) {
	return <ReadAloudButton text={text} className={className} compact />;
}

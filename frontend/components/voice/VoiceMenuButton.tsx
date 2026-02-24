"use client";

import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuShortcut,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import {
	AudioLines,
	Keyboard,
	Loader2,
	MessageSquare,
	MicOff,
} from "lucide-react";
import { useTranslation } from "react-i18next";

export type VoiceMode = "conversation" | "dictation" | null;

export interface VoiceShortcuts {
	conversation: string;
	dictation: string;
}

export const DEFAULT_VOICE_SHORTCUTS: VoiceShortcuts = {
	conversation: "⌥V",
	dictation: "⌥D",
};

interface VoiceMenuButtonProps {
	/** Currently active mode */
	activeMode: VoiceMode;
	/** Voice state for conversation mode */
	voiceState?: "idle" | "listening" | "processing" | "speaking";
	/** Callback when conversation mode is selected */
	onConversation: () => void;
	/** Callback when dictation mode is selected */
	onDictation: () => void;
	/** Callback when stopping either mode */
	onStop: () => void;
	/** Whether the button should be disabled */
	disabled?: boolean;
	/** Keyboard shortcuts to display */
	shortcuts?: VoiceShortcuts;
	/** Locale for labels */
	locale?: "en" | "de";
	/** Additional CSS classes */
	className?: string;
}

export function VoiceMenuButton({
	activeMode,
	voiceState = "idle",
	onConversation,
	onDictation,
	onStop,
	disabled = false,
	shortcuts = DEFAULT_VOICE_SHORTCUTS,
	locale = "en",
	className,
}: VoiceMenuButtonProps) {
	const { t } = useTranslation();
	const isActive = activeMode !== null;
	const isProcessing = voiceState === "processing";

	// When active, clicking the button stops the current mode
	if (isActive) {
		return (
			<button
				type="button"
				onClick={onStop}
				disabled={disabled}
				className={cn(
					"flex-shrink-0 size-8 flex items-center justify-center rounded-full transition-all",
					activeMode === "conversation"
						? "bg-primary text-primary-foreground"
						: "bg-red-500 text-white",
					isProcessing && "animate-pulse",
					!isProcessing && "animate-pulse",
					disabled && "opacity-50 cursor-not-allowed",
					className,
				)}
				title={t("voice.stop")}
			>
				{isProcessing ? (
					<Loader2 className="size-4 animate-spin" />
				) : (
					<MicOff className="size-4" />
				)}
			</button>
		);
	}

	// When inactive, show dropdown menu
	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<button
					type="button"
					disabled={disabled}
					className={cn(
						"flex-shrink-0 size-8 flex items-center justify-center rounded-full transition-all",
						"text-muted-foreground hover:text-foreground hover:bg-muted/50",
						disabled && "opacity-50 cursor-not-allowed",
						className,
					)}
					title={t("voice.voiceMode")}
				>
					<AudioLines className="size-4" />
				</button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="start" side="top" className="w-56">
				<DropdownMenuItem
					onClick={onConversation}
					className="flex flex-col items-start gap-0.5 py-2"
				>
					<div className="flex w-full items-center">
						<MessageSquare className="mr-2 size-4" />
						<span className="font-medium">{t("voice.conversation")}</span>
						<DropdownMenuShortcut>
							{shortcuts.conversation}
						</DropdownMenuShortcut>
					</div>
					<span className="text-xs text-muted-foreground pl-6">
						{t("voice.conversationDesc")}
					</span>
				</DropdownMenuItem>
				<DropdownMenuItem
					onClick={onDictation}
					className="flex flex-col items-start gap-0.5 py-2"
				>
					<div className="flex w-full items-center">
						<Keyboard className="mr-2 size-4" />
						<span className="font-medium">{t("voice.dictation")}</span>
						<DropdownMenuShortcut>{shortcuts.dictation}</DropdownMenuShortcut>
					</div>
					<span className="text-xs text-muted-foreground pl-6">
						{t("voice.dictationDesc")}
					</span>
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

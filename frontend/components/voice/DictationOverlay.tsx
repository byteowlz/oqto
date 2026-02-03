/**
 * Dictation Overlay - input replacement UI.
 *
 * Dictation can produce frequent updates. Resizing the default chat textarea or
 * updating placeholder text on every word tends to cause forced reflows and UI jitter.
 * This overlay provides a fixed-height textarea while dictation is active.
 */

"use client";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Mic, Send, X } from "lucide-react";
import type * as React from "react";
import { useEffect, useRef } from "react";
import { VadProgressBar } from "./VadProgressBar";

export interface DictationOverlayProps
	extends Pick<
		React.TextareaHTMLAttributes<HTMLTextAreaElement>,
		"onKeyDown" | "onPaste" | "onFocus" | "onBlur"
	> {
	open: boolean;
	value: string;
	liveTranscript: string;
	placeholder?: string;
	vadProgress: number;
	/** Whether auto-send on silence is enabled */
	autoSend: boolean;
	/** Callback to toggle auto-send */
	onAutoSendChange: (enabled: boolean) => void;
	onStop: () => void;
	onChange: (e: React.ChangeEvent<HTMLTextAreaElement>) => void;
	textareaRef?: React.Ref<HTMLTextAreaElement>;
	className?: string;
}

export function DictationOverlay({
	open,
	value,
	liveTranscript,
	placeholder,
	vadProgress,
	autoSend,
	onAutoSendChange,
	onStop,
	onChange,
	onKeyDown,
	onPaste,
	onFocus,
	onBlur,
	textareaRef,
	className,
}: DictationOverlayProps) {
	const fallbackRef = useRef<HTMLTextAreaElement>(null);

	useEffect(() => {
		if (!open) return;
		const el =
			(typeof textareaRef === "object" ? textareaRef.current : null) ??
			fallbackRef.current;
		if (!el) return;

		// Auto-scroll only if user is already near the bottom or cursor is at end.
		const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 12;
		const selectionAtEnd =
			document.activeElement !== el ||
			(el.selectionStart ?? 0) === value.length;

		if (atBottom && selectionAtEnd) {
			el.scrollTop = el.scrollHeight;
		}
	}, [open, textareaRef, value.length]);

	if (!open) return null;

	return (
		<div
			className={cn(
				"rounded-xl border bg-background/95 backdrop-blur shadow-lg",
				className,
			)}
		>
			<div className="flex items-center justify-between gap-2 px-3 py-2 border-b">
				<div className="flex items-center gap-2 text-sm font-medium">
					<Mic className="size-4" />
					<span>Dictation</span>
				</div>
				<div className="flex items-center gap-1">
					{/* Auto-send toggle */}
					<button
						type="button"
						onClick={() => onAutoSendChange(!autoSend)}
						className={cn(
							"flex items-center gap-1.5 px-2 py-1 rounded text-xs transition-colors",
							autoSend
								? "bg-primary/15 text-primary"
								: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
						)}
						title={autoSend ? "Auto-send enabled" : "Auto-send disabled"}
					>
						<Send className="size-3" />
						<span>Auto</span>
					</button>
					<Button type="button" variant="ghost" size="icon-sm" onClick={onStop}>
						<X className="size-4" />
					</Button>
				</div>
			</div>

			<div className="p-3 space-y-2">
				<textarea
					ref={textareaRef ?? fallbackRef}
					autoComplete="off"
					autoCorrect="off"
					autoCapitalize="sentences"
					spellCheck={false}
					enterKeyHint="send"
					data-form-type="other"
					placeholder={placeholder}
					value={
						liveTranscript
							? `${value}${value && liveTranscript ? " " : ""}${liveTranscript}`
							: value
					}
					onChange={onChange}
					onKeyDown={onKeyDown}
					onPaste={onPaste}
					onFocus={onFocus}
					onBlur={onBlur}
					className={cn(
						"w-full rounded-md border bg-transparent",
						"px-3 py-2 text-sm leading-5",
						"outline-none focus-visible:ring-1 focus-visible:ring-ring",
						"h-40 resize-none overflow-y-auto",
					)}
				/>

				<div className="min-h-[1.25rem] text-xs text-muted-foreground">
					{liveTranscript ? "Listening..." : "Waiting for speech..."}
				</div>

				<div className="h-1">
					<VadProgressBar progress={vadProgress} />
				</div>
			</div>
		</div>
	);
}

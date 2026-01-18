"use client";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
	ArrowDown,
	ArrowLeft,
	ArrowRight,
	ArrowUp,
	ChevronDown,
	Command,
	CornerDownLeft,
	Option,
} from "lucide-react";
import { useCallback, useState } from "react";

interface MobileKeyboardToolbarProps {
	/** Called when a key sequence should be sent to the terminal */
	onSendKey: (key: string) => void;
	/** Called when the keyboard should be dismissed */
	onDismiss?: () => void;
	/** Additional CSS classes */
	className?: string;
	/** Whether the toolbar is visible */
	visible?: boolean;
}

// ANSI escape sequences for special keys
const KEYS = {
	// Arrow keys
	UP: "\x1b[A",
	DOWN: "\x1b[B",
	RIGHT: "\x1b[C",
	LEFT: "\x1b[D",
	// Control sequences
	CTRL_C: "\x03",
	CTRL_D: "\x04",
	CTRL_Z: "\x1a",
	CTRL_L: "\x0c",
	// Special keys
	TAB: "\t",
	ESC: "\x1b",
	ENTER: "\r",
	// Page navigation
	PAGE_UP: "\x1b[5~",
	PAGE_DOWN: "\x1b[6~",
	HOME: "\x1b[H",
	END: "\x1b[F",
};

type ModifierKey = "ctrl" | "alt" | "none";

/**
 * Mobile keyboard toolbar with terminal-specific keys
 * Inspired by Terminus iOS app
 */
export function MobileKeyboardToolbar({
	onSendKey,
	onDismiss,
	className,
	visible = true,
}: MobileKeyboardToolbarProps) {
	const [activeModifier, setActiveModifier] = useState<ModifierKey>("none");

	const sendKey = useCallback(
		(key: string, bypassModifier = false) => {
			if (!bypassModifier && activeModifier !== "none") {
				// Apply modifier to alphabetic keys
				if (key.length === 1 && /^[a-z]$/i.test(key)) {
					if (activeModifier === "ctrl") {
						// Ctrl+letter = letter code - 64 (for uppercase) or - 96 (for lowercase)
						const code = key.toUpperCase().charCodeAt(0) - 64;
						onSendKey(String.fromCharCode(code));
					} else if (activeModifier === "alt") {
						// Alt/Meta + letter = ESC followed by letter
						onSendKey(`\x1b${key}`);
					}
				} else {
					onSendKey(key);
				}
				setActiveModifier("none");
			} else {
				onSendKey(key);
			}
		},
		[activeModifier, onSendKey],
	);

	const toggleModifier = useCallback((mod: ModifierKey) => {
		setActiveModifier((current) => (current === mod ? "none" : mod));
	}, []);

	if (!visible) return null;

	const modifierButtonClass = (mod: ModifierKey) =>
		cn(
			"min-w-[44px] h-[36px] text-xs font-mono",
			activeModifier === mod
				? "bg-primary text-primary-foreground"
				: "bg-muted/80 text-foreground",
		);

	const keyButtonClass =
		"min-w-[44px] h-[36px] bg-muted/80 text-foreground active:bg-muted";

	return (
		<div
			className={cn(
				"flex items-center gap-1 px-2 py-1.5 bg-background/95 backdrop-blur border-t border-border",
				"safe-area-inset-bottom",
				className,
			)}
			// Prevent focus loss when tapping buttons
			onMouseDown={(e) => e.preventDefault()}
			onTouchStart={(e) => e.preventDefault()}
		>
			{/* Modifier keys */}
			<div className="flex gap-1 mr-2">
				<Button
					variant="ghost"
					size="sm"
					className={modifierButtonClass("ctrl")}
					onClick={() => toggleModifier("ctrl")}
					aria-pressed={activeModifier === "ctrl"}
					aria-label="Control modifier"
				>
					<Command className="size-3.5" />
					<span className="sr-only">Ctrl</span>
				</Button>
				<Button
					variant="ghost"
					size="sm"
					className={modifierButtonClass("alt")}
					onClick={() => toggleModifier("alt")}
					aria-pressed={activeModifier === "alt"}
					aria-label="Alt/Option modifier"
				>
					<Option className="size-3.5" />
					<span className="sr-only">Alt</span>
				</Button>
			</div>

			{/* Common terminal keys */}
			<div className="flex gap-1 mr-2">
				<Button
					variant="ghost"
					size="sm"
					className={keyButtonClass}
					onClick={() => sendKey(KEYS.ESC, true)}
					aria-label="Escape"
				>
					Esc
				</Button>
				<Button
					variant="ghost"
					size="sm"
					className={keyButtonClass}
					onClick={() => sendKey(KEYS.TAB, true)}
					aria-label="Tab"
				>
					Tab
				</Button>
			</div>

			{/* Arrow keys */}
			<div className="flex gap-0.5 mr-2">
				<Button
					variant="ghost"
					size="sm"
					className={keyButtonClass}
					onClick={() => sendKey(KEYS.LEFT, true)}
					aria-label="Left arrow"
				>
					<ArrowLeft className="size-4" />
				</Button>
				<div className="flex flex-col gap-0.5">
					<Button
						variant="ghost"
						size="sm"
						className="min-w-[36px] h-[17px] bg-muted/80 text-foreground"
						onClick={() => sendKey(KEYS.UP, true)}
						aria-label="Up arrow"
					>
						<ArrowUp className="size-3" />
					</Button>
					<Button
						variant="ghost"
						size="sm"
						className="min-w-[36px] h-[17px] bg-muted/80 text-foreground"
						onClick={() => sendKey(KEYS.DOWN, true)}
						aria-label="Down arrow"
					>
						<ArrowDown className="size-3" />
					</Button>
				</div>
				<Button
					variant="ghost"
					size="sm"
					className={keyButtonClass}
					onClick={() => sendKey(KEYS.RIGHT, true)}
					aria-label="Right arrow"
				>
					<ArrowRight className="size-4" />
				</Button>
			</div>

			{/* Spacer */}
			<div className="flex-1" />

			{/* Quick actions: Ctrl+C, Ctrl+D */}
			<div className="flex gap-1 mr-2">
				<Button
					variant="ghost"
					size="sm"
					className={cn(keyButtonClass, "text-red-400")}
					onClick={() => sendKey(KEYS.CTRL_C, true)}
					aria-label="Ctrl+C (interrupt)"
				>
					^C
				</Button>
				<Button
					variant="ghost"
					size="sm"
					className={keyButtonClass}
					onClick={() => sendKey(KEYS.CTRL_D, true)}
					aria-label="Ctrl+D (EOF)"
				>
					^D
				</Button>
			</div>

			{/* Dismiss keyboard button */}
			{onDismiss && (
				<Button
					variant="ghost"
					size="sm"
					className="min-w-[44px] h-[36px] bg-primary/20 text-primary"
					onClick={onDismiss}
					aria-label="Dismiss keyboard"
				>
					<ChevronDown className="size-5" />
				</Button>
			)}
		</div>
	);
}

/**
 * Hook to manage keyboard toolbar visibility based on input focus
 */
export function useKeyboardToolbar() {
	const [isVisible, setIsVisible] = useState(false);

	const showToolbar = useCallback(() => setIsVisible(true), []);
	const hideToolbar = useCallback(() => setIsVisible(false), []);

	return {
		isVisible,
		showToolbar,
		hideToolbar,
	};
}

"use client";

import {
	type GhosttyTerminalHandle,
	MuxGhosttyTerminal,
} from "@/components/terminal/ghostty-terminal";
import { MobileKeyboardToolbar } from "@/components/terminal/mobile-keyboard-toolbar";
import { useIsMobile } from "@/hooks/use-mobile";
import { cn } from "@/lib/utils";
import { useTheme } from "next-themes";
import { useCallback, useRef } from "react";

interface TerminalViewProps {
	workspacePath?: string | null;
}

export function TerminalView({ workspacePath }: TerminalViewProps) {
	const { resolvedTheme } = useTheme();
	const isMobile = useIsMobile();
	const terminalRef = useRef<GhosttyTerminalHandle>(null);

	// Handle sending keys from the toolbar to the terminal
	const handleSendKey = useCallback((key: string) => {
		terminalRef.current?.sendKey(key);
		// Keep terminal focused after sending key
		terminalRef.current?.focus();
	}, []);

	// Handle dismissing/blurring the terminal
	const handleDismiss = useCallback(() => {
		terminalRef.current?.blur();
	}, []);

	// Don't render terminal if no session selected
	if (!workspacePath) {
		return (
			<div
				className="h-full bg-black/70 rounded p-4 text-sm font-mono text-red-300"
				data-spotlight="terminal"
			>
				Select a chat to attach to the terminal.
			</div>
		);
	}

	// Pass theme to terminal so it can include it in its session key
	return (
		<div className="h-full flex flex-col" data-spotlight="terminal">
			{/* Mobile keyboard toolbar - at the top like a nav bar */}
			{isMobile && (
				<MobileKeyboardToolbar
					onSendKey={handleSendKey}
					onDismiss={handleDismiss}
					visible={true}
					className="flex-shrink-0 border border-border rounded-t bg-muted/30"
				/>
			)}

			{/* Terminal container */}
			<div
				className="flex-1 min-h-0"
				onClick={() => terminalRef.current?.focus()}
				onKeyDown={() => terminalRef.current?.focus()}
				role="presentation"
			>
				<MuxGhosttyTerminal
					ref={terminalRef}
					key={`${workspacePath}-${resolvedTheme}`}
					workspacePath={workspacePath}
					className={cn(
						"border border-border h-full",
						isMobile && "border-t-0 rounded-b rounded-t-none",
					)}
					theme={resolvedTheme}
				/>
			</div>
		</div>
	);
}

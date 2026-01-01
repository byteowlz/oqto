/**
 * Global voice command events.
 * Allows command palette to trigger voice modes in the sessions component.
 */

import { useCallback, useEffect } from "react";

export type VoiceCommandType = "conversation" | "dictation" | "stop";

type VoiceCommandListener = (command: VoiceCommandType) => void;

const listeners = new Set<VoiceCommandListener>();

/**
 * Emit a voice command to all listeners.
 */
export function emitVoiceCommand(command: VoiceCommandType) {
	for (const listener of listeners) {
		listener(command);
	}
}

/**
 * Hook to listen for voice commands.
 * Used by the sessions component to receive commands from the palette.
 */
export function useVoiceCommandListener(callback: VoiceCommandListener) {
	useEffect(() => {
		listeners.add(callback);
		return () => {
			listeners.delete(callback);
		};
	}, [callback]);
}

/**
 * Hook to get voice command emitter.
 * Used by command palette to send commands.
 */
export function useVoiceCommandEmitter() {
	const startConversation = useCallback(() => {
		emitVoiceCommand("conversation");
	}, []);

	const startDictation = useCallback(() => {
		emitVoiceCommand("dictation");
	}, []);

	const stopVoice = useCallback(() => {
		emitVoiceCommand("stop");
	}, []);

	return { startConversation, startDictation, stopVoice };
}

/**
 * Keyboard shortcuts for voice commands.
 * Default: Alt+V for conversation, Alt+D for dictation
 */
export const VOICE_SHORTCUTS = {
	conversation: { key: "v", altKey: true },
	dictation: { key: "d", altKey: true },
} as const;

/**
 * Format shortcut for display.
 */
export function formatShortcut(shortcut: {
	key: string;
	altKey?: boolean;
	metaKey?: boolean;
	ctrlKey?: boolean;
}) {
	const parts: string[] = [];
	if (shortcut.ctrlKey) parts.push("Ctrl");
	if (shortcut.altKey) parts.push("⌥");
	if (shortcut.metaKey) parts.push("⌘");
	parts.push(shortcut.key.toUpperCase());
	return parts.join("");
}

/**
 * Hook to register global keyboard shortcuts for voice commands.
 */
export function useVoiceShortcuts(enabled = true) {
	useEffect(() => {
		if (!enabled) return;

		const handleKeyDown = (e: KeyboardEvent) => {
			// Don't trigger if user is typing in an input
			const target = e.target as HTMLElement;
			if (
				target.tagName === "INPUT" ||
				target.tagName === "TEXTAREA" ||
				target.isContentEditable
			) {
				return;
			}

			// Alt+V for conversation
			if (e.altKey && e.key.toLowerCase() === "v") {
				e.preventDefault();
				emitVoiceCommand("conversation");
			}

			// Alt+D for dictation
			if (e.altKey && e.key.toLowerCase() === "d") {
				e.preventDefault();
				emitVoiceCommand("dictation");
			}
		};

		document.addEventListener("keydown", handleKeyDown);
		return () => document.removeEventListener("keydown", handleKeyDown);
	}, [enabled]);
}

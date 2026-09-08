/**
 * Reading a chord from a key press, in the lowercase `ctrl+shift+p` syntax
 * ADR-0040 configuration uses. Modifier-only presses are not a chord, so
 * holding Ctrl while reaching for a key never binds anything.
 */

import type { KeyboardEvent } from "react";

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);

const NAMES: { readonly [key: string]: string } = {
	ArrowLeft: "left",
	ArrowRight: "right",
	ArrowUp: "up",
	ArrowDown: "down",
	Escape: "esc",
	" ": "space",
};

/** The chord for this press, or null when it is not one yet. */
export function captureChord(event: KeyboardEvent<HTMLElement>): string | null {
	if (MODIFIER_KEYS.has(event.key)) return null;
	const key = NAMES[event.key] ?? event.key.toLowerCase();
	const parts = [
		event.ctrlKey ? "ctrl" : "",
		event.altKey ? "alt" : "",
		event.metaKey ? "meta" : "",
		event.shiftKey ? "shift" : "",
		key,
	].filter(Boolean);
	// A bare letter would swallow typing; a chord needs a modifier.
	return parts.length > 1 ? parts.join("+") : null;
}

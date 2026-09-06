/**
 * Merges ADR-0040 configuration bindings (`{ keys, action }` entries from
 * the resolved OqtoUI config, lowercase chord syntax) over the default
 * compositor bindings. Entries naming unknown actions are ignored, so a
 * typo can never bind a key to nothing or to the wrong thing.
 */

import {
	DEFAULT_KEY_BINDINGS,
	type KeyBinding,
	actionFromId,
	actionId,
} from "./keybindings";

export interface ConfiguredBinding {
	readonly keys: string;
	readonly action: string;
}

const MODIFIER_ORDER = ["ctrl", "alt", "meta", "shift"] as const;
const MODIFIER_NAMES: Record<(typeof MODIFIER_ORDER)[number], string> = {
	ctrl: "Ctrl",
	alt: "Alt",
	meta: "Meta",
	shift: "Shift",
};
const KEY_NAMES: Record<string, string> = {
	left: "ArrowLeft",
	right: "ArrowRight",
	up: "ArrowUp",
	down: "ArrowDown",
	esc: "Escape",
	escape: "Escape",
	enter: "Enter",
	tab: "Tab",
	space: " ",
	backspace: "Backspace",
	delete: "Delete",
};

/** "ctrl+shift+p" / "alt+right" -> "Ctrl+Shift+p" / "Alt+ArrowRight"; null if malformed. */
export function normalizeConfigChord(keys: string): string | null {
	const parts = keys
		.split("+")
		.map((part) => part.trim().toLowerCase())
		.filter((part) => part.length > 0);
	if (parts.length === 0) return null;
	const key = parts[parts.length - 1];
	const modifiers = parts.slice(0, -1);
	if (
		!modifiers.every((modifier) =>
			MODIFIER_ORDER.includes(modifier as (typeof MODIFIER_ORDER)[number]),
		)
	) {
		return null;
	}
	const ordered = MODIFIER_ORDER.filter((modifier) =>
		modifiers.includes(modifier),
	).map((modifier) => MODIFIER_NAMES[modifier]);
	const named =
		KEY_NAMES[key] ??
		(key.length === 1 ? key : key.charAt(0).toUpperCase() + key.slice(1));
	return [...ordered, named].join("+");
}

export function bindingsFromConfig(
	entries: readonly ConfiguredBinding[],
	defaults: readonly KeyBinding[] = DEFAULT_KEY_BINDINGS,
): KeyBinding[] {
	const overrides: KeyBinding[] = [];
	for (const entry of entries) {
		const action = actionFromId(entry.action);
		const chord = normalizeConfigChord(entry.keys);
		if (action && chord) overrides.push({ chord, action });
	}
	const overriddenIds = new Set(
		overrides.map((binding) => actionId(binding.action)),
	);
	const overriddenChords = new Set(overrides.map((binding) => binding.chord));
	return [
		...defaults.filter(
			(binding) =>
				!overriddenIds.has(actionId(binding.action)) &&
				!overriddenChords.has(binding.chord),
		),
		...overrides,
	];
}

/**
 * base16 / base24 theming primitives for the oqto mini-app design system.
 *
 * A scheme is a single palette of 24 color slots plus a declared mode. The
 * engine maps the slots deterministically onto oqto's semantic CSS variables
 * (see map-scheme-to-tokens.ts). Any base16/base24 scheme can therefore reskin
 * the entire app and every mini-app at once. oqto's own look ships as two
 * pinned schemes (oqto-dark, oqto-light) that reproduce the current design.
 */

export type ThemeMode = "light" | "dark";

/** The 24 base16/base24 slot identifiers (base00..base0F, base10..base17). */
export type Base24SlotKey =
	| "base00"
	| "base01"
	| "base02"
	| "base03"
	| "base04"
	| "base05"
	| "base06"
	| "base07"
	| "base08"
	| "base09"
	| "base0A"
	| "base0B"
	| "base0C"
	| "base0D"
	| "base0E"
	| "base0F"
	| "base10"
	| "base11"
	| "base12"
	| "base13"
	| "base14"
	| "base15"
	| "base16"
	| "base17";

/**
 * Slot -> color value. Values are any CSS color string (hex, rgb(), oklch()).
 * External base24 schemes use hex; oqto's pinned schemes may store oklch to
 * reproduce the current values bit-for-bit.
 */
export type Base24Slots = Record<Base24SlotKey, string>;

/**
 * A complete theme definition: a 24-slot palette for a given light/dark mode,
 * with optional per-token overrides for values that cannot be expressed as a
 * flat slot (alpha colors) or that intentionally diverge from the generic
 * mapping (e.g. oqto's monochrome charts).
 */
export interface Base24Scheme {
	/** Stable id, e.g. "oqto-dark". */
	id: string;
	/** Human-readable name shown in the scheme picker. */
	name: string;
	/** Which oqto mode this palette is meant to render. */
	mode: ThemeMode;
	/** The 24 color slots. */
	slots: Base24Slots;
	/** Exact overrides for specific semantic CSS vars (win over the mapping). */
	overrides?: Partial<Record<string, string>>;
}

/** Resolved semantic CSS variables, e.g. { "--background": "#222624", ... }. */
export type SemanticTokenMap = Record<string, string>;

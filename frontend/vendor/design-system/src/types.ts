/**
 * Core types for the design-system standard (shadcn-ts implementation).
 *
 * A Scheme is a filled palette of base16 (16) or base24 (24) slots plus a
 * declared mode. The engine normalizes it to base24 (deriving missing slots
 * for base16 per spec/base16-policy.md), maps slots -> abstract roles ->
 * shadcn concrete surface, and emits CSS variables.
 *
 * The shape matches spec/schema.json.
 *
 * @see ../../spec/slots.md
 * @see ../../spec/roles.md
 * @see ../../spec/base16-policy.md
 */

/** Light/dark mode a scheme targets. Drives the `.dark` class + next-themes. */
export type ThemeMode = "light" | "dark";

/** Which slot set a scheme fills. */
export type SchemeSystem = "base16" | "base24";

/** The 16 base16 slot identifiers (base00..base0F). */
export type Base16SlotKey =
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
	| "base0F";

/** The 8 base24-only slot identifiers (base10..base17). */
export type Base24OnlySlotKey =
	| "base10"
	| "base11"
	| "base12"
	| "base13"
	| "base14"
	| "base15"
	| "base16"
	| "base17";

/** All 24 base24 slot identifiers. */
export type Base24SlotKey = Base16SlotKey | Base24OnlySlotKey;

/** A filled palette. Required keys depend on `system` (validated by schema). */
export type Slots = Partial<Record<Base24SlotKey, string>>;

/**
 * A complete scheme definition. See spec/schema.json for the machine contract.
 */
export interface Scheme {
	/** Stable identifier, e.g. "oqto-dark". */
	id: string;
	/** Human-readable name for pickers. */
	name: string;
	/** Mode this palette renders. */
	mode: ThemeMode;
	/** Which slot set is filled. Drives base16 derivation. */
	system: SchemeSystem;
	/** The color slots. */
	slots: Slots;
	/**
	 * Exact values for specific emitted variables that win over the role->slot
	 * mapping (alpha colors, monochrome charts). Per-tool/framework vocabulary.
	 */
	overrides?: Partial<Record<string, string>>;
	/**
	 * Opt into duplication (alias missing slots) instead of derivation when
	 * system = "base16". Escape hatch; see spec/base16-policy.md tier 3.
	 */
	pureBase16?: boolean;
}

/** Back-compat alias; older code calls these Base24Scheme. */
export type Base24Scheme = Scheme;

/** A normalized scheme always has all 24 slots filled (derived if needed). */
export interface NormalizedScheme extends Scheme {
	system: "base24";
	slots: Record<Base24SlotKey, string>;
}

/** Resolved CSS variables, e.g. { "--background": "#222624", ... }. */
export type SemanticTokenMap = Record<string, string>;

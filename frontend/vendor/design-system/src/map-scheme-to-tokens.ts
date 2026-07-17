/**
 * Two-tier slot -> token mapping.
 *
 * Tier 1 — abstract roles (portable): the 15 closed roles, each bound to a
 * slot, emitted as bare CSS vars (`--primary`, `--background`, ...).
 * Tier 2 — shadcn concrete surface (framework vocabulary, non-portable): the
 * `*-foreground` pairs that encode "text readable on this surface" plus the
 * status tokens shadcn lacks, bound to the same slots.
 *
 * A scheme's `overrides` win over both tiers (for per-tool vocabulary like
 * oqto's terminal/code/chart tokens).
 *
 * @see ../../spec/roles.md
 */

import { ROLE_FOR_SLOT } from "./roles.js";
import type { Base24SlotKey, NormalizedScheme, SemanticTokenMap } from "./types.js";

/**
 * shadcn concrete surface: paired names. Maps each emitted shadcn var to the
 * slot its *color* comes from; the paired `-foreground` defaults to the default
 * foreground slot (base05) or the surface's complementary foreground, tunable
 * via overrides.
 */
const SHADCN_SURFACE: Record<string, Base24SlotKey> = {
	// Core surfaces (mirror abstract roles; shadcn consumes these names).
	"--background": "base00",
	"--foreground": "base05",
	"--card": "base01",
	"--card-foreground": "base05",
	"--popover": "base01",
	"--popover-foreground": "base05",
	"--secondary": "base02",
	"--secondary-foreground": "base06",
	"--muted": "base02",
	"--muted-foreground": "base04",
	"--accent": "base02",
	"--accent-foreground": "base06",
	"--destructive": "base08",
	"--destructive-foreground": "base06",
	"--border": "base01",
	"--input": "base01",
	"--ring": "base0B",
	// Primary + statuses (shadcn lacks success/warning/info; we add them).
	"--primary": "base0B",
	"--primary-foreground": "base00",
	"--success": "base0B",
	"--success-foreground": "base00",
	"--warning": "base0A",
	"--warning-foreground": "base00",
	"--info": "base0D",
	"--info-foreground": "base00",
	// Sidebar sits on the darkest backgrounds.
	"--sidebar": "base11",
	"--sidebar-foreground": "base06",
	"--sidebar-primary": "base0B",
	"--sidebar-primary-foreground": "base11",
	"--sidebar-accent": "base00",
	"--sidebar-accent-foreground": "base06",
	"--sidebar-border": "base01",
	"--sidebar-ring": "base0B",
	// Charts: rainbow default so arbitrary community schemes look coherent;
	// a tool's own scheme pins these via overrides (oqto's are monochrome).
	"--chart-1": "base0D",
	"--chart-2": "base0B",
	"--chart-3": "base0E",
	"--chart-4": "base09",
	"--chart-5": "base0C",
};

/** Map abstract roles to bare (non-shadcn) CSS var names. */
function roleVarName(role: keyof typeof ROLE_FOR_SLOT): string {
	// Roles map 1:1 onto shadcn's core names; this keeps the portable layer
	// and the shadcn surface aligned for the shared core set.
	return `--${role}`;
}

/** The 24 raw slot CSS var names (--base00..--base17), for debugging/showcases/direct slot access. */
export const SLOT_VARS = [
	"base00","base01","base02","base03","base04","base05","base06","base07",
	"base08","base09","base0A","base0B","base0C","base0D","base0E","base0F",
	"base10","base11","base12","base13","base14","base15","base16","base17",
] as const as readonly string[];

/**
 * Resolve a normalized scheme into the full set of emitted CSS variables.
 * Emits, in order: raw slots (--baseNN), abstract roles, shadcn surface; then
 * per-scheme overrides win last.
 */
export function mapSchemeToTokens(scheme: NormalizedScheme): SemanticTokenMap {
	const tokens: SemanticTokenMap = {};

	// Raw slots: always emit so showcases/debugging/components can read --baseNN directly.
	for (const slot of SLOT_VARS) {
		const v = scheme.slots[slot as Base24SlotKey];
		if (v) tokens[`--${slot}`] = v;
	}

	// Tier 1: abstract roles (portable).
	for (const [role, slot] of Object.entries(ROLE_FOR_SLOT)) {
		tokens[roleVarName(role as keyof typeof ROLE_FOR_SLOT)] = scheme.slots[slot];
	}

	// Tier 2: shadcn concrete surface (non-portable framework vocabulary).
	for (const [cssVar, slot] of Object.entries(SHADCN_SURFACE)) {
		tokens[cssVar] = scheme.slots[slot];
	}

	// Per-scheme overrides win.
	if (scheme.overrides) {
		for (const [cssVar, value] of Object.entries(scheme.overrides)) {
			if (value !== undefined) tokens[cssVar] = value;
		}
	}

	return tokens;
}

/** Every CSS variable this engine controls (slots + semantic), for clearing/inspection. */
export const MANAGED_SEMANTIC_VARS: readonly string[] = [
	...SLOT_VARS.map((s) => `--${s}`),
	...new Set([
		...Object.keys(SHADCN_SURFACE),
		...Object.values(ROLE_FOR_SLOT).map((s) => `--${s}`),
	]),
];

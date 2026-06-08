import type { Base24Scheme, Base24SlotKey, SemanticTokenMap } from "./types";

/**
 * Deterministic mapping from base24 slots to oqto's semantic CSS variables.
 *
 * The structural tokens (backgrounds, foregrounds, primary, destructive) follow
 * the base16 convention closely. Tokens that oqto themes monochromatically in
 * its own schemes (charts, accent) still get a sensible rainbow default here so
 * that arbitrary community base24 schemes produce a coherent reskin; oqto's own
 * schemes pin those via `overrides`.
 *
 * Background ramp:  base00 (default) -> base01 (cards) -> base02 (selection)
 *                   base10 (darker)  -> base11 (darkest, sidebar/terminal)
 * Foreground ramp:  base03 (dim) -> base04 (muted) -> base05 (default)
 *                   base06 (light) -> base07 (lightest)
 * Accents:          base08 red, base09 orange, base0A yellow, base0B green,
 *                   base0C cyan, base0D blue, base0E magenta, base0F brown
 */
const SLOT_FOR_TOKEN: Record<string, Base24SlotKey> = {
	"--background": "base00",
	"--foreground": "base05",
	"--card": "base01",
	"--card-foreground": "base05",
	"--popover": "base01",
	"--popover-foreground": "base05",
	"--primary": "base0B",
	"--primary-foreground": "base00",
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
	// Charts default to a rainbow spread for community schemes.
	"--chart-1": "base0D",
	"--chart-2": "base0B",
	"--chart-3": "base0E",
	"--chart-4": "base09",
	"--chart-5": "base0C",
	// Sidebar sits on the darkest backgrounds.
	"--sidebar": "base11",
	"--sidebar-foreground": "base06",
	"--sidebar-primary": "base0B",
	"--sidebar-primary-foreground": "base11",
	"--sidebar-accent": "base00",
	"--sidebar-accent-foreground": "base06",
	"--sidebar-border": "base01",
	"--sidebar-ring": "base0B",
	// App-specific surfaces.
	"--panel": "base10",
	"--panel-strong": "base00",
	"--terminal-bg": "base11",
	"--terminal-fg": "base07",
	"--code-bg": "base01",
	"--code-inline-bg": "base02",
	"--code-fg": "base05",
	"--code-border": "base02",
	"--code-muted": "base03",
	"--code-accent": "base0B",
	"--code-success": "base0B",
};

/**
 * Resolve a scheme into the full set of semantic CSS variables.
 * Generic slot mapping first, then per-scheme overrides win.
 */
export function mapSchemeToTokens(scheme: Base24Scheme): SemanticTokenMap {
	const tokens: SemanticTokenMap = {};
	for (const [cssVar, slot] of Object.entries(SLOT_FOR_TOKEN)) {
		tokens[cssVar] = scheme.slots[slot];
	}
	if (scheme.overrides) {
		for (const [cssVar, value] of Object.entries(scheme.overrides)) {
			if (value !== undefined) {
				tokens[cssVar] = value;
			}
		}
	}
	return tokens;
}

/** The semantic CSS variables this engine controls (for clearing/inspection). */
export const MANAGED_SEMANTIC_VARS: readonly string[] =
	Object.keys(SLOT_FOR_TOKEN);

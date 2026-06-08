import {
	MANAGED_SEMANTIC_VARS,
	mapSchemeToTokens,
} from "./map-scheme-to-tokens";
import type { Base24Scheme, ThemeMode } from "./types";

function resolveRoot(root?: HTMLElement): HTMLElement | null {
	if (root) return root;
	if (typeof document === "undefined") return null;
	return document.documentElement;
}

/**
 * Apply a base24 scheme to a root element by writing every semantic CSS
 * variable as an inline style. Inline styles override both the `:root` and
 * `.dark` rules from globals.css at runtime, so the applied scheme renders
 * identically regardless of which mode class is present.
 *
 * The light/dark class toggle remains owned by next-themes; pass `mode` to keep
 * the `.dark` class in sync when the engine drives the mode directly (e.g. in
 * the standalone workbench, which does not mount next-themes).
 */
export function applyScheme(
	scheme: Base24Scheme,
	mode?: ThemeMode,
	root?: HTMLElement,
): void {
	const el = resolveRoot(root);
	if (!el) return;

	const tokens = mapSchemeToTokens(scheme);
	for (const [name, value] of Object.entries(tokens)) {
		el.style.setProperty(name, value);
	}

	const effectiveMode = mode ?? scheme.mode;
	el.classList.toggle("dark", effectiveMode === "dark");
}

/** Remove every semantic variable this engine manages from a root element. */
export function clearScheme(root?: HTMLElement): void {
	const el = resolveRoot(root);
	if (!el) return;
	for (const name of MANAGED_SEMANTIC_VARS) {
		el.style.removeProperty(name);
	}
}

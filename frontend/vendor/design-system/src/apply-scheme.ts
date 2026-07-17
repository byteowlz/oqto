/**
 * Apply a scheme + identity to a root element.
 *
 * Pipeline: normalize scheme (derive base16) -> map to tokens -> apply identity
 * (fonts/spacing) -> apply radius dial (R1) -> apply color tokens -> sync the
 * `.dark` class.
 *
 * Inline styles override `:root` and `.dark` rules from a consumer's globals
 * at runtime, so the applied scheme renders identically regardless of which
 * mode class is present. The light/dark class toggle stays owned by the
 * consumer's theme lib (e.g. next-themes); pass `mode` to keep `.dark` in sync
 * when this engine drives mode directly (e.g. standalone playground).
 */

import { normalizeScheme } from "./derive-base16.js";
import {
	DEFAULT_IDENTITY,
	type IdentityTokens,
	identityVars,
} from "./identity-tokens.js";
import {
	MANAGED_SEMANTIC_VARS,
	mapSchemeToTokens,
} from "./map-scheme-to-tokens.js";
import { RADIUS_VARS, radiusVars } from "./radius.js";
import type { SemanticTokenMap, Scheme, ThemeMode } from "./types.js";

export interface ApplyOptions {
	/** Override per-tool identity (fonts, spacing). Radius is set via `radius`. */
	identity?: Partial<IdentityTokens>;
	/** Override the radius dial (R1). Defaults to identity.radius (sharp). */
	radius?: string;
	/** Override mode; defaults to the scheme's mode. */
	mode?: ThemeMode;
	/** Target element; defaults to document.documentElement. */
	root?: HTMLElement;
}

function resolveRoot(root?: HTMLElement): HTMLElement | null {
	if (root) return root;
	if (typeof document === "undefined") return null;
	return document.documentElement;
}

/**
 * Apply a scheme to a root element: normalizes base16, emits color tokens,
 * identity tokens, and the radius dial, and syncs the `.dark` class.
 */
export function applyScheme(scheme: Scheme, options?: ApplyOptions): void {
	const el = resolveRoot(options?.root);
	if (!el) return;

	const normalized = normalizeScheme(scheme);

	// Color tokens.
	const tokens: SemanticTokenMap = mapSchemeToTokens(normalized);
	for (const [name, value] of Object.entries(tokens)) {
		el.style.setProperty(name, value);
	}

	// Identity (fonts/spacing).
	const identity = { ...DEFAULT_IDENTITY, ...options?.identity };
	for (const [name, value] of Object.entries(identityVars(identity))) {
		el.style.setProperty(name, value);
	}

	// Radius dial (R1).
	const dial = options?.radius ?? identity.radius;
	for (const [name, value] of Object.entries(radiusVars(dial))) {
		el.style.setProperty(name, value);
	}

	// Mode class.
	const effectiveMode = options?.mode ?? normalized.mode;
	el.classList.toggle("dark", effectiveMode === "dark");
}

/** Remove every variable this engine manages (color + radius) from a root. */
export function clearScheme(root?: HTMLElement): void {
	const el = resolveRoot(root);
	if (!el) return;
	for (const name of MANAGED_FOR_CLEAR) {
		el.style.removeProperty(name);
	}
}

/** Color vars (semantic) + radius vars. Identity fonts/spacing left to consumer. */
const MANAGED_FOR_CLEAR: readonly string[] = [
	...MANAGED_SEMANTIC_VARS,
	...RADIUS_VARS,
];

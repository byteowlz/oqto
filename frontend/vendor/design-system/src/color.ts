/**
 * Zero-dependency color math for derivation and alpha.
 *
 * Every function returns a *string* by composing native CSS `color-mix()`,
 * so input may be any CSS color (hex, rgb(), oklch(), named) without parsing.
 * This is SSR-safe (no DOM access) and handles the oklch values real schemes
 * use. `color-mix` is supported in all current evergreen browsers (2023+).
 *
 * @see ../../spec/roles.md (alpha is derived, not slotted)
 */

/**
 * Darken a color toward black by `amount` percent (0-100).
 * darken("#222", 20) -> "color-mix(in srgb, #222 80%, black)"
 */
export function darken(color: string, amount: number): string {
	const pct = clampPct(100 - amount);
	return `color-mix(in srgb, ${color} ${pct}%, black)`;
}

/** Lighten a color toward white by `amount` percent (0-100). */
export function lighten(color: string, amount: number): string {
	const pct = clampPct(100 - amount);
	return `color-mix(in srgb, ${color} ${pct}%, white)`;
}

/**
 * Apply `alpha` (0-1) to a color via mix toward transparent.
 * withAlpha("#3ba77c", 0.2) -> "color-mix(in srgb, #3ba77c 20%, transparent)"
 *
 * Note: Tailwind v4 + shadcn already derive `/20`-style alpha natively from
 * the base CSS variable, so most consumers never call this. It exists for
 * non-Tailwind emission (e.g. inline styles, native Rust ports later).
 */
export function withAlpha(color: string, alpha: number): string {
	const pct = clampPct(alpha * 100);
	return `color-mix(in srgb, ${color} ${pct}%, transparent)`;
}

function clampPct(n: number): number {
	if (Number.isNaN(n)) return 0;
	return Math.min(100, Math.max(0, n));
}

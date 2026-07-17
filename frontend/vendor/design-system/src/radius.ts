/**
 * Radius — single dial, sharp-at-0. See spec/radius.md.
 *
 * The dial is ONE value (a CSS length, e.g. "0.5rem" or "0"). The categorical
 * scale (sm/md/lg/xl) is PROPORTIONAL to the dial so that:
 *   - dial = 0  -> every tier is 0 (dead sharp; oqto's identity)
 *   - dial > 0  -> every tier > 0 (the innermost `sm` rounds too)
 *
 * Why proportional, not additive concentric: strict concentric geometry
 * (outer - inset) decays the inner radius to zero whenever the dial is smaller
 * than the cumulative inset — so the innermost "never has a radius" at normal
 * settings. Multiplicative scaling is what satisfies both sharp-at-0 AND
 * inner-rounds. Harmonious proportional rounding is how real UIs (Apple,
 * shadcn) actually nest in practice.
 *
 * Exact concentric (shared arc centers) is still available via childRadius() /
 * parentRadius() for components that opt into strict per-pair nesting.
 *
 * @see ../../spec/radius.md
 */

/** A CSS length string, e.g. "0.5rem" or "0". */
export type Radius = string;

/**
 * Exact concentric: a child's radius given the parent's and the inset between
 * them. `max(0, ...)` clamps at sharp. Opt-in — decays to 0 when parent < inset.
 */
export function childRadius(parentRadius: Radius, inset: Radius): Radius {
	return `max(0px, calc(${parentRadius} - ${inset}))`;
}

/**
 * Exact concentric (leaf-anchored): a parent's radius given its child's and the
 * inset. parent = child + inset. Grows outward; never clamps. Use when you want
 * nesting to grow from a known leaf radius rather than decay from the outer.
 */
export function parentRadius(childRadiusValue: Radius, inset: Radius): Radius {
	return `calc(${childRadiusValue} + ${inset})`;
}

/** Categorical tiers, PROPORTIONAL to the dial. All -> 0 at dial = 0. */
export interface RadiusScale {
	/** The dial value itself (== lg). */
	radius: Radius;
	sm: Radius;
	md: Radius;
	lg: Radius;
	xl: Radius;
}

/**
 * Build the proportional categorical radius scale from a single dial.
 * Tiers: sm = 0.5x, md = 0.75x, lg = 1x, xl = 1.25x. All scale to 0 at dial=0.
 */
export function radiusScale(dial: Radius): RadiusScale {
	return {
		radius: dial,
		sm: `calc(${dial} * 0.5)`,
		md: `calc(${dial} * 0.75)`,
		lg: dial,
		xl: `calc(${dial} * 1.25)`,
	};
}

/** The CSS variable names this module manages. */
export const RADIUS_VARS = [
	"--radius",
	"--radius-sm",
	"--radius-md",
	"--radius-lg",
	"--radius-xl",
] as const;

/**
 * Resolve the radius scale to CSS variable assignments for a given dial.
 * Returns the vars to write; caller applies them to a root element.
 */
export function radiusVars(dial: Radius): Record<string, string> {
	const s = radiusScale(dial);
	return {
		"--radius": s.radius,
		"--radius-sm": s.sm,
		"--radius-md": s.md,
		"--radius-lg": s.lg,
		"--radius-xl": s.xl,
	};
}

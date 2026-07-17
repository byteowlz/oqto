/**
 * Identity tokens — per-tool non-color values plugged into the shared
 * mechanism. Part of a tool's LOOK (per-tool), not the mechanism.
 *
 * The standard ships SENSIBLE DEFAULTS; consumers override at apply time
 * (see applyScheme options). Notably, radius is a DIAL parameter, not a
 * hardcoded value — defaulting to "0" (sharp) so a fresh consumer renders
 * sharp until the dial is moved, matching oqto's identity and the R1 rule's
 * sharp-at-0 guarantee.
 *
 * @see ./radius.ts
 * @see ../../spec/radius.md
 */

export interface IdentityTokens {
	/** The radius dial (R1). "0" = sharp. Any CSS length. */
	radius: string;
	/** Body font stack. */
	fontSans: string;
	/** Monospace font stack. */
	fontMono: string;
	/** Base spacing unit. */
	spacing: string;
	/** Base letter tracking. */
	trackingNormal: string;
}

/** Neutral defaults. Radius defaults sharp; consumers dial it up. */
export const DEFAULT_IDENTITY: IdentityTokens = {
	radius: "0",
	fontSans: 'ui-sans-serif, system-ui, sans-serif',
	fontMono: 'ui-monospace, SFMono-Regular, monospace',
	spacing: "0.25rem",
	trackingNormal: "0em",
};

/** Resolve identity tokens to CSS variable assignments (font + spacing only). */
export function identityVars(identity: IdentityTokens): Record<string, string> {
	return {
		"--font-sans": identity.fontSans,
		"--font-mono": identity.fontMono,
		"--spacing": identity.spacing,
		"--tracking-normal": identity.trackingNormal,
		// --font-serif intentionally not managed; consumers set if needed.
	};
}

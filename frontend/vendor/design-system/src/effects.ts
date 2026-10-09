/**
 * Effect tokens — elevation shadows and backdrop blur.
 *
 * Flat is the default identity (every token empty/zero), matching sharp-at-0
 * radius: a scheme or user theme opts into depth by overriding these emitted
 * variables. Presets exist so authoring surfaces (and agents) compose
 * coherent elevation instead of inventing per-surface shadows.
 */

export const EFFECT_VARS = [
	"--shadow-sm",
	"--shadow-md",
	"--shadow-lg",
	"--backdrop-blur",
] as const;

export type ShadowPresetId = "none" | "soft" | "strong";

export const SHADOW_PRESETS: Record<
	ShadowPresetId,
	Record<string, string>
> = {
	none: {},
	soft: {
		"--shadow-sm": "0 1px 2px rgba(0, 0, 0, 0.18)",
		"--shadow-md": "0 4px 12px rgba(0, 0, 0, 0.22)",
		"--shadow-lg": "0 12px 32px rgba(0, 0, 0, 0.28)",
	},
	strong: {
		"--shadow-sm": "0 2px 4px rgba(0, 0, 0, 0.35)",
		"--shadow-md": "0 8px 24px rgba(0, 0, 0, 0.4)",
		"--shadow-lg": "0 20px 48px rgba(0, 0, 0, 0.5)",
	},
};

/** Default emission: flat. Overrides (scheme or user) win over these. */
export function effectVars(): Record<string, string> {
	return {
		"--shadow-sm": "none",
		"--shadow-md": "none",
		"--shadow-lg": "none",
		"--backdrop-blur": "0px",
	};
}

/** Identify which preset a set of overrides currently matches. */
export function matchShadowPreset(
	overrides: Record<string, string>,
): ShadowPresetId {
	for (const id of ["soft", "strong"] as const) {
		if (SHADOW_PRESETS[id]["--shadow-md"] === overrides["--shadow-md"]) {
			return id;
		}
	}
	return "none";
}

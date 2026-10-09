import {
	NEUTRAL_TOKEN_HEX,
	SHADOW_PRESETS,
	type ShadowPresetId,
	matchShadowPreset,
	readTokenHex,
} from "@byteowlz/design-system";
import {
	type OqtoUiUserTheme,
	type ParsedUserTheme,
	parseUserThemeJson,
} from "../platform/base24-theme";

export { parseUserThemeJson, NEUTRAL_TOKEN_HEX, matchShadowPreset };
export type { ParsedUserTheme, OqtoUiUserTheme, ShadowPresetId };

export const SHADOW_PRESET_IDS = ["none", "soft", "strong"] as const;

/**
 * Font presets the customizer offers. Only JetBrainsMono Nerd Font is bundled
 * by the app; every other stack must rely on system-available fonts so
 * unresolved entries degrade to the generic family.
 */
export type FontPresetId = "system" | "mono" | "humanist" | "serif";

export const JETBRAINS_MONO_STACK =
	'"JetBrainsMono Nerd Font", ui-monospace, monospace';

export const FONT_SANS_PRESETS: ReadonlyArray<{
	id: FontPresetId;
	stack: string | undefined;
}> = [
	{ id: "system", stack: undefined },
	{
		id: "mono",
		stack: JETBRAINS_MONO_STACK,
	},
	{
		id: "humanist",
		stack: '"Helvetica Neue", Helvetica, Arial, sans-serif',
	},
	{ id: "serif", stack: 'Georgia, "Times New Roman", serif' },
];

export const FONT_MONO_PRESETS: ReadonlyArray<{
	id: Extract<FontPresetId, "system" | "mono">;
	stack: string | undefined;
}> = [
	{ id: "system", stack: undefined },
	{
		id: "mono",
		stack: '"JetBrainsMono Nerd Font", ui-monospace, monospace',
	},
];

export function matchFontPreset(
	presets: ReadonlyArray<{ id: string; stack: string | undefined }>,
	stack: string | undefined,
): string {
	return presets.find((preset) => preset.stack === stack)?.id ?? "custom";
}

export function withShadowPreset(
	theme: OqtoUiUserTheme,
	preset: ShadowPresetId,
): OqtoUiUserTheme {
	const overrides = { ...theme.overrides };
	for (const level of ["--shadow-sm", "--shadow-md", "--shadow-lg"]) {
		delete overrides[level];
	}
	return {
		...theme,
		overrides: { ...overrides, ...SHADOW_PRESETS[preset] },
	};
}

export function snapshotTokenHex(
	root: HTMLElement,
	tokens: readonly string[],
): Record<string, string> {
	const snapshot: Record<string, string> = {};
	for (const token of tokens) {
		snapshot[token] = readTokenHex(root, token);
	}
	return snapshot;
}

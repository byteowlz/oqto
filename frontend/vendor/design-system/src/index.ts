/**
 * @byteowlz/design-system — shadcn-ts implementation.
 *
 * The reusable, tool-agnostic base24 design-system mechanism:
 *  - slots (base24/base16) + closed abstract role layer (portable)
 *  - shadcn concrete surface (framework vocabulary, non-portable)
 *  - R1 concentric radius (single dial, sharp-at-0)
 *  - base16 normalization (derive / pureBase16 alias)
 *  - tinted-theming ecosystem import
 *
 * @see ../../spec/*.md for the target-agnostic contract
 */

// Types.
export type {
	Base16SlotKey,
	Base24OnlySlotKey,
	Base24SlotKey,
	Base24Scheme,
	NormalizedScheme,
	Scheme,
	SchemeSystem,
	Slots,
	SemanticTokenMap,
	ThemeMode,
} from "./types.js";
export type { IdentityTokens } from "./identity-tokens.js";
export type { Radius, RadiusScale } from "./radius.js";
export type { ApplyOptions } from "./apply-scheme.js";
export type { AbstractRole } from "./roles.js";

// Engine.
export { normalizeScheme } from "./derive-base16.js";
export { mapSchemeToTokens, MANAGED_SEMANTIC_VARS, SLOT_VARS } from "./map-scheme-to-tokens.js";
export { applyScheme, clearScheme } from "./apply-scheme.js";
export { DEFAULT_IDENTITY, identityVars } from "./identity-tokens.js";
export {
	childRadius,
	parentRadius,
	radiusScale,
	radiusVars,
	RADIUS_VARS,
} from "./radius.js";
export { darken, lighten, withAlpha } from "./color.js";

// Closed role layer.
export { ROLE_FOR_SLOT, ABSTRACT_ROLES } from "./roles.js";

// Ecosystem import.
export { schemeFromTinted, fromTintedYaml } from "./tinted-loader.js";

// House schemes.
export {
	builtInSchemes,
	builtInSchemeList,
	defaultSchemeForMode,
	oqtoDark,
	oqtoLight,
	ember,
	nordBase16,
} from "./schemes/index.js";

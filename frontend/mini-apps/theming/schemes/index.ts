import type { Base24Scheme, ThemeMode } from "../types";
import emberJson from "./ember.json";
import oqtoDarkJson from "./oqto-dark.json";
import oqtoLightJson from "./oqto-light.json";

const oqtoDark = oqtoDarkJson as Base24Scheme;
const oqtoLight = oqtoLightJson as Base24Scheme;
const ember = emberJson as Base24Scheme;

/** All schemes bundled with the design system, keyed by id. */
export const builtInSchemes: Readonly<Record<string, Base24Scheme>> = {
	[oqtoDark.id]: oqtoDark,
	[oqtoLight.id]: oqtoLight,
	[ember.id]: ember,
};

/** Ordered list for pickers. */
export const builtInSchemeList: ReadonlyArray<Base24Scheme> = [
	oqtoDark,
	oqtoLight,
	ember,
];

/** The canonical oqto scheme for a given mode. */
export function defaultSchemeForMode(mode: ThemeMode): Base24Scheme {
	return mode === "dark" ? oqtoDark : oqtoLight;
}

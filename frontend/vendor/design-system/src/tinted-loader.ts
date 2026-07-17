/**
 * Ecosystem import — load any tinted-theming base16/base24 scheme.
 *
 * tinted spec-0.11 YAML is simple enough to parse naively (flat `palette:`
 * block of `baseNN: "#hex"` lines + top-level `system`/`name`/`variant`).
 * This avoids a YAML dependency for the common case. For complex YAML, parse
 * upstream and pass the object to `schemeFromTinted()`.
 *
 * Loaded base16 schemes are normalized (derived) by normalizeScheme() at apply
 * time; see spec/base16-policy.md.
 *
 * @see ../../spec/base16-policy.md
 */

import type { Scheme, SchemeSystem, Slots, ThemeMode } from "./types.js";

interface TintedParsed {
	system?: string;
	name?: string;
	author?: string;
	variant?: string;
	palette?: Record<string, string>;
}

/**
 * Build a Scheme from a parsed tinted object (e.g. from js-yaml or a YAML lib).
 * `id` defaults to a slug of the name.
 */
export function schemeFromTinted(parsed: TintedParsed, id?: string): Scheme {
	const system: SchemeSystem = parsed.system === "base24" ? "base24" : "base16";
	const slots: Slots = { ...(parsed.palette ?? {}) };
	return {
		id: id ?? slug(parsed.name ?? "imported"),
		name: parsed.name ?? id ?? "Imported scheme",
		mode: (parsed.variant === "light" ? "light" : "dark") as ThemeMode,
		system,
		slots,
	};
}

/**
 * Naive loader for a raw tinted YAML string. Handles the flat palette block
 * used by tinted spec-0.11. Throws on malformed input — use schemeFromTinted()
 * with a real YAML parser for non-standard files.
 */
export function fromTintedYaml(yaml: string, id?: string): Scheme {
	const parsed: TintedParsed = {};
	let inPalette = false;
	for (const raw of yaml.split("\n")) {
		const line = raw.replace(/\s+$/, "");
		if (!line.trimStart() || line.trimStart().startsWith("#")) continue;
		const indent = line.length - line.trimStart().length;
		const m = line.match(/^([a-zA-Z0-9_]+):\s*(.*)$/);
		if (!m) continue;
		const key = m[1] ?? "";
		const val = m[2] ?? "";
		if (indent === 0) {
			inPalette = key === "palette";
			if (key !== "palette") {
				(parsed as Record<string, string>)[key] = val.replace(/['"]/g, "");
			}
			continue;
		}
		if (inPalette && key.startsWith("base")) {
			parsed.palette = parsed.palette ?? {};
			parsed.palette[key] = val.replace(/['"]/g, "");
		}
	}
	return schemeFromTinted(parsed, id);
}

function slug(name: string): string {
	return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

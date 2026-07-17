/**
 * base16 normalization — spec/base16-policy.md.
 *
 * Three tiers:
 *  1. Twin exists -> caller should load the base24 twin instead (not handled
 *     here; the loader picks the twin before calling normalize).
 *  2. base16-only -> DERIVE the 8 missing slots (base10-17).
 *  3. Opt-in duplication (`pureBase16: true`) -> alias instead of derive.
 *
 * Derivation uses color-mix (see color.ts): darken(base00) -> base10/11;
 * lighten(base08..0E) -> base12..17. This is mechanically what a human port
 * of a base16 scheme to base24 does.
 *
 * @see ../../spec/base16-policy.md
 */

import { darken, lighten } from "./color.js";
import type { Base24SlotKey, NormalizedScheme, Scheme } from "./types.js";

const BASE24_ONLY_SLOTS = [
	"base10",
	"base11",
	"base12",
	"base13",
	"base14",
	"base15",
	"base16",
	"base17",
] as const;

/** The base16 slot each bright derives from (base12<-base08, ..., base17<-base0E). */
const BRIGHT_SOURCE: Record<string, string> = {
	base12: "base08",
	base13: "base0A",
	base14: "base0B",
	base15: "base0C",
	base16: "base0D",
	base17: "base0E",
};

/**
 * Normalize any scheme to a full base24 scheme.
 * - base24 schemes pass through (slots assumed complete).
 * - base16 schemes get base10-17 derived or aliased.
 * Throws if a required base16 slot is missing.
 */
export function normalizeScheme(scheme: Scheme): NormalizedScheme {
	if (scheme.system === "base24") {
		return { ...scheme, system: "base24", slots: { ...scheme.slots } } as NormalizedScheme;
	}
	return normalizeBase16(scheme);
}

function normalizeBase16(scheme: Scheme): NormalizedScheme {
	const slots = { ...scheme.slots };
	// Require the 16 base slots.
	for (const k of BASE16_REQUIRED) {
		if (!slots[k as Base24SlotKey]) {
			throw new Error(
				`design-system: base16 scheme "${scheme.id}" missing required slot ${k}`,
			);
		}
	}

	const base00 = slots.base00 as string;

	for (const slot of BASE24_ONLY_SLOTS) {
		if (slots[slot]) continue; // author may have provided some
		slots[slot] = scheme.pureBase16
			? aliasFor(slot, slots)
			: deriveFor(slot, base00, slots);
	}

	return {
		...scheme,
		system: "base24",
		slots: slots as NormalizedScheme["slots"],
	};
}

/** Derive a base24-only slot from base16 values. */
function deriveFor(
	slot: (typeof BASE24_ONLY_SLOTS)[number],
	base00: string,
	slots: Scheme["slots"],
): string {
	switch (slot) {
		case "base10": // darker bg
			return darken(base00, 18);
		case "base11": // darkest bg (sidebar/terminal)
			return darken(base00, 34);
		default: {
			// brights: lighten the matching base16 accent
			const src = BRIGHT_SOURCE[slot];
			const from = (src && slots[src as Base24SlotKey]) || base00;
			return lighten(from, 22);
		}
	}
}

/** Duplicate (alias) policy — tier 3 escape hatch. */
function aliasFor(
	slot: (typeof BASE24_ONLY_SLOTS)[number],
	slots: Scheme["slots"],
): string {
	// Darker backgrounds collapse to base00; brights reuse their base16 source.
	const src = slot === "base10" || slot === "base11" ? "base00" : BRIGHT_SOURCE[slot];
	return (slots[src as Base24SlotKey] as string) ?? (slots.base00 as string);
}

const BASE16_REQUIRED = [
	"base00","base01","base02","base03","base04","base05","base06","base07",
	"base08","base09","base0A","base0B","base0C","base0D","base0E","base0F",
] as const;

/**
 * The closed ABSTRACT ROLE LAYER — the portable surface.
 *
 * A role is a semantic intent bound to a slot. This set is CLOSED (see
 * spec/roles.md). Membership is justified by universal screen-UI intent, not
 * by echoing any framework's token names. F1: minimal ramps (2 foreground
 * levels, 3 surface depths).
 *
 * Multiple roles may bind the same slot (e.g. primary & success -> base0B;
 * secondary/muted/accent -> base02). Hue and role are separate indirections.
 *
 * @see ../../spec/roles.md
 */

import type { Base24SlotKey } from "./types.js";

/** The closed abstract roles. Do not extend in consumer code. */
export type AbstractRole =
	// Foreground ramp (2 levels).
	| "foreground"
	| "muted-foreground"
	// Surface depth (3 levels).
	| "background"
	| "surface"
	| "surface-sunken"
	// Action & status.
	| "primary"
	| "secondary"
	| "muted"
	| "accent"
	| "success"
	| "warning"
	| "danger"
	| "info"
	// Structure.
	| "border"
	| "ring"
	| "input";

/**
 * Default role -> slot binding. 16 roles. See spec/roles.md for the intent of
 * each and why some share a slot.
 */
export const ROLE_FOR_SLOT: Readonly<Record<AbstractRole, Base24SlotKey>> = {
	// Foreground ramp.
	foreground: "base05",
	"muted-foreground": "base04",
	// Surface depth.
	background: "base00",
	surface: "base01",
	"surface-sunken": "base11",
	// Action & status.
	primary: "base0B",
	secondary: "base02",
	muted: "base02",
	accent: "base02",
	success: "base0B",
	warning: "base0A",
	danger: "base08",
	info: "base0D",
	// Structure.
	border: "base01",
	ring: "base0B",
	input: "base01",
};

/** The full closed set, for validation / iteration. */
export const ABSTRACT_ROLES = Object.keys(ROLE_FOR_SLOT) as AbstractRole[];

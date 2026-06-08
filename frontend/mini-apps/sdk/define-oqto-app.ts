import type { OqtoApp, OqtoAppDefinition } from "./types";

/**
 * Declare a mini-app. Thin by design -- it exists so app modules have a single,
 * named entry point and so future validation/registration can hook in here
 * without touching every app.
 */
export function defineOqtoApp(definition: OqtoAppDefinition): OqtoApp {
	return definition;
}

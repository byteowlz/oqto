/** Narrowing helpers for decoded JSON (kernel-internal). */

import type { ExtensionFields, JsonValue } from "../model";

export function isJsonObject(value: JsonValue | undefined): value is ExtensionFields {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Collects properties outside `knownKeys` so unknown variants survive. */
export function extensionsOf(
	record: ExtensionFields,
	knownKeys: readonly string[],
): ExtensionFields | undefined {
	const known = new Set(knownKeys);
	const rest: { [key: string]: JsonValue } = {};
	let found = false;
	for (const [key, value] of Object.entries(record)) {
		if (!known.has(key)) {
			rest[key] = value;
			found = true;
		}
	}
	return found ? rest : undefined;
}

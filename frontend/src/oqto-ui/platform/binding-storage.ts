/**
 * Device-local keyboard overrides. ADR-0040 layers configuration; this is
 * the most specific layer: what this person changed on this device, merged
 * over the resolved config, which is itself merged over the defaults. The
 * stored shape is the config's own `{ keys, action }` so one merge serves
 * every layer.
 */

import type { LayoutDocumentStore } from "./layout-storage";

export interface BindingOverride {
	readonly keys: string;
	readonly action: string;
}

export function bindingStorageKey(platformId: string): string {
	return `oqto-ui:bindings:${platformId}`;
}

function isOverride(value: unknown): value is BindingOverride {
	if (typeof value !== "object" || value === null) return false;
	const candidate = value as { keys?: unknown; action?: unknown };
	return (
		typeof candidate.keys === "string" && typeof candidate.action === "string"
	);
}

/** Unreadable or malformed storage reads as "nothing overridden". */
export function readBindingOverrides(
	store: LayoutDocumentStore,
	key: string,
): readonly BindingOverride[] {
	const document = store.read(key);
	if (document === null) return [];
	try {
		const parsed: unknown = JSON.parse(document);
		return Array.isArray(parsed) ? parsed.filter(isOverride) : [];
	} catch {
		return [];
	}
}

export function writeBindingOverrides(
	store: LayoutDocumentStore,
	key: string,
	overrides: readonly BindingOverride[],
): void {
	store.write(key, JSON.stringify(overrides));
}

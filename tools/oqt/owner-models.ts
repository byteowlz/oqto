import { join } from "node:path";
import type { ModelsStore, ModelsStoreEntry } from "@earendil-works/pi-ai";
import { readOwnedJson } from "./readonly-profile.ts";

// Owner catalog reads with an in-memory write overlay: never let upstream's
// cache refresh/logout bookkeeping create or alter files in the original Pi.
export function ownerModelsStore(agentDirectory: string): ModelsStore {
	const overlay = new Map<string, ModelsStoreEntry | undefined>();
	return {
		async read(providerId, options) {
			options?.signal?.throwIfAborted();
			if (overlay.has(providerId))
				return structuredClone(overlay.get(providerId));
			let data: Record<string, unknown>;
			try {
				data = await readOwnedJson(
					join(agentDirectory, "models-store.json"),
					false,
				);
			} catch (error) {
				if (
					error instanceof Error &&
					"code" in error &&
					error.code === "ENOENT"
				)
					return undefined;
				throw error;
			}
			options?.signal?.throwIfAborted();
			const entry = data[providerId];
			if (entry === undefined) return undefined;
			if (
				typeof entry !== "object" ||
				entry === null ||
				!("models" in entry) ||
				!Array.isArray(entry.models)
			)
				throw new Error("Invalid owner model catalog entry");
			// ModelsStoreEntry is upstream's serialized interchange contract; models
			// themselves are interpreted by the upstream Models runtime, not by tools.
			return structuredClone(entry) as ModelsStoreEntry;
		},
		async write(providerId, entry, options) {
			options?.signal?.throwIfAborted();
			overlay.set(providerId, structuredClone(entry));
		},
		async delete(providerId, options) {
			options?.signal?.throwIfAborted();
			overlay.set(providerId, undefined);
		},
	};
}

export async function ownerModelDefaults(agentDirectory: string) {
	const settings = await readOwnedJson(
		join(agentDirectory, "settings.json"),
		false,
	);
	const { defaultProvider, defaultModel, defaultThinkingLevel } = settings;
	if (
		typeof defaultProvider !== "string" ||
		!defaultProvider ||
		typeof defaultModel !== "string" ||
		!defaultModel
	)
		throw new Error("Explicit owning Pi model defaults required");
	if (
		defaultThinkingLevel !== undefined &&
		typeof defaultThinkingLevel !== "string"
	)
		throw new Error("Invalid owning Pi thinking level");
	return {
		provider: defaultProvider,
		model: defaultModel,
		thinking: defaultThinkingLevel,
	};
}

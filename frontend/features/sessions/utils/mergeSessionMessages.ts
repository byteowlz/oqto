import type { OpenCodeMessageWithParts } from "@/features/sessions/api";

// Merge messages to prevent flickering - preserves existing message references when unchanged.
// Also preserves optimistic (temp-*) messages that haven't been confirmed yet.
export function mergeSessionMessages(
	prev: OpenCodeMessageWithParts[],
	next: OpenCodeMessageWithParts[],
): OpenCodeMessageWithParts[] {
	if (prev.length === 0) return next;
	if (next.length === 0) {
		// Keep optimistic messages even if server returns empty.
		const optimistic = prev.filter((m) => m.info.id.startsWith("temp-"));
		return optimistic.length > 0 ? optimistic : next;
	}

	// Build a map of existing messages by ID for quick lookup.
	const prevById = new Map(prev.map((m) => [m.info.id, m]));

	// Preserve optimistic messages (temp-*) that don't have corresponding real messages yet.
	const optimisticMessages = prev.filter((m) => m.info.id.startsWith("temp-"));
	const pendingOptimistic = optimisticMessages.filter((optMsg) => {
		const optText = optMsg.parts.find((p) => p.type === "text")?.text || "";
		const optCreated = optMsg.info.time?.created;
		const hasMatchingRealMessage = next.some((m) => {
			if (m.info.role !== "user") return false;
			const realText = m.parts.find((p) => p.type === "text")?.text || "";
			if (realText !== optText) return false;
			const realCreated = m.info.time?.created;
			if (optCreated && realCreated) {
				return Math.abs(realCreated - optCreated) < 15000;
			}
			return false;
		});
		return !hasMatchingRealMessage;
	});

	// Merge: keep existing reference if message hasn't changed, otherwise use new one.
	const merged = next.map((newMsg) => {
		const existing = prevById.get(newMsg.info.id);
		if (!existing) return newMsg;

		// Compare parts length and last part to detect changes.
		const existingParts = existing.parts;
		const newParts = newMsg.parts;

		if (existingParts.length !== newParts.length) return newMsg;

		// Check if the last part has changed (most common case during streaming).
		if (newParts.length > 0) {
			const lastNew = newParts[newParts.length - 1];
			const lastExisting = existingParts[existingParts.length - 1];

			if (lastNew.type === "text" && lastExisting.type === "text") {
				if (lastNew.text !== lastExisting.text) return newMsg;
			} else if (lastNew.type === "tool" && lastExisting.type === "tool") {
				if (
					lastNew.state?.status !== lastExisting.state?.status ||
					lastNew.state?.output !== lastExisting.state?.output
				) {
					return newMsg;
				}
			} else if (lastNew.type !== lastExisting.type) {
				return newMsg;
			}
		}

		// No significant changes detected, keep existing reference.
		return existing;
	});

	const combined =
		pendingOptimistic.length > 0 ? [...merged, ...pendingOptimistic] : merged;

	return combined
		.map((message, index) => ({
			message,
			index,
			created: message.info.time?.created ?? 0,
		}))
		.sort((a, b) => a.created - b.created || a.index - b.index)
		.map(({ message }) => message);
}

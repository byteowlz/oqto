import type { MessageWithParts } from "@/features/sessions/api";

// Generate a fingerprint for optimistic message matching
function getOptimisticFingerprint(msg: MessageWithParts): string {
	const text = msg.parts.find((p) => p.type === "text")?.text || "";
	return `${msg.info.role}:${text}`;
}

// Merge messages to prevent flickering - preserves existing message references when unchanged.
// Also preserves optimistic (temp-*) messages that haven't been confirmed yet.
export function mergeSessionMessages(
	prev: MessageWithParts[],
	next: MessageWithParts[],
): MessageWithParts[] {
	if (prev.length === 0) return next;
	if (next.length === 0) {
		// Keep optimistic messages even if server returns empty.
		const optimistic = prev.filter((m) => m.info.id.startsWith("temp-"));
		return optimistic.length > 0 ? optimistic : next;
	}

	// Build a map of existing messages by ID for O(1) lookup.
	const prevById = new Map(prev.map((m) => [m.info.id, m]));

	// Pre-index next messages by fingerprint for O(1) optimistic matching
	const nextByFingerprint = new Map<string, MessageWithParts[]>();
	for (const m of next) {
		if (m.info.role !== "user") continue;
		const fp = getOptimisticFingerprint(m);
		const existing = nextByFingerprint.get(fp);
		if (existing) {
			existing.push(m);
		} else {
			nextByFingerprint.set(fp, [m]);
		}
	}

	// Preserve optimistic messages (temp-*) that don't have corresponding real messages yet.
	const pendingOptimistic: MessageWithParts[] = [];
	for (const optMsg of prev) {
		if (!optMsg.info.id.startsWith("temp-")) continue;

		const optCreated = optMsg.info.time?.created;
		const fp = getOptimisticFingerprint(optMsg);
		const candidates = nextByFingerprint.get(fp);

		let hasMatch = false;
		if (candidates) {
			for (const m of candidates) {
				const realCreated = m.info.time?.created;
				if (optCreated && realCreated) {
					if (Math.abs(realCreated - optCreated) < 15000) {
						hasMatch = true;
						break;
					}
				} else {
					// No timestamps, consider it a match by content
					hasMatch = true;
					break;
				}
			}
		}

		if (!hasMatch) {
			pendingOptimistic.push(optMsg);
		}
	}

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

	if (pendingOptimistic.length === 0) {
		// Fast path: no optimistic messages, check if already sorted
		let isSorted = true;
		for (let i = 1; i < merged.length; i++) {
			const prevCreated = merged[i - 1].info.time?.created ?? 0;
			const currCreated = merged[i].info.time?.created ?? 0;
			if (currCreated < prevCreated) {
				isSorted = false;
				break;
			}
		}
		if (isSorted) return merged;
	}

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

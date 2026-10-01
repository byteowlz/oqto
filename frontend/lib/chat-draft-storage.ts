/** Draft storage is best-effort; unavailable storage must not block composing. */
export function writeChatDraft(storageKey: string, value: string): void {
	try {
		if (value.trim()) localStorage.setItem(storageKey, value);
		else localStorage.removeItem(storageKey);
	} catch {
		// Preserve the editable draft when browser storage is unavailable.
	}
}

/** Completed speech is committed draft content, unlike recognition previews. */
export function appendCompletedDraft(
	storageKey: string,
	currentValue: string,
	text: string,
): string {
	const value = currentValue ? `${currentValue} ${text}` : text;
	writeChatDraft(storageKey, value);
	return value;
}

function sanitizeStorageSegment(value: string): string {
	return value.replace(/[^a-zA-Z0-9._-]+/g, "_");
}

export function buildLegacyDraftStorageKey(storageKeyPrefix: string): string {
	return `${storageKeyPrefix}:draft`;
}

export function buildSessionDraftStorageKey(
	storageKeyPrefix: string,
	sessionId?: string | null,
): string {
	const draftSessionScope = sanitizeStorageSegment(
		sessionId ?? "__no_session__",
	);
	return `${storageKeyPrefix}:session:${draftSessionScope}:draft`;
}

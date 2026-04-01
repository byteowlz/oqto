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

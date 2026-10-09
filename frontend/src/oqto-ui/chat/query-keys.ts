/** Stable cache identities for durable pages and disposable stream drafts. */
export function timelineQueryKey(platformId: string, sessionId: string) {
	return ["oqto-ui", "timeline", platformId, sessionId] as const;
}

export function turnDraftQueryKey(platformId: string, sessionId: string) {
	return ["oqto-ui", "turn-draft", platformId, sessionId] as const;
}

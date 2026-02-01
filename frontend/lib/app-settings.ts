const DEFAULT_CHAT_PREFETCH_LIMIT = 8;
const MAX_CHAT_PREFETCH_LIMIT = 100;

let chatPrefetchLimit = DEFAULT_CHAT_PREFETCH_LIMIT;

function clampPrefetchLimit(value: number) {
	if (!Number.isFinite(value)) return DEFAULT_CHAT_PREFETCH_LIMIT;
	const rounded = Math.trunc(value);
	if (rounded < 0) return 0;
	if (rounded > MAX_CHAT_PREFETCH_LIMIT) return MAX_CHAT_PREFETCH_LIMIT;
	return rounded;
}

export function setChatPrefetchLimit(value: unknown) {
	if (typeof value !== "number") {
		chatPrefetchLimit = DEFAULT_CHAT_PREFETCH_LIMIT;
		return;
	}
	chatPrefetchLimit = clampPrefetchLimit(value);
}

export function getChatPrefetchLimit() {
	return chatPrefetchLimit;
}
